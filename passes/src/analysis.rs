use ir::circuit::Circuit;
use thin_vec::ThinVec;

use crate::depth::{DepthAnalysis, MulCounter, TotalGateDepthCounter, get_depth};

#[derive(Clone, Debug)]
pub struct CircuitStats {
    pub num_inputs: usize,
    pub num_outputs: usize,
    pub depth_analysis: DepthAnalysis,
}

pub fn analyse(circuit: &Circuit) -> CircuitStats {
    let num_outputs = circuit.outputs().len();
    let mut mul_depth_analysis = ThinVec::with_capacity(num_outputs);
    let mut gate_depth_analysis = ThinVec::with_capacity(num_outputs);
    for out in circuit.outputs() {
        let (mul, gate) = get_depth::<MulCounter, TotalGateDepthCounter>(circuit, *out);
        mul_depth_analysis.push(mul);
        gate_depth_analysis.push(gate);
    }
    let num_inputs = circuit.inputs().len();
    let depth_analysis = DepthAnalysis {
        mul: mul_depth_analysis,
        gate: gate_depth_analysis,
    };
    CircuitStats {
        num_inputs,
        num_outputs,
        depth_analysis,
    }
}
