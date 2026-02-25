use la_arena::Arena;

use crate::{SupportedType, expr::Expr};

pub fn fold(expr: Expr, arena: &mut Arena<Expr>, q: SupportedType) -> Expr {
    match expr {
        Expr::Input(_) | Expr::Const(_) => expr,
        Expr::BinOp(bin_op, lhs_idx, rhs_idx) => {
            let lhs = arena[lhs_idx];
            let rhs = arena[rhs_idx];

            if let Expr::Const(lhs_value) = lhs
                && let Expr::Const(rhs_value) = rhs
            {
                let result = match bin_op {
                    op::BinOp::Add => (lhs_value + rhs_value) % q,
                    op::BinOp::Sub => (lhs_value - rhs_value) % q,
                    op::BinOp::Mul => (lhs_value * rhs_value) % q,
                };
                return Expr::Const(result);
            }
            expr
        }
    }
}
