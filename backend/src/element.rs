use ir::Supported;

pub trait NoisyElement {
    fn get_noise(&self) -> usize;
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct MockFHEElement<T: Supported> {
    pub(crate) value: T,
    pub(crate) depth: usize,
    pub(crate) noise: usize,
}

impl<T: Supported> NoisyElement for MockFHEElement<T> {
    fn get_noise(&self) -> usize {
        self.noise
    }
}

impl<T: Supported> NoisyElement for T {
    fn get_noise(&self) -> usize {
        0
    }
}
