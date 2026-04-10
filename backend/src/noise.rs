use crate::error::BackendError;

pub trait Noise {
    fn new(noise_budget: usize) -> Self;
    fn get_noise_budget(&self) -> usize;
    fn update(&mut self, value: usize, index: usize) -> Result<(), BackendError>;
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct MockNoise {
    noise_budget: usize,
    max_so_far: usize,
    index: usize,
}

impl Noise for MockNoise {
    fn new(noise_budget: usize) -> Self {
        Self {
            noise_budget,
            ..Default::default()
        }
    }
    fn get_noise_budget(&self) -> usize {
        self.noise_budget
    }

    fn update(&mut self, value: usize, index: usize) -> Result<(), BackendError> {
        if value < self.max_so_far {
            return Ok(());
        }

        self.max_so_far = value;
        self.index = index;

        if self.max_so_far <= self.noise_budget {
            return Ok(());
        }

        Err(BackendError::NoiseBudgetExceeded(
            self.noise_budget,
            self.max_so_far,
            self.index,
        ))
    }
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Noiseless {}

impl Noise for Noiseless {
    fn new(_noise_budget: usize) -> Self {
        Self {}
    }
    fn get_noise_budget(&self) -> usize {
        0
    }

    fn update(&mut self, _value: usize, _index: usize) -> Result<(), BackendError> {
        Ok(())
    }
}
