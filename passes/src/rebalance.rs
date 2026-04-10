//! Implements the tree-rebalancing algebraic simplification that try to minimize the circuit depth. When possible it
//! folds constants. The tree-rebalancing pass traverses the circuit and finds roots that a
//! rebalancing can be performed on by isolating operation sub-trees and prioritizing according to
//! operator precedence, e.g. rebalances * roots before + roots.
//! More on the algorithm can be found on the corresponding wiki page.
use ir::{
    Supported,
    circuit::Circuit,
    gate::{Gate, GateIdx},
};
use op::BinOp;

use fxhash::{FxBuildHasher, FxHashMap, FxHashSet};
use la_arena::Arena;
use prio_queue::PrioQueue;
use thin_vec::{ThinVec, thin_vec};

use crate::{PropagationError, folding::fold, idx_to_usize, new_circuit_from, usize_to_idx};

#[derive(Debug)]
pub enum BalancingError<T: Supported> {
    BalanceReceivedNoOpGate(GateIdx<T>),
    FlattenReceivedTombstone(GateIdx<T>),
    RebuildReceivedNoOpSubtree(String),
    RebuildFinalisedWithEmptyPrioQueue,
    PropagationFailure(PropagationError<T>),
}

pub type BalacingResult<T> = Result<Circuit<T>, BalancingError<T>>;

#[derive(Debug)]
struct RebalancePass<T: Supported> {
    index: usize,
    old_to_new_gate_idx_map: FxHashMap<GateIdx<T>, GateIdx<T>>,
    outputs: FxHashSet<GateIdx<T>>,
    plain_text_modulus: T,
    ranks: ThinVec<isize>,
    rebalanced: Arena<Gate<T>>,
    roots: FxHashSet<GateIdx<T>>,
    usages: ThinVec<FxHashSet<GateIdx<T>>>,
}

impl<T: Supported> RebalancePass<T> {
    #[inline]
    fn new(circuit: &Circuit<T>, roots: FxHashSet<GateIdx<T>>) -> Self {
        let gates_len = circuit.gates().len();
        let index = gates_len;
        let q = circuit.plain_text_modulus;
        let ranks = thin_vec![-1; gates_len];
        let rebalanced = Arena::with_capacity(gates_len);
        let root_old_to_new_gate_idx_map = FxHashMap::with_hasher(FxBuildHasher::default());
        let usages = thin_vec![FxHashSet::with_hasher(FxBuildHasher::default()); gates_len];
        // NOTE: each output will become a root, thus they will stay in the same index throughout
        // the whole pass. We have to track them so that we can bind looped usages (self as user).
        let outputs = FxHashSet::from_iter(circuit.outputs().iter().cloned());
        Self {
            index,
            old_to_new_gate_idx_map: root_old_to_new_gate_idx_map,
            outputs,
            plain_text_modulus: q,
            ranks,
            rebalanced,
            roots,
            usages,
        }
    }
    #[inline]
    fn is_rebalanced(&self, idx: usize) -> bool {
        self.ranks[idx] >= 0
    }
    #[inline]
    fn record_mapping(&mut self, from_unbalanced: GateIdx<T>, new_rebalanced: GateIdx<T>) {
        self.old_to_new_gate_idx_map
            .insert(from_unbalanced, new_rebalanced);
    }
    #[inline]
    fn balance(&mut self, root_gate_idx: GateIdx<T>, gates: &Arena<Gate<T>>) -> Result<(), BalancingError<T>> {
        let idx = idx_to_usize(root_gate_idx);
        if self.is_rebalanced(idx) {
            return Ok(());
        }

        let mut priority_queue =
            PrioQueue::new(|l: &(GateIdx<T>, isize), r: &(GateIdx<T>, isize)| l.1 < r.1);

        let (bin_op, lhs, rhs) = if let Gate::BinOp(bin_op, lhs, rhs) = gates[root_gate_idx] {
            (bin_op, lhs, rhs)
        } else {
            return Err(BalancingError::BalanceReceivedNoOpGate(root_gate_idx));
        };

        self.ranks[idx] = self.flatten(&mut priority_queue, lhs, gates)?
            + self.flatten(&mut priority_queue, rhs, gates)?;

        let rebalanced_gate_idx = self.rebuild(&mut priority_queue, bin_op, gates)?;
        self.record_mapping(root_gate_idx, rebalanced_gate_idx);

        if self.outputs.contains(&root_gate_idx) {
            self.usages[idx_to_usize(rebalanced_gate_idx)].insert(rebalanced_gate_idx);
        }
        Ok(())
    }
    #[inline]
    fn flatten(
        &mut self,
        priority_queue: &mut PrioQueue<(GateIdx<T>, isize)>,
        gate_idx: GateIdx<T>,
        gates: &Arena<Gate<T>>,
    ) -> Result<isize, BalancingError<T>> {
        let gate = gates[gate_idx];
        let idx = idx_to_usize(gate_idx);
        let rank = match gate {
            Gate::Input(_) => {
                let rank = 1;
                self.ranks[idx] = rank;
                priority_queue.push((gate_idx, rank));
                rank
            }
            Gate::Const(_) => {
                let rank = 0;
                self.ranks[idx] = rank;
                priority_queue.push((gate_idx, rank));
                rank
            }
            Gate::BinOp(_, lhs, rhs) => {
                if self.roots.contains(&gate_idx) {
                    self.balance(gate_idx, gates)?;
                    let rank = self.ranks[idx];
                    priority_queue.push((gate_idx, rank));
                    rank
                } else {
                    self.flatten(priority_queue, lhs, gates)?;
                    self.flatten(priority_queue, rhs, gates)?;
                    self.ranks[idx] = self.ranks[idx_to_usize(lhs)] + self.ranks[idx_to_usize(rhs)];
                    self.ranks[idx]
                }
            }
            Gate::Tombstone => return Err(BalancingError::FlattenReceivedTombstone(gate_idx)),
        };
        Ok(rank)
    }
    #[inline]
    fn get_mapped_index_allocating(&mut self, gate_idx: GateIdx<T>, gates: &Arena<Gate<T>>) -> GateIdx<T> {
        if let Some(mapped) = self.old_to_new_gate_idx_map.get(&gate_idx) {
            *mapped
        } else {
            let from_old = gates[gate_idx];
            self.alloc_with_mapped_idx(from_old, gate_idx)
        }
    }
    #[inline]
    fn alloc_with_mapped_idx(&mut self, gate: Gate<T>, to_be_mapped_to: GateIdx<T>) -> GateIdx<T> {
        let index = self.rebalanced.alloc(gate);
        self.record_mapping(to_be_mapped_to, index);
        index
    }
    #[inline]
    fn get_distinct_index(&mut self) -> GateIdx<T> {
        let d = self.index;
        self.index += 1;
        usize_to_idx(d)
    }
    #[inline]
    fn rebuild(
        &mut self,
        priority_queue: &mut PrioQueue<(GateIdx<T>, isize)>,
        op: BinOp,
        gates: &Arena<Gate<T>>,
    ) -> Result<GateIdx<T>, BalancingError<T>> {
        // Since we are operating on binary operations, we need 2 operands. If not, it means it is
        // the finished root. Since the priority_queue also is formed from a binary operation
        // sub-tree, it cannot start with a single element.
        while priority_queue.len() > 1 {
            let (lhs, lhs_rank) = priority_queue.pop().ok_or_else(|| {
                BalancingError::RebuildReceivedNoOpSubtree(String::from(
                    "priority_queue to have a lhs",
                ))
            })?;
            let (rhs, rhs_rank) = priority_queue.pop().ok_or_else(|| {
                BalancingError::RebuildReceivedNoOpSubtree(String::from(
                    "priority_queue to have a rhs",
                ))
            })?;

            let lhs_index = self.get_mapped_index_allocating(lhs, gates);
            let rhs_index = self.get_mapped_index_allocating(rhs, gates);

            let mut new_gate = Gate::BinOp(op, lhs_index, rhs_index);
            // Fold 2 constants in place
            let rank = if lhs_rank == 0 && rhs_rank == 0 {
                new_gate = fold(new_gate, &self.rebalanced, self.plain_text_modulus);
                0
            } else {
                lhs_rank + rhs_rank
            };
            let distinct_index = self.get_distinct_index();
            let rebalanced_idx = self.alloc_with_mapped_idx(new_gate, distinct_index);
            // NOTE: by not adding any usages to lhs and rhs, we are effectively pruning unused
            // gates. At the end, we form the new arena by looping over the usages, an unused
            // gate, will not be transferred to the new arena.
            if rank != 0 {
                self.usages[idx_to_usize(lhs_index)].insert(rebalanced_idx);
                self.usages[idx_to_usize(rhs_index)].insert(rebalanced_idx);
            }
            priority_queue.push((distinct_index, rank));
        }
        if priority_queue.is_empty() {
            return Err(BalancingError::RebuildFinalisedWithEmptyPrioQueue);
        }
        let (root_gate_idx, _) = priority_queue
            .pop()
            .expect("priority_queue to have 1 element");
        Ok(self.old_to_new_gate_idx_map[&root_gate_idx])
    }
}

#[inline]
fn is_root<T: Supported>(
    gate_idx: GateIdx<T>,
    op: BinOp,
    usages: &[FxHashSet<GateIdx<T>>],
    gates: &Arena<Gate<T>>,
) -> bool {
    let idx = idx_to_usize(gate_idx);
    if usages[idx].len() > 1 {
        return true;
    }
    if let Some(only_usage_idx) = usages[idx].iter().next()
        && let Gate::BinOp(usage_op, _, _) = gates[*only_usage_idx]
    {
        return op != usage_op;
    }
    false
}

pub fn rebalance<T: Supported>(circuit: &Circuit<T>) -> BalacingResult<T> {
    let (mut roots_wrt_op_precedence, root_set) = {
        let gates_len = circuit.gates().len();
        let mut usages = thin_vec![FxHashSet::with_hasher(FxBuildHasher::default()); gates_len];
        let mut operation_gates = ThinVec::<GateIdx<T>>::new();

        for (gate_idx, gate) in circuit.gates().iter() {
            if let Gate::BinOp(_, lhs_idx, rhs_idx) = gate {
                operation_gates.push(gate_idx);
                // Update the operands usages by adding this gate to the set.
                for operand in [lhs_idx, rhs_idx] {
                    let operand_idx = idx_to_usize(*operand);
                    usages[operand_idx].insert(gate_idx);
                }
            }
        }

        let mut roots_wrt_precedence =
            PrioQueue::new(|l: &(GateIdx<T>, usize), r: &(GateIdx<T>, usize)| l.1 > r.1);
        let mut roots: FxHashSet<GateIdx<T>> = FxHashSet::with_hasher(FxBuildHasher::default());
        for op_gate_idx in operation_gates {
            let op_gate = circuit.gates()[op_gate_idx];
            if let Gate::BinOp(op, _, _) = op_gate
                && (is_root(op_gate_idx, op, usages.as_slice(), circuit.gates())
                    || circuit.outputs().contains(&op_gate_idx))
            {
                roots_wrt_precedence.push((op_gate_idx, op.precedence()));
                roots.insert(op_gate_idx);
            }
        }
        (roots_wrt_precedence, roots)
    };

    let mut rebalance_pass = RebalancePass::new(circuit, root_set);
    while !roots_wrt_op_precedence.is_empty() {
        let (root, _) = roots_wrt_op_precedence.pop().expect("to have a root");
        rebalance_pass.balance(root, circuit.gates())?;
    }

    new_circuit_from(circuit, rebalance_pass.rebalanced, &rebalance_pass.usages)
        .map_err(BalancingError::PropagationFailure)
}

#[cfg(test)]
mod test {

    use super::*;

    use la_arena::Arena;
    use parameterized_test::create;

    fn assert_supported<T: Supported>() {}

    #[test]
    fn all_supported_integer_types_work_for_rebalance() {
        assert_supported::<i8>();
        assert_supported::<i16>();
        assert_supported::<i32>();
        assert_supported::<i64>();
        assert_supported::<u8>();
        assert_supported::<u16>();
        assert_supported::<u32>();
        assert_supported::<u64>();
    }

    create! {
        tree_rebalancing_reassociate,
        (gates, inputs, outputs, expected_gates, expected_inputs, expected_outputs), {
            let idx_inputs : ThinVec<GateIdx<u64>> = inputs.iter().map(|i: &usize| usize_to_idx(*i)).collect();
            let idx_outputs : ThinVec<GateIdx<u64>> = outputs.iter().map(|i: &usize| usize_to_idx(*i)).collect();
            let q = 11u64;

            let circuit = Circuit::with(q, Arena::from_iter(gates.iter().cloned()), idx_inputs, idx_outputs);

            let new_circuit = rebalance(&circuit).expect("rebalance to finalise without errors");
            assert_eq!(
                &Arena::from_iter(expected_gates.iter().cloned()),
                new_circuit.gates()
            );
            let expected_inputs: ThinVec<GateIdx<u64>> = ThinVec::from_iter(expected_inputs.iter().cloned().map(usize_to_idx));
            assert_eq!(expected_inputs, new_circuit.inputs());
            let expected_outputs: ThinVec<GateIdx<u64>> = ThinVec::from_iter(expected_outputs.iter().cloned().map(usize_to_idx));
            assert_eq!(expected_outputs, new_circuit.outputs());
        }
    }

    tree_rebalancing_reassociate! {
        /*
        * let output = a+b+c+d;
        *
        * becomes
        * let x = a+b;
        * let y = c+d;
        * let z = x+y;
        * NOTE: order switches due to priority_queue mechanics
        */
        left_heavy_rewrite: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::Input(3),                                            // d
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                Gate::BinOp(BinOp::Add, usize_to_idx(4), usize_to_idx(2)), // (a+b)+c
                Gate::BinOp(BinOp::Add, usize_to_idx(5), usize_to_idx(3)), // ((a+b)+c)+d
            ],
            &[0,1,2,3],
            &[6],
            &[
                Gate::Input(0),                                            // a
                Gate::Input(3),                                            // b
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                Gate::Input(2),                                            // c
                Gate::Input(1),                                            // d
                Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(4)), // c+d
                Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(5)), // (a+b) + (c+d)
            ],
            &[0,1,3,4],
            &[6],
        ),
        folding_constants_rewrite: (
            &[
                Gate::Const(5u64),
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(0)),
            ],
            &[],
            &[1],
            &[
                Gate::Const(10u64),                                         // folded 5+5
            ],
            &[],
            &[0],
        ),
        /*
        * let output = d+(c+(a+b));
        *
        * becomes
        * let x = d+b;
        * let y = a+c;
        * let z = x+y;
        * NOTE: order switches due to priority_queue mechanics
        */
        right_heavy_scattered_constants_rewrite: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::Input(3),                                            // d
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(4)), // (a+b)+c
                Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(5)), // ((a+b)+c)+d
            ],
            &[0,1,2,3],
            &[6],
            &[
                Gate::Input(3),                                            // d
                Gate::Input(1),                                            // b
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // b+d
                Gate::Input(0),                                            // c
                Gate::Input(2),                                            // d
                Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(4)), // a+c
                Gate::BinOp(BinOp::Add, usize_to_idx(2), usize_to_idx(5)), // (b+d) + (a+c)
            ],
            &[0,1,3,4],
            &[6],
        ),
        /*
        * let output = a+b+5+c+d+5;
        *
        * becomes
        * let f = 10;
        * let x = b+f;
        * let y = x+c;
        * let z = a+d;
        * let output = y+z;
        */
        left_heavy_scattered_constants_rewrite: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::Input(3),                                            // d
                Gate::Const(5u64),
                /*5*/Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // a+b
                /*6*/Gate::BinOp(BinOp::Add, usize_to_idx(5), usize_to_idx(4)), // (a+b)+5
                /*7*/Gate::BinOp(BinOp::Add, usize_to_idx(6), usize_to_idx(2)), // ((a+b)+5)+c
                /*8*/Gate::BinOp(BinOp::Add, usize_to_idx(7), usize_to_idx(3)), // (((a+b)+5)+c)+d
                /*9*/Gate::BinOp(BinOp::Add, usize_to_idx(8), usize_to_idx(4)), // ((((a+b)+5)+c)+d)+5
            ],
            &[0,1,2,3],
            &[9],
            &[
                Gate::Const(10u64),                                         // folded
                Gate::Input(1),                                            // b
                Gate::BinOp(BinOp::Add, usize_to_idx(0), usize_to_idx(1)), // b+10
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Add, usize_to_idx(3), usize_to_idx(2)), // (b+10)+c
                Gate::Input(0),                                            // a
                Gate::Input(3),                                            // d
                Gate::BinOp(BinOp::Add, usize_to_idx(6), usize_to_idx(5)), // a+d
                Gate::BinOp(BinOp::Add, usize_to_idx(7), usize_to_idx(4)), // (a+d) + ((b+10)+c)
            ],
            &[1,3,5,6],
            &[8],
        ),
        /*
        * let output = a*b*5*c*d*5;
        *
        * becomes
        * let f = 3;  // 25 % 11 = 3
        * let x = b*f;
        * let y = x*c;
        * let z = a*d;
        * let output = y*z;
        */
        left_heavy_scattered_constants_rewrite_mul: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::Input(3),                                            // d
                Gate::Const(5u64),
                /*5*/Gate::BinOp(BinOp::Mul, usize_to_idx(0), usize_to_idx(1)), // a*b
                /*6*/Gate::BinOp(BinOp::Mul, usize_to_idx(5), usize_to_idx(4)), // (a*b)*5
                /*7*/Gate::BinOp(BinOp::Mul, usize_to_idx(6), usize_to_idx(2)), // ((a*b)*5)*c
                /*8*/Gate::BinOp(BinOp::Mul, usize_to_idx(7), usize_to_idx(3)), // (((a*b)*5)*c)*d
                /*9*/Gate::BinOp(BinOp::Mul, usize_to_idx(8), usize_to_idx(4)), // ((((a*b)*5)*c)*d)*5
            ],
            &[0,1,2,3],
            &[9],
            &[
                Gate::Const(3u64),                                          // folded
                Gate::Input(1),                                            // b
                Gate::BinOp(BinOp::Mul, usize_to_idx(0), usize_to_idx(1)), // b*3
                Gate::Input(2),                                            // c
                Gate::BinOp(BinOp::Mul, usize_to_idx(3), usize_to_idx(2)), // (b*10)*c
                Gate::Input(0),                                            // a
                Gate::Input(3),                                            // d
                Gate::BinOp(BinOp::Mul, usize_to_idx(6), usize_to_idx(5)), // a*d
                Gate::BinOp(BinOp::Mul, usize_to_idx(7), usize_to_idx(4)), // (a*d) * ((b*10)*c)
            ],
            &[1,3,5,6],
            &[8],
        ),
        /*
        * let output = a*5*b*5+c+d+e;
        *
        * becomes
        * let f = 3;  // 25 % 11 = 3
        * let x = b*f;
        * let y = x*a;
        * let z1 = y+c;
        * let z2 = d+e;
        * let output = z1+z2;
        */
        left_heavy_scattered_constants_rewrite_add_mul: (
            &[
                Gate::Input(0),                                            // a
                Gate::Input(1),                                            // b
                Gate::Input(2),                                            // c
                Gate::Input(3),                                            // d
                Gate::Input(4),                                            // e
                Gate::Const(5u64),

                /*6*/Gate::BinOp(BinOp::Mul, usize_to_idx(0), usize_to_idx(5)), // a*5
                /*7*/Gate::BinOp(BinOp::Mul, usize_to_idx(6), usize_to_idx(1)), // (a*5)*b
                /*8*/Gate::BinOp(BinOp::Mul, usize_to_idx(7), usize_to_idx(5)), // ((a*5)*b)*5

                /*9*/Gate::BinOp(BinOp::Add, usize_to_idx(8), usize_to_idx(2)), // ((a*5)*b)*5+c
                /*10*/Gate::BinOp(BinOp::Add, usize_to_idx(9), usize_to_idx(3)),
            //(((a*5)*b)*5+c)+d
                /*11*/Gate::BinOp(BinOp::Add, usize_to_idx(10), usize_to_idx(4)),
            //((((a*5)*b)*5+c)+d)+e
            ],
            &[0,1,2,3,4],
            &[11],
            &[
                Gate::Const(3u64),                                          // folded
                Gate::Input(1),                                            // b
                Gate::BinOp(BinOp::Mul, usize_to_idx(0), usize_to_idx(1)), // b*3
                Gate::Input(0),                                            // a
                Gate::BinOp(BinOp::Mul, usize_to_idx(3), usize_to_idx(2)), // (b*3)*a
                Gate::Input(2),                                            // c
                Gate::Input(3),                                            // d
                Gate::BinOp(BinOp::Add, usize_to_idx(5), usize_to_idx(6)), // (c+d)
                Gate::Input(4),                                            // e
                Gate::BinOp(BinOp::Add, usize_to_idx(8), usize_to_idx(7)), // (c+d)+e
                Gate::BinOp(BinOp::Add, usize_to_idx(9), usize_to_idx(4)), // (b*3)*a) + ((c+d)+e)
            ],
            &[1,3,5,6, 8],
            &[10],
        ),
    }
}
