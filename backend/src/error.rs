use std::fmt::Display;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BackendError {
    InvalidInputLen(usize, usize),
}

impl Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::InvalidInputLen(exp, got) => {
                write!(f, "expected input of length {}, got {}", exp, got)
            }
        }
    }
}
