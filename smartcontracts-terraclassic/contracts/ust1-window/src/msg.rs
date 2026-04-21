use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;
use ust1_oracle::msg::StateResponse as OracleStateResponse;

#[cw_serde]
pub struct InstantiateMsg {
    pub governance: String,
    pub oracle: String,
    pub vfdusd_token: String,
    pub ust1_token: String,
    pub fee_bps: u16,
    pub per_tx_ust1_limit: Uint128,
    pub rolling_24h_ust1_limit: Uint128,
}

#[cw_serde]
pub enum Cw20HookMsg {
    /// Deposit vFDUSD; mints UST1 to `sender` of the cw20 transfer (unless overridden).
    Deposit {},
    /// Burn received UST1 and receive vFDUSD. `amount` in callback is gross UST1.
    Withdraw { min_vfdusd_out: Uint128 },
}

#[cw_serde]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    SetLimits {
        per_tx_ust1_limit: Uint128,
        rolling_24h_ust1_limit: Uint128,
    },
    SetPaused {
        paused: bool,
    },
    /// Governance-only: update UST1-leg fee (`fee_bps`); same validation as instantiate (`<= 10_000`).
    SetFeeBps {
        fee_bps: u16,
    },
    ProposeGovernance {
        address: String,
    },
    AcceptGovernance {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    /// Effective swap limits, rolling-volume state, and oracle `State` as used for the next quote.
    #[returns(EffectiveSwapResponse)]
    EffectiveSwap {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub governance: String,
    pub oracle: String,
    pub vfdusd_token: String,
    pub ust1_token: String,
    pub fee_bps: u16,
    pub per_tx_ust1_limit: Uint128,
    pub rolling_24h_ust1_limit: Uint128,
    pub paused: bool,
}

/// Parameters and oracle view that apply to the next deposit or withdraw (single query for integrators).
#[cw_serde]
pub struct EffectiveSwapResponse {
    pub fee_bps: u16,
    pub per_tx_ust1_limit: Uint128,
    pub rolling_24h_ust1_limit: Uint128,
    pub paused: bool,
    /// Start of the current rolling 24h window (block time seconds), or `0` before first swap.
    pub rolling_window_start_sec: u64,
    /// UST1 notional consumed in the current rolling window.
    pub rolling_volume_ust1: Uint128,
    /// Oracle `State` from the configured oracle (includes `rate` used for swap math).
    pub oracle: OracleStateResponse,
}

#[cw_serde]
pub struct MigrateMsg {}
