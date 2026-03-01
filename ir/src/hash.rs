use crate::{
    SupportedType,
    gate::{Gate, GateIdx},
};

use op::BinOp;

use std::hash::{Hash, Hasher};

impl Hash for Gate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let gate_hash = GateHash::from(self);
        gate_hash.hash(state);
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
// NOTE: We don't hash Gate::Thombstones
pub(crate) enum GateHash {
    Input(usize),
    Const(SupportedType),
    /// GateIdx are ordered before forming to avoid order originating duplicates,
    /// except non-commutatives.
    BinOp(BinOp, GateIdx, GateIdx),
}

impl From<Gate> for GateHash {
    fn from(gate: Gate) -> Self {
        GateHash::from(&gate)
    }
}

impl From<&Gate> for GateHash {
    fn from(gate: &Gate) -> Self {
        let (op, mut lhs, mut rhs) = match gate {
            Gate::Input(index) => return Self::Input(*index),
            Gate::Const(v) => return Self::Const(*v),
            Gate::BinOp(bin_op, lhs, rhs) => {
                if *bin_op == BinOp::Sub {
                    return Self::BinOp(*bin_op, *lhs, *rhs);
                }
                (*bin_op, *lhs, *rhs)
            }
            // NOTE: We don't hash Gate::Thombstones
            Gate::Thombstone => unreachable!(),
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

    fn usize_to_idx(i: usize) -> GateIdx {
        GateIdx::from_raw(RawIdx::from_u32(i as u32))
    }

    #[test]
    fn same_data_same_hash_for_ac() {
        let mut hash_set = HashSet::new();

        for bin_op in [BinOp::Add, BinOp::Mul] {
            let gate = Gate::BinOp(bin_op, usize_to_idx(0), usize_to_idx(1));
            let gate_swapped = Gate::BinOp(bin_op, usize_to_idx(1), usize_to_idx(0));

            hash_set.insert(gate);
            assert!(hash_set.contains(&gate_swapped));
        }
    }
    #[test]
    fn same_data_same_hash_for_non_ac() {
        let gate = Gate::BinOp(BinOp::Sub, usize_to_idx(0), usize_to_idx(1));
        let gate_swapped = Gate::BinOp(BinOp::Sub, usize_to_idx(1), usize_to_idx(0));

        let mut hash_set = HashSet::new();

        hash_set.insert(gate);
        assert!(!hash_set.contains(&gate_swapped));
    }
}
