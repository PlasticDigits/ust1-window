//! Minimal treasury stub for fast integration tests: `InstantWithdrawCw20` → CW20 Transfer.
//!
//! Mirrors the ustr-cmm Option 3 **wire shape** without depending on the treasury crate.
//! Production ACL / pause / 24h limits are covered by `real_treasury_integration.rs`.
//!
//! # Strictness (INV-SCHEMA-001)
//!
//! `#[cw_serde]` applies `deny_unknown_fields` + snake_case — same as production CosmWasm
//! decode on the real treasury. Do **not** strip `deny_unknown_fields` for "forward compat";
//! that hides wire drift (audit C-1 / [#21](https://gitlab.com/PlasticDigits/ust1-window/-/issues/21)).

use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdResult, Uint128,
    WasmMsg,
};
use cw20::Cw20ExecuteMsg;

#[cosmwasm_schema::cw_serde]
pub struct InstantiateMsg {}

#[cosmwasm_schema::cw_serde]
pub enum ExecuteMsg {
    InstantWithdrawCw20 {
        recipient: String,
        token: String,
        amount: Uint128,
    },
}

pub fn instantiate(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    Ok(Response::new().add_attribute("action", "stub_treasury_instantiate"))
}

pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response> {
    match msg {
        ExecuteMsg::InstantWithdrawCw20 {
            recipient,
            token,
            amount,
        } => Ok(Response::new()
            .add_message(WasmMsg::Execute {
                contract_addr: token,
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer { recipient, amount })?,
                funds: vec![],
            })
            .add_attribute("action", "instant_withdraw_cw20")),
    }
}

pub fn query(_deps: Deps, _env: Env, _msg: Empty) -> StdResult<Binary> {
    to_json_binary(&Empty {})
}

#[cfg(test)]
mod strictness_tests {
    use super::*;
    use cosmwasm_std::from_json;

    #[test]
    fn stub_rejects_unknown_fields() {
        let raw = br#"{"instant_withdraw_cw20":{"recipient":"u","token":"t","amount":"1","memo":"nope"}}"#;
        assert!(from_json::<ExecuteMsg>(raw).is_err());
    }

    #[test]
    fn stub_rejects_renamed_recipient() {
        let raw = br#"{"instant_withdraw_cw20":{"receiver":"u","token":"t","amount":"1"}}"#;
        assert!(from_json::<ExecuteMsg>(raw).is_err());
    }

    #[test]
    fn stub_accepts_canonical_msg() {
        let raw = br#"{"instant_withdraw_cw20":{"recipient":"u","token":"t","amount":"1"}}"#;
        let msg: ExecuteMsg = from_json(raw).unwrap();
        assert_eq!(
            msg,
            ExecuteMsg::InstantWithdrawCw20 {
                recipient: "u".into(),
                token: "t".into(),
                amount: Uint128::new(1),
            }
        );
    }
}
