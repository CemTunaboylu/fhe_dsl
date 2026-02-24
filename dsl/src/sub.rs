use std::ops::Sub;

use crate::expr::{Expr, ExprHandle};

use op::BinOp;

impl Sub for &ExprHandle {
    type Output = ExprHandle;

    fn sub(self, rhs: Self) -> Self::Output {
        let expr = Expr::BinOp(BinOp::Sub, self.idx, rhs.idx);
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
