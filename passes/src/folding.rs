use la_arena::Arena;

use ir::{SupportedType, gate::Gate};

pub fn fold(gate: Gate, arena: &Arena<Gate>, plain_text_modulus: SupportedType) -> Gate {
    match gate {
        Gate::Tombstone => gate,
        Gate::Input(_) | Gate::Const(_) => gate,
        Gate::BinOp(bin_op, lhs_idx, rhs_idx) => {
            let lhs = arena[lhs_idx];
            let rhs = arena[rhs_idx];

            if let Gate::Const(lhs_value) = lhs
                && let Gate::Const(rhs_value) = rhs
            {
                let result = match bin_op {
                    op::BinOp::Add => (lhs_value + rhs_value) % plain_text_modulus,
                    op::BinOp::Sub => (lhs_value - rhs_value) % plain_text_modulus,
                    op::BinOp::Mul => (lhs_value * rhs_value) % plain_text_modulus,
                };
                return Gate::Const(result);
            }
            gate
        }
    }
}
