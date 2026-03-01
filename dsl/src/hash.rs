use crate::{
    SupportedType,
    expr::{Expr, ExprIdx},
};

use op::BinOp;

use std::hash::{Hash, Hasher};

impl Hash for Expr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let expr_hash = ExprHash::from(self);
        expr_hash.hash(state);
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub(crate) enum ExprHash {
    Input(usize),
    Const(SupportedType),
    /// ExprIds are ordered before forming Double to avoid order originating duplicates,
    /// except non-commutatives.
    BinOp(BinOp, ExprIdx, ExprIdx),
}

impl From<Expr> for ExprHash {
    fn from(expr: Expr) -> Self {
        ExprHash::from(&expr)
    }
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

#[cfg(test)]
mod test {
    use super::*;
    use la_arena::RawIdx;
    use std::collections::HashSet;

    fn usize_to_idx(i: usize) -> ExprIdx {
        ExprIdx::from_raw(RawIdx::from_u32(i as u32))
    }

    #[test]
    fn same_data_same_hash_for_ac() {
        let mut hash_set = HashSet::new();

        for bin_op in [BinOp::Add, BinOp::Mul] {
            let expr = Expr::BinOp(bin_op, usize_to_idx(0), usize_to_idx(1));
            let expr_swapped = Expr::BinOp(bin_op, usize_to_idx(1), usize_to_idx(0));

            hash_set.insert(expr);
            assert!(hash_set.contains(&expr_swapped));
        }
    }
    #[test]
    fn same_data_same_hash_for_non_ac() {
        let gate = Expr::BinOp(BinOp::Sub, usize_to_idx(0), usize_to_idx(1));
        let gate_swapped = Expr::BinOp(BinOp::Sub, usize_to_idx(1), usize_to_idx(0));

        let mut hash_set = HashSet::new();

        hash_set.insert(gate);
        assert!(!hash_set.contains(&gate_swapped));
    }
}
