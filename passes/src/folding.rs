use la_arena::Arena;

use common::Supported;
use ir::gate::Gate;

pub fn fold<T: Supported>(gate: Gate<T>, arena: &Arena<Gate<T>>, plain_text_modulus: T) -> Gate<T> {
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
