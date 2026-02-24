use la_arena::Idx;

use crate::SupportedType;

pub type GateIdx = Idx<Gate>;

/// SSA instruction style gates
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gate {
    Input(usize),
    Const(SupportedType),
    Add(GateIdx, GateIdx),
    Sub(GateIdx, GateIdx),
    Mul(GateIdx, GateIdx),
}

#[derive(Clone, Debug)]
pub(crate) struct GateHandle {
    idx: GateIdx,
}
