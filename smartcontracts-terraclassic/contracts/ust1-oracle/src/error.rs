use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("unauthorized")]
    Unauthorized {},

    #[error("contract is paused")]
    Paused {},

    #[error("rate must be strictly positive")]
    ZeroRate {},

    #[error("rate cannot decrease (monotonic oracle)")]
    RateDecreased {},

    #[error("rate exceeds UTC daily increase cap")]
    DailyCapExceeded {},

    #[error("oracle update too soon (min interval {min_interval}s)")]
    UpdateTooSoon { min_interval: u64 },

    #[error("no pending governance proposal")]
    NoPendingGovernance {},

    #[error("invalid governance proposal")]
    InvalidGovernanceProposal {},
}
