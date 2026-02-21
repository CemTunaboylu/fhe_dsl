use std::{cell::RefCell, rc::Rc};

use crate::ctx::{Context, ContextHandle};

pub mod ctx;
pub mod expr;

pub fn new_context(q: usize) -> ContextHandle {
    let ctx = Context::new(q);
    let ref_cell = RefCell::new(ctx);
    let rc = Rc::new(ref_cell);
    ContextHandle(rc)
}
