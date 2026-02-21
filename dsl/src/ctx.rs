use crate::expr::{Expr, ExprHandle, ExprIdx};
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
    pub fn input(&self, value: usize) -> ExprHandle {
        let kind = Expr::Input(value);
        self.expr_handle_for(kind)
    }
    pub fn constant(&self, value: usize) -> ExprHandle {
        let kind = Expr::Const(value);
        self.expr_handle_for(kind)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
enum ExprHash {
    Value(usize),
    /// ExprIds are ordered before forming Double to avoid order originating duplicates.
    Double(ExprIdx, ExprIdx),
}

impl From<&Expr> for ExprHash {
    fn from(expr: &Expr) -> Self {
        match expr {
            Expr::Input(v) | Expr::Const(v) => Self::Value(*v),
            Expr::Add(idx, idx1) | Expr::Sub(idx, idx1) | Expr::Mul(idx, idx1) => {
                let (mut idx_1, mut idx_2) = (*idx, *idx1);
                if idx_1 > idx_2 {
                    (idx_1, idx_2) = (idx_2, idx_1)
                }
                Self::Double(idx_1, idx_2)
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Context {
    q: usize,
    pub(crate) arena: Arena<Expr>,
    map: HashMap<ExprHash, ExprIdx>,
}

impl Context {
    pub(crate) fn new(q: usize) -> Self {
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
