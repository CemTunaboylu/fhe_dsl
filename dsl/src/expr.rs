use std::ops::{Add, Mul, Sub};

use crate::ctx::ContextHandle;
use la_arena::Idx;

pub(crate) type ExprIdx = Idx<Expr>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Expr {
    Input(usize),
    Const(usize),
    Add(ExprIdx, ExprIdx),
    Sub(ExprIdx, ExprIdx),
    Mul(ExprIdx, ExprIdx),
}

#[derive(Clone, Debug)]
pub struct ExprHandle {
    pub(crate) idx: ExprIdx,
    pub(crate) ctx_handle: ContextHandle,
}

impl ExprHandle {
    fn push_in_context_of(&self, expr: Expr) -> ExprIdx {
        let mut ctx = self.ctx_handle.0.borrow_mut();
        ctx.append(expr)
    }
    fn get_handle(&self) -> ContextHandle {
        self.ctx_handle.clone()
    }
    fn extend_new_handle(&self, expr: Expr) -> Self {
        let expr_idx = self.push_in_context_of(expr);
        Self {
            idx: expr_idx,
            ctx_handle: self.get_handle(),
        }
    }
}

impl Add for &ExprHandle {
    type Output = ExprHandle;

    fn add(self, rhs: Self) -> Self::Output {
        let expr = Expr::Add(self.idx, rhs.idx);
        self.extend_new_handle(expr)
    }
}

impl Add for ExprHandle {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        (&self).add(&rhs)
    }
}

impl Add for &mut ExprHandle {
    type Output = ExprHandle;

    fn add(self, rhs: Self) -> Self::Output {
        (&*self).add(&*rhs)
    }
}

impl Sub for &ExprHandle {
    type Output = ExprHandle;

    fn sub(self, rhs: Self) -> Self::Output {
        let expr = Expr::Sub(self.idx, rhs.idx);
        self.extend_new_handle(expr)
    }
}

impl Sub for ExprHandle {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        (&self).sub(&rhs)
    }
}

impl Sub for &mut ExprHandle {
    type Output = ExprHandle;

    fn sub(self, rhs: Self) -> Self::Output {
        (&*self).sub(&*rhs)
    }
}

impl Mul for &ExprHandle {
    type Output = ExprHandle;

    fn mul(self, rhs: Self) -> Self::Output {
        let expr = Expr::Mul(self.idx, rhs.idx);
        self.extend_new_handle(expr)
    }
}

impl Mul for ExprHandle {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        (&self).mul(&rhs)
    }
}

impl Mul for &mut ExprHandle {
    type Output = ExprHandle;

    fn mul(self, rhs: Self) -> Self::Output {
        (&*self).mul(&*rhs)
    }
}

#[cfg(test)]
mod tests {
    use parameterized_test::create;

    use crate::new_context;

    use super::*;

    fn test_ctx_handle() -> ContextHandle {
        new_context(7)
    }

    #[test]
    fn test_input() {
        let ctx_handle = test_ctx_handle();
        let value = 9;
        let input = ctx_handle.input(value);

        assert_eq!(input.idx.into_raw().into_u32(), 0);

        let inserted_expr = ctx_handle.0.borrow().arena[input.idx];
        assert_eq!(inserted_expr, Expr::Input(value));
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
    enum Op {
        Add,
        Sub,
        Mul,
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
        op: Op,
        operand_1: O,
        operand_2: O,
    ) -> (ExprHandle, fn(ExprIdx, ExprIdx) -> Expr)
    where
        O: Add<Output = ExprHandle> + Sub<Output = ExprHandle> + Mul<Output = ExprHandle>,
    {
        match op {
            Op::Add => {
                let result = add(operand_1, operand_2);
                (result, Expr::add)
            }
            Op::Sub => {
                let result = sub(operand_1, operand_2);
                (result, Expr::sub)
            }
            Op::Mul => {
                let result = mul(operand_1, operand_2);
                (result, Expr::mul)
            }
        }
    }

    fn perform_op_with_expectation_mode(
        op: Op,
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
        add: Op::Add,
        sub: Op::Sub,
        mul: Op::Mul,
    }
}
