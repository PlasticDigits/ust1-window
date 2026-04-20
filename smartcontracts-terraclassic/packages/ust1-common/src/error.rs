use cosmwasm_std::{DivideByZeroError, OverflowError};
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum MathError {
    #[error("overflow: {0}")]
    Overflow(#[from] OverflowError),

    #[error("division by zero")]
    DivisionByZero,

    #[error("fee bps must be <= 10000")]
    InvalidFeeBps,

    #[error("amount does not fit in Uint128")]
    TooLarge,
}

impl From<DivideByZeroError> for MathError {
    fn from(_: DivideByZeroError) -> Self {
        MathError::DivisionByZero
    }
}
