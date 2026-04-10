use std::ops::Sub;

use common::Supported;

use crate::expr::{Expr, ExprHandle};

use op::BinOp;

impl<T: Supported> Sub for &ExprHandle<T> {
    type Output = ExprHandle<T>;

    fn sub(self, rhs: Self) -> Self::Output {
        let expr = Expr::BinOp(BinOp::Sub, self.idx, rhs.idx);
        self.extend_new_handle(expr)
    }
}

impl<T: Supported> Sub for ExprHandle<T> {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        (&self).sub(&rhs)
    }
}

impl<T: Supported> Sub for &mut ExprHandle<T> {
    type Output = ExprHandle<T>;

    fn sub(self, rhs: Self) -> Self::Output {
        (&*self).sub(&*rhs)
    }
}
