use enum_iterator::all;
use ir::{
    SupportedType,
    circuit::Circuit,
    gate::{Gate, GateIdx},
};

use fxhash::FxBuildHasher;
use la_arena::{Arena, Idx, RawIdx};
use thin_vec::ThinVec;

use std::collections::HashMap;

use crate::{
    compilation_mode::{CompilationMode, Strictness, StrictnessOn},
    ctx::ContextHandle,
    expr::{Expr, ExprHandle, ExprIdx},
};

#[derive(Clone, Debug)]
pub struct CompilationError {
    pub unused_inputs: ThinVec<Expr>,
    pub unused_constants: ThinVec<Expr>,
    pub unused_operations: ThinVec<Expr>,
}

pub type CompilationResult = Result<Circuit, CompilationError>;

impl ContextHandle {
    pub fn compile(&self, output: ExprHandle) -> CompilationResult {
        let circuit_builder = CircuitCompiler::with(self.clone());
        circuit_builder.build_from(&[output])
    }
    pub fn compile_many(&self, outputs: &[ExprHandle]) -> CompilationResult {
        let circuit_builder = CircuitCompiler::with(self.clone());
        circuit_builder.build_from(outputs)
    }
}

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
    /// Gathers all of the unused Expressions and returns error on them if any.
    fn apply_mode(&self) -> Result<(), CompilationError> {
        let ctx = self.context_handle.0.borrow();
        if matches!(ctx.mode, CompilationMode::Loose) {
            return Ok(());
        }
        let mut unused = ctx.create_set_of_all_indices();
        for expr_idx in self.expr_idx_to_gate_idx.keys() {
            let expr_u32 = expr_idx.into_raw().into_u32();
            unused.remove(expr_u32 as usize);
        }

        if unused.is_empty() {
            return Ok(());
        }

        let mut unused_inputs = ThinVec::new();
        let mut unused_constants = ThinVec::new();
        let mut unused_operations = ThinVec::new();

        // Accumulate all unused with their respective variants to report wholistically at once.
        for idx in unused.iter() {
            let expr_idx = Idx::from_raw(RawIdx::from_u32(idx as u32));
            let expr = self.context_handle.get(expr_idx);
            let to_push_in = match &expr {
                Expr::Input(_) => &mut unused_inputs,
                Expr::Const(_) => &mut unused_constants,
                Expr::BinOp(_, _, _) => &mut unused_operations,
            };
            to_push_in.push(expr);
        }

        let strictness_mode: Strictness = Strictness::from(&ctx.mode);
        let mut fucked_it_up = false;

        for strictness_on in all::<StrictnessOn>() {
            let strict: Strictness = (&strictness_on).into();
            let is_strict = &strictness_mode & &strict;
            if !is_strict {
                continue;
            }
            fucked_it_up = match strictness_on {
                StrictnessOn::Input if !unused_inputs.is_empty() => true,
                StrictnessOn::Const if !unused_constants.is_empty() => true,
                StrictnessOn::Op if !unused_operations.is_empty() => true,
                _ => continue,
            };
            break;
        }
        if !fucked_it_up {
            return Ok(());
        }

        Err(CompilationError {
            unused_inputs,
            unused_constants,
            unused_operations,
        })
    }
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

    fn build_from(mut self, outputs: &[ExprHandle]) -> CompilationResult {
        let mut roots = outputs.iter();
        let mut output_gate_indices = ThinVec::new();
        let mut dfs_stack = ThinVec::new();

        let into_expr_idx = |h: &ExprHandle| h.idx;

        let mut current_node = roots.next().map(into_expr_idx);
        output_gate_indices.push(current_node.unwrap());

        // Iterative post order traversal from each output node eliminates all the
        // unused/unreachable Exprs since it only follows roots of outputs.
        loop {
            // If we have a node at hand, take it and start lowering.
            if let Some(current_expr_idx) = current_node.take() {
                if self.is_lowered(&current_expr_idx) {
                    continue;
                }
                let expr = self.context_handle.get(current_expr_idx);
                let (gate, is_input) = match expr {
                    Expr::Input(index) => {
                        let gate = Gate::Input(index);
                        (gate, true)
                    }
                    Expr::Const(value) => {
                        let gate = Gate::Const(value);
                        (gate, false)
                    }
                    // Here, if we haven't already, we push children into the stack to first lower them, (post-order)
                    // or we retrieve their gate indices to form the op gate.
                    Expr::BinOp(bin_op, lhs, rhs) => {
                        let lhs_gate_idx_opt = self.get_lowered(&lhs);
                        let rhs_gate_idx_opt = self.get_lowered(&rhs);

                        // We want the visit order to be lhs, rhs and then parent so that we can form the
                        // gate for operation with lowered children. If they are not lowered yet when we are at the parent
                        // (first time while DFSing), we push the parent to the stack again (we popped it from the stack and took the root_node),
                        // then the unlowered ones, so that we visit them first.
                        // TLDR: we want to ensure the order in the stack:
                        // [<current op>, <left child if not lowered>, <right child if not lowered>]
                        let mut rhs_child_expr_idx = None;
                        // if the rhs child is not lowered yet, reserve it to push into the stack
                        if rhs_gate_idx_opt.is_none() {
                            rhs_child_expr_idx = Some(rhs);
                        }
                        // if the lhs child is not lowered yet, move root_node to lhs, if rhs is already lowered,
                        // push the parent on the stack again and continue; or move on to pushing
                        // rhs and parent in the stack.
                        if lhs_gate_idx_opt.is_none() {
                            current_node = Some(lhs);
                            if rhs_child_expr_idx.is_none() {
                                dfs_stack.push(current_expr_idx);
                                continue;
                            }
                        }
                        // If we rhs child to lower, we reinsert the parent operation
                        // first, and then add the child to the stack to visit rhs before parent.
                        if let Some(push) = rhs_child_expr_idx {
                            dfs_stack.extend_from_slice(&[current_expr_idx, push]);
                            continue;
                        }

                        // At this point, lhs and rhs childen are all lowered, we lower the
                        // operation with their gate indices.
                        let lhs_gate_idx = lhs_gate_idx_opt.unwrap();
                        let rhs_gate_idx = rhs_gate_idx_opt.unwrap();

                        let gate = Gate::BinOp(bin_op, *lhs_gate_idx, *rhs_gate_idx);
                        (gate, false)
                    }
                };
                let gate_idx = self.put_in_gates(gate);
                if is_input {
                    self.mark_as_input(gate_idx);
                }
                self.pair(current_expr_idx, gate_idx);
            }
            // If stack is empty, we consumed all the sub-tree of the current node, thus we
            // retrieve next if any.
            else if dfs_stack.is_empty() {
                if let Some(expr_idx) = roots.next().map(into_expr_idx) {
                    dfs_stack.push(expr_idx);
                    output_gate_indices.push(expr_idx);
                } else {
                    break;
                }
            }
            // If we don't have a node at hand, we first try the stack.
            // The current_node is already lowered, so we continue consuming the stack.
            else if current_node.is_none() {
                current_node = dfs_stack.pop();
            }
            // The stack and roots are consumed, we stop the lowering.
            else {
                break;
            }
        }

        self.apply_mode()?;

        // Push the lowered root(s) into outputs list.
        for out_expr_idx in output_gate_indices.iter() {
            let current_output_gate_idx = self
                .get_lowered(out_expr_idx)
                .expect("output to be lowered");
            self.outputs.push(*current_output_gate_idx);
        }

        dbg!(&self);

        Ok(Circuit::with(self.q, self.gates, self.inputs, self.outputs))
    }
}

#[cfg(test)]
mod tests {

    use parameterized_test::create;
    use thin_vec::thin_vec;

    use la_arena::RawIdx;
    use op::BinOp;

    use crate::{
        compilation_mode::CompilationMode, new_folding_strict_context, new_loose_context,
        new_strict_context,
    };

    use super::*;

    fn test_ctx_handle() -> ContextHandle {
        new_strict_context(7)
    }

    fn test_loose_ctx_handle() -> ContextHandle {
        new_loose_context(5)
    }

    fn test_folding_ctx_handle() -> ContextHandle {
        new_folding_strict_context(11)
    }

    fn into_gate_idx(idx: u32) -> GateIdx {
        GateIdx::from_raw(RawIdx::from_u32(idx))
    }

    #[test]
    fn test_single_addition_with_same_constants_loose_also_folds() {
        let ctx_handle = test_loose_ctx_handle();
        let value = 9;
        let constant_1 = ctx_handle.constant(value);
        let constant_2 = ctx_handle.constant(value);
        let out = constant_1 + constant_2;

        let expected = value * 2 % ctx_handle.0.borrow().q;

        // (Add, lhs_constant, rhs_constant) -> will fold to Const(lhs_constant + rhs_constant)
        let expected_length = 1;

        let circuit = ctx_handle.compile(out).expect("to compile");

        assert_eq!(expected_length, circuit.gates().len());
        assert_eq!(0, circuit.inputs().len());
        assert_eq!(1, circuit.outputs().len());

        let const_gate_idx = into_gate_idx(0);
        assert_eq!(Gate::Const(expected), circuit.gates()[const_gate_idx]);
    }

    #[test]
    fn test_single_addition_with_same_constants_strict_also_folds() {
        let ctx_handle = test_ctx_handle();
        let value = 9;
        let constant_1 = ctx_handle.constant(value);
        let constant_2 = ctx_handle.constant(value);
        let out = constant_1 + constant_2;

        // (Add, lhs_constant, rhs_constant) -> will fold to Const(lhs_constant + rhs_constant)
        // but lhs_constant and rhs_constant will be left behind as orphans thus will fail strict
        // compilation.
        let error = ctx_handle
            .compile(out)
            .expect_err("to fail compilation due to orphan const");

        assert!(error.unused_inputs.is_empty());
        assert_eq!(1, error.unused_constants.len());
        assert!(error.unused_operations.is_empty());
    }

    #[test]
    fn test_single_addition_with_same_constants_folding_strict_folds_leaves_orphan_behind() {
        let ctx_handle = test_folding_ctx_handle();
        let value = 9;
        let constant_1 = ctx_handle.constant(value);
        let constant_2 = ctx_handle.constant(value);
        let out = constant_1 + constant_2;

        let expected = value * 2 % ctx_handle.0.borrow().q;

        // (Add, lhs_constant, rhs_constant) -> will fold to Const(lhs_constant + rhs_constant)
        let expected_length = 1;

        // This time tough, it will compile because of Strictness flags allow orphan constants
        let circuit = ctx_handle.compile(out).expect("to compile");

        assert_eq!(expected_length, circuit.gates().len());
        assert_eq!(0, circuit.inputs().len());
        assert_eq!(1, circuit.outputs().len());

        let const_gate_idx = into_gate_idx(0);
        assert_eq!(Gate::Const(expected), circuit.gates()[const_gate_idx]);
    }
    #[test]
    fn test_single_addition_with_constant_and_input_strict() {
        let ctx_handle = test_ctx_handle();
        let value = 9;
        let index = 0;
        let constant = ctx_handle.constant(value);
        let input = ctx_handle.input(index);
        let out = constant + input;

        let expected_length = 3;

        let circuit = ctx_handle.compile(out).expect("to compile");

        assert_eq!(expected_length, circuit.gates().len());
        assert_eq!(1, circuit.inputs().len());
        assert_eq!(1, circuit.outputs().len());

        let const_gate_idx = into_gate_idx(0);
        assert_eq!(Gate::Const(value), circuit.gates()[const_gate_idx]);

        let input_gate_idx = into_gate_idx(1);
        assert_eq!(Gate::Input(index), circuit.gates()[input_gate_idx]);

        let add_gate_idx = into_gate_idx(2);
        assert_eq!(
            Gate::BinOp(BinOp::Add, const_gate_idx, input_gate_idx),
            circuit.gates()[add_gate_idx]
        );
    }

    #[test]
    fn test_same_double_addition_and_multiplication_strict() {
        let ctx_handle = test_ctx_handle();
        let index = 0;
        let value = 9;
        let input = ctx_handle.input(index);
        let constant = ctx_handle.constant(value);
        let addition = input + constant;
        let out = &addition * &addition;

        let expected_length = 4;

        let circuit = ctx_handle.compile(out).expect("to compile");

        assert_eq!(expected_length, circuit.gates().len());
        assert_eq!(1, circuit.inputs().len());
        assert_eq!(1, circuit.outputs().len());

        let input_gate_idx = into_gate_idx(0);
        assert_eq!(Gate::Input(index), circuit.gates()[input_gate_idx]);

        let const_gate_idx = into_gate_idx(1);
        assert_eq!(Gate::Const(value), circuit.gates()[const_gate_idx]);

        let add_gate_idx = into_gate_idx(2);
        assert_eq!(
            Gate::BinOp(BinOp::Add, input_gate_idx, const_gate_idx),
            circuit.gates()[add_gate_idx]
        );

        let mul_gate_idx = into_gate_idx(3);
        assert_eq!(
            Gate::BinOp(BinOp::Mul, add_gate_idx, add_gate_idx),
            circuit.gates()[mul_gate_idx]
        );
    }

    #[test]
    fn test_large_number_of_constants_and_ops_with_them_yields_single_gate() {
        let ctx_handle = test_folding_ctx_handle();
        let total = 100;
        let mut constants = ThinVec::with_capacity(total);
        for value in 1..=total {
            let constant = ctx_handle.constant(value as SupportedType);
            constants.push(constant);
        }

        let mut additions = ThinVec::with_capacity(total - 1);

        for index in 1..total {
            let constant_1 = &constants[index - 1];
            let constant_2 = &constants[index];
            let addition = constant_1 + constant_2;
            additions.push(addition);
        }

        let all_multiplied = additions
            .iter()
            .map(Clone::clone)
            .reduce(|acc, e| acc * e)
            .expect("all to be multiplied");

        let expected_length = 1;

        let circuit = ctx_handle.compile(all_multiplied).expect("to compile");

        assert_eq!(expected_length, circuit.gates().len());
        assert_eq!(0, circuit.inputs().len());
        assert_eq!(1, circuit.outputs().len());

        let const_gate_idx = into_gate_idx(0);
        assert_eq!(Gate::Const(0), circuit.gates()[const_gate_idx]);
    }

    #[test]
    fn test_different_double_addition_and_multiplication_strict() {
        let ctx_handle = test_ctx_handle();

        let values = [1, 2, 3, 4];
        let input_1 = ctx_handle.input(values[0] as usize);
        let constant_1 = ctx_handle.constant(values[1]);
        let addition_1 = input_1 + constant_1;

        let input_2 = ctx_handle.input(values[2] as usize);
        let constant_2 = ctx_handle.constant(values[3]);
        let addition_2 = input_2 + constant_2;
        let out = &addition_1 * &addition_2;

        let expected_length = 7;

        let circuit = ctx_handle.compile(out).expect("to compile");

        assert_eq!(expected_length, circuit.gates().len());
        assert_eq!(2, circuit.inputs().len());
        assert_eq!(1, circuit.outputs().len());

        for (val_ix, const_idx) in [0, 1, 3, 4].iter().enumerate() {
            let gate_idx = into_gate_idx(*const_idx);
            let expected_expr = match val_ix % 2 {
                // input
                0 => Gate::Input(values[val_ix] as usize),
                // const
                _ => Gate::Const(values[val_ix]),
            };
            assert_eq!(expected_expr, circuit.gates()[gate_idx]);
        }

        let add_gate_idx = into_gate_idx(2);
        let input_1_gate_idx = into_gate_idx(0);
        let const_1_gate_idx = into_gate_idx(1);

        assert_eq!(
            Gate::BinOp(BinOp::Add, input_1_gate_idx, const_1_gate_idx),
            circuit.gates()[add_gate_idx]
        );

        let add_gate_idx_2 = into_gate_idx(5);
        let input_2_gate_idx = into_gate_idx(3);
        let const_2_gate_idx = into_gate_idx(4);

        assert_eq!(
            Gate::BinOp(BinOp::Add, input_2_gate_idx, const_2_gate_idx),
            circuit.gates()[add_gate_idx_2]
        );

        let mul_gate_idx = into_gate_idx(6);
        assert_eq!(
            Gate::BinOp(BinOp::Mul, add_gate_idx, add_gate_idx_2),
            circuit.gates()[mul_gate_idx]
        );
    }

    fn moded_ctx(mode: CompilationMode) -> ContextHandle {
        if matches!(mode, CompilationMode::StrictAll) {
            test_ctx_handle()
        } else {
            test_loose_ctx_handle()
        }
    }
    create! {
        create_compilation_mode_test,
        (mode, f, errs, num_elms_in_err_list), {
            let ctx_handle = moded_ctx(mode);
            let outs = f(ctx_handle.clone());
            let result = if outs.len() == 1 {
                ctx_handle.compile(outs[0].clone())
            } else {
                ctx_handle.compile_many(outs.as_slice())
            };
            assert_eq!(errs, result.is_err());

            if let Err(error) = result {
                for (ix, num_elm) in num_elms_in_err_list.iter().enumerate() {
                    match ix {
                        0 => assert_eq!(*num_elm, error.unused_inputs.len()),
                        1 => assert_eq!(*num_elm, error.unused_constants.len()),
                        2 => assert_eq!(*num_elm, error.unused_operations.len()),
                        _ => unreachable!(),
                    }
                }

            }
        }
    }

    const STRICT: CompilationMode = CompilationMode::StrictAll;
    const LOOSE: CompilationMode = CompilationMode::Loose;

    const ERRS: bool = true;
    const COMPILES: bool = false;

    const NONE: &[usize] = &[];

    #[allow(unused)]
    fn unused_input(ctx_handle: ContextHandle) -> ThinVec<ExprHandle> {
        let unused_input = ctx_handle.input(0);
        let input = ctx_handle.input(1);

        let out = &input + &input;
        thin_vec![out]
    }

    #[allow(unused)]
    fn unused_constant(ctx_handle: ContextHandle) -> ThinVec<ExprHandle> {
        let unused_constant = ctx_handle.constant(0);
        let input = ctx_handle.input(0);
        let constant = ctx_handle.constant(1);

        let out = &input + &constant;
        thin_vec![out]
    }

    #[allow(unused)]
    fn unused_operation(ctx_handle: ContextHandle) -> ThinVec<ExprHandle> {
        let input = ctx_handle.input(1);
        let constant = ctx_handle.constant(1);

        let unused_add = &input + &constant;

        let out = &input * &constant;
        thin_vec![out]
    }

    #[allow(unused)]
    fn unused_all(ctx_handle: ContextHandle) -> ThinVec<ExprHandle> {
        let unused_input = ctx_handle.input(0);
        let input = ctx_handle.input(1);

        let unused_constant = ctx_handle.constant(0);
        let constant = ctx_handle.constant(1);

        let unused_add = &input + &constant;

        let out = &input * &constant;
        thin_vec![out]
    }

    create_compilation_mode_test! {
        unused_input_do_not_compile_when_strict: (STRICT, unused_input, ERRS, &[1,0,0]),
        unused_input_compiles_when_loose: (LOOSE, unused_input, COMPILES, NONE),
        unused_constant_do_not_compile_when_strict: (STRICT, unused_constant, ERRS, &[0,1,0]),
        unused_constant_compiles_when_loose: (LOOSE, unused_constant, COMPILES, NONE),
        unused_operation_do_not_compile_when_strict: (STRICT, unused_operation, ERRS, &[0,0,1]),
        unused_operation_compiles_when_loose: (LOOSE, unused_operation, COMPILES, NONE),
        unused_all_do_not_compile_when_strict: (STRICT, unused_all, ERRS, &[1,1,1]),
        unused_all_compiles_when_loose: (LOOSE, unused_all, COMPILES, NONE),
    }
}
