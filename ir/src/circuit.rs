use la_arena::Arena;
use thin_vec::ThinVec;

use crate::{
    SupportedType,
    gate::{Gate, GateIdx},
};

#[derive(Clone, Debug)]
pub struct Circuit {
    q: SupportedType,
    // This is a topologically ordered DAG
    pub(crate) gates: Arena<Gate>,
    pub(crate) inputs: ThinVec<GateIdx>,
    pub(crate) outputs: ThinVec<GateIdx>,
}

impl Circuit {
    pub fn with(
        q: SupportedType,
        gates: Arena<Gate>,
        inputs: ThinVec<GateIdx>,
        outputs: ThinVec<GateIdx>,
    ) -> Self {
        Self {
            q,
            gates,
            inputs,
            outputs,
        }
    }

    pub fn gates(&self) -> &Arena<Gate> {
        &self.gates
    }

    pub fn inputs(&self) -> &[GateIdx] {
        self.inputs.as_slice()
    }
    pub fn outputs(&self) -> &[GateIdx] {
        self.outputs.as_slice()
    }
}
