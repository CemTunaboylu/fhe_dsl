use std::{cell::RefCell, rc::Rc};

use crate::{
    compilation_mode::*,
    ctx::{Context, ContextHandle},
};

pub use common::Supported;
use common::idx_to_u32 as c_idx_to_u32;

use crate::expr::{Expr, ExprIdx};

pub mod add;
pub mod compilation_mode;
pub mod compile;
pub mod ctx;
pub mod expr;
pub mod folding;
pub mod hash;
pub mod mul;
pub mod sub;

pub(crate) fn idx_to_u32<T: Supported>(expr_idx: ExprIdx<T>) -> u32 {
    c_idx_to_u32::<Expr<T>>(expr_idx)
}

pub fn new_folding_strict_context<T: Supported>(q: T) -> ContextHandle<T> {
    let strictness: Strictness = [StrictnessOn::Input, StrictnessOn::Op].as_slice().into();
    let mode = CompilationMode::with(strictness);
    let ctx = Context::new(q, mode);
    let ref_cell = RefCell::new(ctx);
    let rc = Rc::new(ref_cell);
    ContextHandle(rc)
}

pub fn new_strict_context<T: Supported>(q: T) -> ContextHandle<T> {
    let mode = CompilationMode::StrictAll;
    let ctx = Context::new(q, mode);
    let ref_cell = RefCell::new(ctx);
    let rc = Rc::new(ref_cell);
    ContextHandle(rc)
}

pub fn new_loose_context<T: Supported>(q: T) -> ContextHandle<T> {
    let mode = CompilationMode::Loose;
    let ctx = Context::new(q, mode);
    let ref_cell = RefCell::new(ctx);
    let rc = Rc::new(ref_cell);
    ContextHandle(rc)
}
