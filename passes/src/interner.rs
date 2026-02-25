use std::{collections::HashMap, hash::Hash};

use fxhash::FxBuildHasher;
use la_arena::{Arena, Idx};

type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

pub trait Internable: Clone + Eq + Hash + PartialEq {}
#[derive(Clone, Debug, Default)]
/// Given an arbitrary hashable node, if it already exists, instead of forming the new, returns the old.
pub struct Interner<I: Internable> {
    map: FxHashMap<I, Idx<I>>,
}

impl<I: Internable> Interner<I> {
    pub fn new() -> Self {
        Self {
            map: HashMap::<I, Idx<I>, FxBuildHasher>::with_hasher(FxBuildHasher::default()),
        }
    }
    pub fn intern(&mut self, key: I, arena: &mut Arena<I>) -> Idx<I> {
        if let Some(idx) = self.get(&key) {
            return *idx;
        }
        let idx = arena.alloc(key.clone());
        self.add(key, idx);
        idx
    }
    fn add(&mut self, key: I, value: Idx<I>) {
        self.map.insert(key, value);
    }
    fn get(&self, key: &I) -> Option<&Idx<I>> {
        self.map.get(key)
    }
}
