use fxhash::{FxBuildHasher, FxHashMap, FxHashSet};
use ir::{
    circuit::Circuit,
    gate::{Gate, GateIdx},
};
use la_arena::Arena;
use op::BinOp;
use thin_vec::ThinVec;

use crate::{idx_to_usize, liveness::LivenessWRTUsage, usize_to_idx};

/* Find sub-graphs that is pure AC (associative, commutative) op of the same kind, and try to
* rewrite them to reuse other pre-computed instructions.
* let x = a+b;
* let y = a+c+b;
*
* becomes
* let x = a+b;
* let y = x+c;
*
* a+c+b -> a+b+c -> x+c
*/
pub fn reuse_driven_reassociate(circuit: &Circuit) -> Circuit {
    let len_gates = circuit.gates().len();

    let mut liveness_wrt_usage = LivenessWRTUsage::new(len_gates);
    let mut dead = vec![false; len_gates];
    // From Gate to idx index to be able to reassociate with already computed Ops.
    let mut seen: FxHashMap<Gate, GateIdx> = FxHashMap::with_hasher(FxBuildHasher::default());
    let mut gates = circuit.gates().clone();

    let operation_gates = record_gate_relationships(&gates, &mut liveness_wrt_usage, &mut seen);

    let mut reassociated = true;
    while reassociated {
        reassociated = false;
        // Loop over recorded Ops instead of all the gates.
        for idx in 0..operation_gates.len() {
            let gate_idx = operation_gates[idx];
            let mut gate = gates[gate_idx];
            // We search for I = ((a op b) op c) or (a op (b op c)) to try associate.
            if let Gate::BinOp(bin_op_of_i, lhs_idx, rhs_idx) = gate
            // To reassociate, we need associativity and commutativity 
            && is_op_associative_and_commutative(bin_op_of_i)
            && let Some(reassociation_candidates) =
                        extract_reassociation_candidates(circuit, bin_op_of_i, lhs_idx, rhs_idx)
            // We want the instruction we plan to rewrite is to be used once. 
            // The 'why' of this is explain in the wiki.
            && liveness_wrt_usage.num_usage(reassociation_candidates.instruction_to_rewrite) <= 1
            && let Some(reassociation) = try_reassociate_candidates(reassociation_candidates, bin_op_of_i, &seen, &dead)
            {
                gates[gate_idx] = reassociation.gate;
                seen.remove(&gate);
                liveness_wrt_usage.increment(reassociation.replacing, gate_idx);
                liveness_wrt_usage.decrement(reassociation.replaced, gate_idx);
                gate = reassociation.gate;
                seen.insert(gate, gate_idx);
                reassociated = true;
            }
        }
        // TODO: remove Thombstoned indices from operation_gates
        kill_unused_gates(&mut liveness_wrt_usage, &mut dead, &mut gates);
    }

    new_reassociated_circuit_from(circuit, &mut gates, &liveness_wrt_usage)
}

fn is_op_associative_and_commutative(bin_op_of_i: BinOp) -> bool {
    bin_op_of_i.is_associative() && bin_op_of_i.is_commutative()
}

fn record_gate_relationships(
    gates: &Arena<Gate>,
    liveness_wrt_usage: &mut LivenessWRTUsage,
    seen: &mut FxHashMap<Gate, GateIdx>,
) -> ThinVec<GateIdx> {
    let mut operation_gates = ThinVec::<GateIdx>::new();
    let len_gates = gates.len();
    for idx in 0..len_gates {
        let gate_idx = usize_to_idx(idx);
        let gate = gates[gate_idx];
        match gate {
            Gate::Input(_) | Gate::Const(_) | Gate::Thombstone => {}
            Gate::BinOp(_, lhs_idx, rhs_idx) => {
                operation_gates.push(gate_idx);
                // Update the operands usages by adding this gate to the set.
                for operand in [lhs_idx, rhs_idx] {
                    liveness_wrt_usage.increment(operand, gate_idx);
                }
            }
        }
        seen.insert(gate, gate_idx);
    }
    operation_gates
}

fn kill_unused_gates(
    liveness_wrt_usage: &mut LivenessWRTUsage,
    dead: &mut [bool],
    gates: &mut Arena<Gate>,
) {
    // When thombstone is a Gate, it's operands if necessary, should have their usages change.
    for to_kill_idx in liveness_wrt_usage.get_killing_list() {
        let dead_idx = idx_to_usize(*to_kill_idx);
        dead[dead_idx] = true;
        // Propagate Thombstones if operans are only used by this.
        if let Gate::BinOp(_bin_op, lhs, rhs) = gates[*to_kill_idx] {
            for operand in [lhs, rhs] {
                if liveness_wrt_usage.num_usage(operand) == 0 {
                    gates[operand] = Gate::Thombstone;
                }
            }
        }
        gates[*to_kill_idx] = Gate::Thombstone;
    }
    liveness_wrt_usage.clear();
}

fn new_reassociated_circuit_from(
    circuit: &Circuit,
    gates: &mut Arena<Gate>,
    liveness_wrt_usage: &LivenessWRTUsage,
) -> Circuit {
    // input indices may have changed
    let mut inputs = ThinVec::with_capacity(circuit.inputs().len());
    // output indices may have changed
    let mut outputs = ThinVec::with_capacity(circuit.outputs().len());
    let mut alive_gates = Arena::new();

    // We have usages list at hand, we traverse and copy mutated arena by correcting indices on
    // the way and filtering Thombstones into a new arena. Since the gates are in topological
    // order, it is guaranteed that any Operation using an operand comes after it.
    let mut old_outputs = FxHashSet::from_iter(circuit.outputs().iter());

    for idx in 0..gates.len() {
        let gate_idx = usize_to_idx(idx);
        let gate = gates[gate_idx];

        if matches!(gate, Gate::Thombstone) {
            continue;
        }

        let new_gate_idx = alive_gates.alloc(gate);

        if matches!(gate, Gate::Input(_)) {
            inputs.push(new_gate_idx);
        }

        if old_outputs.remove(&gate_idx) {
            outputs.push(new_gate_idx);
        }

        if new_gate_idx == gate_idx {
            continue;
        }

        for user in liveness_wrt_usage.get_usages(gate_idx) {
            let mut user_gate = gates[*user];
            match &mut user_gate {
                // NOTE: folding will be another pass thus those cannot be in usage list.
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
            gates[*user] = user_gate;
        }
    }
    Circuit::with(circuit.q, alive_gates, inputs, outputs)
}

#[derive(Debug)]
struct ReassociationCandidates {
    a: GateIdx,
    b: GateIdx,
    c: GateIdx,
    is_lhs: bool,
    instruction_to_rewrite: GateIdx,
}

// We want at least one of them to be of same Op and corresponding uses value
// to be 1, so that we can attempt to reassociate the operands.
// We expect something like i) ((a op b) op c) or ii) (a op (b op c)) so that we can
// try (a op c) and (b op c) for i and (a op c) and (a op b) for ii.
fn extract_reassociation_candidates(
    circuit: &Circuit,
    current_instruction_bin_op: BinOp,
    lhs_idx: GateIdx,
    rhs_idx: GateIdx,
) -> Option<ReassociationCandidates> {
    let lhs_gate = circuit.gates()[lhs_idx];
    let rhs_gate = circuit.gates()[rhs_idx];

    // We want at least one of them to be of same Op and corresponding uses value
    // to be 1, so that we can attempt to reassociate the operands.
    // We expect something like i) ((a op b) op c) or ii) (a op (b op c)) so that we can
    // try (a op c) and (b op c) for i and (a op c) and (a op b) for ii.
    let (a, b, c, is_lhs, i) = match lhs_gate {
        // lhs is (a op b), thus we want (a op b) op c i.e rhs to be NOT an op
        Gate::BinOp(child_bin_op, a, b) if child_bin_op == current_instruction_bin_op => {
            let is_lhs = true;
            let c = match rhs_gate {
                // lhs is (a op b) we want rhs to be NOT an op
                Gate::Input(_) | Gate::Const(_) => rhs_idx,
                _ => return None,
            };
            (a, b, c, is_lhs, lhs_idx)
        }
        // lhs is a, thus we want a op (b op c) i.e rhs to be an op
        Gate::Input(_) | Gate::Const(_) => {
            let is_lhs = false;
            let a = lhs_idx;
            let (b, c) = match rhs_gate {
                // lhs is a we want rhs to be an op
                Gate::BinOp(child_bin_op, b, c) if child_bin_op == current_instruction_bin_op => {
                    (b, c)
                }
                // If it is a Thombstone or not same Op, return None
                _ => return None,
            };
            (a, b, c, is_lhs, rhs_idx)
        }
        // If it is a Thombstone or not same Op, return None
        _ => return None,
    };
    let reassociation_candidates = ReassociationCandidates {
        a,
        b,
        c,
        is_lhs,
        instruction_to_rewrite: i,
    };
    Some(reassociation_candidates)
}

#[derive(Debug)]
struct Reassociation {
    replacing: GateIdx,
    replaced: GateIdx,
    gate: Gate,
}

fn try_reassociate_candidates(
    reassociation_candidate: ReassociationCandidates,
    bin_op_of_i: BinOp,
    seen: &FxHashMap<Gate, GateIdx>,
    dead: &[bool],
) -> Option<Reassociation> {
    let ReassociationCandidates {
        a,
        b,
        c,
        is_lhs,
        instruction_to_rewrite: i,
    } = reassociation_candidate;

    // i) ((a op b) op c) -> try (a op c) and (b op c)
    // ii) (a op (b op c)) -> (a op c) and (a op b) for ii.
    // left_right is the common (a op c), mid_other is (b op c) or (a op b)
    let (left_right, (with_mid, other)) = if is_lhs {
        let a_op_c = (a, c);
        let b_op_c = (b, c);
        (a_op_c, (b_op_c, a))
    } else {
        let a_op_c = (a, c);
        let a_op_b = (a, b);
        (a_op_c, (a_op_b, c))
    };

    for to_try in [(left_right, b), (with_mid, other)] {
        if let Some((new_gate, reassoc_gate_idx)) = form_a_gate(bin_op_of_i, to_try, seen, dead) {
            let reassociation = Reassociation {
                replacing: reassoc_gate_idx,
                replaced: i,
                gate: new_gate,
            };
            return Some(reassociation);
        }
    }

    None
}

fn form_a_gate(
    bin_op_of_i: BinOp,
    ops: ((GateIdx, GateIdx), GateIdx),
    seen: &FxHashMap<Gate, GateIdx>,
    dead: &[bool],
) -> Option<(Gate, GateIdx)> {
    let (new_ops, other) = ops;
    let new_reassoc_gate = Gate::BinOp(bin_op_of_i, new_ops.0, new_ops.1);
    if let Some(new_reassoc_gate_idx) = seen.get(&new_reassoc_gate)
        && !dead[idx_to_usize(*new_reassoc_gate_idx)]
    {
        let new_gate = Gate::BinOp(bin_op_of_i, *new_reassoc_gate_idx, other);
        return Some((new_gate, *new_reassoc_gate_idx));
    }
    None
}

#[cfg(test)]
mod test {

    use super::*;
    use thin_vec::thin_vec;

    use la_arena::Arena;

    use parameterized_test::create;

    #[test]
    fn test_extract_reassociation_candidates_a_c() {
        let gates = thin_vec![
            Gate::Input(0),                                            // a
            Gate::Input(1),                                            // b
            Gate::Input(2),                                            // c
            Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a + b
            Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(2)), // a + c
            Gate::BinOp(BinOp::Add, usize_to_idx(4), usize_to_idx(1)), // (a+c)+b
        ];
        let inputs = thin_vec![usize_to_idx(0), usize_to_idx(1), usize_to_idx(2)];
        let outputs = thin_vec![usize_to_idx(3), usize_to_idx(5)];
        let q = 11;

        let circuit = Circuit::with(q, Arena::from_iter(gates.iter().cloned()), inputs, outputs);

        let len_gates = circuit.gates().len();

        let dead = vec![false; len_gates];
        let mut seen: FxHashMap<Gate, GateIdx> = FxHashMap::with_hasher(FxBuildHasher::default());

        seen.insert(circuit.gates()[usize_to_idx(3)], usize_to_idx(3));

        let reassociation_candidate = extract_reassociation_candidates(
            &circuit,
            BinOp::Add,
            usize_to_idx(4),
            usize_to_idx(1),
        )
        .expect("to have a reassociation candidate");

        assert!(reassociation_candidate.is_lhs);

        assert_eq!(usize_to_idx(0), reassociation_candidate.a);
        assert_eq!(usize_to_idx(2), reassociation_candidate.b);
        assert_eq!(usize_to_idx(1), reassociation_candidate.c);
        assert_eq!(
            usize_to_idx(4),
            reassociation_candidate.instruction_to_rewrite
        );

        let new_gate = Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(2));

        let reassociation =
            try_reassociate_candidates(reassociation_candidate, BinOp::Add, &seen, &dead)
                .expect("to reassociate");

        assert_eq!(usize_to_idx(3), reassociation.replacing);
        assert_eq!(usize_to_idx(4), reassociation.replaced);
        assert_eq!(new_gate, reassociation.gate);
    }

    #[test]
    fn test_extract_reassociation_candidates_c_a() {
        let gates = thin_vec![
            Gate::Input(0),                                            // a
            Gate::Input(1),                                            // b
            Gate::Input(2),                                            // c
            Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(1)), // c+b
            Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(0)), // c+a
            Gate::BinOp(BinOp::Add, usize_to_idx(1), usize_to_idx(4)), // b+(c+a)
        ];

        let inputs = thin_vec![usize_to_idx(0), usize_to_idx(1), usize_to_idx(2)];
        let outputs = thin_vec![usize_to_idx(3), usize_to_idx(5)];
        let q = 11;

        let circuit = Circuit::with(q, Arena::from_iter(gates.iter().cloned()), inputs, outputs);

        let len_gates = circuit.gates().len();

        let dead = vec![false; len_gates];
        let mut seen: FxHashMap<Gate, GateIdx> = FxHashMap::with_hasher(FxBuildHasher::default());

        seen.insert(circuit.gates()[usize_to_idx(3)], usize_to_idx(3));

        let reassociation_candidate = extract_reassociation_candidates(
            &circuit,
            BinOp::Add,
            usize_to_idx(1),
            usize_to_idx(4),
        )
        .expect("to have a reassociation candidate");

        dbg!(&reassociation_candidate);
        assert!(!reassociation_candidate.is_lhs);

        // b+(c+a)
        assert_eq!(usize_to_idx(1), reassociation_candidate.a);
        assert_eq!(usize_to_idx(2), reassociation_candidate.b);
        assert_eq!(usize_to_idx(0), reassociation_candidate.c);
        assert_eq!(
            usize_to_idx(4),
            reassociation_candidate.instruction_to_rewrite
        );

        let new_gate = Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(0));

        let reassociation =
            try_reassociate_candidates(reassociation_candidate, BinOp::Add, &seen, &dead)
                .expect("to reassociate");

        assert_eq!(usize_to_idx(3), reassociation.replacing);
        assert_eq!(usize_to_idx(4), reassociation.replaced);
        assert_eq!(new_gate, reassociation.gate);
    }

    create! {
        reuse_driven_reassociate,
        (gates, inputs, outputs, expected_gates, expected_outputs), {
            let idx_inputs : ThinVec<GateIdx> = inputs.iter().map(|i: &usize| usize_to_idx(*i)).collect();
            let idx_outputs : ThinVec<GateIdx> = outputs.iter().map(|i: &usize| usize_to_idx(*i)).collect();
            let q = 11;

            let circuit = Circuit::with(q, Arena::from_iter(gates.iter().cloned()), idx_inputs, idx_outputs);

            let new_circuit = reuse_driven_reassociate(&circuit);
            assert_eq!(
                &Arena::from_iter(expected_gates.iter().cloned()),
                new_circuit.gates()
            );
            assert_eq!(new_circuit.inputs(), circuit.inputs());
            let expected_outputs = thin_vec![usize_to_idx(3), usize_to_idx(4)];
            assert_eq!(expected_outputs, new_circuit.outputs());
        }
    }

    reuse_driven_reassociate! {
        /*
        * let x = a+b;
        * let y = a+c+b;
        *
        * becomes
        * let x = a+b;
        * let y = x+c;
        */
        left_right_rewrite: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(2)), // a+c
                Gate::BinOp(BinOp::Add, usize_to_idx(4), usize_to_idx(1)), // (a+c)+b
            ],
            &[0,1,2],
            &[3,5],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(2)), // (a+b)+ c
            ],
            &[3,4],
        ),
        /*
        * let x = a+b;
        * let y = c+a+b;
        *
        * becomes
        * let x = a+b;
        * let y = x+c;
        */
        mid_right_rewrite: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(0)), // c+a
                Gate::BinOp(BinOp::Add, usize_to_idx(4), usize_to_idx(1)), // (a+c)+b
            ],
            &[0,1,2],
            &[3,5],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(2)), // (a+b)+c
            ],
            &[3,4],
        ),
        /*
        * let x = b+c;
        * let z = c+a;
        * let y = z+b;
        *
        * becomes
        * let x = b+c;
        * let y = x+a;
        */
        left_right_rewrite_swapped_order: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(1), usize_to_idx(2)), // b+c
                Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(0)), // c+a
                Gate::BinOp(BinOp::Add, usize_to_idx(4), usize_to_idx(1)), // (a+c)+b
            ],
            &[0,1,2],
            &[3,5],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(1), usize_to_idx(2)), // b+c
                Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(0)), // (b+c)+a
            ],
            &[3,4],
        ),
        /*
        * let x = b+c;
        * let z = c+a;
        * let y = b+z;
        *
        * becomes
        * let x = b+c;
        * let y = x+a;
        * TODO: add this test case
        */

        /*
        * let x = c+b;
        * let z = c+a;
        * let y = z+b;
        *
        * becomes
        * let x = c+b;
        * let y = x+a;
        */
        left_right_rewrite_swapped_order_in_output: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(1)), // c+b
                Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(0)), // c+a
                Gate::BinOp(BinOp::Add, usize_to_idx(1), usize_to_idx(4)), // b+(c+a)
            ],
            &[0,1,2],
            &[3,5],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(1)), // c+b
                Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(0)), // (c+b)+a
            ],
            &[3,4],
        ),
        /*
        * let x = a+b;
        * let z = a+c;
        * let y = z+b;
        *
        * eliminates z
        * let x = a+c;
        * let y = x+c;
        */
        single_use_coming_after_is_eliminated_a_c: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(2)), // a+c
                Gate::BinOp(BinOp::Add, usize_to_idx(4), usize_to_idx(1)), // (a+c)+b
            ],
            &[0,1,2],
            &[3,5],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(2)), // (a+b)+c
            ],
            &[3,4],
        ),
        /*
        * let x = a+c;
        * let z = a+b;
        * let y = z+c;
        *
        * eliminates z
        * let x = a+c;
        * let y = x+b;
        */
        single_use_coming_after_is_eliminated_a_b: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(2)), // a+c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                Gate::BinOp(BinOp::Add, usize_to_idx(4), usize_to_idx(2)), // (a+b)+c
            ],
            &[0,1,2],
            &[3,5],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(2)), // a+c
                Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(1)), // (a+c)+b
            ],
            &[3,4],
        ),
        /*
        * let x = a+c;
        * let y = b+c;
        *
        * stays the same
        * let x = a+c;
        * let y = b+c;
        */
        staying_the_same_if_no_candidates: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(2)), // a + c
                Gate::BinOp(BinOp::Add, usize_to_idx(1), usize_to_idx(2)), // a + b
            ],
            &[0,1,2],
            &[3,4],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(2)), // a + c
                Gate::BinOp(BinOp::Add, usize_to_idx(1), usize_to_idx(2)), // a + b
            ],
            &[3,4],
        ),
    }
}
