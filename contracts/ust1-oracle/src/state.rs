use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

pub const CONTRACT_NAME: &str = "crates.io:ust1-oracle";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Governance: admin ops. Oracle operator: rate updates only.
///
/// # Invariants
///
/// - **INV-ORACLE-PAUSE-001**: When `paused`, `UpdateRate` is rejected and `State.paused`
///   is `true` so all window readers fail closed immediately (GitLab #22 / audit C-2 #1).
///   Pause and unpause are governance-only (no operator auto-unpause).
#[cw_serde]
pub struct Config {
    pub governance: Addr,
    pub oracle_operator: Addr,
    pub paused: bool,
}

pub const CONFIG: Item<Config> = Item::new("cfg");

/// On-chain oracle state.
///
/// # Invariants
///
/// - **INV-ORACLE-MONO-001**: `rate` never decreases except via explicit migrate (not used for value).
/// - **INV-ORACLE-DAILY-001**: Within a UTC day, `rate <= day_baseline_rate * (10000 + MAX_DAILY_INCREASE_BPS) / 10000`
///   where `day_baseline_rate` is `rate` at the instant the UTC day boundary was crossed.
#[cw_serde]
pub struct OracleState {
    /// Fixed-point rate R: FDUSD per vFDUSD (`ust1_common::RATE_SCALE`).
    pub rate: Uint128,
    /// Unix timestamp (seconds) of last successful `UpdateRate`, or 0 if never updated after init.
    pub last_update_sec: u64,
    /// `block_time / 86400` (UTC day index).
    pub utc_day_id: u64,
    /// Baseline rate for the current `utc_day_id` (snapshot at day boundary).
    pub day_baseline_rate: Uint128,
}

pub const ORACLE_STATE: Item<OracleState> = Item::new("oracle");

#[cw_serde]
pub struct PendingGovernance {
    pub new_address: Addr,
}

pub const PENDING_GOVERNANCE: Item<PendingGovernance> = Item::new("pending_gov");
