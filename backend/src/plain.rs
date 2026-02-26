use ir::{SupportedType, circuit::Circuit, gate::Gate};
use op::BinOp;
use thin_vec::{ThinVec, thin_vec};

use crate::{Backend, BackendError, BackendResult};

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
        if circuit.inputs().len() != with.len() {
            let exp_len = circuit.inputs().len();
            let got_len = with.len();
            return BackendResult::Err(BackendError::InvalidInputLen(exp_len, got_len));
        }
        self.q = circuit.q;
        let mut results: ThinVec<Self::Elem> =
            thin_vec![SupportedType::default(); circuit.gates().len()];

        // One-to-one mapping between gate and elements, thus the same indices.
        for (ix, (_, gate)) in circuit.gates().iter().enumerate() {
            let element = match gate {
                Gate::Input(index) => self.input(with[*index]),
                Gate::Const(value) => self.constant(*value),
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
