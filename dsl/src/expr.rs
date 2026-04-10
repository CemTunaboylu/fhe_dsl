use la_arena::Idx;
use op::BinOp;
use passes::interner::Internable;

use common::Supported;

use crate::{ctx::ContextHandle, hash::ExprHash};

pub type ExprIdx<T> = Idx<Expr<T>>;

#[derive(Clone, Copy, Debug)]
pub enum Expr<T: Supported> {
    /// takes the index of the argument
    Input(usize),
    Const(T),
    BinOp(BinOp, ExprIdx<T>, ExprIdx<T>),
}

impl<T: Supported> PartialEq for Expr<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Input(l0), Self::Input(r0)) => l0 == r0,
            (Self::Const(l0), Self::Const(r0)) => l0 == r0,
            (Self::BinOp(_, _, _), Self::BinOp(_, _, _)) => {
                ExprHash::from(self) == ExprHash::from(other)
            }
            _ => false,
        }
    }
}

impl<T: Supported> Eq for Expr<T> {}

impl<T: Supported> Internable for Expr<T> {}

#[derive(Clone, Debug)]
pub struct ExprHandle<T: Supported> {
    pub idx: ExprIdx<T>,
    pub ctx_handle: ContextHandle<T>,
}

impl<T: Supported> ExprHandle<T> {
    pub(crate) fn extend_new_handle(&self, expr: Expr<T>) -> Self {
        self.ctx_handle.expr_handle_for(expr)
    }
    pub fn get_expr(&self) -> Expr<T> {
        self.ctx_handle.get(self.idx)
    }
}

#[cfg(test)]
mod tests {
    use parameterized_test::create;
    use std::ops::{Add, Mul, Sub};

    use crate::{idx_to_u32, new_loose_context};

    use super::*;

    fn test_ctx_handle() -> ContextHandle<u64> {
        new_loose_context(5u64)
    }

    #[test]
    fn test_input() {
        let ctx_handle = test_ctx_handle();
        let index = 0;
        let input = ctx_handle.input(index);

        assert_eq!(idx_to_u32(input.idx), 0);

        let inserted_expr = ctx_handle.get(input.idx);
        assert_eq!(inserted_expr, Expr::Input(index));
    }

    #[test]
    fn test_constant() {
        let ctx_handle = test_ctx_handle();
        let value = 9u64;
        let constant = ctx_handle.constant(value);

        assert_eq!(idx_to_u32(constant.idx), 0);

        let inserted_expr = ctx_handle.get(constant.idx);
        assert_eq!(inserted_expr, Expr::Const(value));
    }

    fn get_arena_len<T: Supported>(context_handle: &ContextHandle<T>) -> usize {
        context_handle.0.borrow().arena.len()
    }

    #[test]
    fn test_hash_consing_for_constants_and_inputs() {
        let ctx_handle = test_ctx_handle();
        let constant_value = 9u64;
        let constant = ctx_handle.constant(constant_value);

        let expected_length_for_constant = get_arena_len(&ctx_handle);
        assert_eq!(idx_to_u32(constant.idx), 0);

        let same_constant = ctx_handle.constant(constant_value);
        assert_eq!(idx_to_u32(same_constant.idx), 0);
        assert_eq!(get_arena_len(&ctx_handle), expected_length_for_constant);

        let index = 0;
        let input = ctx_handle.input(index);

        let expected_length_for_input = get_arena_len(&ctx_handle);
        assert_eq!(idx_to_u32(input.idx), 1);

        let another_input = ctx_handle.input(index);
        assert_eq!(idx_to_u32(another_input.idx), 1);

        assert_eq!(get_arena_len(&ctx_handle), expected_length_for_input);
    }

    #[derive(Clone, Debug)]
    enum Mode {
        Move,
        Borrow,
        BorrowMut,
    }

    fn perform_op_with_expectation<O>(op: BinOp, operand_1: O, operand_2: O) -> (ExprHandle<u64>, BinOp)
    where
        O: Add<Output = ExprHandle<u64>> + Sub<Output = ExprHandle<u64>> + Mul<Output = ExprHandle<u64>>,
    {
        match op {
            BinOp::Add => {
                let result = operand_1 + operand_2;
                (result, BinOp::Add)
            }
            BinOp::Sub => {
                let result = operand_1 - operand_2;
                (result, BinOp::Sub)
            }
            BinOp::Mul => {
                let result = operand_1 * operand_2;
                (result, BinOp::Mul)
            }
        }
    }

    fn perform_op_with_expectation_mode(
        op: BinOp,
        mut expr_handle_1: ExprHandle<u64>,
        mut expr_handle_2: ExprHandle<u64>,
        mode: Mode,
    ) -> (ExprHandle<u64>, Expr<u64>) {
        match mode {
            Mode::Move => {
                let (idx_1, idx_2) = (expr_handle_1.idx, expr_handle_2.idx);
                let (expr_handle, bin_op) =
                    perform_op_with_expectation(op, expr_handle_1, expr_handle_2);
                (expr_handle, Expr::BinOp(bin_op, idx_1, idx_2))
            }
            Mode::Borrow => {
                let (idx_1, idx_2) = (expr_handle_1.idx, expr_handle_2.idx);
                let (expr_handle, bin_op) =
                    perform_op_with_expectation(op, &expr_handle_1, &expr_handle_2);
                (expr_handle, Expr::BinOp(bin_op, idx_1, idx_2))
            }
            Mode::BorrowMut => {
                let (idx_1, idx_2) = (expr_handle_1.idx, expr_handle_2.idx);
                let (expr_handle, bin_op) =
                    perform_op_with_expectation(op, &mut expr_handle_1, &mut expr_handle_2);
                (expr_handle, Expr::BinOp(bin_op, idx_1, idx_2))
            }
        }
    }

    create! {
        create_op_test,
        (op), {

            let ctx_handle = test_ctx_handle();
            let index = 0;
            let value = 9u64;

            let input = ctx_handle.input(index);
            let constant = ctx_handle.constant(value);

            for mode in [Mode::Move, Mode::Borrow, Mode::BorrowMut] {
                let (expr_handle, expectation) = perform_op_with_expectation_mode(op.clone(), input.clone(), constant.clone(), mode);

                let inserted_expr = ctx_handle.get(expr_handle.idx);
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
            let index = 0;
            let value = 9u64;

            // Agnostic to the operands mode (move, borrowed), if operation is the same and the
            // values of operands are the same, it will be re-used.
            // + 1 for the operation at hand, +1 for the input and +1 for the  constant
            let expected_arena_length = get_arena_len(&ctx_handle) + 3;
            let input = ctx_handle.input(index);
            let constant = ctx_handle.constant(value);

            for mode in [Mode::Move, Mode::Borrow, Mode::BorrowMut] {
                let (expr_handle, _expectation) = perform_op_with_expectation_mode(op.clone(), input.clone(), constant.clone(), mode.clone());
                let (same_expr_handle, _expectation) = perform_op_with_expectation_mode(op.clone(), input.clone(), constant.clone(), mode);

                assert_eq!(expr_handle.idx, same_expr_handle.idx);
                let current_arena_length = get_arena_len(&ctx_handle);
                assert_eq!(expected_arena_length, current_arena_length);
            }

            if matches!(op, BinOp::Sub) {
                // Reverse order should not yield the same
                for mode in [Mode::Move, Mode::Borrow, Mode::BorrowMut] {
                    let (expr_handle, _expectation) = perform_op_with_expectation_mode(op.clone(), constant.clone(), input.clone(), mode.clone());
                    let (same_expr_handle, _expectation) = perform_op_with_expectation_mode(op.clone(), constant.clone(), input.clone(), mode);

                    assert!(expr_handle.idx == same_expr_handle.idx);
                    let current_arena_length = get_arena_len(&ctx_handle);
                    assert_eq!(expected_arena_length + 1, current_arena_length);
                }
            } else {
                // Reverse order should also yield the same
                for mode in [Mode::Move, Mode::Borrow, Mode::BorrowMut] {
                    let (expr_handle, _expectation) = perform_op_with_expectation_mode(op.clone(), constant.clone(), input.clone(), mode.clone());
                    let (same_expr_handle, _expectation) = perform_op_with_expectation_mode(op.clone(), constant.clone(), input.clone(), mode);

                    assert_eq!(expr_handle.idx, same_expr_handle.idx);
                    let current_arena_length = get_arena_len(&ctx_handle);
                    assert_eq!(expected_arena_length, current_arena_length);
                }

            }
        }
    }

    create_op_hash_consing_test! {
        add: BinOp::Add,
        sub: BinOp::Sub,
        mul: BinOp::Mul,
    }

    fn assert_supported_expr<T: Supported>() {
        // Compile-time check that Expr<T> is usable for all supported integer types.
        // We verify that Expr<T> satisfies the Internable bounds: Clone + Eq + Hash + PartialEq.
        fn _check<I: Internable>() {}
        _check::<Expr<T>>();
    }

    #[test]
    fn all_supported_integer_types_work_as_expr_const() {
        assert_supported_expr::<i8>();
        assert_supported_expr::<i16>();
        assert_supported_expr::<i32>();
        assert_supported_expr::<i64>();
        assert_supported_expr::<u8>();
        assert_supported_expr::<u16>();
        assert_supported_expr::<u32>();
        assert_supported_expr::<u64>();
    }
}
