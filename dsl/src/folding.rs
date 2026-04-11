use la_arena::Arena;

use common::Supported;

use crate::expr::Expr;

pub fn fold<T: Supported>(expr: Expr<T>, arena: &mut Arena<Expr<T>>, plain_text_modulus: T) -> Expr<T> {
    match expr {
        Expr::Input(_) | Expr::Const(_) => expr,
        Expr::BinOp(bin_op, lhs_idx, rhs_idx) => {
            let lhs = arena[lhs_idx];
            let rhs = arena[rhs_idx];

            if let Expr::Const(lhs_value) = lhs
                && let Expr::Const(rhs_value) = rhs
            {
                let result = match bin_op {
                    op::BinOp::Add => (lhs_value + rhs_value) % plain_text_modulus,
                    op::BinOp::Sub => (lhs_value - rhs_value) % plain_text_modulus,
                    op::BinOp::Mul => (lhs_value * rhs_value) % plain_text_modulus,
                };
                return Expr::Const(result);
            }
            expr
        }
    }
}
