use std::cmp::max;

use fxhash::FxBuildHasher;
use ir::{
    SupportedType,
    circuit::Circuit,
    gate::{Gate, GateIdx},
};
use op::BinOp;
use std::collections::HashMap;
use thin_vec::{ThinVec, thin_vec};

use crate::{Backend, BackendError, BackendResult, validation::validate_same_length};

type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

#[derive(Clone, Debug, Default)]
pub struct MockFHEBackend {
    q: SupportedType,
    noise: Noise,
}

impl MockFHEBackend {
    pub fn new(noise_budget: usize) -> Self {
        Self {
            q: SupportedType::default(),
            noise: Noise::new(noise_budget),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct FHEElement {
    value: SupportedType,
    depth: usize,
    noise: usize,
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct Noise {
    noise_budget: usize,
    max_so_far: usize,
    index: usize,
}

impl Noise {
    fn new(noise_budget: usize) -> Self {
        Self {
            noise_budget,
            ..Default::default()
        }
    }
    fn update(&mut self, value: usize, index: usize) -> Result<(), BackendError> {
        if value < self.max_so_far {
            return Ok(());
        }

        self.max_so_far = value;
        self.index = index;

        if self.max_so_far <= self.noise_budget {
            return Ok(());
        }

        Err(BackendError::NoiseBudgetExceeded(
            self.noise_budget,
            self.max_so_far,
            self.index,
        ))
    }
}

const ENC_NOISE: usize = 1; // initial noise added during encryption 
const ADD_COST: usize = 1; // noises are added 
const MUL_COST: usize = 5; // noises generally compound thus larger mock value

impl Backend for MockFHEBackend {
    type Elem = FHEElement;

    fn add(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem {
        let mut element = self.constant(lhs.value + rhs.value);
        element.depth = max(lhs.depth, rhs.depth);
        element.noise = max(lhs.noise, rhs.noise) + ADD_COST;
        element
    }
    fn sub(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem {
        let mut element = self.constant(lhs.value - rhs.value);
        element.depth = max(lhs.depth, rhs.depth);
        element.noise = max(lhs.noise, rhs.noise) + ADD_COST;
        element
    }
    // NOTE: Multiplication by a known constant is often cheaper, our folding at dsl ensures that
    // we don't have an operation in the circuit which is constant op constant, it is automatically
    // folded thus we don't consider it's depth here. We consider other_than_const * op is to have
    // the same depth and noise as other_than_const * other_than_const.
    fn mul(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem {
        let mut element = self.constant(lhs.value * rhs.value);
        element.depth = max(lhs.depth, rhs.depth) + 1;
        element.noise = lhs.noise + rhs.noise + MUL_COST;
        element
    }
    fn constant(&mut self, c: SupportedType) -> Self::Elem {
        FHEElement {
            value: c % self.q,
            ..Default::default()
        }
    }
    fn input(&mut self, c: SupportedType) -> Self::Elem {
        FHEElement {
            value: c % self.q,
            noise: ENC_NOISE,
            ..Default::default()
        }
    }
    // TODO: Introduce a cache to avoid evaluating the same circuit twice.
    // NOTE: eval currently acts as the inputs are given plaintext, thus the mock fhe backend
    // pseudo-encrypts them with elements having a noise.
    fn eval(
        &mut self,
        circuit: &Circuit,
        with: &[SupportedType],
    ) -> BackendResult<ThinVec<Self::Elem>> {
        validate_same_length(circuit, with)?;
        self.q = circuit.q;
        // NOTE: we restart noise because certain outputs can be below the budget but if we
        // evaluate an output which overwhelms the noise budget, we won't be able to evalute
        // anything after that. Also the max and index values will be wrong no matter the case,
        // once a circuit is evaluated.
        self.noise = Noise::new(self.noise.noise_budget);

        let mut results: ThinVec<Self::Elem> =
            thin_vec![Self::Elem::default(); circuit.gates().len()];
        // One-to-one mapping between gate and elements, thus the same indices.
        for (ix, (_, gate)) in circuit.gates().iter().enumerate() {
            let element = match gate {
                Gate::Input(index) => self.input(with[*index]),
                Gate::Const(constant) => self.constant(*constant),
                Gate::BinOp(bin_op, lhs, rhs) => {
                    let lhs_result_index = lhs.into_raw().into_u32();
                    let rhs_result_index = rhs.into_raw().into_u32();

                    let lhs_result = &results[lhs_result_index as usize];
                    let rhs_result = &results[rhs_result_index as usize];

                    match bin_op {
                        BinOp::Add => self.add(lhs_result, rhs_result),
                        BinOp::Sub => self.sub(lhs_result, rhs_result),
                        BinOp::Mul => self.mul(lhs_result, rhs_result),
                    }
                }
            };
            self.noise.update(element.noise, ix)?;
            results[ix] = element;
        }

        BackendResult::Ok(results)
    }
    fn eval_outputs(
        &mut self,
        circuit: &Circuit,
        with: &[SupportedType],
    ) -> BackendResult<ThinVec<Self::Elem>> {
        validate_same_length(circuit, with)?;
        self.q = circuit.q;
        // NOTE: we restart noise because certain outputs can be below the budget but if we
        // evaluate an output which overwhelms the noise budget, we won't be able to evalute
        // anything after that. Also the max and index values will be wrong no matter the case,
        // once a circuit is evaluated.
        self.noise = Noise::new(self.noise.noise_budget);

        let mut roots = circuit.outputs().iter();
        let mut dfs_stack = ThinVec::new();

        let mut current_node = roots.next().copied();
        let mut gate_idx_to_elem: FxHashMap<GateIdx, Self::Elem> =
            HashMap::with_hasher(FxBuildHasher::default());

        // Iterative post order traversal from each output node eliminates all the
        // unused/unreachable Exprs since it only follows roots of outputs.
        loop {
            // If we have a node at hand, take it and start lowering.
            if let Some(current_gate_idx) = current_node.take() {
                if gate_idx_to_elem.contains_key(&current_gate_idx) {
                    continue;
                }
                let gate = circuit.gates()[current_gate_idx];
                let element = match gate {
                    Gate::Input(index) => self.input(with[index]),
                    Gate::Const(constant) => self.constant(constant),
                    // Here, if we haven't already, we push children into the stack to first evaluate  them, (post-order)
                    // or we retrieve their evaluted elements form the map.
                    Gate::BinOp(bin_op, lhs, rhs) => {
                        let lhs_elem_opt = gate_idx_to_elem.get(&lhs);
                        let rhs_elem_opt = gate_idx_to_elem.get(&rhs);

                        // We want the visit order to be lhs, rhs and then parent so that we can form the
                        // gate for operation with evaluated children. If they are not evaluated yet when we are at the parent
                        // (first time while DFSing), we push the parent to the stack again (we popped it from the stack and took the current_gate_idx),
                        // then the unevaluated ones, so that we visit them first.
                        // TLDR: we want to ensure the order in the stack:
                        // [<current op>, <left child if not evaluated>, <right child if not evaluated>]
                        let mut rhs_gate_idx = None;
                        // if the rhs child is not evaluated yet, reserve it to push into the stack
                        if rhs_elem_opt.is_none() {
                            rhs_gate_idx = Some(rhs);
                        }
                        // if the lhs child is not evaluated yet, move current_gate_idx to lhs, if rhs is already evaluated,
                        // push the parent on the stack again and continue; or move on to pushing
                        // rhs and parent in the stack.
                        if lhs_elem_opt.is_none() {
                            current_node = Some(lhs);
                            if rhs_gate_idx.is_none() {
                                dfs_stack.push(current_gate_idx);
                                continue;
                            }
                        }
                        // If we rhs child to evaluate, we reinsert the parent operation
                        // first, and then add the child to the stack to visit rhs before parent.
                        if let Some(push) = rhs_gate_idx {
                            dfs_stack.extend_from_slice(&[current_gate_idx, push]);
                            continue;
                        }

                        // At this point, lhs and rhs childen are all evaluted, we evalute the
                        // operation with their gate indices.
                        let lhs_result = lhs_elem_opt.unwrap();
                        let rhs_result = rhs_elem_opt.unwrap();

                        match bin_op {
                            BinOp::Add => self.add(lhs_result, rhs_result),
                            BinOp::Sub => self.sub(lhs_result, rhs_result),
                            BinOp::Mul => self.mul(lhs_result, rhs_result),
                        }
                    }
                };

                self.noise.update(
                    element.noise,
                    current_gate_idx.into_raw().into_u32() as usize,
                )?;
                gate_idx_to_elem.insert(current_gate_idx, element);
            }
            // If stack is empty, we consumed all the sub-tree of the current node, thus we
            // retrieve next if any.
            else if dfs_stack.is_empty() {
                if let Some(expr_idx) = roots.next().copied() {
                    dfs_stack.push(expr_idx);
                } else {
                    break;
                }
            }
            // If we don't have a node at hand, we first try the stack.
            // The current_node is already evaluated, so we continue consuming the stack.
            else if current_node.is_none() {
                current_node = dfs_stack.pop();
            }
            // The stack and roots are consumed, we stop the dfs.
            else {
                break;
            }
        }

        let mut result = ThinVec::with_capacity(circuit.outputs().len());
        for out_idx in circuit.outputs() {
            let element = gate_idx_to_elem.get(out_idx).unwrap();
            result.push(element.clone());
        }

        BackendResult::Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use ir::gate::GateIdx;

    use la_arena::{Arena, Idx};

    fn into_gate_idx(i: u32) -> GateIdx {
        Idx::from_raw(i.into())
    }

    #[test]
    fn test_mock_fhe_in_noise_budget() {
        let gates = thin_vec![
            Gate::Input(0),
            Gate::Input(1),
            Gate::BinOp(BinOp::Mul, into_gate_idx(0), into_gate_idx(1)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(2)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(3), into_gate_idx(3)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(3))
        ];
        let inputs = thin_vec![into_gate_idx(0), into_gate_idx(1)];
        let outputs = thin_vec![into_gate_idx(3), into_gate_idx(4), into_gate_idx(5)];
        let q = 11;

        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        );

        let noise_budget = 100;
        let mut plain_mod_q = MockFHEBackend::new(noise_budget);
        let error = plain_mod_q
            .eval(&circuit, &[1])
            .expect_err("should have failed");
        assert_eq!(BackendError::InvalidInputLen(2, 1), error);

        let results = plain_mod_q
            .eval(&circuit, &[1, 1])
            .expect("should have evaluated");

        let input_noise = ENC_NOISE;
        let single_mul_noise = 2 * input_noise + MUL_COST;

        let expected_elements = thin_vec![
            FHEElement {
                value: 1,
                depth: 0,
                noise: input_noise,
            },
            FHEElement {
                value: 1,
                depth: 0,
                noise: input_noise,
            },
            FHEElement {
                value: 1,
                depth: 1,
                noise: single_mul_noise,
            },
            FHEElement {
                value: 1,
                depth: 2,
                noise: 2 * single_mul_noise + MUL_COST,
            },
            FHEElement {
                value: 1,
                depth: 3,
                noise: 2 * (2 * single_mul_noise + MUL_COST) + MUL_COST,
            },
            FHEElement {
                value: 1,
                depth: 3,
                noise: single_mul_noise + (2 * single_mul_noise + MUL_COST) + MUL_COST,
            },
        ];

        assert_eq!(expected_elements, results);
    }

    #[test]
    fn test_mock_fhe_noise_budget_exceeded() {
        let gates = thin_vec![
            Gate::Input(0),
            Gate::Input(1),
            Gate::BinOp(BinOp::Mul, into_gate_idx(0), into_gate_idx(1)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(2)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(3), into_gate_idx(3)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(3))
        ];
        let inputs = thin_vec![into_gate_idx(0), into_gate_idx(1)];
        let outputs = thin_vec![into_gate_idx(3), into_gate_idx(4), into_gate_idx(5)];
        let q = 11;

        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        );

        let noise_budget = ENC_NOISE + MUL_COST;
        let mut plain_mod_q = MockFHEBackend::new(noise_budget);
        let error = plain_mod_q
            .eval(&circuit, &[1, 1])
            .expect_err("should have failed due to noise budget");

        let input_noise = ENC_NOISE;
        let single_mul_noise = 2 * input_noise + MUL_COST;
        assert_eq!(
            BackendError::NoiseBudgetExceeded(noise_budget, single_mul_noise, 2),
            error
        );
    }

    #[test]
    fn test_mock_fhe_add_sub() {
        let gates = thin_vec![
            Gate::Input(0),
            Gate::Input(1),
            Gate::BinOp(BinOp::Add, into_gate_idx(0), into_gate_idx(1)),
            Gate::BinOp(BinOp::Sub, into_gate_idx(2), into_gate_idx(2)),
            Gate::BinOp(BinOp::Add, into_gate_idx(3), into_gate_idx(3)),
            Gate::BinOp(BinOp::Sub, into_gate_idx(2), into_gate_idx(3))
        ];
        let inputs = thin_vec![into_gate_idx(0), into_gate_idx(1)];
        let outputs = thin_vec![into_gate_idx(3), into_gate_idx(4), into_gate_idx(5)];
        let q = 11;

        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        );

        let noise_budget = ENC_NOISE + MUL_COST;
        let mut plain_mod_q = MockFHEBackend::new(noise_budget);
        let results = plain_mod_q
            .eval(&circuit, &[1, 1])
            .expect("should have evaluated");

        let input_noise = ENC_NOISE;
        let single_add_noise = input_noise + ADD_COST;

        let expected_elements = thin_vec![
            FHEElement {
                value: 1,
                depth: 0,
                noise: input_noise,
            },
            FHEElement {
                value: 1,
                depth: 0,
                noise: input_noise,
            },
            FHEElement {
                value: 2,
                depth: 0,
                noise: single_add_noise,
            },
            FHEElement {
                value: 0,
                depth: 0,
                noise: single_add_noise + ADD_COST,
            },
            FHEElement {
                value: 0,
                depth: 0,
                noise: single_add_noise + 2 * ADD_COST,
            },
            FHEElement {
                value: 2,
                depth: 0,
                noise: single_add_noise + 2 * ADD_COST,
            },
        ];

        assert_eq!(expected_elements, results);
    }

    #[test]
    fn test_mock_fhe_add_sub_eval_outputs() {
        let gates = thin_vec![
            Gate::Input(0),
            Gate::Input(1),
            Gate::BinOp(BinOp::Add, into_gate_idx(0), into_gate_idx(1)),
            Gate::BinOp(BinOp::Sub, into_gate_idx(2), into_gate_idx(2)),
            Gate::BinOp(BinOp::Add, into_gate_idx(3), into_gate_idx(3)),
            Gate::BinOp(BinOp::Sub, into_gate_idx(2), into_gate_idx(3))
        ];
        let inputs = thin_vec![into_gate_idx(0), into_gate_idx(1)];
        let outputs = thin_vec![into_gate_idx(3), into_gate_idx(4), into_gate_idx(5)];
        let q = 11;

        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        );

        let noise_budget = ENC_NOISE + MUL_COST;
        let mut plain_mod_q = MockFHEBackend::new(noise_budget);
        let results = plain_mod_q
            .eval_outputs(&circuit, &[1, 1])
            .expect("should have evaluated");

        let input_noise = ENC_NOISE;
        let single_add_noise = input_noise + ADD_COST;

        let expected_elements = thin_vec![
            FHEElement {
                value: 0,
                depth: 0,
                noise: single_add_noise + ADD_COST,
            },
            FHEElement {
                value: 0,
                depth: 0,
                noise: single_add_noise + 2 * ADD_COST,
            },
            FHEElement {
                value: 2,
                depth: 0,
                noise: single_add_noise + 2 * ADD_COST,
            },
        ];

        assert_eq!(expected_elements, results);
    }

    #[test]
    fn test_mock_fhe_in_noise_budget_eval_outputs() {
        let gates = thin_vec![
            Gate::Input(0),
            Gate::Input(1),
            Gate::BinOp(BinOp::Mul, into_gate_idx(0), into_gate_idx(1)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(2)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(3), into_gate_idx(3)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(3))
        ];
        let inputs = thin_vec![into_gate_idx(0), into_gate_idx(1)];
        let outputs = thin_vec![into_gate_idx(3), into_gate_idx(4), into_gate_idx(5)];
        let q = 11;

        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        );

        let noise_budget = 100;
        let mut plain_mod_q = MockFHEBackend::new(noise_budget);
        let results = plain_mod_q
            .eval_outputs(&circuit, &[1, 1])
            .expect("should have evaluated");

        let input_noise = ENC_NOISE;
        let single_mul_noise = 2 * input_noise + MUL_COST;

        let expected_elements = thin_vec![
            FHEElement {
                value: 1,
                depth: 2,
                noise: 2 * single_mul_noise + MUL_COST,
            },
            FHEElement {
                value: 1,
                depth: 3,
                noise: 2 * (2 * single_mul_noise + MUL_COST) + MUL_COST,
            },
            FHEElement {
                value: 1,
                depth: 3,
                noise: single_mul_noise + (2 * single_mul_noise + MUL_COST) + MUL_COST,
            },
        ];

        assert_eq!(expected_elements, results);
    }
}
