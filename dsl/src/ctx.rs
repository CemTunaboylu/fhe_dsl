use common::{NonZeroModulusResult, Supported, validate_non_zero};

use crate::{
    compilation_mode::CompilationMode,
    expr::{Expr, ExprHandle, ExprIdx},
    folding::fold,
};

use bit_set::BitSet;
use la_arena::Arena;
use passes::interner::Interner;

#[cfg(feature = "graphview")]
use crate::idx_to_u32;
#[cfg(feature = "graphview")]
use thin_vec::{IntoIter as ThinIntoIter, ThinVec};

use std::{cell::RefCell, rc::Rc};

pub type ContextRef<T> = RefCell<Context<T>>;

#[derive(Clone, Debug)]
pub struct ContextHandle<T: Supported>(pub Rc<ContextRef<T>>);

impl<T: Supported> ContextHandle<T> {
    pub fn get(&self, ix: ExprIdx<T>) -> Expr<T> {
        let ctx_ref = self.0.borrow();
        ctx_ref.arena[ix]
    }
    pub(crate) fn expr_handle_for(&self, expr: Expr<T>) -> ExprHandle<T> {
        let expr_idx = self.append(expr);
        ExprHandle {
            idx: expr_idx,
            ctx_handle: self.clone(),
        }
    }
    pub fn input(&self, index: usize) -> ExprHandle<T> {
        let kind = Expr::Input(index);
        self.expr_handle_for(kind)
    }
    pub fn constant(&self, value: T) -> ExprHandle<T> {
        let kind = Expr::Const(value);
        self.expr_handle_for(kind)
    }
    pub(crate) fn append(&self, expr: Expr<T>) -> ExprIdx<T> {
        let mut ctx_ref = self.0.borrow_mut();
        ctx_ref.append(expr)
    }
}

#[derive(Clone, Debug)]
pub struct Context<T: Supported> {
    plain_text_modulus: T,
    pub(crate) mode: CompilationMode,
    pub(crate) arena: Arena<Expr<T>>,
    pub(crate) interner: Interner<Expr<T>>,
}

impl<T: Supported> Context<T> {
    pub(crate) fn new(q: T, mode: CompilationMode) -> NonZeroModulusResult<Self> {
        validate_non_zero(q)?;
        let arena = Arena::new();
        let interner = Interner::new();
        let s = Self {
            plain_text_modulus: q,
            mode,
            arena,
            interner,
        };
        Ok(s)
    }

    pub(crate) fn get_plain_text_modulus(&self) -> T {
        self.plain_text_modulus
    }
    pub(crate) fn create_set_of_all_indices(&self) -> BitSet {
        let mut set = BitSet::<u32>::new();

        let len = self.arena.len();
        for i in 0..len {
            set.insert(i);
        }
        set
    }
    pub fn append(&mut self, mut expr: Expr<T>) -> ExprIdx<T> {
        expr = fold(expr, &mut self.arena, self.plain_text_modulus);
        self.interner.intern(expr, &mut self.arena)
    }
    /// Eliminates unused edges by operating on operator expressions, also returns the set of
    /// unused ExprIdx.
    #[cfg(feature = "graphview")]
    pub fn into_edges_and_unused(&self) -> (ThinIntoIter<(u32, u32)>, BitSet<u32>) {
        let mut edges = ThinVec::new();
        let mut unused = self.create_set_of_all_indices();

        for (expr_idx, expr) in self.arena.iter() {
            let expr_u32 = idx_to_u32(expr_idx);
            unused.remove(expr_u32 as usize);
            match expr {
                Expr::Input(_) | Expr::Const(_) => continue,
                Expr::BinOp(_bin_op, lhs, rhs) => {
                    let lhs_u32 = idx_to_u32(*lhs);
                    let rhs_u32 = idx_to_u32(*rhs);

                    unused.remove(lhs_u32 as usize);
                    unused.remove(rhs_u32 as usize);

                    edges.extend_from_slice(&[(lhs_u32, expr_u32), (rhs_u32, expr_u32)]);
                }
            }
        }
        (edges.into_iter(), unused)
    }
}
