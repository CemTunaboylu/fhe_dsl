use la_arena::{Idx, RawIdx};
use std::fmt::Debug;
use std::hash::Hash;

use num_traits::{PrimInt, WrappingAdd, WrappingMul, WrappingSub};

/// Marker trait for all integer types that can be used as plaintext values in the FHE DSL.
/// Requires arithmetic operations, plus the standard derive-friendly traits needed for hashing, ordering, and copying through arenas.
pub trait Supported:
    Copy + Clone + Debug + Default + Hash + PrimInt + WrappingAdd + WrappingSub + WrappingMul
{
}

macro_rules! impl_supported {
    ($($t:ty),* $(,)?) => {
        $(impl Supported for $t {})*
    };
}

impl_supported!(i8, i16, i32, i64, u8, u16, u32, u64);

pub fn idx_to_u32<I>(some_idx: Idx<I>) -> u32 {
    some_idx.into_raw().into_u32()
}
pub fn u32_to_idx<I>(i: u32) -> Idx<I> {
    Idx::<I>::from_raw(RawIdx::from_u32(i))
}

pub fn idx_to_usize<I>(some_idx: Idx<I>) -> usize {
    idx_to_u32::<I>(some_idx) as usize
}
pub fn usize_to_idx<I>(i: usize) -> Idx<I> {
    u32_to_idx::<I>(i as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_supported<T: Supported>() {}

    #[test]
    fn all_integer_types_implement_supported() {
        assert_supported::<i8>();
        assert_supported::<i16>();
        assert_supported::<i32>();
        assert_supported::<i64>();
        assert_supported::<u8>();
        assert_supported::<u16>();
        assert_supported::<u32>();
        assert_supported::<u64>();
    }
}
