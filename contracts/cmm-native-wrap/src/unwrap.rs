//! cw20 receive → burn wrapped, send native (1:1 atoms after fee).

use cosmwasm_std::{
    coin, to_json_binary, BankMsg, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use crate::error::ContractError;
use crate::fee_accounting::with_fee_split_attributes;
use crate::limits::ensure_limits;
use crate::msg::Cw20HookMsg;
use crate::state::{RollingVolume, CONFIG, ROLLING};

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    if cfg.paused {
        return Err(ContractError::Paused {});
    }
    let pair = cfg
        .pairs
        .iter()
        .find(|p| p.wrapped_token == info.sender)
        .ok_or(ContractError::UnknownWrappedToken {})?;

    let hook: Cw20HookMsg = cosmwasm_std::from_json(&cw20_msg.msg)?;
    let min_out = match hook {
        Cw20HookMsg::Unwrap { min_native_out } => min_native_out,
    };
    let beneficiary = deps.api.addr_validate(&cw20_msg.sender)?;
    let gross = cw20_msg.amount;

    let native_out = ust1_common::math::apply_fee_ust1(gross, cfg.fee_bps)?;
    if native_out < min_out {
        return Err(ContractError::BelowMinimum {});
    }

    let bal = deps
        .querier
        .query_balance(env.contract.address.clone(), &pair.native_denom)?;
    if bal.amount < native_out {
        return Err(ContractError::InsufficientNative {});
    }

    let mut rolling = ROLLING
        .may_load(deps.storage, pair.native_denom.as_str())?
        .unwrap_or(RollingVolume {
            window_start_sec: 0,
            volume_wrap: Uint128::zero(),
        });
    ensure_limits(&env, &mut rolling, pair, gross)?;
    ROLLING.save(deps.storage, pair.native_denom.as_str(), &rolling)?;

    let burn = WasmMsg::Execute {
        contract_addr: pair.wrapped_token.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Burn { amount: gross })?,
        funds: vec![],
    };

    let send = BankMsg::Send {
        to_address: beneficiary.to_string(),
        amount: vec![coin(native_out.u128(), pair.native_denom.clone())],
    };

    Ok(with_fee_split_attributes(
        Response::new()
            .add_message(burn)
            .add_message(send)
            .add_attribute("action", "unwrap")
            .add_attribute("denom", pair.native_denom.as_str())
            .add_attribute("native_out", native_out),
        cfg.fee_bps,
    ))
}
