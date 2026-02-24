use std::ops::Add;

use crate::expr::{Expr, ExprHandle};

use op::BinOp;

impl Add for &ExprHandle {
    type Output = ExprHandle;

    fn add(self, rhs: Self) -> Self::Output {
        let expr = Expr::BinOp(BinOp::Add, self.idx, rhs.idx);
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
