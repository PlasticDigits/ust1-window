//! Shared constants and pure math for UST1 / vFDUSD.
//!
//! # Invariants (cross-linked from tests)
//!
//! - **INV-MATH-001**: `RATE_SCALE` fixed-point: raw rate `R` means `R/RATE_SCALE` FDUSD per 1 vFDUSD.
//! - **INV-MATH-002**: Fee applies to UST1 notional using basis points (`fee_bps`); default 1% with split in [`fee_split`].
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

/// Max relative increase of oracle rate within one UTC calendar day (2%).
pub const MAX_DAILY_INCREASE_BPS: u16 = 200;

/// Minimum seconds between on-chain oracle updates.
pub const MIN_ORACLE_UPDATE_INTERVAL_SECS: u64 = 4 * 60 * 60;

/// Default maximum age of the oracle rate for swap execution (`ust1-window` staleness guard).
///
/// Must be at least [`MIN_ORACLE_UPDATE_INTERVAL_SECS`] so swaps are not impossible between
/// on-chain oracle updates.
pub const DEFAULT_MAX_ORACLE_AGE_SECS: u64 = 6 * 60 * 60;

/// Basis points denominator.
pub const BPS_DENOM: u128 = 10_000;
