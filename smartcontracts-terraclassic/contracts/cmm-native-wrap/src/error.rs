use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("unauthorized")]
    Unauthorized {},

    #[error("contract is paused")]
    Paused {},

    #[error("unknown or disallowed cw20 hook")]
    InvalidCw20Hook {},

    #[error("wrap per-tx limit exceeded")]
    PerTxLimit {},

    #[error("wrap rolling 24h limit exceeded")]
    RollingLimit {},

    #[error("native balance too low for unwrap")]
    InsufficientNative {},

    #[error("below minimum native receive")]
    BelowMinimum {},

    #[error("invalid native funds: expected exactly one coin")]
    InvalidNativeFunds {},

    #[error("unknown native denom for wrap")]
    UnknownNativeDenom {},

    #[error("unknown cw20 token for unwrap")]
    UnknownWrappedToken {},

    #[error("instantiate must configure exactly {expected} native pairs")]
    InvalidPairCount { expected: usize },

    #[error("instantiate pairs must be {a} and {b} exactly once each")]
    InvalidPairDenoms { a: String, b: String },

    #[error("duplicate wrapped token address")]
    DuplicateWrappedToken {},

    #[error("math error: {0}")]
    Math(#[from] ust1_common::MathError),

    #[error("no pending governance proposal")]
    NoPendingGovernance {},

    #[error("invalid governance proposal")]
    InvalidGovernanceProposal {},

    #[error("overflow")]
    Overflow(#[from] OverflowError),
}
