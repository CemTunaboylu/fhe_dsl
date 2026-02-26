use ir::{SupportedType, circuit::Circuit};
use thin_vec::ThinVec;

use error::BackendError;

pub mod error;
pub mod plain;

pub type BackendResult<T> = Result<T, BackendError>;

pub trait Backend {
    type Elem;

    fn add(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem;
    fn sub(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem;
    fn mul(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem;
    fn constant(&mut self, c: SupportedType) -> Self::Elem;
    fn eval(
        &mut self,
        circuit: &Circuit,
        with: &[SupportedType],
    ) -> BackendResult<ThinVec<Self::Elem>>;
}
