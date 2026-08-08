//! Minimal CMM Treasury execute client for Option 3 withdraws.
//!
//! Wire format must stay aligned with PlasticDigits2/ustr-cmm treasury
//! (`InstantWithdrawCw20` from issues [#6](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/6)
//! / [#7](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/7)).
//!
//! Schema authority is the pinned `cmm-treasury` git rev
//! ([`USTR_CMM_TREASURY_SCHEMA_REV`]) — see [ust1-window#21](https://gitlab.com/PlasticDigits/ust1-window/-/issues/21)
//! / audit C-1. Conformance tests compare this client byte-for-byte against
//! `cmm_treasury::msg::ExecuteMsg::InstantWithdrawCw20` from that pin.
//!
//! See [`skills/window-instant-withdraw-cw20/SKILL.md`](../../../skills/window-instant-withdraw-cw20/SKILL.md).

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Addr, CosmosMsg, StdResult, Uint128, WasmMsg};

/// Git URL for the ustr-cmm treasury schema authority.
pub const USTR_CMM_REPO: &str = "https://gitlab.com/PlasticDigits2/ustr-cmm.git";

/// Pinned `PlasticDigits2/ustr-cmm` git revision whose treasury `ExecuteMsg` is
/// the InstantWithdrawCw20 schema authority for this window client.
///
/// Keep in sync with:
/// - workspace dep `cmm-treasury` in root `Cargo.toml`
/// - `contracts/ust1-window/testdata/instant_withdraw_cw20_golden.json`
/// - `docs/DEPLOYMENT.md` (Phase 5 schema pin)
/// - `skills/window-instant-withdraw-cw20/SKILL.md`
/// - `scripts/verify_treasury_wire_schema.sh`
pub const USTR_CMM_TREASURY_SCHEMA_REV: &str = "e6c4b7cf33f2f56d21c0e9fb2828efe87f032ded";

/// Subset of ustr-cmm treasury `ExecuteMsg` used by this window.
///
/// Other treasury variants (native InstantWithdraw, SetCw20Spender, etc.) are
/// intentionally omitted — window never sends them.
///
/// `#[cw_serde]` includes `deny_unknown_fields` (same strictness as production
/// CosmWasm decode on the treasury).
#[cw_serde]
pub enum TreasuryExecuteMsg {
    /// Registered spender pulls CW20 inventory; treasury emits `Cw20ExecuteMsg::Transfer`.
    InstantWithdrawCw20 {
        recipient: String,
        token: String,
        amount: Uint128,
    },
}

/// Build the Wasm execute message the window emits on withdraw.
///
/// # Invariants
///
/// - **INV-WITHDRAW-001**: Redeem path calls treasury `InstantWithdrawCw20` (not CW20
///   `TransferFrom` / allowance).
/// - **INV-SCHEMA-001**: Serialized JSON matches pinned ustr-cmm treasury schema
///   (`USTR_CMM_TREASURY_SCHEMA_REV`) byte-for-byte for the InstantWithdrawCw20 variant.
/// - Recipient must be the cw20 Send `sender` (user), not a caller-controlled field.
pub fn instant_withdraw_cw20_msg(
    treasury: &Addr,
    recipient: &Addr,
    token: &Addr,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    Ok(WasmMsg::Execute {
        contract_addr: treasury.to_string(),
        msg: to_json_binary(&TreasuryExecuteMsg::InstantWithdrawCw20 {
            recipient: recipient.to_string(),
            token: token.to_string(),
            amount,
        })?,
        funds: vec![],
    }
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmm_treasury::msg::ExecuteMsg as TreasuryAuthorityMsg;
    use cosmwasm_std::{from_json, to_json_binary, Uint128};

    fn window_msg(recipient: &str, token: &str, amount: Uint128) -> TreasuryExecuteMsg {
        TreasuryExecuteMsg::InstantWithdrawCw20 {
            recipient: recipient.into(),
            token: token.into(),
            amount,
        }
    }

    fn authority_msg(recipient: &str, token: &str, amount: Uint128) -> TreasuryAuthorityMsg {
        TreasuryAuthorityMsg::InstantWithdrawCw20 {
            recipient: recipient.into(),
            token: token.into(),
            amount,
        }
    }

    #[test]
    fn schema_pin_constants() {
        assert_eq!(
            USTR_CMM_TREASURY_SCHEMA_REV,
            "e6c4b7cf33f2f56d21c0e9fb2828efe87f032ded"
        );
        assert_eq!(
            USTR_CMM_REPO,
            "https://gitlab.com/PlasticDigits2/ustr-cmm.git"
        );
    }

    #[test]
    fn instant_withdraw_cw20_json_matches_pinned_ustr_cmm_treasury() {
        let recipient = "terra1user";
        let token = "terra1vfdusd";
        for amount in [
            Uint128::new(1),
            Uint128::new(42),
            Uint128::MAX,
            Uint128::new(10_000_000_000),
        ] {
            let window = to_json_binary(&window_msg(recipient, token, amount)).unwrap();
            let authority = to_json_binary(&authority_msg(recipient, token, amount)).unwrap();
            assert_eq!(
                window,
                authority,
                "INV-SCHEMA-001 drift at amount={amount}: window={} authority={}",
                String::from_utf8_lossy(&window),
                String::from_utf8_lossy(&authority),
            );
        }
    }

    #[test]
    fn window_json_decodes_as_pinned_treasury_execute_msg() {
        let window = window_msg("terra1user", "terra1vfdusd", Uint128::new(99));
        let bin = to_json_binary(&window).unwrap();
        let decoded: TreasuryAuthorityMsg = from_json(&bin).unwrap();
        assert_eq!(
            decoded,
            authority_msg("terra1user", "terra1vfdusd", Uint128::new(99))
        );
    }

    #[test]
    fn golden_file_cases_match_window_and_authority() {
        let golden = include_str!("../testdata/instant_withdraw_cw20_golden.json");
        let v: serde_json::Value = serde_json::from_str(golden).unwrap();
        assert_eq!(
            v["ustr_cmm_rev"].as_str(),
            Some(USTR_CMM_TREASURY_SCHEMA_REV)
        );
        assert_eq!(v["ustr_cmm_repo"].as_str(), Some(USTR_CMM_REPO));

        for case in v["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let canonical = case["canonical_json"].as_str().unwrap();
            let amount = match name {
                "representative" => Uint128::new(42),
                "max_uint128" => Uint128::MAX,
                other => panic!("unexpected golden case {other}"),
            };
            let window = to_json_binary(&window_msg("terra1user", "terra1vfdusd", amount)).unwrap();
            let authority =
                to_json_binary(&authority_msg("terra1user", "terra1vfdusd", amount)).unwrap();
            let window_s = String::from_utf8(window.to_vec()).unwrap();
            let authority_s = String::from_utf8(authority.to_vec()).unwrap();
            assert_eq!(window_s, canonical, "window vs golden ({name})");
            assert_eq!(authority_s, canonical, "authority vs golden ({name})");
        }
    }

    #[test]
    fn golden_negative_fixtures_rejected_by_window_and_authority() {
        let golden = include_str!("../testdata/instant_withdraw_cw20_golden.json");
        let v: serde_json::Value = serde_json::from_str(golden).unwrap();
        for case in v["negative_fixtures"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let raw = case["json"].as_str().unwrap().as_bytes();
            assert!(
                from_json::<TreasuryExecuteMsg>(raw).is_err(),
                "window should reject negative fixture {name}"
            );
            assert!(
                from_json::<TreasuryAuthorityMsg>(raw).is_err(),
                "treasury authority should reject negative fixture {name}"
            );
        }
    }

    #[test]
    fn unknown_field_rejected_like_production_treasury() {
        let bad =
            br#"{"instant_withdraw_cw20":{"recipient":"r","token":"t","amount":"1","extra":"x"}}"#;
        let err = from_json::<TreasuryExecuteMsg>(bad).unwrap_err();
        assert!(
            err.to_string().contains("unknown field"),
            "expected deny_unknown_fields, got: {err}"
        );
        assert!(from_json::<TreasuryAuthorityMsg>(bad)
            .unwrap_err()
            .to_string()
            .contains("unknown field"));
    }
}
