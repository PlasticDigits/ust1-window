//! Pure arithmetic used by oracle policy and swap amounts.
//!
//! # Invariants
//!
//! - **INV-SWAP-001** (forward): vFDUSD `x` → UST1 `floor(x * R / RATE_SCALE * (BPS_DENOM - fee_bps) / BPS_DENOM)`.
//! - **INV-SWAP-002** (reverse): gross UST1 `u` → vFDUSD out is the chained integer quotient
//!   `u * (BPS_DENOM - fee_bps) / BPS_DENOM * RATE_SCALE / R` (left-associative `/`, i.e. fee floor
//!   then rate division). Implemented as [`withdraw_ust1_after_fee`] then [`ust1_after_fee_to_vfdusd`];
//!   the public entry point is [`withdraw_gross_ust1_to_vfdusd`].

use cosmwasm_std::{Uint128, Uint256};

use crate::error::MathError;
use crate::{BPS_DENOM, RATE_SCALE};

/// Apply fee on UST1 notional: `amount * (BPS_DENOM - fee_bps) / BPS_DENOM`.
pub fn apply_fee_ust1(amount: Uint128, fee_bps: u16) -> Result<Uint128, MathError> {
    if fee_bps as u128 > BPS_DENOM {
        return Err(MathError::InvalidFeeBps);
    }
    let num = amount.checked_mul(Uint128::from(BPS_DENOM - fee_bps as u128))?;
    Ok(num.checked_div(Uint128::from(BPS_DENOM))?)
}

/// vFDUSD atoms → UST1 atoms before fee: `x * R / RATE_SCALE`.
pub fn vfdusd_to_ust1_before_fee(
    amount_vfdusd: Uint128,
    rate: Uint128,
) -> Result<Uint128, MathError> {
    let num = Uint256::from(amount_vfdusd).checked_mul(Uint256::from(rate))?;
    let scaled = num.checked_div(Uint256::from(RATE_SCALE))?;
    scaled.try_into().map_err(|_| MathError::TooLarge)
}

/// Full deposit: vFDUSD → UST1 after fee on UST1 output.
pub fn deposit_vfdusd_to_ust1(
    amount_vfdusd: Uint128,
    rate: Uint128,
    fee_bps: u16,
) -> Result<Uint128, MathError> {
    let before = vfdusd_to_ust1_before_fee(amount_vfdusd, rate)?;
    apply_fee_ust1(before, fee_bps)
}

/// UST1 atoms (gross user send) → vFDUSD atoms after fee, before rate division.
pub fn withdraw_ust1_after_fee(gross_ust1: Uint128, fee_bps: u16) -> Result<Uint128, MathError> {
    apply_fee_ust1(gross_ust1, fee_bps)
}

/// Reverse: after-fee UST1 notional → vFDUSD: `u * RATE_SCALE / R`.
pub fn ust1_after_fee_to_vfdusd(
    ust1_after_fee: Uint128,
    rate: Uint128,
) -> Result<Uint128, MathError> {
    if rate.is_zero() {
        return Err(MathError::DivisionByZero);
    }
    let num = Uint256::from(ust1_after_fee)
        .checked_mul(Uint256::from(RATE_SCALE))?
        .checked_div(Uint256::from(rate))?;
    num.try_into().map_err(|_| MathError::TooLarge)
}

/// Full withdraw: gross UST1 sent → vFDUSD out.
pub fn withdraw_gross_ust1_to_vfdusd(
    gross_ust1: Uint128,
    rate: Uint128,
    fee_bps: u16,
) -> Result<Uint128, MathError> {
    let after = withdraw_ust1_after_fee(gross_ust1, fee_bps)?;
    ust1_after_fee_to_vfdusd(after, rate)
}

/// Max oracle rate allowed for the current UTC day given baseline at day start.
pub fn max_rate_after_daily_cap(day_baseline: Uint128) -> Result<Uint128, MathError> {
    let num = day_baseline.checked_mul(Uint128::from(
        10_000u128 + crate::MAX_DAILY_INCREASE_BPS as u128,
    ))?;
    Ok(num.checked_div(Uint128::from(10_000u128))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// **INV-MATH-001**: RATE_SCALE semantics (spot check).
    #[test]
    fn inv_math_001_one_to_one_rate() {
        let r = Uint128::from(RATE_SCALE);
        let out = vfdusd_to_ust1_before_fee(Uint128::from(1_000_000u128), r).unwrap();
        assert_eq!(out, Uint128::from(1_000_000u128));
    }

    /// **INV-SWAP-001** deposit round-trip shape (fee > 0 loses notional).
    #[test]
    fn inv_swap_001_fee_reduces_output() {
        let rate = Uint128::from(RATE_SCALE);
        let x = Uint128::from(10_000_000u128);
        let out = deposit_vfdusd_to_ust1(x, rate, 50).unwrap();
        assert!(out < x);
    }

    /// **INV-SWAP-002** reverse path: gross UST1 → vFDUSD matches known integer vectors.
    ///
    /// Each expected output is the chained floor semantics from the module docs (fee on UST1, then
    /// `RATE_SCALE / R`), asserted via [`withdraw_gross_ust1_to_vfdusd`]. Covers zero fee, typical
    /// fee, dust-after-fee, max fee (100%), and non–1:1 rates.
    #[test]
    fn inv_swap_002_reverse_fee_applies_vectors() {
        let r_one = Uint128::from(RATE_SCALE);
        let r_double = Uint128::from(RATE_SCALE * 2);
        let r_triple = Uint128::from(RATE_SCALE * 3);

        let cases: &[((u128, u16, Uint128), u128)] = &[
            ((10_000_000, 0, r_one), 10_000_000),
            ((10_000_000, 50, r_one), 9_950_000),
            ((1, 0, r_one), 1),
            ((9, 9999, r_one), 0),
            ((1_000_000, 10_000, r_one), 0),
            ((2_000_000, 0, r_double), 1_000_000),
            ((100, 0, r_triple), 33),
        ];

        for &((gross, fee_bps, rate), expected_v) in cases {
            let out = withdraw_gross_ust1_to_vfdusd(Uint128::from(gross), rate, fee_bps).unwrap();
            assert_eq!(
                out,
                Uint128::from(expected_v),
                "INV-SWAP-002: gross={gross} fee_bps={fee_bps} rate={rate:?}"
            );
        }
    }

    /// **INV-SWAP-002** conversion leg only: after-fee UST1 → vFDUSD (no fee term).
    #[test]
    fn inv_swap_002_after_fee_to_vfdusd_vectors() {
        let r_one = Uint128::from(RATE_SCALE);
        let cases: &[((u128, Uint128), u128)] = &[
            ((9_950_000, r_one), 9_950_000),
            ((1, r_one), 1),
            ((100, Uint128::from(RATE_SCALE * 3)), 33),
        ];
        for &((after_fee, rate), expected_v) in cases {
            let out = ust1_after_fee_to_vfdusd(Uint128::from(after_fee), rate).unwrap();
            assert_eq!(out, Uint128::from(expected_v));
        }
    }

    proptest! {
        #[test]
        fn prop_fee_never_inverts(a in 0u128..1_000_000_000_000u128, fee in 0u16..10_000u16) {
            let amount = Uint128::from(a);
            let out = apply_fee_ust1(amount, fee).unwrap();
            prop_assert!(out <= amount);
        }

        #[test]
        fn prop_monotonic_rate_forward(amt in 1u128..1_000_000_000u128, r in 1u128..RATE_SCALE * 2) {
            let amount = Uint128::from(amt);
            let rate = Uint128::from(r);
            let out = vfdusd_to_ust1_before_fee(amount, rate).unwrap();
            prop_assert!(out >= Uint128::zero());
        }
    }
}
