use ir::{
    SupportedType,
    circuit::Circuit,
    gate::{Gate, GateIdx},
};

use fxhash::FxBuildHasher;
use la_arena::Arena;
use thin_vec::ThinVec;

use std::collections::HashMap;

use crate::{
    ctx::ContextHandle,
    expr::{Expr, ExprHandle, ExprIdx},
};

type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

#[derive(Clone, Debug)]
struct CircuitCompiler {
    q: SupportedType,
    context_handle: ContextHandle,
    pub gates: Arena<Gate>,
    pub inputs: ThinVec<GateIdx>,
    pub outputs: ThinVec<GateIdx>,
    expr_idx_to_gate_idx: FxHashMap<ExprIdx, GateIdx>,
}

impl CircuitCompiler {
    pub fn with(context_handle: ContextHandle) -> Self {
        let q = context_handle.0.borrow().q;
        let expr_idx_to_gate_idx = HashMap::with_hasher(FxBuildHasher::default());
        Self {
            q,
            context_handle,
            gates: Arena::new(),
            inputs: ThinVec::new(),
            outputs: ThinVec::new(),
            expr_idx_to_gate_idx,
        }
    }
    fn put_in_gates(&mut self, gate: Gate) -> GateIdx {
        self.gates.alloc(gate)
    }
    fn mark_as_input(&mut self, gate_index: GateIdx) {
        self.inputs.push(gate_index);
    }
    fn pair(&mut self, expr_index: ExprIdx, gate_index: GateIdx) {
        self.expr_idx_to_gate_idx.insert(expr_index, gate_index);
    }
    fn is_lowered(&self, expr_index: &ExprIdx) -> bool {
        self.expr_idx_to_gate_idx.contains_key(expr_index)
    }
    fn get_lowered(&self, expr_index: &ExprIdx) -> Option<&GateIdx> {
        self.expr_idx_to_gate_idx.get(expr_index)
    }

    fn build_from(mut self, outputs: &[ExprHandle]) -> Circuit {
        let mut roots = outputs.iter();
        let mut dfs_stack = ThinVec::new();

        // iterative post order traversal
        loop {
            // either we didn't start from a root, or consumed it. Get the next.
            if dfs_stack.is_empty() {
                if let Some(expr_handle) = roots.next() {
                    let expr = expr_handle.get_expr();
                    let expr_idx = expr_handle.idx;
                    dfs_stack.push((expr_idx, expr));
                } else {
                    break;
                }
            } else if let Some((expr_idx, expr)) = dfs_stack.pop() {
                if self.is_lowered(&expr_idx) {
                    continue;
                }
                let (gate, is_input) = match expr {
                    Expr::Input(index) => {
                        let gate = Gate::Input(index);
                        (gate, true)
                    }
                    Expr::Const(value) => {
                        let gate = Gate::Const(value);
                        (gate, false)
                    }
                    // From here, we push the children into the stack to first lower them, (post-order)
                    // if we haven't already; or we retrieve their gate indices and form the op
                    // gate.
                    Expr::Add(lhs, rhs) | Expr::Sub(lhs, rhs) | Expr::Mul(lhs, rhs) => {
                        let lhs_gate_idx_opt = self.get_lowered(&lhs);
                        let rhs_gate_idx_opt = self.get_lowered(&rhs);

                        // We want to first visit the left child, and then right and then form the
                        // gate for operation with them. If we have not pushed them already, since
                        // we popped the element at hand right now, we have to ensure the order in
                        // the stack [<current op>, <left child>, <right child>]
                        let mut to_push = ThinVec::new();
                        if rhs_gate_idx_opt.is_none() {
                            let rhs_expr = self.context_handle.get(rhs);
                            to_push.push((rhs, rhs_expr));
                        }
                        if lhs_gate_idx_opt.is_none() {
                            let lhs_expr = self.context_handle.get(lhs);
                            to_push.push((lhs, lhs_expr));
                        }
                        // If we have children to compile to, we reinsert the parent operation
                        // first, and then add the children to the stack.
                        if !to_push.is_empty() {
                            dfs_stack.push((expr_idx, expr));
                            dfs_stack.extend_from_slice(to_push.as_slice());
                            continue;
                        }

                        let lhs_gate_idx = lhs_gate_idx_opt.unwrap();
                        let rhs_gate_idx = rhs_gate_idx_opt.unwrap();
                        let gate = if matches!(expr, Expr::Add(_, _)) {
                            Gate::Add(*lhs_gate_idx, *rhs_gate_idx)
                        } else if matches!(expr, Expr::Sub(_, _)) {
                            Gate::Sub(*lhs_gate_idx, *rhs_gate_idx)
                        } else {
                            Gate::Mul(*lhs_gate_idx, *rhs_gate_idx)
                        };
                        (gate, false)
                    }
                };
                let gate_idx = self.put_in_gates(gate);
                if is_input {
                    self.mark_as_input(gate_idx);
                }
                self.pair(expr_idx, gate_idx);
            } else {
                break;
            }
        }
        dbg!(&self);
        Circuit::with(self.q, self.gates, self.inputs, self.outputs)
    }
}

impl ContextHandle {
    pub fn compile(&self, output: ExprHandle) -> Circuit {
        let circuit_builder = CircuitCompiler::with(self.clone());
        circuit_builder.build_from(&[output])
    }
    pub fn compile_many(&self, outputs: &[ExprHandle]) -> Circuit {
        let circuit_builder = CircuitCompiler::with(self.clone());
        circuit_builder.build_from(outputs)
    }
}

#[cfg(test)]
mod tests {

    use la_arena::RawIdx;

    use crate::new_context;

    use super::*;

    fn test_ctx_handle() -> ContextHandle {
        new_context(7)
    }

    fn into_gate_idx(idx: u32) -> GateIdx {
        GateIdx::from_raw(RawIdx::from_u32(idx))
    }

    #[test]
    fn test_single_addition_with_same_constants() {
        let ctx_handle = test_ctx_handle();
        let value = 9;
        let constant_1 = ctx_handle.constant(value);
        let constant_2 = ctx_handle.constant(value);
        let out = constant_1 + constant_2;

        let expected_length = 2;

        let circuit = ctx_handle.compile(out);

        assert_eq!(expected_length, circuit.gates().len());
        assert_eq!(0, circuit.inputs().len());
        assert_eq!(0, circuit.outputs().len());

        let const_gate_idx = into_gate_idx(0);
        assert_eq!(Gate::Const(value), circuit.gates()[const_gate_idx]);

        let add_gate_idx = into_gate_idx(1);
        assert_eq!(
            Gate::Add(const_gate_idx, const_gate_idx),
            circuit.gates()[add_gate_idx]
        );
    }

    #[test]
    fn test_same_double_addition_and_multiplication() {
        let ctx_handle = test_ctx_handle();
        let value = 9;
        let constant_1 = ctx_handle.constant(value);
        let constant_2 = ctx_handle.constant(value);
        let addition = constant_1 + constant_2;
        let out = &addition * &addition;

        let expected_length = 3;

        let circuit = ctx_handle.compile(out);

        assert_eq!(expected_length, circuit.gates().len());
        assert_eq!(0, circuit.inputs().len());
        assert_eq!(0, circuit.outputs().len());

        let const_gate_idx = into_gate_idx(0);
        assert_eq!(Gate::Const(value), circuit.gates()[const_gate_idx]);

        let add_gate_idx = into_gate_idx(1);
        assert_eq!(
            Gate::Add(const_gate_idx, const_gate_idx),
            circuit.gates()[add_gate_idx]
        );

        let mul_gate_idx = into_gate_idx(2);
        assert_eq!(
            Gate::Mul(add_gate_idx, add_gate_idx),
            circuit.gates()[mul_gate_idx]
        );
    }

    #[test]
    fn test_different_double_addition_and_multiplication() {
        let ctx_handle = test_ctx_handle();

        let values = [1, 2, 3, 4];
        let constant_1 = ctx_handle.constant(values[0]);
        let constant_2 = ctx_handle.constant(values[1]);
        let addition_1 = constant_1 + constant_2;

        let constant_3 = ctx_handle.constant(values[2]);
        let constant_4 = ctx_handle.constant(values[3]);
        let addition_2 = constant_3 + constant_4;
        let out = &addition_1 * &addition_2;

        let expected_length = 7;

        let circuit = ctx_handle.compile(out);

        assert_eq!(expected_length, circuit.gates().len());
        assert_eq!(0, circuit.inputs().len());
        assert_eq!(0, circuit.outputs().len());

        for (val_ix, const_idx) in [0, 1, 3, 4].iter().enumerate() {
            let const_gate_idx = into_gate_idx(*const_idx);
            assert_eq!(Gate::Const(values[val_ix]), circuit.gates()[const_gate_idx]);
        }

        let add_gate_idx = into_gate_idx(2);
        let const_1_gate_idx = into_gate_idx(0);
        let const_2_gate_idx = into_gate_idx(1);

        assert_eq!(
            Gate::Add(const_1_gate_idx, const_2_gate_idx),
            circuit.gates()[add_gate_idx]
        );

        let add_gate_idx_2 = into_gate_idx(5);
        let const_3_gate_idx = into_gate_idx(3);
        let const_4_gate_idx = into_gate_idx(4);

        assert_eq!(
            Gate::Add(const_3_gate_idx, const_4_gate_idx),
            circuit.gates()[add_gate_idx_2]
        );

        let mul_gate_idx = into_gate_idx(6);
        assert_eq!(
            Gate::Mul(add_gate_idx, add_gate_idx_2),
            circuit.gates()[mul_gate_idx]
        );
    }
}
