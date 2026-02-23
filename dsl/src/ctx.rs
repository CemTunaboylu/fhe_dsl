use crate::{
    SupportedType,
    expr::{Expr, ExprHandle, ExprIdx},
    hash::ExprHash,
};
use bit_set::BitSet;
use la_arena::Arena;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use thin_vec::{IntoIter as ThinIntoIter, ThinVec};

pub type ContextRef = RefCell<Context>;

#[derive(Clone, Debug)]
pub struct ContextHandle(pub Rc<ContextRef>);

impl ContextHandle {
    pub fn get(&self, ix: ExprIdx) -> Expr {
        self.0.borrow().arena[ix]
    }
    pub(crate) fn expr_handle_for(&self, expr: Expr) -> ExprHandle {
        let expr_idx = self.0.borrow_mut().append(expr);
        ExprHandle {
            idx: expr_idx,
            ctx_handle: self.clone(),
        }
    }
    pub fn input(&self, index: usize) -> ExprHandle {
        let kind = Expr::Input(index);
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

    /// Eliminates unused edges by operating on operator expressions, also returns the set of
    /// unused ExprIdx.
    pub fn into_edges_and_unused(&self) -> (ThinIntoIter<(u32, u32)>, BitSet<u32>) {
        let mut edges = ThinVec::new();
        let mut unused = BitSet::<u32>::new();

        for i in 0..self.arena.len() {
            unused.insert(i);
        }
        for (expr_idx, expr) in self.arena.iter() {
            let expr_u32 = expr_idx.into_raw().into_u32();
            unused.remove(expr_u32 as usize);
            match expr {
                Expr::Input(_) | Expr::Const(_) => continue,
                Expr::Add(lhs, rhs) | Expr::Sub(lhs, rhs) | Expr::Mul(lhs, rhs) => {
                    let lhs_u32 = lhs.into_raw().into_u32();
                    let rhs_u32 = rhs.into_raw().into_u32();

                    unused.remove(lhs_u32 as usize);
                    unused.remove(rhs_u32 as usize);

                    edges.extend_from_slice(&[(lhs_u32, expr_u32), (rhs_u32, expr_u32)]);
                }
            }
        }
        (edges.into_iter(), unused)
    }
}
