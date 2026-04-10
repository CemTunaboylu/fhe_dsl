use std::ops::Add;

use common::Supported;

use crate::expr::{Expr, ExprHandle};

use op::BinOp;

impl<T: Supported> Add for &ExprHandle<T> {
    type Output = ExprHandle<T>;

    fn add(self, rhs: Self) -> Self::Output {
        let expr = Expr::BinOp(BinOp::Add, self.idx, rhs.idx);
        self.extend_new_handle(expr)
    }
}

impl<T: Supported> Add for ExprHandle<T> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        (&self).add(&rhs)
    }
}

impl<T: Supported> Add for &mut ExprHandle<T> {
    type Output = ExprHandle<T>;

    fn add(self, rhs: Self) -> Self::Output {
        (&*self).add(&*rhs)
    }
}
