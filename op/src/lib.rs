#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
}

impl BinOp {
    pub fn is_associative(&self) -> bool {
        matches!(self, Self::Add | Self::Mul)
    }

    pub fn is_commutative(&self) -> bool {
        matches!(self, Self::Add | Self::Mul)
    }
}
