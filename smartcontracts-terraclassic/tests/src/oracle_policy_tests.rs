//! Cross-links to `ust1_common::oracle_policy` (**INV-ORACLE-DAILY-001**, throttle).
use cosmwasm_std::Uint128;
use proptest::prelude::*;
use ust1_common::oracle_policy::{check_rate_update, OraclePolicyError};
use ust1_common::RATE_SCALE;

#[test]
fn inv_oracle_daily_cap_blocks_large_jump() {
    let old = Uint128::from(RATE_SCALE);
    let baseline = old;
    let day = 10u64;
    // +3% in one day — should fail (max 2%)
    let new = old
        .checked_mul(Uint128::from(103u128))
        .unwrap()
        .checked_div(Uint128::from(100u128))
        .unwrap();
    let err = check_rate_update(day * 86_400 + 100, 0, old, new, day, baseline).unwrap_err();
    assert_eq!(err, OraclePolicyError::DailyCapExceeded);
}

proptest! {
    #[test]
    fn prop_daily_cap_allows_flat_rate(
        day in 1u64..10_000u64,
    ) {
        let r = Uint128::from(RATE_SCALE);
        let ok = check_rate_update(day * 86_400 + 50, 0, r, r, day, r);
        prop_assert!(ok.is_ok());
    }
}
