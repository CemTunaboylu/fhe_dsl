use std::ops::Mul;

use common::Supported;

use crate::expr::{Expr, ExprHandle};

use op::BinOp;

impl<T: Supported> Mul for &ExprHandle<T> {
    type Output = ExprHandle<T>;

    fn mul(self, rhs: Self) -> Self::Output {
        let expr = Expr::BinOp(BinOp::Mul, self.idx, rhs.idx);
        self.extend_new_handle(expr)
    }
}

impl<T: Supported> Mul for ExprHandle<T> {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        (&self).mul(&rhs)
    }
}

impl<T: Supported> Mul for &mut ExprHandle<T> {
    type Output = ExprHandle<T>;

    fn mul(self, rhs: Self) -> Self::Output {
        (&*self).mul(&*rhs)
    }
}
