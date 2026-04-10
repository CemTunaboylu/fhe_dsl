use la_arena::Arena;
use thin_vec::ThinVec;

use crate::{
    Supported,
    gate::{Gate, GateIdx},
};

#[derive(Clone, Debug)]
pub struct Circuit<T: Supported> {
    pub plain_text_modulus: T,
    // This is a topologically ordered DAG
    pub(crate) gates: Arena<Gate<T>>,
    pub(crate) inputs: ThinVec<GateIdx<T>>,
    pub(crate) outputs: ThinVec<GateIdx<T>>,
}

impl<T: Supported> Circuit<T> {
    pub fn with(
        q: T,
        gates: Arena<Gate<T>>,
        inputs: ThinVec<GateIdx<T>>,
        outputs: ThinVec<GateIdx<T>>,
    ) -> Self {
        Self {
            plain_text_modulus: q,
            gates,
            inputs,
            outputs,
        }
    }
    pub fn gates(&self) -> &Arena<Gate<T>> {
        &self.gates
    }

    pub fn inputs(&self) -> &[GateIdx<T>] {
        self.inputs.as_slice()
    }
    pub fn outputs(&self) -> &[GateIdx<T>] {
        self.outputs.as_slice()
    }
}
