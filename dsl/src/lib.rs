use std::{cell::RefCell, rc::Rc};

use crate::ctx::{Context, ContextHandle};

pub mod add;
pub mod ctx;
pub mod expr;
pub mod hash;
pub mod mul;
pub mod op;
pub mod sub;

pub type SupportedType = u64;

pub fn new_context(q: SupportedType) -> ContextHandle {
    let ctx = Context::new(q);
    let ref_cell = RefCell::new(ctx);
    let rc = Rc::new(ref_cell);
    ContextHandle(rc)
}
