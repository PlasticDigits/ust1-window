//! Minimal CMM Treasury execute client for Option 3 withdraws.
//!
//! Wire format must stay aligned with PlasticDigits2/ustr-cmm treasury
//! (`InstantWithdrawCw20` from issues [#6](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/6)
//! / [#7](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/7)).
//!
//! See [`skills/window-instant-withdraw-cw20/SKILL.md`](../../../skills/window-instant-withdraw-cw20/SKILL.md).

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Addr, CosmosMsg, StdResult, Uint128, WasmMsg};

/// Subset of ustr-cmm treasury `ExecuteMsg` used by this window.
///
/// Other treasury variants (native InstantWithdraw, SetCw20Spender, etc.) are
/// intentionally omitted — window never sends them.
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
