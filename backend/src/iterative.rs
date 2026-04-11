use common::idx_to_usize;
use fxhash::{FxBuildHasher, FxHashMap};
use ir::Supported;
use ir::circuit::Circuit;
use ir::gate::{Gate, GateIdx};
use op::BinOp;
use thin_vec::ThinVec;

use crate::validation::validate_same_length;
use crate::{Backend, NoisyElement};
use crate::{BackendResult, noise::Noise};

pub(crate) fn eval_with_iterative_traversal<E, N, S, B>(
    circuit: &Circuit<S>,
    backend: &mut B,
    with: &[S],
) -> BackendResult<ThinVec<B::Elem>>
where
    E: NoisyElement,
    N: Noise,
    S: Supported,
    B: Backend<S, Elem = E>,
{
    validate_same_length(circuit, with)?;
    backend
        .set_plain_text_modulus(circuit.get_plain_text_modulus())
        .expect("circuit to have non-zero modulus");

    let mut roots = circuit.outputs().iter();
    let mut dfs_stack = ThinVec::new();

    let mut current_node = roots.next().copied();
    let mut gate_idx_to_elem: FxHashMap<GateIdx<S>, E> =
        FxHashMap::with_hasher(FxBuildHasher::default());

    // Iterative post order traversal from each output node eliminates all the
    // unused/unreachable Exprs since it only follows roots of outputs.
    loop {
        // If we have a node at hand, take it and start lowering.
        if let Some(current_gate_idx) = current_node.take() {
            if gate_idx_to_elem.contains_key(&current_gate_idx) {
                continue;
            }
            let gate = circuit.gates()[current_gate_idx];
            let element = match gate {
                ir::gate::Gate::Tombstone => continue,
                Gate::Input(index) => backend.input(with[index]),
                Gate::Const(constant) => backend.constant(constant),
                // Here, if we haven't already, we push children into the stack to first evaluate them, (post-order)
                // or we retrieve their evaluated elements form the map.
                Gate::BinOp(bin_op, lhs, rhs) => {
                    let lhs_elem_opt = gate_idx_to_elem.get(&lhs);
                    let rhs_elem_opt = gate_idx_to_elem.get(&rhs);

                    // We want the visit order to be lhs, rhs and then parent so that we can form the
                    // gate for operation with evaluated children. If they are not evaluated yet when we are at the parent
                    // (first time while DFSing), we push the parent to the stack again (we popped it from the stack and took the current_gate_idx),
                    // then the unevaluated ones, so that we visit them first.
                    // TLDR: we want to ensure the order in the stack:
                    // [<current op>, <left child if not evaluated>, <right child if not evaluated>]
                    let mut rhs_gate_idx = None;
                    // if the rhs child is not evaluated yet, reserve it to push into the stack
                    if rhs_elem_opt.is_none() {
                        rhs_gate_idx = Some(rhs);
                    }
                    // if the lhs child is not evaluated yet, move current_gate_idx to lhs, if rhs is already evaluated,
                    // push the parent on the stack again and continue; or move on to pushing
                    // rhs and parent in the stack.
                    if lhs_elem_opt.is_none() {
                        current_node = Some(lhs);
                        if rhs_gate_idx.is_none() {
                            dfs_stack.push(current_gate_idx);
                            continue;
                        }
                    }
                    // If we rhs child to evaluate, we reinsert the parent operation
                    // first, and then add the child to the stack to visit rhs before parent.
                    if let Some(push) = rhs_gate_idx {
                        dfs_stack.extend_from_slice(&[current_gate_idx, push]);
                        continue;
                    }

                    // At this point, lhs and rhs children are all evaluated, we evaluate the
                    // operation with their gate indices.
                    let lhs_result = lhs_elem_opt.unwrap();
                    let rhs_result = rhs_elem_opt.unwrap();

                    match bin_op {
                        BinOp::Add => backend.add(lhs_result, rhs_result),
                        BinOp::Sub => backend.sub(lhs_result, rhs_result),
                        BinOp::Mul => backend.mul(lhs_result, rhs_result),
                    }
                }
            };
            backend
                .get_noise_mul()
                .update(element.get_noise(), idx_to_usize(current_gate_idx))?;
            gate_idx_to_elem.insert(current_gate_idx, element);
        }
        // If stack is empty, we consumed all the sub-tree of the current node, thus we
        // retrieve next if any.
        else if dfs_stack.is_empty() {
            if let Some(expr_idx) = roots.next().copied() {
                dfs_stack.push(expr_idx);
            } else {
                break;
            }
        }
        // If we don't have a node at hand, we first try the stack.
        // The current_node is already evaluated, so we continue consuming the stack.
        else if current_node.is_none() {
            current_node = dfs_stack.pop();
        }
        // The stack and roots are consumed, we stop the dfs.
        else {
            break;
        }
    }

    let mut result = ThinVec::with_capacity(circuit.outputs().len());
    for out_idx in circuit.outputs() {
        let element = gate_idx_to_elem.remove(out_idx).unwrap();
        result.push(element);
    }
    BackendResult::Ok(result)
}
