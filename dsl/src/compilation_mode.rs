use std::ops::{Add, AddAssign, BitAnd, Not, Sub};

use enum_iterator::Sequence;

#[derive(Clone, Debug, Default)]
pub enum CompilationMode {
    Loose,
    #[default]
    StrictAll,
    // last 3 bits: [on_operations] [on_constants] [on_inputs]
    Strict(Strictness),
}

impl CompilationMode {
    pub fn with(strictness: Strictness) -> Self {
        Self::Strict(strictness)
    }
}

impl From<Strictness> for CompilationMode {
    fn from(value: Strictness) -> Self {
        if value.0 == 0 {
            Self::Loose
        } else if &value
            & &Strictness::from(
                [StrictnessOn::Input, StrictnessOn::Const, StrictnessOn::Op].as_slice(),
            )
        {
            Self::StrictAll
        } else {
            Self::Strict(value)
        }
    }
}

#[derive(Clone, Debug, Default, Sequence)]
pub enum StrictnessOn {
    #[default]
    Input,
    Const,
    Op,
}

#[derive(Clone, Debug, Default)]
pub struct Strictness(u8);

impl From<&CompilationMode> for Strictness {
    fn from(value: &CompilationMode) -> Self {
        match value {
            CompilationMode::Loose => Self(0),
            CompilationMode::StrictAll => Strictness::from(
                [StrictnessOn::Input, StrictnessOn::Const, StrictnessOn::Op].as_slice(),
            ),
            CompilationMode::Strict(strictness) => strictness.clone(),
        }
    }
}

impl From<&StrictnessOn> for Strictness {
    fn from(value: &StrictnessOn) -> Self {
        let u = match value {
            StrictnessOn::Input => 1 << 0,
            StrictnessOn::Const => 1 << 1,
            StrictnessOn::Op => 1 << 2,
        };
        Strictness(u)
    }
}

impl From<StrictnessOn> for Strictness {
    fn from(value: StrictnessOn) -> Self {
        Self::from(&value)
    }
}

#[allow(clippy::suspicious_arithmetic_impl)]
#[allow(clippy::suspicious_op_assign_impl)]
impl Add for Strictness {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let or = self.0 | rhs.0;
        Self(or)
    }
}

#[allow(clippy::suspicious_arithmetic_impl)]
#[allow(clippy::suspicious_op_assign_impl)]
impl AddAssign for Strictness {
    fn add_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[allow(clippy::suspicious_arithmetic_impl)]
#[allow(clippy::suspicious_op_assign_impl)]
impl Sub for Strictness {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let and = self.0 & (!rhs.0);
        Self(and)
    }
}

impl Not for Strictness {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl BitAnd for &Strictness {
    type Output = bool;

    fn bitand(self, rhs: Self) -> Self::Output {
        (self.0 & rhs.0) > 0
    }
}

impl BitAnd for Strictness {
    type Output = bool;

    fn bitand(self, rhs: Self) -> Self::Output {
        (&self).bitand(&rhs)
    }
}

impl From<&[StrictnessOn]> for Strictness {
    fn from(value: &[StrictnessOn]) -> Self {
        let mut combined = Self(0);
        for v in value {
            combined += v.into();
        }
        combined
    }
}
