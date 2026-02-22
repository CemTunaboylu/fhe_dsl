use crate::{
    SupportedType,
    expr::{Expr, ExprHandle, ExprIdx},
    hash::ExprHash,
};
use la_arena::Arena;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

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
    pub fn var(&self, value: SupportedType) -> ExprHandle {
        let kind = Expr::Var(value);
        self.expr_handle_for(kind)
    }
    pub fn constant(&self, value: SupportedType) -> ExprHandle {
        let kind = Expr::Const(value);
        self.expr_handle_for(kind)
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Context {
    q: SupportedType,
    pub(crate) arena: Arena<Expr>,
    map: HashMap<ExprHash, ExprIdx>,
}

impl Context {
    pub(crate) fn new(q: SupportedType) -> Self {
        Self {
            q,
            arena: Arena::new(),
            map: HashMap::new(),
        }
    }

    pub(crate) fn append(&mut self, expr: Expr) -> ExprIdx {
        let expr_hash = ExprHash::from(&expr);
        if let Some(expr_idx) = self.map.get(&expr_hash) {
            return *expr_idx;
        }
        let expr_idx = self.arena.alloc(expr);
        self.map.insert(expr_hash, expr_idx);
        expr_idx
    }
}
