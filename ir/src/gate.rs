use la_arena::Idx;

use crate::{Supported, hash::GateHash};
use op::BinOp;

pub type GateIdx<T> = Idx<Gate<T>>;

/// SSA instruction style gates
#[derive(Clone, Copy, Debug)]
pub enum Gate<T: Supported> {
    Input(usize),
    Const(T),
    // NOTE: We preserve BinOp::Sub and not canonicalize it to (Add, lhs, (Mul, rhs, -1)) because
    // it will potentially increase multiplicative depth. In real FHE context, we still don't
    // because negation is essentially free by just negating coefficients.
    BinOp(BinOp, GateIdx<T>, GateIdx<T>),
    Tombstone,
}

impl<T: Supported> PartialEq for Gate<T> {
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

impl<T: Supported> Eq for Gate<T> {}
