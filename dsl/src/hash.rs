use crate::{
    SupportedType,
    expr::{Expr, ExprIdx},
};

use op::BinOp;

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub(crate) enum ExprHash {
    Input(usize),
    Const(SupportedType),
    /// ExprIds are ordered before forming Double to avoid order originating duplicates,
    /// except non-commutatives.
    BinOp(BinOp, ExprIdx, ExprIdx),
}

impl From<&Expr> for ExprHash {
    fn from(expr: &Expr) -> Self {
        let (op, mut lhs, mut rhs) = match expr {
            Expr::Input(index) => return Self::Input(*index),
            Expr::Const(v) => return Self::Const(*v),
            Expr::BinOp(bin_op, lhs, rhs) => {
                if *bin_op == BinOp::Sub {
                    return Self::BinOp(*bin_op, *lhs, *rhs);
                }

                (*bin_op, *lhs, *rhs)
            }
        };

        if lhs > rhs {
            (lhs, rhs) = (rhs, lhs)
        }
        Self::BinOp(op, lhs, rhs)
    }
}
