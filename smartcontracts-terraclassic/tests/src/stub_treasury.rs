//! Minimal treasury stub for integration tests: `InstantWithdrawCw20` → CW20 Transfer.
//! Mirrors ustr-cmm Option 3 without depending on the treasury crate.

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
