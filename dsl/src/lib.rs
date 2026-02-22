use std::{cell::RefCell, rc::Rc};

use crate::ctx::{Context, ContextHandle};

pub mod add;
pub mod ctx;
pub mod expr;
pub mod mul;
pub mod sub;

pub fn new_context(q: usize) -> ContextHandle {
    let ctx = Context::new(q);
    let ref_cell = RefCell::new(ctx);
    let rc = Rc::new(ref_cell);
    ContextHandle(rc)
}
