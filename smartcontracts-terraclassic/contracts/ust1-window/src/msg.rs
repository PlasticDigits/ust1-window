use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;

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

#[cw_serde]
pub struct MigrateMsg {}
