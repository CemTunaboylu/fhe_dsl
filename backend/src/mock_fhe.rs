use std::cmp::max;

use ir::{SupportedType, circuit::Circuit, gate::Gate};
use op::BinOp;
use thin_vec::{ThinVec, thin_vec};

use crate::{Backend, BackendError, BackendResult};

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

        return Err(BackendError::NoiseBudgetExceeded(
            self.noise_budget,
            self.max_so_far,
            self.index,
        ));
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
    // TODO: Introduce a cache to avoid evaluating the same circuit twice.
    // NOTE: eval currently acts as the inputs are given plaintext, thus the mock fhe backend
    // pseudo-encrypts them with elements having a noise.
    fn eval(
        &mut self,
        circuit: &Circuit,
        with: &[SupportedType],
    ) -> BackendResult<ThinVec<Self::Elem>> {
        if circuit.inputs().len() != with.len() {
            let exp_len = circuit.inputs().len();
            let got_len = with.len();
            return BackendResult::Err(BackendError::InvalidInputLen(exp_len, got_len));
        }
        self.q = circuit.q;
        let mut results: ThinVec<Self::Elem> =
            thin_vec![Self::Elem::default(); circuit.gates().len()];
        let q = self.q;
        let modulo = |val| val % q;
        // One-to-one mapping between gate and elements, thus the same indices.
        for (ix, (_, gate)) in circuit.gates().iter().enumerate() {
            let (mut value, depth, noise) = match gate {
                Gate::Input(index) => (with[*index], 0, ENC_NOISE),
                Gate::Const(constant) => (*constant, 0, 0),
                Gate::BinOp(bin_op, lhs, rhs) => {
                    let lhs_result_index = lhs.into_raw().into_u32();
                    let rhs_result_index = rhs.into_raw().into_u32();

                    let lhs_result = &results[lhs_result_index as usize];
                    let rhs_result = &results[rhs_result_index as usize];

                    let element = match bin_op {
                        BinOp::Add => self.add(lhs_result, rhs_result),
                        BinOp::Sub => self.sub(lhs_result, rhs_result),
                        BinOp::Mul => self.mul(lhs_result, rhs_result),
                    };
                    self.noise.update(element.noise, ix)?;
                    results[ix] = element;
                    continue;
                }
            };
            value = modulo(value);
            let element = Self::Elem {
                value,
                depth,
                noise,
            };
            self.noise.update(element.noise, ix)?;
            results[ix] = element;
        }

        BackendResult::Ok(results)
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
}
