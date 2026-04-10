use ir::SupportedType;

pub trait NoisyElement {
    fn get_noise(&self) -> usize;
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct MockFHEElement {
    pub(crate) value: SupportedType,
    pub(crate) depth: usize,
    pub(crate) noise: usize,
}

impl NoisyElement for MockFHEElement {
    fn get_noise(&self) -> usize {
        self.noise
    }
}

impl NoisyElement for SupportedType {
    fn get_noise(&self) -> usize {
        0
    }
}
