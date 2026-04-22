//! JSON messages for native wrap / unwrap (no oracle).

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;

#[cw_serde]
pub struct PairInstantiateMsg {
    pub native_denom: String,
    pub wrapped_token: String,
    pub per_tx_wrap_limit: Uint128,
    pub rolling_24h_wrap_limit: Uint128,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub governance: String,
    pub fee_bps: u16,
    /// Must contain **exactly** [`crate::state::LUNC_DENOM`] and [`crate::state::USTC_DENOM`], each once.
    pub pairs: Vec<PairInstantiateMsg>,
}

#[cw_serde]
pub enum Cw20HookMsg {
    /// Burn received wrapped tokens and send native to the cw20 `sender`.
    Unwrap { min_native_out: Uint128 },
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Send **exactly one** native coin (`uluna` or `uusd`); mint wrapped cw20 to the sender.
    Wrap {},
    Receive(Cw20ReceiveMsg),
    SetPairLimits {
        native_denom: String,
        per_tx_wrap_limit: Uint128,
        rolling_24h_wrap_limit: Uint128,
    },
    SetPaused {
        paused: bool,
    },
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
    #[returns(EffectiveWrapResponse)]
    EffectiveWrap { denom: String },
}

#[cw_serde]
pub struct DenomPairResponse {
    pub native_denom: String,
    pub wrapped_token: String,
    pub per_tx_wrap_limit: Uint128,
    pub rolling_24h_wrap_limit: Uint128,
}

#[cw_serde]
pub struct ConfigResponse {
    pub governance: String,
    pub fee_bps: u16,
    pub paused: bool,
    pub pairs: Vec<DenomPairResponse>,
}

#[cw_serde]
pub struct EffectiveWrapResponse {
    pub denom: String,
    pub fee_bps: u16,
    pub paused: bool,
    pub per_tx_wrap_limit: Uint128,
    pub rolling_24h_wrap_limit: Uint128,
    pub rolling_window_start_sec: u64,
    pub rolling_volume_wrap: Uint128,
    pub wrapped_token: String,
}

#[cw_serde]
pub struct MigrateMsg {}
