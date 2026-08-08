//! Cross-repo InstantWithdrawCw20 conformance (audit C-1 / issue #21).
//!
//! Compares `ust1_window::treasury::TreasuryExecuteMsg` serialization to the pinned
//! ustr-cmm `treasury::msg::ExecuteMsg` variant byte-for-byte.

use cosmwasm_std::{from_json, to_json_binary, Uint128};
use ust1_window::treasury::{TreasuryExecuteMsg, USTR_CMM_REPO, USTR_CMM_TREASURY_SCHEMA_REV};

fn window_msg(amount: Uint128) -> TreasuryExecuteMsg {
    TreasuryExecuteMsg::InstantWithdrawCw20 {
        recipient: "terra1user".into(),
        token: "terra1vfdusd".into(),
        amount,
    }
}

fn treasury_msg(amount: Uint128) -> cmm_treasury::msg::ExecuteMsg {
    cmm_treasury::msg::ExecuteMsg::InstantWithdrawCw20 {
        recipient: "terra1user".into(),
        token: "terra1vfdusd".into(),
        amount,
    }
}

#[test]
fn pin_constant_documented() {
    assert_eq!(
        USTR_CMM_TREASURY_SCHEMA_REV,
        "e6c4b7cf33f2f56d21c0e9fb2828efe87f032ded"
    );
    assert!(USTR_CMM_REPO.contains("PlasticDigits2/ustr-cmm"));
}

#[test]
fn window_instant_withdraw_cw20_matches_ustr_cmm_treasury_byte_for_byte() {
    for amount in [
        Uint128::from(42u128),
        Uint128::MAX,
        Uint128::from(10_000_000_000u128),
    ] {
        let w = to_json_binary(&window_msg(amount)).unwrap();
        let t = to_json_binary(&treasury_msg(amount)).unwrap();
        assert_eq!(
            w,
            t,
            "INV-SCHEMA-001: window vs treasury JSON diverge for amount {amount}: {} vs {}",
            String::from_utf8_lossy(&w),
            String::from_utf8_lossy(&t)
        );
    }
}

#[test]
fn treasury_decodes_window_bytes_and_rejects_drift() {
    let w = to_json_binary(&window_msg(Uint128::from(99u128))).unwrap();
    let decoded: cmm_treasury::msg::ExecuteMsg = from_json(w.as_slice()).unwrap();
    assert_eq!(decoded, treasury_msg(Uint128::from(99u128)));

    // Renamed field must fail on the real treasury enum (production strictness).
    let drifted = br#"{"instant_withdraw_cw20":{"receiver":"terra1user","token":"terra1vfdusd","amount":"99"}}"#;
    assert!(from_json::<cmm_treasury::msg::ExecuteMsg>(drifted).is_err());

    let extra = br#"{"instant_withdraw_cw20":{"recipient":"terra1user","token":"terra1vfdusd","amount":"99","memo":"x"}}"#;
    assert!(from_json::<cmm_treasury::msg::ExecuteMsg>(extra).is_err());
}

#[test]
fn golden_file_pin_matches_workspace_dep() {
    let golden =
        include_str!("../../../contracts/ust1-window/testdata/instant_withdraw_cw20_golden.json");
    let v: serde_json::Value = serde_json::from_str(golden).unwrap();
    assert_eq!(
        v["ustr_cmm_rev"].as_str(),
        Some(USTR_CMM_TREASURY_SCHEMA_REV)
    );
}
