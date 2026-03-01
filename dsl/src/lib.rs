use std::{cell::RefCell, rc::Rc};

use crate::{
    compilation_mode::*,
    ctx::{Context, ContextHandle},
    expr::{Expr, ExprIdx},
};

use common::idx_to_u32 as c_idx_to_u32;

pub mod add;
pub mod compilation_mode;
pub mod compile;
pub mod ctx;
pub mod expr;
pub mod folding;
pub mod hash;
pub mod mul;
pub mod sub;

pub type SupportedType = u64;

pub(crate) fn idx_to_u32(expr_idx: ExprIdx) -> u32 {
    c_idx_to_u32::<Expr>(expr_idx)
}

pub fn new_folding_strict_context(q: SupportedType) -> ContextHandle {
    let strictness: Strictness = [StrictnessOn::Input, StrictnessOn::Op].as_slice().into();
    let mode = CompilationMode::with(strictness);
    let ctx = Context::new(q, mode);
    let ref_cell = RefCell::new(ctx);
    let rc = Rc::new(ref_cell);
    ContextHandle(rc)
}

pub fn new_strict_context(q: SupportedType) -> ContextHandle {
    let mode = CompilationMode::StrictAll;
    let ctx = Context::new(q, mode);
    let ref_cell = RefCell::new(ctx);
    let rc = Rc::new(ref_cell);
    ContextHandle(rc)
}

pub fn new_loose_context(q: SupportedType) -> ContextHandle {
    let mode = CompilationMode::Loose;
    let ctx = Context::new(q, mode);
    let ref_cell = RefCell::new(ctx);
    let rc = Rc::new(ref_cell);
    ContextHandle(rc)
}
