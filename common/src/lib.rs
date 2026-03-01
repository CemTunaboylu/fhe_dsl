use la_arena::{Idx, RawIdx};

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
