use la_arena::Arena;

use ir::{SupportedType, gate::Gate};

pub fn fold(gate: Gate, arena: &mut Arena<Gate>, q: SupportedType) -> Gate {
    match gate {
        Gate::Input(_) | Gate::Const(_) => gate,
        Gate::BinOp(bin_op, lhs_idx, rhs_idx) => {
            let lhs = arena[lhs_idx];
            let rhs = arena[rhs_idx];

            if let Gate::Const(lhs_value) = lhs
                && let Gate::Const(rhs_value) = rhs
            {
                let result = match bin_op {
                    op::BinOp::Add => (lhs_value + rhs_value) % q,
                    op::BinOp::Sub => (lhs_value - rhs_value) % q,
                    op::BinOp::Mul => (lhs_value * rhs_value) % q,
                };
                return Gate::Const(result);
            }
            gate
        }
    }
}

