//! Persistent config and rolling volume keyed by native denom.

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

pub const CONTRACT_NAME: &str = "crates.io:cmm-native-wrap";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Native denom for LUNC on Terra Classic.
pub const LUNC_DENOM: &str = "uluna";
/// Native denom for USTC on Terra Classic.
pub const USTC_DENOM: &str = "uusd";

/// # Invariants
///
/// - **INV-LIMIT-NATIVE-001**: Wrapped output (deposit) and gross wrapped input (withdrawal)
///   per swap and per rolling window must respect governance caps **per native denom**.
#[cw_serde]
pub struct DenomPair {
    pub native_denom: String,
    pub wrapped_token: Addr,
    pub per_tx_wrap_limit: Uint128,
    pub rolling_24h_wrap_limit: Uint128,
}

#[cw_serde]
pub struct Config {
    pub governance: Addr,
    pub fee_bps: u16,
    pub paused: bool,
    pub pairs: Vec<DenomPair>,
}

pub const CONFIG: Item<Config> = Item::new("cfg");

/// Rolling 24h volume tracker (wrapped-token smallest units — minted on wrap or burned on unwrap).
#[cw_serde]
pub struct RollingVolume {
    pub window_start_sec: u64,
    pub volume_wrap: Uint128,
}

pub const ROLLING: Map<&str, RollingVolume> = Map::new("roll");

#[cw_serde]
pub struct PendingGovernance {
    pub new_address: Addr,
}

pub const PENDING_GOVERNANCE: Item<PendingGovernance> = Item::new("pending_gov");
