use std::{cell::RefCell, rc::Rc};

use crate::ctx::{CompilationMode, Context, ContextHandle};

pub mod add;
pub mod compile;
pub mod ctx;
pub mod expr;
pub mod folding;
pub mod hash;
pub mod mul;
pub mod sub;

pub type SupportedType = u64;

pub fn new_strict_context(q: SupportedType) -> ContextHandle {
    let mode = CompilationMode::Strict;
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
