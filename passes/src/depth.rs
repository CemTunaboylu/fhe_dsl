use std::cmp::max;

use ir::{
    circuit::Circuit,
    gate::{Gate, GateIdx},
};
use op::BinOp;
use thin_vec::{ThinVec, thin_vec};

/*
* For multiplicative depth:
    -	depth[input/const] = 0
    -	depth[add/sub] = 0
    -	depth[mul] = max(depth[a], depth[b]) + 1
* For total depth:
    -	depth[input/const] = 1
    -	depth[add/sub] = max(depth[a], depth[b])
    -	depth[mul] = max(depth[a], depth[b])
*/

pub struct DepthAnalysis {
    mul: ThinVec<usize>,
    total: ThinVec<usize>,
}

trait Counter {
    fn count(gate: &Gate) -> usize;
}

struct MulCounter;

impl Counter for MulCounter {
    fn count(gate: &Gate) -> usize {
        if matches!(gate, Gate::BinOp(BinOp::Mul, _, _)) {
            1
        } else {
            0
        }
    }
}

struct TotalDepthCounter;

impl Counter for TotalDepthCounter {
    fn count(_gate: &Gate) -> usize {
        1
    }
}

fn get_depth<C, T>(circuit: &Circuit, of: GateIdx) -> (usize, usize)
where
    C: Counter,
    T: Counter,
{
    let gate = circuit.gates()[of];
    let c_depth = C::count(&gate);
    let t_depth = T::count(&gate);
    let depth = (c_depth, t_depth);
    match gate {
        Gate::Input(_) | Gate::Const(_) => depth,
        Gate::BinOp(_, lhs, rhs) => {
            let left_depth = get_depth::<C, T>(circuit, lhs);
            let right_depth = get_depth::<C, T>(circuit, rhs);
            let c_depth = max(left_depth.0, right_depth.0) + depth.0;
            let t_depth = max(left_depth.1, right_depth.1) + depth.1;
            (c_depth, t_depth)
        }
    }
}

pub fn depth_analysis_of(circuit: &Circuit) -> DepthAnalysis {
    let num_outputs = circuit.outputs().len();
    let mut mul_depths = thin_vec![0; num_outputs];
    let mut total_depths = thin_vec![0; num_outputs];

    for (ix, out) in circuit.outputs().iter().enumerate() {
        let (mul_depth, total_depth) = get_depth::<MulCounter, TotalDepthCounter>(circuit, *out);
        mul_depths[ix] = mul_depth;
        total_depths[ix] = total_depth;
    }

    DepthAnalysis {
        mul: mul_depths,
        total: total_depths,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use la_arena::{Arena, Idx};

    fn into_gate_idx(i: u32) -> GateIdx {
        Idx::from_raw(i.into())
    }

    #[test]
    fn mul_depth_is_one_less_total_depth() {
        let gates = thin_vec![
            Gate::Input(0),
            Gate::Input(1),
            Gate::BinOp(BinOp::Mul, into_gate_idx(0), into_gate_idx(1)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(2))
        ];
        let inputs = thin_vec![into_gate_idx(0), into_gate_idx(1)];
        let outputs = thin_vec![into_gate_idx(3)];
        let q = 11;

        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        );

        let depth_analysis = depth_analysis_of(&circuit);
        assert_eq!(thin_vec![3], depth_analysis.total);
        assert_eq!(1, depth_analysis.mul.len());
        assert_eq!(2, depth_analysis.mul[0]);
    }

    #[test]
    fn multiple_output_circuit_mul_depth_is_one_less_total_depth() {
        let gates = thin_vec![
            Gate::Input(0),
            Gate::Input(1),
            Gate::BinOp(BinOp::Mul, into_gate_idx(0), into_gate_idx(1)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(2)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(3), into_gate_idx(3)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(3))
        ];
        let inputs = thin_vec![into_gate_idx(0), into_gate_idx(1)];
        let outputs = thin_vec![into_gate_idx(3), into_gate_idx(4), into_gate_idx(5)];
        let q = 11;

        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        );

        let depth_analysis = depth_analysis_of(&circuit);
        assert_eq!(thin_vec![3, 4, 4], depth_analysis.total);
        assert_eq!(thin_vec![2, 3, 3], depth_analysis.mul);
    }
}
