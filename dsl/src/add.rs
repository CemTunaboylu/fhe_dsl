use std::ops::Add;

use crate::expr::{Expr, ExprHandle};

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
