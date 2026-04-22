//! Native coin in → mint wrapped cw20 (1:1 atoms after fee).

use cosmwasm_std::{to_json_binary, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;

use crate::error::ContractError;
use crate::fee_accounting::with_fee_split_attributes;
use crate::limits::ensure_limits;
use crate::state::{RollingVolume, CONFIG, ROLLING};

pub fn execute_wrap(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    if cfg.paused {
        return Err(ContractError::Paused {});
    }
    if info.funds.len() != 1 {
        return Err(ContractError::InvalidNativeFunds {});
    }
    let coin = &info.funds[0];
    if coin.amount.is_zero() {
        return Err(ContractError::InvalidNativeFunds {});
    }
    let pair = cfg
        .pairs
        .iter()
        .find(|p| p.native_denom == coin.denom)
        .ok_or(ContractError::UnknownNativeDenom {})?;

    let mint_amount = ust1_common::math::apply_fee_ust1(coin.amount, cfg.fee_bps)?;

    let mut rolling = ROLLING
        .may_load(deps.storage, pair.native_denom.as_str())?
        .unwrap_or(RollingVolume {
            window_start_sec: 0,
            volume_wrap: Uint128::zero(),
        });
    ensure_limits(&env, &mut rolling, pair, mint_amount)?;
    ROLLING.save(deps.storage, pair.native_denom.as_str(), &rolling)?;

    let mint = WasmMsg::Execute {
        contract_addr: pair.wrapped_token.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Mint {
            recipient: info.sender.to_string(),
            amount: mint_amount,
        })?,
        funds: vec![],
    };

    Ok(with_fee_split_attributes(
        Response::new()
            .add_message(mint)
            .add_attribute("action", "wrap")
            .add_attribute("denom", &coin.denom)
            .add_attribute("wrapped_out", mint_amount),
        cfg.fee_bps,
    ))
}
