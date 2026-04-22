//! Governance-only configuration updates.

use cosmwasm_std::{DepsMut, MessageInfo, Response, Uint128};

use crate::error::ContractError;
use crate::state::{PendingGovernance, CONFIG, PENDING_GOVERNANCE};

pub fn exec_set_pair_limits(
    deps: DepsMut,
    info: MessageInfo,
    native_denom: String,
    per_tx_wrap_limit: Uint128,
    rolling_24h_wrap_limit: Uint128,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.governance {
        return Err(ContractError::Unauthorized {});
    }
    let pair = cfg
        .pairs
        .iter_mut()
        .find(|p| p.native_denom == native_denom)
        .ok_or(ContractError::UnknownNativeDenom {})?;
    pair.per_tx_wrap_limit = per_tx_wrap_limit;
    pair.rolling_24h_wrap_limit = rolling_24h_wrap_limit;
    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::new().add_attribute("action", "set_pair_limits"))
}

pub fn exec_set_paused(
    deps: DepsMut,
    info: MessageInfo,
    paused: bool,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.governance {
        return Err(ContractError::Unauthorized {});
    }
    cfg.paused = paused;
    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::new().add_attribute("action", "set_paused"))
}

pub fn exec_set_fee_bps(
    deps: DepsMut,
    info: MessageInfo,
    fee_bps: u16,
) -> Result<Response, ContractError> {
    if fee_bps as u128 > ust1_common::BPS_DENOM {
        return Err(ContractError::Math(ust1_common::MathError::InvalidFeeBps));
    }
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.governance {
        return Err(ContractError::Unauthorized {});
    }
    let old = cfg.fee_bps;
    cfg.fee_bps = fee_bps;
    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::new()
        .add_attribute("action", "set_fee_bps")
        .add_attribute("old_fee_bps", old.to_string())
        .add_attribute("new_fee_bps", fee_bps.to_string()))
}

pub fn exec_propose_gov(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.governance {
        return Err(ContractError::Unauthorized {});
    }
    let new_address = deps.api.addr_validate(&address)?;
    PENDING_GOVERNANCE.save(deps.storage, &PendingGovernance { new_address })?;
    Ok(Response::new().add_attribute("action", "propose_governance"))
}

pub fn exec_accept_gov(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let pending = PENDING_GOVERNANCE
        .may_load(deps.storage)?
        .ok_or(ContractError::NoPendingGovernance {})?;
    if info.sender != pending.new_address {
        return Err(ContractError::InvalidGovernanceProposal {});
    }
    let mut cfg = CONFIG.load(deps.storage)?;
    cfg.governance = pending.new_address.clone();
    CONFIG.save(deps.storage, &cfg)?;
    PENDING_GOVERNANCE.remove(deps.storage);
    Ok(Response::new().add_attribute("action", "accept_governance"))
}
