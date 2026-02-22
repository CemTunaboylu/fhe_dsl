use std::ops::Mul;

use crate::expr::{Expr, ExprHandle};

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
