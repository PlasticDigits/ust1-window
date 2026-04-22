//! Instantiate, dispatch, query, migrate.

use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::fee_accounting::with_fee_split_attributes;
use crate::gov::{
    exec_accept_gov, exec_propose_gov, exec_set_fee_bps, exec_set_pair_limits, exec_set_paused,
};
use crate::msg::{
    ConfigResponse, DenomPairResponse, EffectiveWrapResponse, ExecuteMsg, InstantiateMsg,
    MigrateMsg, QueryMsg,
};
use crate::pairs::validate_pairs;
use crate::state::{
    Config, DenomPair, RollingVolume, CONFIG, CONTRACT_NAME, CONTRACT_VERSION, ROLLING,
};
use crate::unwrap::receive_cw20;
use crate::wrap::execute_wrap;

pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    if msg.fee_bps as u128 > ust1_common::BPS_DENOM {
        return Err(ContractError::Math(ust1_common::MathError::InvalidFeeBps));
    }
    let mut pairs: Vec<DenomPair> = Vec::with_capacity(msg.pairs.len());
    for p in msg.pairs {
        pairs.push(DenomPair {
            native_denom: p.native_denom,
            wrapped_token: deps.api.addr_validate(&p.wrapped_token)?,
            per_tx_wrap_limit: p.per_tx_wrap_limit,
            rolling_24h_wrap_limit: p.rolling_24h_wrap_limit,
        });
    }
    validate_pairs(&pairs)?;
    let cfg = Config {
        governance: deps.api.addr_validate(&msg.governance)?,
        fee_bps: msg.fee_bps,
        paused: false,
        pairs,
    };
    CONFIG.save(deps.storage, &cfg)?;
    Ok(with_fee_split_attributes(
        Response::new().add_attribute("action", "instantiate"),
        cfg.fee_bps,
    ))
}

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Wrap {} => execute_wrap(deps, env, info),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::SetPairLimits {
            native_denom,
            per_tx_wrap_limit,
            rolling_24h_wrap_limit,
        } => exec_set_pair_limits(
            deps,
            info,
            native_denom,
            per_tx_wrap_limit,
            rolling_24h_wrap_limit,
        ),
        ExecuteMsg::SetPaused { paused } => exec_set_paused(deps, info, paused),
        ExecuteMsg::SetFeeBps { fee_bps } => exec_set_fee_bps(deps, info, fee_bps),
        ExecuteMsg::ProposeGovernance { address } => exec_propose_gov(deps, info, address),
        ExecuteMsg::AcceptGovernance {} => exec_accept_gov(deps, info),
    }
}

pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::EffectiveWrap { denom } => to_json_binary(&query_effective_wrap(deps, denom)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let c = CONFIG.load(deps.storage)?;
    let pairs = c
        .pairs
        .into_iter()
        .map(|p| DenomPairResponse {
            native_denom: p.native_denom,
            wrapped_token: p.wrapped_token.to_string(),
            per_tx_wrap_limit: p.per_tx_wrap_limit,
            rolling_24h_wrap_limit: p.rolling_24h_wrap_limit,
        })
        .collect();
    Ok(ConfigResponse {
        governance: c.governance.to_string(),
        fee_bps: c.fee_bps,
        paused: c.paused,
        pairs,
    })
}

fn query_effective_wrap(deps: Deps, denom: String) -> StdResult<EffectiveWrapResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let pair = cfg
        .pairs
        .iter()
        .find(|p| p.native_denom == denom)
        .ok_or_else(|| cosmwasm_std::StdError::generic_err("unknown denom"))?;
    let rolling = ROLLING
        .may_load(deps.storage, pair.native_denom.as_str())?
        .unwrap_or(RollingVolume {
            window_start_sec: 0,
            volume_wrap: cosmwasm_std::Uint128::zero(),
        });
    let (fee_chain_tax_bps, fee_cmm_protocol_bps) =
        ust1_common::fee_split::chain_tax_and_cmm_protocol(cfg.fee_bps);
    Ok(EffectiveWrapResponse {
        denom: pair.native_denom.clone(),
        fee_bps: cfg.fee_bps,
        fee_chain_tax_bps,
        fee_cmm_protocol_bps,
        paused: cfg.paused,
        per_tx_wrap_limit: pair.per_tx_wrap_limit,
        rolling_24h_wrap_limit: pair.rolling_24h_wrap_limit,
        rolling_window_start_sec: rolling.window_start_sec,
        rolling_volume_wrap: rolling.volume_wrap,
        wrapped_token: pair.wrapped_token.to_string(),
    })
}

pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}
