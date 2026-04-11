use thin_vec::{ThinVec, thin_vec};

use common::{NonZeroModulusResult, default_non_zero, idx_to_usize, validate_non_zero};
use ir::{Supported, circuit::Circuit, gate::Gate};
use op::BinOp;

use crate::{
    Backend, BackendResult,
    iterative::eval_with_iterative_traversal,
    noise::{Noise, Noiseless},
    validation::validate_same_length,
};

#[derive(Clone, Debug, Default)]
pub struct PlainModQBackend<T: Supported> {
    plain_text_modulus: T,
    noise: Noiseless,
}

impl<T: Supported> PlainModQBackend<T> {
    pub fn new() -> Self {
        Self {
            plain_text_modulus: default_non_zero(),
            noise: Noiseless {},
        }
    }
}

impl<T: Supported> Backend<T> for PlainModQBackend<T> {
    type Elem = T;

    fn add(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem {
        self.constant(lhs.wrapping_add(rhs))
    }
    fn sub(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem {
        self.constant(lhs.wrapping_sub(rhs))
    }
    fn mul(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem {
        self.constant(lhs.wrapping_mul(rhs))
    }
    fn input(&mut self, c: T) -> Self::Elem {
        c % self.plain_text_modulus
    }
    fn constant(&mut self, c: T) -> Self::Elem {
        c % self.plain_text_modulus
    }
    // TODO: Introduce a cache to avoid evaluating the same circuit twice.
    fn eval(&mut self, circuit: &Circuit<T>, with: &[T]) -> BackendResult<ThinVec<Self::Elem>> {
        validate_same_length(circuit, with)?;
        self.plain_text_modulus = circuit.get_plain_text_modulus();
        let mut results: ThinVec<Self::Elem> = thin_vec![T::default(); circuit.gates().len()];

        // One-to-one mapping between gate and elements, thus the same indices.
        for (ix, (_, gate)) in circuit.gates().iter().enumerate() {
            let element = match gate {
                Gate::Tombstone => continue,
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
        circuit: &Circuit<T>,
        with: &[T],
    ) -> BackendResult<ThinVec<Self::Elem>> {
        eval_with_iterative_traversal::<Self::Elem, Noiseless, T, Self>(circuit, self, with)
    }
    fn set_plain_text_modulus(&mut self, q: T) -> NonZeroModulusResult<()> {
        validate_non_zero(q)?;
        self.plain_text_modulus = q;
        Ok(())
    }
    fn get_noise_mul(&mut self) -> &mut impl Noise {
        &mut self.noise
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::error::BackendError;

    use common::u32_to_idx;
    use ir::gate::GateIdx;
    use la_arena::Arena;

    fn into_gate_idx(i: u32) -> GateIdx<u64> {
        u32_to_idx(i)
    }

    fn assert_supported<T: Supported>() {}

    #[test]
    fn all_supported_integer_types_work_for_plain_backend() {
        assert_supported::<i8>();
        assert_supported::<i16>();
        assert_supported::<i32>();
        assert_supported::<i64>();
        assert_supported::<u8>();
        assert_supported::<u16>();
        assert_supported::<u32>();
        assert_supported::<u64>();
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
        let q = 11u64;

        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        )
        .expect("to have non-zero modulus");

        let mut plain_mod_q = PlainModQBackend::new();
        let error = plain_mod_q
            .eval(&circuit, &[1])
            .expect_err("should have failed");
        assert_eq!(BackendError::InvalidInputLen(2, 1), error);

        let results = plain_mod_q
            .eval(&circuit, &[1, 1])
            .expect("should have evaluated");

        assert_eq!(thin_vec![1u64;6], results);
    }
}
