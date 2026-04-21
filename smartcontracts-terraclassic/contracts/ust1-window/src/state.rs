use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

pub const CONTRACT_NAME: &str = "crates.io:ust1-window";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn default_max_oracle_age_sec() -> u64 {
    ust1_common::DEFAULT_MAX_ORACLE_AGE_SECS
}

/// # Invariants
///
/// - **INV-LIMIT-001**: UST1 notional per swap and per rolling window must respect governance caps.
#[cw_serde]
pub struct Config {
    pub governance: Addr,
    pub oracle: Addr,
    pub vfdusd_token: Addr,
    pub ust1_token: Addr,
    /// UST1-leg swap fee in basis points; updatable via governance (`SetFeeBps`).
    pub fee_bps: u16,
    pub per_tx_ust1_limit: Uint128,
    pub rolling_24h_ust1_limit: Uint128,
    pub paused: bool,
    /// Reject deposits/withdraws if `block_time - oracle.last_update_sec` exceeds this (seconds).
    #[serde(default = "default_max_oracle_age_sec")]
    pub max_oracle_age_sec: u64,
}

pub const CONFIG: Item<Config> = Item::new("cfg");

/// Rolling 24h volume tracker (UST1 smallest units).
///
/// `window_start_sec` is reset when a swap occurs after `window_start_sec + 86400`.
#[cw_serde]
pub struct RollingVolume {
    pub window_start_sec: u64,
    pub volume_ust1: Uint128,
}

pub const ROLLING: Item<RollingVolume> = Item::new("roll");

#[cw_serde]
pub struct PendingGovernance {
    pub new_address: Addr,
}

pub const PENDING_GOVERNANCE: Item<PendingGovernance> = Item::new("pending_gov");
