use la_arena::Idx;

use crate::SupportedType;
use op::BinOp;

pub type GateIdx = Idx<Gate>;

/// SSA instruction style gates
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gate {
    Input(usize),
    Const(SupportedType),
    // NOTE: We preserve BinOp::Sub and not canonicalize it to (Add, lhs, (Mul, rhs, -1)) because
    // it will potentially increase multiplicative depth.
    BinOp(BinOp, GateIdx, GateIdx),
}
