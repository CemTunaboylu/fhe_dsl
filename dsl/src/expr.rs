use crate::{SupportedType, ctx::ContextHandle};
use la_arena::Idx;

pub type ExprIdx = Idx<Expr>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Expr {
    Var(SupportedType),
    Const(SupportedType),
    Add(ExprIdx, ExprIdx),
    Sub(ExprIdx, ExprIdx),
    Mul(ExprIdx, ExprIdx),
}

#[derive(Clone, Debug)]
pub struct ExprHandle {
    pub idx: ExprIdx,
    pub ctx_handle: ContextHandle,
}

impl ExprHandle {
    pub(crate) fn extend_new_handle(&self, expr: Expr) -> Self {
        self.ctx_handle.expr_handle_for(expr)
    }
    pub fn get_expr(&self) -> Expr {
        self.ctx_handle.get(self.idx)
    }
}

#[cfg(test)]
mod tests {
    use parameterized_test::create;
    use std::ops::{Add, Mul, Sub};

    use crate::{new_context, op::BinOp};

    use super::*;

    fn test_ctx_handle() -> ContextHandle {
        new_context(7)
    }

    #[test]
    fn test_var() {
        let ctx_handle = test_ctx_handle();
        let value = 9;
        let var = ctx_handle.var(value);

        assert_eq!(var.idx.into_raw().into_u32(), 0);

        let inserted_expr = ctx_handle.0.borrow().arena[var.idx];
        assert_eq!(inserted_expr, Expr::Var(value));
    }

    #[test]
    fn test_constant() {
        let ctx_handle = test_ctx_handle();
        let value = 9;
        let constant = ctx_handle.constant(value);

        assert_eq!(constant.idx.into_raw().into_u32(), 0);

        let inserted_expr = ctx_handle.0.borrow().arena[constant.idx];
        assert_eq!(inserted_expr, Expr::Const(value));
    }

    #[test]
    fn test_hash_consing_for_values() {
        let ctx_handle = test_ctx_handle();
        let constant_value = 9;
        let constant = ctx_handle.constant(constant_value);

        let expected_length_for_constant = ctx_handle.0.borrow().arena.len();
        assert_eq!(constant.idx.into_raw().into_u32(), 0);

        let same_constant = ctx_handle.constant(constant_value);
        assert_eq!(same_constant.idx.into_raw().into_u32(), 0);
        assert_eq!(
            ctx_handle.0.borrow().arena.len(),
            expected_length_for_constant
        );

        let var_value = 8;
        let variable = ctx_handle.var(var_value);

        let expected_length_for_var = ctx_handle.0.borrow().arena.len();
        assert_eq!(variable.idx.into_raw().into_u32(), 1);

        let same_var = ctx_handle.var(var_value);
        assert_eq!(same_var.idx.into_raw().into_u32(), 1);
        assert_eq!(ctx_handle.0.borrow().arena.len(), expected_length_for_var);
    }

    impl Expr {
        fn add(expr_idx_1: ExprIdx, expr_idx_2: ExprIdx) -> Self {
            Self::Add(expr_idx_1, expr_idx_2)
        }
        fn sub(expr_idx_1: ExprIdx, expr_idx_2: ExprIdx) -> Self {
            Self::Sub(expr_idx_1, expr_idx_2)
        }
        fn mul(expr_idx_1: ExprIdx, expr_idx_2: ExprIdx) -> Self {
            Self::Mul(expr_idx_1, expr_idx_2)
        }
    }

    #[derive(Clone, Debug)]
    enum Mode {
        Move,
        Borrow,
        BorrowMut,
    }

    fn add<O: Add>(e_1: O, e_2: O) -> O::Output {
        e_1 + e_2
    }
    fn sub<O: Sub>(e_1: O, e_2: O) -> O::Output {
        e_1 - e_2
    }
    fn mul<O: Mul>(e_1: O, e_2: O) -> O::Output {
        e_1 * e_2
    }

    fn perform_op_with_expectation<O>(
        op: BinOp,
        operand_1: O,
        operand_2: O,
    ) -> (ExprHandle, fn(ExprIdx, ExprIdx) -> Expr)
    where
        O: Add<Output = ExprHandle> + Sub<Output = ExprHandle> + Mul<Output = ExprHandle>,
    {
        match op {
            BinOp::Add => {
                let result = add(operand_1, operand_2);
                (result, Expr::add)
            }
            BinOp::Sub => {
                let result = sub(operand_1, operand_2);
                (result, Expr::sub)
            }
            BinOp::Mul => {
                let result = mul(operand_1, operand_2);
                (result, Expr::mul)
            }
        }
    }

    fn perform_op_with_expectation_mode(
        op: BinOp,
        mut expr_handle_1: ExprHandle,
        mut expr_handle_2: ExprHandle,
        mode: Mode,
    ) -> (ExprHandle, Expr) {
        match mode {
            Mode::Move => {
                let (idx_1, idx_2) = (expr_handle_1.idx, expr_handle_2.idx);
                let (expr_handle, expr_kind) =
                    perform_op_with_expectation(op, expr_handle_1, expr_handle_2);
                (expr_handle, expr_kind(idx_1, idx_2))
            }
            Mode::Borrow => {
                let (idx_1, idx_2) = (expr_handle_1.idx, expr_handle_2.idx);
                let (expr_handle, expr_kind) =
                    perform_op_with_expectation(op, &expr_handle_1, &expr_handle_2);
                (expr_handle, expr_kind(idx_1, idx_2))
            }
            Mode::BorrowMut => {
                let (idx_1, idx_2) = (expr_handle_1.idx, expr_handle_2.idx);
                let (expr_handle, expr_kind) =
                    perform_op_with_expectation(op, &mut expr_handle_1, &mut expr_handle_2);
                (expr_handle, expr_kind(idx_1, idx_2))
            }
        }
    }

    create! {
        create_op_test,
        (op), {

            let ctx_handle = test_ctx_handle();
            let value = 9;
            let constant_1 = ctx_handle.constant(value);
            let constant_2 = ctx_handle.constant(value);

            for mode in [Mode::Move, Mode::Borrow, Mode::BorrowMut] {
                let constant_1 = ctx_handle.constant(value);
                let constant_2 = ctx_handle.constant(value);
                let (expr_handle, expectation) = perform_op_with_expectation_mode(op.clone(), constant_1, constant_2, mode);

                let inserted_expr = ctx_handle.0.borrow().arena[expr_handle.idx];
                assert_eq!(inserted_expr, expectation);
            }
        }
    }

    create_op_test! {
        add: BinOp::Add,
        sub: BinOp::Sub,
        mul: BinOp::Mul,
    }

    create! {
        create_op_hash_consing_test,
        (op), {

            let ctx_handle = test_ctx_handle();
            let value = 9;
            let constant_1 = ctx_handle.constant(value);
            let constant_2 = ctx_handle.constant(value);

            // Agnostic to the operands mode (move, borrowed), if operation is the same and the
            // values of operands are the same, it will be re-used.
            let expected_arena_length = ctx_handle.0.borrow().arena.len() + 1;
            for mode in [Mode::Move, Mode::Borrow, Mode::BorrowMut] {
                let constant_1 = ctx_handle.constant(value);
                let constant_2 = ctx_handle.constant(value);
                let (expr_handle, expectation) = perform_op_with_expectation_mode(op.clone(), constant_1.clone(), constant_2.clone(), mode.clone());
                let (same_expr_handle, expectation) = perform_op_with_expectation_mode(op.clone(), constant_1, constant_2, mode);

                assert_eq!(expr_handle.idx, same_expr_handle.idx);
                let current_arena_length = ctx_handle.0.borrow().arena.len();
                assert_eq!(expected_arena_length, current_arena_length);
            }
        }
    }

    create_op_hash_consing_test! {
        add: BinOp::Add,
        sub: BinOp::Sub,
        mul: BinOp::Mul,
    }
}
