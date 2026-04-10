use std::cmp::max;

use ir::{
    Supported,
    circuit::Circuit,
    gate::{Gate, GateIdx},
};
use op::BinOp;
use thin_vec::{ThinVec, thin_vec};

/*
* For multiplicative depth:
    -	depth[input/const] = 0
    -	depth[add/sub] = max(depth[lhs], depth[rhs])
    -	depth[mul] = max(depth[a], depth[b]) + 1
* For total depth:
    -	depth[input/const] = 1
    -	depth[add/sub] = max(depth[a], depth[b])
    -	depth[mul] = max(depth[a], depth[b])
*/
#[derive(Clone, Debug)]
pub struct DepthAnalysis {
    pub mul: ThinVec<usize>,
    pub gate: ThinVec<usize>,
}

pub trait Counter<T: Supported> {
    fn count(gate: &Gate<T>) -> usize;
}

pub struct MulCounter;

impl<T: Supported> Counter<T> for MulCounter {
    fn count(gate: &Gate<T>) -> usize {
        if matches!(gate, Gate::BinOp(BinOp::Mul, _, _)) {
            1
        } else {
            0
        }
    }
}

pub struct TotalGateDepthCounter;

impl<T: Supported> Counter<T> for TotalGateDepthCounter {
    fn count(gate: &Gate<T>) -> usize {
        if matches!(gate, Gate::Input(_) | Gate::Const(_)) {
            0
        } else {
            1
        }
    }
}

pub fn get_depth<C, TC, T: Supported>(circuit: &Circuit<T>, of: GateIdx<T>) -> (usize, usize)
where
    C: Counter<T>,
    TC: Counter<T>,
{
    let gate = circuit.gates()[of];
    let c_depth = C::count(&gate);
    let t_depth = TC::count(&gate);
    let depth = (c_depth, t_depth);
    match gate {
        Gate::Tombstone => depth,
        Gate::Input(_) | Gate::Const(_) => depth,
        Gate::BinOp(_, lhs, rhs) => {
            let left_depth = get_depth::<C, TC, T>(circuit, lhs);
            let right_depth = get_depth::<C, TC, T>(circuit, rhs);
            let c_depth = max(left_depth.0, right_depth.0) + depth.0;
            let t_depth = max(left_depth.1, right_depth.1) + depth.1;
            (c_depth, t_depth)
        }
    }
}

pub fn depth_analysis_of<T: Supported>(circuit: &Circuit<T>) -> DepthAnalysis {
    let num_outputs = circuit.outputs().len();
    let mut mul_depths = thin_vec![0; num_outputs];
    let mut total_depths = thin_vec![0; num_outputs];

    for (ix, out) in circuit.outputs().iter().enumerate() {
        let (mul_depth, total_depth) =
            get_depth::<MulCounter, TotalGateDepthCounter, T>(circuit, *out);
        mul_depths[ix] = mul_depth;
        total_depths[ix] = total_depth;
    }

    DepthAnalysis {
        mul: mul_depths,
        gate: total_depths,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use la_arena::{Arena, Idx};

    fn into_gate_idx<T: Supported>(i: u32) -> GateIdx<T> {
        Idx::from_raw(i.into())
    }

    #[test]
    fn mul_depth_is_one_less_total_depth() {
        let gates: ThinVec<Gate<u64>> = thin_vec![
            Gate::Input(0),
            Gate::Input(1),
            Gate::BinOp(BinOp::Mul, into_gate_idx(0), into_gate_idx(1)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(2))
        ];
        let inputs = thin_vec![into_gate_idx::<u64>(0), into_gate_idx(1)];
        let outputs = thin_vec![into_gate_idx::<u64>(3)];
        let q = 11u64;

        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        );

        let depth_analysis = depth_analysis_of(&circuit);
        assert_eq!(thin_vec![2], depth_analysis.gate);
        assert_eq!(1, depth_analysis.mul.len());
        assert_eq!(2, depth_analysis.mul[0]);
    }

    #[test]
    fn multiple_output_circuit_mul_depth_is_one_less_total_depth() {
        let gates: ThinVec<Gate<u64>> = thin_vec![
            Gate::Input(0),
            Gate::Input(1),
            Gate::BinOp(BinOp::Add, into_gate_idx(0), into_gate_idx(1)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(2)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(3), into_gate_idx(3)),
            Gate::BinOp(BinOp::Mul, into_gate_idx(2), into_gate_idx(3))
        ];
        let inputs = thin_vec![into_gate_idx::<u64>(0), into_gate_idx(1)];
        let outputs = thin_vec![into_gate_idx::<u64>(3), into_gate_idx(4), into_gate_idx(5)];
        let q = 11u64;

        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        );

        let depth_analysis = depth_analysis_of(&circuit);
        assert_eq!(thin_vec![2, 3, 3], depth_analysis.gate);
        assert_eq!(thin_vec![1, 2, 2], depth_analysis.mul);
    }

    fn assert_supported_depth<T: Supported>()
    where
        MulCounter: Counter<T>,
        TotalGateDepthCounter: Counter<T>,
    {
        // Compile-time check that depth analysis works for all supported types
        let gates: ThinVec<Gate<T>> = thin_vec![Gate::Input(0)];
        let inputs = thin_vec![into_gate_idx::<T>(0)];
        let outputs = thin_vec![into_gate_idx::<T>(0)];
        let q = T::default();
        let circuit = Circuit::with(
            q,
            Arena::from_iter(gates.iter().map(Clone::clone)),
            inputs,
            outputs,
        );
        let _ = depth_analysis_of(&circuit);
    }

    #[test]
    fn all_supported_integer_types_work_for_depth_analysis() {
        assert_supported_depth::<i8>();
        assert_supported_depth::<i16>();
        assert_supported_depth::<i32>();
        assert_supported_depth::<i64>();
        assert_supported_depth::<u8>();
        assert_supported_depth::<u16>();
        assert_supported_depth::<u32>();
        assert_supported_depth::<u64>();
    }
}
