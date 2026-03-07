use fxhash::{FxBuildHasher, FxHashSet};
use ir::gate::GateIdx;
use thin_vec::{ThinVec, thin_vec};

use crate::idx_to_usize;

#[derive(Debug)]
pub(crate) struct LivenessWRTUsage {
    // Number of uses of each Gate by other gates
    usages: ThinVec<FxHashSet<GateIdx>>,
    // Set of GateIdx that ended up as not used by any gates at the end of the pass.
    to_kill: FxHashSet<GateIdx>,
}

impl LivenessWRTUsage {
    pub(crate) fn new(len_gates: usize) -> Self {
        Self {
            usages: thin_vec![FxHashSet::with_hasher(FxBuildHasher::default()); len_gates],
            to_kill: FxHashSet::with_hasher(FxBuildHasher::default()),
        }
    }
    pub(crate) fn clear(&mut self) {
        self.to_kill.clear();
    }
    pub(crate) fn num_usage(&self, gate_idx: GateIdx) -> usize {
        let idx_usize = idx_to_usize(gate_idx);
        self.usages[idx_usize].len()
    }

    pub(crate) fn get_usages(&self) -> &[FxHashSet<GateIdx>] {
        self.usages.as_slice()
    }
    pub(crate) fn decrement(&mut self, gate_idx: GateIdx, rem: GateIdx) -> usize {
        let usage = self.rem_usage_of(gate_idx, rem);
        if usage == 0 {
            self.to_kill.insert(gate_idx);
        }
        usage
    }
    pub(crate) fn increment(&mut self, gate_idx: GateIdx, add: GateIdx) -> usize {
        let usage = self.add_usage_of(gate_idx, add);
        if usage == 1 {
            self.to_kill.remove(&gate_idx);
        }
        usage
    }
    pub(crate) fn get_killing_list(&self) -> &FxHashSet<GateIdx> {
        &self.to_kill
    }
    fn change_usage_of(&mut self, gate_idx: GateIdx, of: GateIdx, add: bool) -> usize {
        let idx_usize = idx_to_usize(gate_idx);
        if add {
            self.usages[idx_usize].insert(of);
        } else {
            self.usages[idx_usize].remove(&of);
        }
        self.usages[idx_usize].len()
    }
    fn rem_usage_of(&mut self, gate_idx: GateIdx, of: GateIdx) -> usize {
        self.change_usage_of(gate_idx, of, false)
    }
    fn add_usage_of(&mut self, gate_idx: GateIdx, of: GateIdx) -> usize {
        self.change_usage_of(gate_idx, of, true)
    }
}
