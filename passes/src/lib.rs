use common::{idx_to_usize as c_idx_to_usize, usize_to_idx as c_usize_to_idx};
use fxhash::FxHashSet;
use ir::{
    circuit::Circuit,
    gate::{Gate, GateIdx},
};
use la_arena::Arena;
use op::BinOp;
use thin_vec::ThinVec;

pub mod analysis;
pub mod depth;
pub mod folding;
pub mod interner;
pub mod liveness;
pub mod reassociate;
pub mod rebalance;

pub(crate) fn idx_to_usize(gate_idx: GateIdx) -> usize {
    c_idx_to_usize::<Gate>(gate_idx)
}
pub(crate) fn usize_to_idx(i: usize) -> GateIdx {
    c_usize_to_idx::<Gate>(i)
}

pub(crate) fn is_op_associative_and_commutative(bin_op_of_i: BinOp) -> bool {
    bin_op_of_i.is_associative() && bin_op_of_i.is_commutative()
}

pub(crate) fn propagate_new_index_to_using_gates(
    gate_idx: GateIdx,
    new_gate_idx: GateIdx,
    associated_gates: &mut Arena<Gate>,
    usages: &FxHashSet<GateIdx>,
    is_an_output: bool,
) {
    for user in usages {
        // Outputs are self-used to prevent them being killed.
        if is_an_output && *user == gate_idx {
            continue;
        }
        let mut user_gate = associated_gates[*user];
        match &mut user_gate {
            // NOTE: A gate user cannot be a constant or an input, their in-degree is 0.
            Gate::Input(_) | Gate::Const(_) => unreachable!(),
            Gate::Thombstone => continue,
            Gate::BinOp(_bin_op, lhs, rhs) => {
                let replace = if *lhs == gate_idx {
                    lhs
                } else if *rhs == gate_idx {
                    rhs
                } else {
                    unreachable!()
                };
                *replace = new_gate_idx;
            }
        }
        associated_gates[*user] = user_gate;
    }
}

pub(crate) fn transfer_gate_to_final_arena(
    gate_idx: GateIdx,
    old_gates: &Arena<Gate>,
    new_gates: &mut Arena<Gate>,
    inputs: &mut ThinVec<GateIdx>,
    outputs: &mut ThinVec<GateIdx>,
    old_outputs: &mut FxHashSet<&GateIdx>,
) -> Option<(GateIdx, bool)> {
    let gate = old_gates[gate_idx];

    let new_gate_idx = new_gates.alloc(gate);

    if matches!(gate, Gate::Input(_)) {
        inputs.push(new_gate_idx);
    }

    let is_an_output = old_outputs.remove(&gate_idx);
    if is_an_output {
        outputs.push(new_gate_idx);
    }

    if new_gate_idx == gate_idx {
        return None;
    }
    Some((new_gate_idx, is_an_output))
}

pub(crate) fn new_reassociated_circuit_from(
    circuit: &Circuit,
    mut passed_gates: Arena<Gate>,
    usages: &[FxHashSet<GateIdx>],
) -> Circuit {
    // input indices may have changed
    let mut inputs = ThinVec::with_capacity(circuit.inputs().len());
    // We have usages list at hand, we traverse and copy mutated arena by correcting indices on
    // the way and filtering Thombstones into a new arena. Since the gates are in topological
    // order, it is guaranteed that any Operation using an operand comes after it.
    let mut old_outputs = FxHashSet::from_iter(circuit.outputs().iter());
    // output indices may have changed
    let mut outputs = ThinVec::with_capacity(circuit.outputs().len());

    let mut alive_gates = Arena::new();

    for (idx, usage) in usages.iter().enumerate().take(passed_gates.len()) {
        if usage.is_empty() {
            continue;
        }
        let gate_idx = usize_to_idx(idx);

        if let Some((new_gate_idx, is_an_output)) = transfer_gate_to_final_arena(
            gate_idx,
            &passed_gates,
            &mut alive_gates,
            &mut inputs,
            &mut outputs,
            &mut old_outputs,
        ) {
            propagate_new_index_to_using_gates(
                gate_idx,
                new_gate_idx,
                &mut passed_gates,
                usage,
                is_an_output,
            );
        }
    }
    Circuit::with(circuit.q, alive_gates, inputs, outputs)
}
