use crate::{
    Supported,
    gate::{Gate, GateIdx},
};

use op::BinOp;

use std::hash::{Hash, Hasher};

impl<T: Supported> Hash for Gate<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let gate_hash = GateHash::from(self);
        gate_hash.hash(state);
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
// NOTE: We don't hash Gate::Tombstones
pub(crate) enum GateHash<T: Supported> {
    Input(usize),
    Const(T),
    /// GateIdx are ordered before forming to avoid order originating duplicates,
    /// except non-commutatives.
    BinOp(BinOp, GateIdx<T>, GateIdx<T>),
}

impl<T: Supported> From<Gate<T>> for GateHash<T> {
    fn from(gate: Gate<T>) -> Self {
        GateHash::from(&gate)
    }
}

impl<T: Supported> From<&Gate<T>> for GateHash<T> {
    fn from(gate: &Gate<T>) -> Self {
        let (op, mut lhs, mut rhs) = match gate {
            Gate::Input(index) => return Self::Input(*index),
            Gate::Const(v) => return Self::Const(*v),
            Gate::BinOp(bin_op, lhs, rhs) => {
                if *bin_op == BinOp::Sub {
                    return Self::BinOp(*bin_op, *lhs, *rhs);
                }
                (*bin_op, *lhs, *rhs)
            }
            // NOTE: We don't hash Gate::Tombstones
            Gate::Tombstone => unreachable!(),
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

    fn usize_to_idx<T: Supported>(i: usize) -> GateIdx<T> {
        GateIdx::from_raw(RawIdx::from_u32(i as u32))
    }

    fn assert_supported_gate_hash<T: Supported>() {
        let mut hash_set: HashSet<Gate<T>> = HashSet::new();

        for bin_op in [BinOp::Add, BinOp::Mul] {
            let gate = Gate::BinOp(bin_op, usize_to_idx(0), usize_to_idx(1));
            let gate_swapped = Gate::BinOp(bin_op, usize_to_idx(1), usize_to_idx(0));

            hash_set.insert(gate);
            assert!(hash_set.contains(&gate_swapped));
        }

        let gate_sub = Gate::BinOp(BinOp::Sub, usize_to_idx::<T>(0), usize_to_idx(1));
        let gate_sub_swapped = Gate::BinOp(BinOp::Sub, usize_to_idx::<T>(1), usize_to_idx(0));
        let mut sub_set: HashSet<Gate<T>> = HashSet::new();
        sub_set.insert(gate_sub);
        assert!(!sub_set.contains(&gate_sub_swapped));
    }

    #[test]
    fn same_data_same_hash_for_ac() {
        let mut hash_set: HashSet<Gate<u64>> = HashSet::new();

        for bin_op in [BinOp::Add, BinOp::Mul] {
            let gate = Gate::BinOp(bin_op, usize_to_idx(0), usize_to_idx(1));
            let gate_swapped = Gate::BinOp(bin_op, usize_to_idx(1), usize_to_idx(0));

            hash_set.insert(gate);
            assert!(hash_set.contains(&gate_swapped));
        }
    }
    #[test]
    fn same_data_same_hash_for_non_ac() {
        let gate: Gate<u64> = Gate::BinOp(BinOp::Sub, usize_to_idx(0), usize_to_idx(1));
        let gate_swapped = Gate::BinOp(BinOp::Sub, usize_to_idx(1), usize_to_idx(0));

        let mut hash_set = HashSet::new();

        hash_set.insert(gate);
        assert!(!hash_set.contains(&gate_swapped));
    }

    #[test]
    fn all_supported_integer_types_work_as_gate_const() {
        assert_supported_gate_hash::<i8>();
        assert_supported_gate_hash::<i16>();
        assert_supported_gate_hash::<i32>();
        assert_supported_gate_hash::<i64>();
        assert_supported_gate_hash::<u8>();
        assert_supported_gate_hash::<u16>();
        assert_supported_gate_hash::<u32>();
        assert_supported_gate_hash::<u64>();
    }
}
