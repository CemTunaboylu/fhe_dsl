use crate::{
    SupportedType,
    expr::{Expr, ExprIdx},
    op::BinOp,
};

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
            Expr::Add(idx, idx1) => (BinOp::Add, *idx, *idx1),
            Expr::Sub(idx, idx1) => return Self::BinOp(BinOp::Sub, *idx, *idx1),
            Expr::Mul(idx, idx1) => (BinOp::Mul, *idx, *idx1),
        };

        if lhs > rhs {
            (lhs, rhs) = (rhs, lhs)
        }
        Self::BinOp(op, lhs, rhs)
    }
}
