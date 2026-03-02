use fxhash::{FxBuildHasher, FxHashMap};
use thin_vec::{ThinVec, thin_vec};

use common::idx_to_usize;
use ir::{
    SupportedType,
    circuit::Circuit,
    gate::{Gate, GateIdx},
};
use op::BinOp;

use crate::{Backend, BackendResult, validation::validate_same_length};

#[derive(Clone, Debug, Default)]
pub struct PlainModQBackend {
    q: SupportedType,
}

impl PlainModQBackend {
    pub fn new() -> Self {
        Self {
            q: SupportedType::default(),
        }
    }
}

impl Backend for PlainModQBackend {
    type Elem = SupportedType;

    fn add(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem {
        self.constant(lhs + rhs)
    }
    fn sub(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem {
        self.constant(lhs - rhs)
    }
    fn mul(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem {
        self.constant(lhs * rhs)
    }
    fn input(&mut self, c: SupportedType) -> Self::Elem {
        c % self.q
    }
    fn constant(&mut self, c: SupportedType) -> Self::Elem {
        c % self.q
    }
    // TODO: Introduce a cache to avoid evaluating the same circuit twice.
    fn eval(
        &mut self,
        circuit: &Circuit,
        with: &[SupportedType],
    ) -> BackendResult<ThinVec<Self::Elem>> {
        validate_same_length(circuit, with)?;
        self.q = circuit.q;
        let mut results: ThinVec<Self::Elem> =
            thin_vec![SupportedType::default(); circuit.gates().len()];

        // One-to-one mapping between gate and elements, thus the same indices.
        for (ix, (_, gate)) in circuit.gates().iter().enumerate() {
            let element = match gate {
                Gate::Thombstone => continue,
                Gate::Input(index) => self.input(with[*index]),
                Gate::Const(value) => self.constant(*value),
                Gate::BinOp(bin_op, lhs, rhs) => {
                    let lhs_result_index = idx_to_usize(*lhs);
                    let rhs_result_index = idx_to_usize(*rhs);

                    let lhs_result = &results[lhs_result_index];
                    let rhs_result = &results[rhs_result_index];

                    match bin_op {
                        BinOp::Add => self.add(lhs_result, rhs_result),
                        BinOp::Sub => self.sub(lhs_result, rhs_result),
                        BinOp::Mul => self.mul(lhs_result, rhs_result),
                    }
                }
            };
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

        let mut roots = circuit.outputs().iter();
        let mut dfs_stack = ThinVec::new();

        let mut current_node = roots.next().copied();
        let mut gate_idx_to_elem: FxHashMap<GateIdx, Self::Elem> =
            FxHashMap::with_hasher(FxBuildHasher::default());

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
                    Gate::Thombstone => continue,
                    Gate::Input(index) => self.input(with[index]),
                    Gate::Const(constant) => self.constant(constant),
                    // Here, if we haven't already, we push children into the stack to first evaluate  them, (post-order)
                    // or we retrieve their evaluated elements form the map.
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

                        // At this point, lhs and rhs children are all evaluated, we evaluate the
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
            result.push(*element);
        }

        BackendResult::Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::error::BackendError;

    use common::u32_to_idx;
    use la_arena::Arena;

    fn into_gate_idx(i: u32) -> GateIdx {
        u32_to_idx(i)
    }

    #[test]
    fn test_plain_mod_q() {
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

        let mut plain_mod_q = PlainModQBackend::new();
        let error = plain_mod_q
            .eval(&circuit, &[1])
            .expect_err("should have failed");
        assert_eq!(BackendError::InvalidInputLen(2, 1), error);

        let results = plain_mod_q
            .eval(&circuit, &[1, 1])
            .expect("should have evaluated");

        assert_eq!(thin_vec![1;6], results);
    }
}
