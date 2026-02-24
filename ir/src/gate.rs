use la_arena::Idx;

use crate::SupportedType;
use op::BinOp;

pub type GateIdx = Idx<Gate>;

/// SSA instruction style gates
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gate {
    Input(usize),
    Const(SupportedType),
    BinOp(BinOp, GateIdx, GateIdx),
}

#[derive(Clone, Debug)]
pub(crate) struct GateHandle {
    idx: GateIdx,
}
