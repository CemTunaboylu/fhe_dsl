use common::{idx_to_usize as c_idx_to_usize, usize_to_idx as c_usize_to_idx};
use ir::gate::{Gate, GateIdx};
use op::BinOp;

pub mod analysis;
pub mod depth;
pub mod folding;
pub mod interner;
pub mod liveness;
pub mod reassociate;

pub(crate) fn idx_to_usize(gate_idx: GateIdx) -> usize {
    c_idx_to_usize::<Gate>(gate_idx)
}
pub(crate) fn usize_to_idx(i: usize) -> GateIdx {
    c_usize_to_idx::<Gate>(i)
}

pub(crate) fn is_op_associative_and_commutative(bin_op_of_i: BinOp) -> bool {
    bin_op_of_i.is_associative() && bin_op_of_i.is_commutative()
}
