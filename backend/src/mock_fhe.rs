use std::cmp::max;

use crate::{
    element::MockFHEElement,
    iterative::eval_with_iterative_traversal,
    noise::{MockNoise, Noise},
};
use common::idx_to_usize;
use ir::{SupportedType, circuit::Circuit, gate::Gate};
use op::BinOp;
use thin_vec::{ThinVec, thin_vec};

use crate::{Backend, BackendResult, validation::validate_same_length};

#[derive(Clone, Debug, Default)]
pub struct MockFHEBackend<N: Noise> {
    plain_text_modulus: SupportedType,
    noise: N,
}

impl<N: Noise> MockFHEBackend<N> {
    pub fn new(noise_budget: usize) -> Self {
        Self {
            plain_text_modulus: SupportedType::default(),
            noise: Noise::new(noise_budget),
        }
    }
}

pub fn default_mock_fhe_backend(noise_budget: usize) -> MockFHEBackend<MockNoise> {
    MockFHEBackend::<MockNoise>::new(noise_budget)
}

const ENC_NOISE: usize = 1; // initial noise added during encryption 
const ADD_COST: usize = 1; // noises are added 
const MUL_COST: usize = 5; // noises generally compound thus larger mock value

impl<N: Noise> Backend for MockFHEBackend<N> {
    type Elem = MockFHEElement;

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
        // NOTE: Noise during multiplication grows exponentially, thus the multiplication.
        element.noise = lhs.noise * rhs.noise + MUL_COST;
        element
    }
    fn constant(&mut self, c: SupportedType) -> Self::Elem {
        MockFHEElement {
            value: c % self.plain_text_modulus,
            ..Default::default()
        }
    }
    fn input(&mut self, c: SupportedType) -> Self::Elem {
        MockFHEElement {
            value: c % self.plain_text_modulus,
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
        self.plain_text_modulus = circuit.plain_text_modulus;
        // NOTE: we restart noise because certain outputs can be below the budget but if we
        // evaluate an output which overwhelms the noise budget, we won't be able to evaluate
        // anything after that. Also the max and index values will be wrong no matter the case,
        // once a circuit is evaluated.
        self.noise = Noise::new(self.noise.get_noise_budget());

        let mut results: ThinVec<Self::Elem> =
            thin_vec![Self::Elem::default(); circuit.gates().len()];
        // One-to-one mapping between gate and elements, thus the same indices.
        for (ix, (_, gate)) in circuit.gates().iter().enumerate() {
            let element = match gate {
                Gate::Tombstone => continue,
                Gate::Input(index) => self.input(with[*index]),
                Gate::Const(constant) => self.constant(*constant),
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
        // NOTE: we restart noise because certain outputs can be below the budget but if we
        // evaluate an output which overwhelms the noise budget, we won't be able to evaluate
        // anything after that. Also the max and index values will be wrong no matter the case,
        // once a circuit is evaluated.
        self.noise = Noise::new(self.noise.get_noise_budget());
        eval_with_iterative_traversal::<Self::Elem, N, Self>(circuit, self, with)
    }
    fn set_plain_text_modulus(&mut self, q: SupportedType) {
        self.plain_text_modulus = q;
    }
    fn get_noise_mul(&mut self) -> &mut impl Noise {
        &mut self.noise
    }
}

#[cfg(test)]
mod test {
    use crate::error::BackendError;

    use super::*;

    use common::u32_to_idx;
    use ir::gate::GateIdx;
    use la_arena::Arena;

    fn into_gate_idx(i: u32) -> GateIdx {
        u32_to_idx(i)
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

        let noise_budget = 1689; // noise becomes 1686 at the end 
        let mut mock_fhe = default_mock_fhe_backend(noise_budget);
        let error = mock_fhe
            .eval(&circuit, &[1])
            .expect_err("should have failed");
        assert_eq!(BackendError::InvalidInputLen(2, 1), error);

        let results = mock_fhe
            .eval(&circuit, &[1, 1])
            .expect("should have evaluated");

        let input_noise = ENC_NOISE;
        let mul_noise = |operand_noise| operand_noise * operand_noise + MUL_COST;
        let single_mul_noise = mul_noise(input_noise);
        let double_mul_noise = mul_noise(single_mul_noise);
        let triple_mul_noise = mul_noise(double_mul_noise);
        let single_and_double_mul_noise = single_mul_noise * double_mul_noise + MUL_COST;

        let expected_elements = thin_vec![
            MockFHEElement {
                value: 1,
                depth: 0,
                noise: input_noise,
            },
            MockFHEElement {
                value: 1,
                depth: 0,
                noise: input_noise,
            },
            MockFHEElement {
                value: 1,
                depth: 1,
                noise: single_mul_noise,
            },
            MockFHEElement {
                value: 1,
                depth: 2,
                noise: double_mul_noise,
            },
            MockFHEElement {
                value: 1,
                depth: 3,
                noise: triple_mul_noise,
            },
            MockFHEElement {
                value: 1,
                depth: 3,
                noise: single_and_double_mul_noise,
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
        let mut mock_fhe = default_mock_fhe_backend(noise_budget);
        let error = mock_fhe
            .eval(&circuit, &[1, 1])
            .expect_err("should have failed due to noise budget");

        let input_noise = ENC_NOISE;
        let mul_noise = |operand_noise| operand_noise * operand_noise + MUL_COST;
        let single_mul_noise = mul_noise(input_noise);
        let double_mul_noise = mul_noise(single_mul_noise);
        assert_eq!(
            BackendError::NoiseBudgetExceeded(noise_budget, double_mul_noise, 3),
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
        let mut mock_fhe = default_mock_fhe_backend(noise_budget);
        let results = mock_fhe
            .eval(&circuit, &[1, 1])
            .expect("should have evaluated");

        let input_noise = ENC_NOISE;
        let single_add_noise = input_noise + ADD_COST;

        let expected_elements = thin_vec![
            MockFHEElement {
                value: 1,
                depth: 0,
                noise: input_noise,
            },
            MockFHEElement {
                value: 1,
                depth: 0,
                noise: input_noise,
            },
            MockFHEElement {
                value: 2,
                depth: 0,
                noise: single_add_noise,
            },
            MockFHEElement {
                value: 0,
                depth: 0,
                noise: single_add_noise + ADD_COST,
            },
            MockFHEElement {
                value: 0,
                depth: 0,
                noise: single_add_noise + 2 * ADD_COST,
            },
            MockFHEElement {
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
        let mut mock_fhe = default_mock_fhe_backend(noise_budget);
        let results = mock_fhe
            .eval_outputs(&circuit, &[1, 1])
            .expect("should have evaluated");

        let input_noise = ENC_NOISE;
        let single_add_noise = input_noise + ADD_COST;

        let expected_elements = thin_vec![
            MockFHEElement {
                value: 0,
                depth: 0,
                noise: single_add_noise + ADD_COST,
            },
            MockFHEElement {
                value: 0,
                depth: 0,
                noise: single_add_noise + 2 * ADD_COST,
            },
            MockFHEElement {
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

        let noise_budget = 1689; // noise at the end reaches 1686
        let mut mock_fhe = default_mock_fhe_backend(noise_budget);
        let results = mock_fhe
            .eval_outputs(&circuit, &[1, 1])
            .expect("should have evaluated");

        let input_noise = ENC_NOISE;
        let mul_noise = |operand_noise| operand_noise * operand_noise + MUL_COST;
        let single_mul_noise = mul_noise(input_noise);
        let double_mul_noise = mul_noise(single_mul_noise);
        let triple_mul_noise = mul_noise(double_mul_noise);
        let single_and_double_noise = single_mul_noise * double_mul_noise + MUL_COST;

        let expected_elements = thin_vec![
            MockFHEElement {
                value: 1,
                depth: 2,
                noise: double_mul_noise,
            },
            MockFHEElement {
                value: 1,
                depth: 3,
                noise: triple_mul_noise,
            },
            MockFHEElement {
                value: 1,
                depth: 3,
                noise: single_and_double_noise,
            },
        ];

        assert_eq!(expected_elements, results);
    }
}
