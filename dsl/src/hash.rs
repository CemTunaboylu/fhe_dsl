use crate::{
    expr::{Expr, ExprIdx},
};

use common::Supported;
use op::BinOp;

use std::hash::{Hash, Hasher};

impl<T: Supported> Hash for Expr<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let expr_hash = ExprHash::from(self);
        expr_hash.hash(state);
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub(crate) enum ExprHash<T: Supported> {
    Input(usize),
    Const(T),
    /// ExprIds are ordered before forming Double to avoid order originating duplicates,
    /// except non-commutatives.
    BinOp(BinOp, ExprIdx<T>, ExprIdx<T>),
}

impl<T: Supported> From<Expr<T>> for ExprHash<T> {
    fn from(expr: Expr<T>) -> Self {
        ExprHash::from(&expr)
    }
}

impl<T: Supported> From<&Expr<T>> for ExprHash<T> {
    fn from(expr: &Expr<T>) -> Self {
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

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashSet;

    fn u32_to_idx<T: Supported>(i: u32) -> ExprIdx<T> {
        common::u32_to_idx::<Expr<T>>(i)
    }

    #[test]
    fn same_data_same_hash_for_ac() {
        let mut hash_set: HashSet<Expr<u64>> = HashSet::new();

        for bin_op in [BinOp::Add, BinOp::Mul] {
            let expr = Expr::BinOp(bin_op, u32_to_idx(0), u32_to_idx(1));
            let expr_swapped = Expr::BinOp(bin_op, u32_to_idx(1), u32_to_idx(0));

            hash_set.insert(expr);
            assert!(hash_set.contains(&expr_swapped));
        }
    }
    #[test]
    fn same_data_same_hash_for_non_ac() {
        let gate: Expr<u64> = Expr::BinOp(BinOp::Sub, u32_to_idx(0), u32_to_idx(1));
        let gate_swapped = Expr::BinOp(BinOp::Sub, u32_to_idx(1), u32_to_idx(0));

        let mut hash_set = HashSet::new();

        hash_set.insert(gate);
        assert!(!hash_set.contains(&gate_swapped));
    }
}
