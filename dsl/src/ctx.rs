use crate::expr::{Expr, ExprHandle, ExprIdx};
use la_arena::Arena;
use std::{cell::RefCell, rc::Rc};

pub type ContextRef = RefCell<Context>;

#[derive(Clone, Debug)]
pub struct ContextHandle(pub Rc<ContextRef>);

impl ContextHandle {
    fn expr_handle_for(&self, expr: Expr) -> ExprHandle {
        let expr_idx = self.0.borrow_mut().append(expr);
        ExprHandle {
            idx: expr_idx,
            ctx_handle: self.clone(),
        }
    }
    pub fn input(&self, value: usize) -> ExprHandle {
        let kind = Expr::Input(value);
        self.expr_handle_for(kind)
    }
    pub fn constant(&self, value: usize) -> ExprHandle {
        let kind = Expr::Const(value);
        self.expr_handle_for(kind)
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Context {
    q: usize,
    pub(crate) arena: Arena<Expr>,
    // TODO: add hash-consing map for CSE
}

impl Context {
    pub(crate) fn new(q: usize) -> Self {
        Self {
            q,
            arena: Arena::new(),
        }
    }

    pub(crate) fn append(&mut self, expr: Expr) -> ExprIdx {
        self.arena.alloc(expr)
    }
}
