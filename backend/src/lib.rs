use ir::{Supported, circuit::Circuit};
use thin_vec::ThinVec;

use error::BackendError;

use crate::{element::NoisyElement, noise::Noise};

pub mod element;
pub mod error;
mod iterative;
pub mod mock_fhe;
mod noise;
pub mod plain;
pub mod validation;

pub type BackendResult<T> = Result<T, BackendError>;

pub trait Backend<S: Supported> {
    type Elem: NoisyElement;

    fn add(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem;
    fn sub(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem;
    fn mul(&mut self, lhs: &Self::Elem, rhs: &Self::Elem) -> Self::Elem;
    fn input(&mut self, c: S) -> Self::Elem;
    fn constant(&mut self, c: S) -> Self::Elem;
    fn eval(
        &mut self,
        circuit: &Circuit<S>,
        with: &[S],
    ) -> BackendResult<ThinVec<Self::Elem>>;
    fn eval_outputs(
        &mut self,
        circuit: &Circuit<S>,
        with: &[S],
    ) -> BackendResult<ThinVec<Self::Elem>>;
    fn set_plain_text_modulus(&mut self, q: S);
    fn get_noise_mul(&mut self) -> &mut impl Noise;
}
