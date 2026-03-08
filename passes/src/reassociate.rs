//! Implements the reuse-driven reassociation algebraic simplification that try to reuse previously computed expressions
//! by reassociating/rewriting sub-trees. By also favoring constant folding rewrites, tries to minimize number of constants
//! and unnecessary operations on them. The reuse-driven reassociation pass traverses the circuit and finds roots to subgraphs that have:
//!     - one child with same operation kind
//!     - children that are only used by the parent
//! so that they can be rewritten by reassociating it's operands on the operation to reuse past expressions.
//! This is called a dominator meaning that any expression that
//! comes before dominates the current, thus it will be used to rewrite the operation. It is a
//! best-effort algorithm that only rewrites cases where an elimination of the node is possible. 
//! Note that the operation has to be associative and commutative to be written, - for example
//! cannot be reassociated. 
//! let x = a+b;
//! let i = a+c;
//! let y = i+b;
//! becomes
//! let x = a+b;
//! let y = x+c; 
//! a+c+b -> a+b+c -> x+c
//! More on the algorithm can be found on the corresponding wiki page.
use ir::{
    SupportedType, circuit::Circuit, gate::{Gate, GateIdx}
};
use fxhash::{FxBuildHasher, FxHashMap, FxHashSet};
use la_arena::Arena;
use op::BinOp;
use thin_vec::{ThinVec, thin_vec};

use crate::{folding, is_op_associative_and_commutative, liveness::LivenessWRTUsage, new_reassociated_circuit_from, usize_to_idx};

#[derive(Debug)]
struct ReassociationPass {
    q: SupportedType,
    gates: Arena<Gate>,
    dead: FxHashSet<GateIdx>,
    liveness_wrt_usage: LivenessWRTUsage,
    seen: FxHashMap<Gate, GateIdx>,
    operation_gates: ThinVec<GateIdx>,
    constant_gates: FxHashSet<GateIdx>,
}

impl ReassociationPass {
    #[inline]
    fn new(circuit: &Circuit) -> Self {
        let q = circuit.q;
        let gates = circuit.gates().clone();
        let dead: FxHashSet<GateIdx> = FxHashSet::with_hasher(FxBuildHasher::default());
        // Learn topology of the circuit
        let (operation_gates, constant_gates, liveness_wrt_usage, seen) =
            learn_topology_of(circuit);
        Self {
            q,
            gates,
            dead,
            liveness_wrt_usage,
            seen,
            operation_gates,
            constant_gates,
        }
    }
    #[inline]
    fn propagated_decrement(&mut self, from: GateIdx, dec: GateIdx) {
        let is_unused = self.liveness_wrt_usage.decrement(from, dec) == 0;
        if is_unused && let Gate::BinOp(_, lhs, rhs) = self.gates[from] {
            for child in [lhs, rhs] {
                self.propagated_decrement(child, from);
            }
        }
    }
    #[inline]
    fn propagated_increment(&mut self, from: GateIdx, incr:GateIdx) {
        let has_become_used = self.liveness_wrt_usage.increment(from, incr) == 1;
        if has_become_used && let Gate::BinOp(_, lhs, rhs) = self.gates[from]{
            for child in [lhs, rhs] {
                self.propagated_increment(child, from);
            }
        }
   }
    #[inline]
    fn accept_reassociation(&mut self,reassociation: Reassociation, rewritten_root_op_gate: Gate, rewritten_root_op_gate_idx: GateIdx) {
        // The old root gate is changed thus remove it
        self.seen.remove(&rewritten_root_op_gate);

        let (new_root, to_severe_from_root, to_connect_root) = match reassociation{
            // Swapped one of it's child with an already existing (dominator) gate
            Reassociation::Ternary{new_root, usage_update} => {
                let UsageUpdate { to_severe_from_root , to_connect_root} = usage_update;
                (new_root, to_severe_from_root, to_connect_root)
            },
            // We folded 2 constants that are neighbours, thus we have to housekeep: replace with
            // the discarded gate, decrement it's usages and propagate them.
            Reassociation::Folding { new_root, folded_gate, replacing_index, usage_update } => {
                let discarded = self.gates[replacing_index]; 
                self.seen.remove(&discarded);
                // Remove usage of the discarded by rewritten root, and propagate this to it's
                // children recursively. It will be put replacing_index into killing list but once 
                // we increment it's usage again, it will be remove from the killing list.
                self.propagated_decrement(replacing_index, rewritten_root_op_gate_idx);

                self.gates[replacing_index] = folded_gate;
                self.seen.insert(folded_gate, replacing_index);
                // Removes from killing list if necessary
                self.propagated_increment(replacing_index, rewritten_root_op_gate_idx);
                self.constant_gates.insert(replacing_index);

                let UsageUpdate { to_severe_from_root , to_connect_root} = usage_update;
                (new_root, to_severe_from_root, to_connect_root)
            },
        }; 

        self.gates[rewritten_root_op_gate_idx] = new_root;
        self.seen.insert(new_root, rewritten_root_op_gate_idx);

        // cutting ties with the new root: the old gate that is rewritten, or (in case of Ternary) is the
        // input/const moved 'in' to the new reassociated child (already existing dominator).
        for decr in to_severe_from_root {
            self.propagated_decrement(decr, rewritten_root_op_gate_idx);
        }

        // cutting ties with the new root, one is the old gate that is rewritten, other is the
        // input/constant that is moved 'in' to the new reassociated child.
        for incr in to_connect_root {
            self.liveness_wrt_usage.increment(incr, rewritten_root_op_gate_idx);
        }
    }
    #[inline]
    fn prepare_for_next_round(&mut self) {
        self.kill_unused_gates();
        self.collect_survivors_in_operation_gates();
    }
    #[inline]
    // If the reassociation pass is successful, it will render certain nodes of operation gates redundant. They will be killed after the pass, thus we have to housekeep to keep only alive operation gates in the list.  
    fn collect_survivors_in_operation_gates(&mut self) {
        // Unfortunately, we can't swap_remove, we have to keep the list in topological order.
        let mut survivors = ThinVec::with_capacity(self.operation_gates.len());

        for op_gate_idx in &self.operation_gates {
            if self.dead.contains(op_gate_idx) || self.constant_gates.contains(op_gate_idx) {
                continue;
            }
            survivors.push(*op_gate_idx);
        }
        survivors.shrink_to_fit();
        self.operation_gates = survivors;
    }
    #[inline]
    fn kill_unused_gates(&mut self) {
        // When thombstone is a Gate, it's operands if necessary, should have their usages change.
        for to_kill_idx in self.liveness_wrt_usage.get_killing_list() {
            if self.dead.contains(to_kill_idx) { continue; }
            self.dead.insert(*to_kill_idx);

            let to_kill = self.gates[*to_kill_idx]; 

            match to_kill {
                Gate::Input(_) => {},
                Gate::Const(_) => {
                    self.constant_gates.remove(to_kill_idx);
                },
                // Propagate Thombstones if operands are only used by this.
                Gate::BinOp(_, lhs, rhs) => {
                    for operand in [lhs, rhs] {
                        if self.liveness_wrt_usage.num_usage(operand) == 0 {
                            self.gates[operand] = Gate::Thombstone;
                            self.dead.insert(operand);
                        }
                    }
                },
                // A thombstone never will be in the killing list. 
                Gate::Thombstone => unreachable!(),
            }
            self.seen.remove(&to_kill);
            self.gates[*to_kill_idx] = Gate::Thombstone;
        }
        self.liveness_wrt_usage.clear();
    }
}

#[inline]
fn learn_topology_of(
    circuit: &Circuit,
) -> (
    ThinVec<GateIdx>,
    FxHashSet<GateIdx>,
    LivenessWRTUsage,
    FxHashMap<Gate, GateIdx>,
) {
    let gates= circuit.gates();
    let mut operation_gates = ThinVec::<GateIdx>::new();
    let mut constant_gates: FxHashSet<GateIdx> = FxHashSet::with_hasher(FxBuildHasher::default());

    let len_gates = gates.len();

    let mut liveness_wrt_usage = LivenessWRTUsage::new(len_gates);
    // From Gate to index to be able to reassociate with already computed Ops.
    let mut seen: FxHashMap<Gate, GateIdx> = FxHashMap::with_hasher(FxBuildHasher::default());

    for idx in 0..len_gates {
        let gate_idx = usize_to_idx(idx);
        let gate = gates[gate_idx];
        match gate {
            Gate::Input(_) | Gate::Thombstone => {}
            Gate::Const(_) => {
                constant_gates.insert(gate_idx);
            }
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

    // To ensure that outputs are immune to killing, we add the outputs to it's own usages list so
    // that it is never deleted or reassociated away.
    for out_idx in circuit.outputs() {
        liveness_wrt_usage.increment(*out_idx,* out_idx);
    }
    (operation_gates, constant_gates, liveness_wrt_usage, seen)
}

/* Find a sub-graph of an AC (associative, commutative) op of the same kind of the current node, and try to
* rewrite by reusing other pre-computed instructions.
* let x = a+b;
* let i = a+c;
* let y = i+b;
*
* becomes
* let x = a+b;
* let y = x+c;
*
* a+c+b -> a+b+c -> x+c
*/
pub fn reuse_driven_reassociate(circuit: &Circuit) -> Circuit {
    let mut reassociation_pass = ReassociationPass::new(circuit);

    let mut reassociated = true;
    while reassociated {
        reassociated = false;
        // Loop over recorded Ops instead of all the gates.
        for idx in 0..reassociation_pass.operation_gates.len(){
            let gate_idx = reassociation_pass.operation_gates[idx];
            let gate = reassociation_pass.gates[gate_idx];

            // We search for I = ((a op b) op c) or (a op (b op c)) to try associate.
            if let Gate::BinOp(bin_op_of_i, lhs_idx, rhs_idx) = gate
            // To reassociate, we need associativity and commutativity 
            && is_op_associative_and_commutative(bin_op_of_i)
            && let Some(reassociation_candidates) =
                        extract_reassociation_candidates(&reassociation_pass.gates, bin_op_of_i, lhs_idx, rhs_idx)
            // We want the instruction we plan to rewrite is to be used once. 
            // The 'why' of this is explain in the wiki.
            && reassociation_pass.liveness_wrt_usage.num_usage(reassociation_candidates.instruction_to_reassociate) <= 1
            && let Some(reassociation) = try_reassociate_candidates(reassociation_candidates, bin_op_of_i, 
               &mut reassociation_pass)
            {
                reassociation_pass.accept_reassociation(reassociation, gate, gate_idx);
                reassociated = true;
            }
        }
        reassociation_pass.prepare_for_next_round();
    }

    new_reassociated_circuit_from(circuit, reassociation_pass.gates, reassociation_pass.liveness_wrt_usage.get_usages())
}

#[derive(Debug)]
struct ReassociationCandidates {
    a: GateIdx,
    b: GateIdx,
    c: GateIdx,
    is_lhs: bool,
    instruction_to_reassociate: GateIdx,
}

// We want at least one of them to be of same Op and corresponding uses value
// to be 1, so that we can attempt to reassociate the operands.
// We expect something like i) ((a op b) op c) or ii) (a op (b op c)) so that we can
// try (a op c) and (b op c) for i and (a op c) and (a op b) for ii.
#[inline]
fn extract_reassociation_candidates(
    gates: &Arena<Gate>,
    current_instruction_bin_op: BinOp,
    lhs_idx: GateIdx,
    rhs_idx: GateIdx,
) -> Option<ReassociationCandidates> {
    let lhs_gate = gates[lhs_idx];
    let rhs_gate = gates[rhs_idx];

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
        instruction_to_reassociate: i,
    };
    Some(reassociation_candidates)
}


#[derive(Debug)]
struct UsageUpdate {
    to_severe_from_root: ThinVec<GateIdx>, to_connect_root: ThinVec<GateIdx>,
}


#[derive(Debug)]
enum Reassociation {
    Ternary{new_root: Gate, usage_update: UsageUpdate},
    Folding{new_root: Gate, folded_gate: Gate, replacing_index: GateIdx, usage_update: UsageUpdate}
}

#[inline]
fn try_reassociate_candidates(
    reassociation_candidate: ReassociationCandidates,
    bin_op_of_i: BinOp,
    reassociation_pass: &mut ReassociationPass,
) -> Option<Reassociation> {
    let ReassociationCandidates {
        a,
        b,
        c,
        is_lhs,
        instruction_to_reassociate: instruction_to_rewrite ,
    } = reassociation_candidate;

    // i) ((a op b) op c) -> try (a op c) and (b op c)
    // left_right is the common (a op c), mid_other is (b op c) or (a op b)
    let (left_right, (with_mid, other)) = if is_lhs {
        let a_op_c = (a, c);
        let b_op_c = (b, c);
        (a_op_c, (b_op_c, a))
    } 
    // ii) (a op (b op c)) -> (a op c) and (a op b) for ii.
    else {
        let a_op_c = (a, c);
        let a_op_b = (a, b);
        (a_op_c, (a_op_b, c))
    };

    for to_try in [(left_right, b), (with_mid, other)] {
        {
            let reassociation =  form_a_gate(bin_op_of_i, instruction_to_rewrite, to_try, is_lhs, reassociation_pass);
            if reassociation.is_some() {return reassociation}
        }
    }
    None
}

#[inline]
fn form_a_gate(
    bin_op_of_i: BinOp,
    instruction_to_rewrite: GateIdx,
    ops: ((GateIdx, GateIdx), GateIdx),
    is_lhs: bool,
    reassociation_pass: &mut ReassociationPass,
) -> Option<Reassociation> {
    let (new_ops, other) = ops;
    let new_reassociated_child = Gate::BinOp(bin_op_of_i, new_ops.0, new_ops.1);
    if let Some(new_reassoc_gate_idx) = reassociation_pass.seen.get(&new_reassociated_child)
        && !reassociation_pass.dead.contains(new_reassoc_gate_idx)
    {
        let new_root = Gate::BinOp(bin_op_of_i, *new_reassoc_gate_idx, other);
        let moved_in_child = if is_lhs {
            new_ops.1
        } else {
            new_ops.0
        };
        let usage_update = UsageUpdate{to_severe_from_root: thin_vec![instruction_to_rewrite, moved_in_child], to_connect_root: thin_vec![*new_reassoc_gate_idx, other]};
        return Some(Reassociation::Ternary{new_root, usage_update});
    }
    // If the new operation is constant op constant, to enable folding we reassociate.
    // We create a new Gate::Const with folding::fold 
    else if reassociation_pass.constant_gates.contains(&new_ops.0) && reassociation_pass.constant_gates.contains(&new_ops.1) {
        // We know that instruction to rewrite is only used by us, so we will replace it with our folded gate directly.
        let folded_gate= folding::fold(Gate::BinOp(bin_op_of_i, new_ops.0, new_ops.1), &reassociation_pass.gates, reassociation_pass.q);
    
        // We have to severe the usage connections of the constants with their parent Ops. 
        // In case association is lhs and left/right, left-most constant and the `other` is attached to gate that is being
        // replaced, right-most constant is attached to the root of this sub-tree.
        // Since the ops come in order, we can deduce which nodes are connected to which.
        // Only severe const attached to root, dead one will be propagated during replacement, other non-const must
        // be connected to the new root now.
        let to_severe_from_root = if is_lhs {
            new_ops.1
        } else {
            new_ops.0
        };

        let usage_update = UsageUpdate {
            to_severe_from_root: thin_vec![to_severe_from_root], to_connect_root: thin_vec![other],
        };

        // The folded gate will be put in place of the gate at instruction_to_rewrite.
        let new_root = Gate::BinOp(bin_op_of_i, instruction_to_rewrite, other);
        let replacing_index = instruction_to_rewrite;
        return Some(Reassociation::Folding{new_root, folded_gate, replacing_index, usage_update});
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

        let mut reassociation_pass = ReassociationPass::new(&circuit);
        let reassociation_candidate = extract_reassociation_candidates(
            &reassociation_pass.gates,
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
            reassociation_candidate.instruction_to_reassociate
        );

        let new_gate = Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(2));

        let reassociation =
            try_reassociate_candidates(reassociation_candidate, BinOp::Add, &mut reassociation_pass)
                .expect("to reassociate");

        match reassociation {
            Reassociation::Ternary { new_root, usage_update } => {
                assert_eq!(new_gate, new_root);
                assert_eq!(&[usize_to_idx(4), usize_to_idx(1)], usage_update.to_severe_from_root.as_slice());
                assert_eq!(&[usize_to_idx(3), usize_to_idx(2)], usage_update.to_connect_root.as_slice());
            },
            _ => panic!(),
        }
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

        let mut reassociation_pass = ReassociationPass::new(&circuit);
        let reassociation_candidate = extract_reassociation_candidates(
            &reassociation_pass.gates,
            BinOp::Add,
            usize_to_idx(1),
            usize_to_idx(4),
        )
        .expect("to have a reassociation candidate");

        assert!(!reassociation_candidate.is_lhs);

        // b+(c+a)
        assert_eq!(usize_to_idx(1), reassociation_candidate.a);
        assert_eq!(usize_to_idx(2), reassociation_candidate.b);
        assert_eq!(usize_to_idx(0), reassociation_candidate.c);
        assert_eq!(
            usize_to_idx(4),
            reassociation_candidate.instruction_to_reassociate
        );

        let new_gate = Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(0));

        let reassociation =
            try_reassociate_candidates(reassociation_candidate, BinOp::Add, &mut reassociation_pass)
                .expect("to reassociate");

        match reassociation {
            Reassociation::Ternary { new_root, usage_update } => {
                assert_eq!(new_gate, new_root);
                assert_eq!(&[usize_to_idx(4), usize_to_idx(1)], usage_update.to_severe_from_root.as_slice());
                assert_eq!(&[usize_to_idx(3), usize_to_idx(0)], usage_update.to_connect_root.as_slice());
            },
            _ => panic!(),
        }
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
            let expected_outputs = ThinVec::from_iter(expected_outputs.iter().cloned().map(usize_to_idx));
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
        /*
        * let x = a+c;
        * let z = b+d;
        * let add_1 = a+b;
        * let add_2 = add_1 + c;
        * let y = add_2 + d;
        *
        * stays the same
        * let x = a+c;
        * let z = b+d;
        * let y = ((a+c)+b)+d;
        */
        multiple_reassociations: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::Input(3),                                            // d
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(2)), // a + c
                Gate::BinOp(BinOp::Add, usize_to_idx(1), usize_to_idx(3)), // b + d
                // will be removed
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a + b
                // will be removed
                Gate::BinOp(BinOp::Add, usize_to_idx(6), usize_to_idx(2)), // (a+b)+c
                Gate::BinOp(BinOp::Add, usize_to_idx(7), usize_to_idx(3)), // ((a+b)+c)+d
            ],
            &[0,1,2,3],
            &[4,5,8],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::Input(3),                                            // d
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(2)), // a + c
                Gate::BinOp(BinOp::Add, usize_to_idx(1), usize_to_idx(3)), // b + d
                Gate::BinOp(BinOp::Add, usize_to_idx(4), usize_to_idx(5)), // ((a+b)+c)+d
            ],
            &[4,5,6],
        ),
        /*
        * let c1 = 2;
        * let c2 = 3;
        * let x = a+b
        * let a_c1 = a+c1;
        * let a_c1_c2 = a_c1 + c2;   -> (a+c1)+c2 -> a+(c3)
        * let y = a_c1_c2 + b;
        *
        * will become 
        * let x = a+b
        * let c_fold = ctx.constant(2+3);
        * let y = c_fold+x;
        */
        multiple_reassociations_enables_constant_folding_1_round_for_folding: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Const(2),                                            // c1
                Gate::Const(3),                                            // c2
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a + b 
                // replacing this one
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(2)), // a + c1
                // to make this one (a+c3)
                Gate::BinOp(BinOp::Add, usize_to_idx(5), usize_to_idx(3)), // a_c1 + c2
                Gate::BinOp(BinOp::Add, usize_to_idx(6), usize_to_idx(1)), // a_c1_c2 + b
            ],
            &[0,1],
            &[4,7],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a + b 
                Gate::Const(2+3),                                          // c1 + c2
                Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(3)), // c1_c2 + x
            ],
            &[2,4],
        ),
        /*
        * let c1 = 2;
        * let c2 = 3;
        * let c3 = 4;
        * let x = a+b;
        * let a_c1 = a + c1;
        * let b_c2 = b + c2;
        * let a_c1_c2 = a_c1 + c2;   -> (a+c1)+c2 -> a+(c3)
        * let b_c2_c3 = b_c2 + c3;   -> (a+c1)+c2 -> a+(c3)
        * let y = a_c1_c2 + b_c2_c3;
        *
        * will become 
        * let x = a+b
        * let c_fold_12 = ctx.constant(2+3);
        * let c_fold_23 = ctx.constant(3+4);
        * // intermediate state constants, won't be in the final circuit.
        * let a_c1_c2 = a + c_fold_12;
        * let b_c2_c3 = b + c_fold_23;
        * let c_fold_12_23 = c_fold_12 + c_fold_23;
        * let y = x + c_fold_12_23
        */
        multiple_reassociations_enables_constant_folding_3_folds_1_round: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Const(2),                                            // c1
                Gate::Const(3),                                            // c2
                Gate::Const(4),                                            // c3
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a + b 
                /*6. */Gate::BinOp(BinOp::Add, usize_to_idx(1), usize_to_idx(3)),// b + c2
                // This will be replaced with b + (c5)
                /*7. */Gate::BinOp(BinOp::Add, usize_to_idx(6), usize_to_idx(4)),// b_c2 + c3
                // This will be replaced with (c5) + x
                /*8. */Gate::BinOp(BinOp::Add, usize_to_idx(7), usize_to_idx(0)),// b_c2_c3 + a
                // This will be replaced with (c6) + x
                /*9. */Gate::BinOp(BinOp::Add, usize_to_idx(8), usize_to_idx(2)),// b_c2_c3_a + c1
            ],
            &[0,1],
            &[5,9],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a + b 
                Gate::Const(2+3+4),                                      // c1 + c2 + c3
                Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(3)), // c1_c2_c3 + x
            ],
            &[2,4],
        ),
    }
}
