//! Shared constants and pure math for UST1 / vFDUSD.
//!
//! # Invariants (cross-linked from tests)
//!
//! - **INV-MATH-001**: `RATE_SCALE` fixed-point: raw rate `R` means `R/RATE_SCALE` FDUSD per 1 vFDUSD.
//! - **INV-MATH-002**: Fee applies via basis points (`fee_bps`) using [`math::apply_fee_ust1`]; default 1% with
//!   accounting split in [`fee_split`] (UST1 window + native wrap contracts).
//! - **INV-SWAP-002**: Reverse gross UST1 → vFDUSD: see `math` module docs and `inv_swap_002_*` tests.

pub mod error;
pub mod fee_split;
pub mod math;
pub mod oracle_policy;

pub use error::MathError;

/// Fixed-point scale for oracle rate (matches typical 1e18 Venus-style exchange rates).
pub const RATE_SCALE: u128 = 1_000_000_000_000_000_000; // 1e18

/// Default swap fee: 1.0% on the UST1 leg (each direction); see [`fee_split`] for accounting.
pub const DEFAULT_FEE_BPS: u16 = 100;

/// Default per-tx UST1 notional cap in raw base units (6 decimals): **1,000 UST1**.
///
/// Governance-updatable after instantiate via `ust1-window` `SetLimits`.
pub const DEFAULT_PER_TX_UST1_LIMIT: u128 = 1_000_000_000;

/// Default rolling 24h UST1 notional cap in raw base units (6 decimals): **10,000 UST1**.
///
/// Governance-updatable after instantiate via `ust1-window` `SetLimits`.
pub const DEFAULT_ROLLING_24H_UST1_LIMIT: u128 = 10_000_000_000;

/// Max relative increase of oracle rate within one UTC calendar day (2%).
pub const MAX_DAILY_INCREASE_BPS: u16 = 200;

/// Minimum seconds between on-chain oracle updates.
pub const MIN_ORACLE_UPDATE_INTERVAL_SECS: u64 = 4 * 60 * 60;

/// Default maximum age of the oracle rate for swap execution (`ust1-window` staleness guard).
///
/// Must be at least [`MIN_ORACLE_UPDATE_INTERVAL_SECS`] so swaps are not impossible between
/// on-chain oracle updates.
///
/// Off-chain oracle service defaults (poll / silence) are aligned to this budget — see
/// `ust1-oracle-service` **INV-ORACLE-OPS-POLL-001** / **INV-ORACLE-OPS-SILENCE-001**,
/// `docs/DEPLOYMENT.md`, `skills/oracle-ops-poll-silence/SKILL.md`, and
/// glab [#24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24).
pub const DEFAULT_MAX_ORACLE_AGE_SECS: u64 = 6 * 60 * 60;

/// Basis points denominator.
pub const BPS_DENOM: u128 = 10_000;
