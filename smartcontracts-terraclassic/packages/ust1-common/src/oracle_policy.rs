//! Off-chain and on-chain shared rules for oracle rate updates.
//!
//! # Invariants
//!
//! - **INV-ORACLE-THROTTLE-001**: If `last_update_sec > 0`, then `now_sec - last_update_sec >= MIN_ORACLE_UPDATE_INTERVAL_SECS`.
//! - **INV-ORACLE-DAILY-001**: After UTC day rollover, baseline resets; `new_rate <= day_baseline * (10000 + MAX_DAILY_INCREASE_BPS) / 10000`.
//!   **Bootstrap:** when `last_update_sec == 0` (never updated), the daily cap is skipped so ops can
//!   seed the live Venus-normalized rate; returned baseline becomes `new_rate`.
//! - **INV-ORACLE-MONO-001**: `new_rate >= old_rate`.

use cosmwasm_std::Uint128;

use crate::math::max_rate_after_daily_cap;
use crate::MIN_ORACLE_UPDATE_INTERVAL_SECS;

/// Roll UTC day and return updated `(utc_day_id, day_baseline_rate)` before applying a new rate.
pub fn roll_utc_day(
    block_time_sec: u64,
    utc_day_id: u64,
    day_baseline_rate: Uint128,
    current_rate: Uint128,
) -> (u64, Uint128) {
    let day_id = block_time_sec / 86_400;
    if day_id != utc_day_id {
        (day_id, current_rate)
    } else {
        (utc_day_id, day_baseline_rate)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OraclePolicyError {
    ZeroRate,
    RateDecreased,
    DailyCapExceeded,
    UpdateTooSoon { min_interval: u64 },
}

/// Validate a proposed oracle update (matches `ust1-oracle` contract).
///
/// Returns `(utc_day_id, day_baseline_rate)` to persist after a successful update.
pub fn check_rate_update(
    now_sec: u64,
    last_update_sec: u64,
    old_rate: Uint128,
    new_rate: Uint128,
    utc_day_id: u64,
    day_baseline_rate: Uint128,
) -> Result<(u64, Uint128), OraclePolicyError> {
    if new_rate.is_zero() {
        return Err(OraclePolicyError::ZeroRate);
    }
    if last_update_sec > 0
        && now_sec.saturating_sub(last_update_sec) < MIN_ORACLE_UPDATE_INTERVAL_SECS
    {
        return Err(OraclePolicyError::UpdateTooSoon {
            min_interval: MIN_ORACLE_UPDATE_INTERVAL_SECS,
        });
    }
    if new_rate < old_rate {
        return Err(OraclePolicyError::RateDecreased);
    }
    let (day_id, baseline) = roll_utc_day(now_sec, utc_day_id, day_baseline_rate, old_rate);

    // First post-instantiate update: allow seeding the real Venus rate (may exceed +2%).
    // Persist baseline = new_rate so later same-day caps are relative to the seeded level.
    if last_update_sec == 0 {
        return Ok((day_id, new_rate));
    }

    let max_r =
        max_rate_after_daily_cap(baseline).map_err(|_| OraclePolicyError::DailyCapExceeded)?;
    if new_rate > max_r {
        return Err(OraclePolicyError::DailyCapExceeded);
    }
    Ok((day_id, baseline))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// **INV-ORACLE-THROTTLE-001**
    #[test]
    fn inv_oracle_throttle_blocks_quick_updates() {
        let r = Uint128::new(crate::RATE_SCALE);
        let err = check_rate_update(100, 50, r, r, 0, r).unwrap_err();
        assert!(matches!(err, OraclePolicyError::UpdateTooSoon { .. }));
    }

    /// First update after init (`last_update_sec == 0`) ignores throttle.
    #[test]
    fn first_update_skips_throttle() {
        let r = Uint128::new(crate::RATE_SCALE);
        let ok = check_rate_update(100, 0, r, r, 0, r);
        assert!(ok.is_ok());
    }

    /// Bootstrap: first update may jump above +2% daily cap; baseline becomes `new_rate`.
    #[test]
    fn first_update_skips_daily_cap_and_seeds_baseline() {
        let old = Uint128::new(crate::RATE_SCALE);
        // ~+22.5% (live Venus-normalized vFDUSD→FDUSD ≈ 1.225e18)
        let new = Uint128::new(1_225_104_516_022_056_627);
        let (day_id, baseline) = check_rate_update(1_700_000_000, 0, old, new, 0, old).unwrap();
        assert_eq!(baseline, new);
        assert_eq!(day_id, 1_700_000_000 / 86_400);
        // After bootstrap, same-day +3% from seeded baseline is still capped.
        let too_high = baseline
            .checked_mul(Uint128::from(103u128))
            .unwrap()
            .checked_div(Uint128::from(100u128))
            .unwrap();
        let err = check_rate_update(
            1_700_000_000 + crate::MIN_ORACLE_UPDATE_INTERVAL_SECS + 1,
            1_700_000_000,
            new,
            too_high,
            day_id,
            baseline,
        )
        .unwrap_err();
        assert_eq!(err, OraclePolicyError::DailyCapExceeded);
    }

    proptest! {
        #[test]
        fn prop_monotonic_required(old in 1u128..(1u128 << 60), new in 1u128..(1u128 << 60)) {
            let old_r = Uint128::from(old);
            let new_r = Uint128::from(new);
            if new < old {
                let day = 1u64;
                let err = check_rate_update(86_400 * day, 0, old_r, new_r, day, old_r).unwrap_err();
                prop_assert_eq!(err, OraclePolicyError::RateDecreased);
            }
        }
    }
}
