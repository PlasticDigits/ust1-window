use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub governance: String,
    pub oracle_operator: String,
    /// Initial fixed-point rate (typically `10^18` for 1:1).
    pub initial_rate: Uint128,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Only `oracle_operator`. Enforces 4h throttle, UTC daily 2% cap, monotonicity.
    UpdateRate {
        new_rate: Uint128,
    },
    /// Governance: rotate oracle bot wallet.
    SetOracleOperator {
        address: String,
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
    #[returns(StateResponse)]
    State {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub governance: String,
    pub oracle_operator: String,
    pub paused: bool,
}

#[cw_serde]
pub struct StateResponse {
    pub rate: Uint128,
    pub last_update_sec: u64,
    pub utc_day_id: u64,
    pub day_baseline_rate: Uint128,
}

#[cw_serde]
pub struct MigrateMsg {}
