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

    #[error("UST1 per-tx limit exceeded")]
    PerTxLimit {},

    #[error("UST1 rolling 24h limit exceeded")]
    RollingLimit {},

    #[error("vFDUSD balance too low for withdrawal")]
    InsufficientVfdusd {},

    #[error("below minimum vFDUSD receive")]
    BelowMinimum {},

    #[error("oracle rate is stale or not yet committed on-chain (run at least one UpdateRate)")]
    OracleStale {},

    #[error("max oracle age must be at least {min_seconds}s (oracle min update interval)")]
    MaxOracleAgeTooShort { min_seconds: u64 },

    #[error("math error: {0}")]
    Math(#[from] ust1_common::MathError),

    #[error("no pending governance proposal")]
    NoPendingGovernance {},

    #[error("invalid governance proposal")]
    InvalidGovernanceProposal {},

    #[error("overflow")]
    Overflow(#[from] OverflowError),
}
