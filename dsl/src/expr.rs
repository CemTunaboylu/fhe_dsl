use std::ops::{Add, Mul, Sub};

use crate::ctx::ContextHandle;
use la_arena::Idx;

pub(crate) type ExprId = Idx<Expr>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExprKind {
    Input(usize),
    Const(usize),
    Add(ExprId, ExprId),
    Sub(ExprId, ExprId),
    Mul(ExprId, ExprId),
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Expr {
    kind: ExprKind,
}

impl From<ExprKind> for Expr {
    fn from(kind: ExprKind) -> Self {
        Self { kind }
    }
}

#[derive(Clone, Debug)]
pub struct ExprHandle {
    pub(crate) id: ExprId,
    pub(crate) ctx_handle: ContextHandle,
}

impl Add for ExprHandle {
    type Output = ExprHandle;

    fn add(self, rhs: Self) -> Self::Output {
        let kind = ExprKind::Add(self.id, rhs.id);
        let expr = Expr::from(kind);
        let mut ctx = self.ctx_handle.0.borrow_mut();
        let expr_idx = ctx.append(expr);
        Self::Output {
            id: expr_idx,
            ctx_handle: self.ctx_handle.clone(),
        }
    }
}

impl Add for &ExprHandle {
    type Output = ExprHandle;

    fn add(self, rhs: Self) -> Self::Output {
        let kind = ExprKind::Add(self.id, rhs.id);
        let expr = Expr::from(kind);
        let mut ctx = self.ctx_handle.0.borrow_mut();
        let expr_idx = ctx.append(expr);
        Self::Output {
            id: expr_idx,
            ctx_handle: self.ctx_handle.clone(),
        }
    }
}

impl Add for &mut ExprHandle {
    type Output = ExprHandle;

    fn add(self, rhs: Self) -> Self::Output {
        let kind = ExprKind::Add(self.id, rhs.id);
        let expr = Expr::from(kind);
        let mut ctx = self.ctx_handle.0.borrow_mut();
        let expr_idx = ctx.append(expr);
        Self::Output {
            id: expr_idx,
            ctx_handle: self.ctx_handle.clone(),
        }
    }
}

impl Sub for ExprHandle {
    type Output = ExprHandle;

    fn sub(self, rhs: Self) -> Self::Output {
        let kind = ExprKind::Sub(self.id, rhs.id);
        let expr = Expr::from(kind);
        let mut ctx = self.ctx_handle.0.borrow_mut();
        let expr_idx = ctx.append(expr);
        Self::Output {
            id: expr_idx,
            ctx_handle: self.ctx_handle.clone(),
        }
    }
}

impl Sub for &ExprHandle {
    type Output = ExprHandle;

    fn sub(self, rhs: Self) -> Self::Output {
        let kind = ExprKind::Sub(self.id, rhs.id);
        let expr = Expr::from(kind);
        let mut ctx = self.ctx_handle.0.borrow_mut();
        let expr_idx = ctx.append(expr);
        Self::Output {
            id: expr_idx,
            ctx_handle: self.ctx_handle.clone(),
        }
    }
}

impl Sub for &mut ExprHandle {
    type Output = ExprHandle;

    fn sub(self, rhs: Self) -> Self::Output {
        let kind = ExprKind::Sub(self.id, rhs.id);
        let expr = Expr::from(kind);
        let mut ctx = self.ctx_handle.0.borrow_mut();
        let expr_idx = ctx.append(expr);
        Self::Output {
            id: expr_idx,
            ctx_handle: self.ctx_handle.clone(),
        }
    }
}

impl Mul for ExprHandle {
    type Output = ExprHandle;

    fn mul(self, rhs: Self) -> Self::Output {
        let kind = ExprKind::Mul(self.id, rhs.id);
        let expr = Expr::from(kind);
        let mut ctx = self.ctx_handle.0.borrow_mut();
        let expr_idx = ctx.append(expr);
        Self::Output {
            id: expr_idx,
            ctx_handle: self.ctx_handle.clone(),
        }
    }
}

impl Mul for &ExprHandle {
    type Output = ExprHandle;

    fn mul(self, rhs: Self) -> Self::Output {
        let kind = ExprKind::Mul(self.id, rhs.id);
        let expr = Expr::from(kind);
        let mut ctx = self.ctx_handle.0.borrow_mut();
        let expr_idx = ctx.append(expr);
        Self::Output {
            id: expr_idx,
            ctx_handle: self.ctx_handle.clone(),
        }
    }
}

impl Mul for &mut ExprHandle {
    type Output = ExprHandle;

    fn mul(self, rhs: Self) -> Self::Output {
        let kind = ExprKind::Mul(self.id, rhs.id);
        let expr = Expr::from(kind);
        let mut ctx = self.ctx_handle.0.borrow_mut();
        let expr_idx = ctx.append(expr);
        Self::Output {
            id: expr_idx,
            ctx_handle: self.ctx_handle.clone(),
        }
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

        assert_eq!(input.id.into_raw().into_u32(), 0);

        let inserted_expr = ctx_handle.0.borrow().arena[input.id];
        assert_eq!(inserted_expr.kind, ExprKind::Input(value));
    }

    #[test]
    fn test_constant() {
        let ctx_handle = test_ctx_handle();
        let value = 9;
        let constant = ctx_handle.constant(value);

        assert_eq!(constant.id.into_raw().into_u32(), 0);

        let inserted_expr = ctx_handle.0.borrow().arena[constant.id];
        assert_eq!(inserted_expr.kind, ExprKind::Const(value));
    }

    enum Op {
        Add,
        Sub,
        Mul,
    }

    create! {
        create_op_test,
        (op), {

            let ctx_handle = test_ctx_handle();
            let value = 9;
            let constant_1 = ctx_handle.constant(value);
            let constant_2 = ctx_handle.constant(value);

            let (op_handle, expected) = match op {
                Op::Add => (&constant_1 + &constant_2, ExprKind::Add(constant_1.id, constant_2.id)),
                Op::Sub=> (&constant_1 - &constant_2, ExprKind::Sub(constant_1.id, constant_2.id)),
                Op::Mul=> (&constant_1 * &constant_2, ExprKind::Mul(constant_1.id, constant_2.id)),
            };

        let inserted_expr = ctx_handle.0.borrow().arena[op_handle.id];
        assert_eq!(inserted_expr.kind,expected);
        }
    }

    create_op_test! {
        add: Op::Add,
        sub: Op::Sub,
        mul: Op::Mul,
    }
}
