use la_arena::Idx;

use crate::{SupportedType, hash::GateHash};
use op::BinOp;

pub type GateIdx = Idx<Gate>;

/// SSA instruction style gates
#[derive(Clone, Copy, Debug)]
pub enum Gate {
    Input(usize),
    Const(SupportedType),
    // NOTE: We preserve BinOp::Sub and not canonicalize it to (Add, lhs, (Mul, rhs, -1)) because
    // it will potentially increase multiplicative depth.
    BinOp(BinOp, GateIdx, GateIdx),
}

impl PartialEq for Gate {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Input(l0), Self::Input(r0)) => l0 == r0,
            (Self::Const(l0), Self::Const(r0)) => l0 == r0,
            (Self::BinOp(_, _, _), Self::BinOp(_, _, _)) => {
                GateHash::from(self) == GateHash::from(other)
            }
            _ => false,
        }
    }
}

impl Eq for Gate {}
