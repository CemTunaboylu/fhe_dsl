use crate::{
    SupportedType,
    expr::{Expr, ExprHandle, ExprIdx},
};

use bit_set::BitSet;
use passes::interner::Interner;
use thin_vec::{IntoIter as ThinIntoIter, ThinVec};

use std::{cell::RefCell, rc::Rc};

pub type ContextRef = RefCell<Context>;

#[derive(Clone, Debug)]
pub struct ContextHandle(pub Rc<ContextRef>);

impl ContextHandle {
    pub fn get(&self, ix: ExprIdx) -> Expr {
        let ctx_ref = self.0.borrow();
        ctx_ref.interner.arena[ix]
    }
    pub(crate) fn expr_handle_for(&self, expr: Expr) -> ExprHandle {
        let expr_idx = self.append(expr);
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
    pub(crate) fn append(&self, expr: Expr) -> ExprIdx {
        self.0.borrow_mut().interner.intern(expr)
    }
}

#[derive(Clone, Debug, Default)]
pub enum CompilationMode {
    Loose,
    #[default]
    Strict,
}

#[derive(Clone, Debug)]
pub struct Context {
    pub(crate) q: SupportedType,
    pub(crate) mode: CompilationMode,
    pub(crate) interner: Interner<Expr>,
}

impl Context {
    pub(crate) fn new(q: SupportedType, mode: CompilationMode) -> Self {
        let interner = Interner::new();
        Self { q, mode, interner }
    }
    pub(crate) fn create_set_of_all_indices(&self) -> BitSet {
        let mut set = BitSet::<u32>::new();

        let len = self.interner.arena.len();
        for i in 0..len {
            set.insert(i);
        }
        set
    }
    /// Eliminates unused edges by operating on operator expressions, also returns the set of
    /// unused ExprIdx.
    #[cfg(feature = "graphview")]
    pub fn into_edges_and_unused(&self) -> (ThinIntoIter<(u32, u32)>, BitSet<u32>) {
        let mut edges = ThinVec::new();
        let mut unused = self.create_set_of_all_indices();

        let arena = &self.interner.arena;
        for (expr_idx, expr) in arena.iter() {
            let expr_u32 = expr_idx.into_raw().into_u32();
            unused.remove(expr_u32 as usize);
            match expr {
                Expr::Input(_) | Expr::Const(_) => continue,
                Expr::BinOp(_bin_op, lhs, rhs) => {
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
