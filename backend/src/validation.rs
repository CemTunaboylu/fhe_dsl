use ir::{Supported, circuit::Circuit};

use crate::{BackendResult, error::BackendError};

pub(crate) fn validate_same_length<T: Supported>(
    circuit: &Circuit<T>,
    inputs: &[T],
) -> BackendResult<()> {
    if circuit.inputs().len() != inputs.len() {
        let exp_len = circuit.inputs().len();
        let got_len = inputs.len();
        BackendResult::Err(BackendError::InvalidInputLen(exp_len, got_len))
    } else {
        BackendResult::Ok(())
    }
}
