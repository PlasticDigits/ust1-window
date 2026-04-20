//! Oracle: monotonic FDUSD-per-vFDUSD rate with UTC daily cap and 4h throttle.

use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, StateResponse};
use crate::state::{
    Config, OracleState, CONFIG, CONTRACT_NAME, CONTRACT_VERSION, ORACLE_STATE, PENDING_GOVERNANCE,
};
use ust1_common::oracle_policy::{check_rate_update, OraclePolicyError};

pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let governance = deps.api.addr_validate(&msg.governance)?;
    let oracle_operator = deps.api.addr_validate(&msg.oracle_operator)?;
    if msg.initial_rate.is_zero() {
        return Err(ContractError::ZeroRate {});
    }

    let cfg = Config {
        governance: governance.clone(),
        oracle_operator,
        paused: false,
    };
    CONFIG.save(deps.storage, &cfg)?;

    let day_id = env.block.time.seconds() / 86_400;
    let st = OracleState {
        rate: msg.initial_rate,
        last_update_sec: 0,
        utc_day_id: day_id,
        day_baseline_rate: msg.initial_rate,
    };
    ORACLE_STATE.save(deps.storage, &st)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("governance", governance))
}

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateRate { new_rate } => execute_update_rate(deps, env, info, new_rate),
        ExecuteMsg::SetOracleOperator { address } => execute_set_operator(deps, info, address),
        ExecuteMsg::SetPaused { paused } => execute_set_paused(deps, info, paused),
        ExecuteMsg::ProposeGovernance { address } => execute_propose_gov(deps, info, address),
        ExecuteMsg::AcceptGovernance {} => execute_accept_gov(deps, info),
    }
}

fn execute_update_rate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    new_rate: cosmwasm_std::Uint128,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    if cfg.paused {
        return Err(ContractError::Paused {});
    }
    if info.sender != cfg.oracle_operator {
        return Err(ContractError::Unauthorized {});
    }

    let mut st = ORACLE_STATE.load(deps.storage)?;
    let now = env.block.time.seconds();

    let (day_id, baseline) = check_rate_update(
        now,
        st.last_update_sec,
        st.rate,
        new_rate,
        st.utc_day_id,
        st.day_baseline_rate,
    )
    .map_err(policy_err)?;

    st.utc_day_id = day_id;
    st.day_baseline_rate = baseline;
    st.rate = new_rate;
    st.last_update_sec = now;
    ORACLE_STATE.save(deps.storage, &st)?;

    Ok(Response::new()
        .add_attribute("action", "update_rate")
        .add_attribute("rate", new_rate))
}

fn policy_err(e: OraclePolicyError) -> ContractError {
    match e {
        OraclePolicyError::ZeroRate => ContractError::ZeroRate {},
        OraclePolicyError::RateDecreased => ContractError::RateDecreased {},
        OraclePolicyError::DailyCapExceeded => ContractError::DailyCapExceeded {},
        OraclePolicyError::UpdateTooSoon { min_interval } => {
            ContractError::UpdateTooSoon { min_interval }
        }
    }
}

fn execute_set_operator(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.governance {
        return Err(ContractError::Unauthorized {});
    }
    cfg.oracle_operator = deps.api.addr_validate(&address)?;
    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::new()
        .add_attribute("action", "set_oracle_operator")
        .add_attribute("address", address))
}

fn execute_set_paused(
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
    Ok(Response::new()
        .add_attribute("action", "set_paused")
        .add_attribute("paused", paused.to_string()))
}

fn execute_propose_gov(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.governance {
        return Err(ContractError::Unauthorized {});
    }
    let new_address = deps.api.addr_validate(&address)?;
    PENDING_GOVERNANCE.save(
        deps.storage,
        &crate::state::PendingGovernance { new_address },
    )?;
    Ok(Response::new().add_attribute("action", "propose_governance"))
}

fn execute_accept_gov(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
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

pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::State {} => to_json_binary(&query_state(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        governance: cfg.governance.to_string(),
        oracle_operator: cfg.oracle_operator.to_string(),
        paused: cfg.paused,
    })
}

fn query_state(deps: Deps) -> StdResult<StateResponse> {
    let st = ORACLE_STATE.load(deps.storage)?;
    Ok(StateResponse {
        rate: st.rate,
        last_update_sec: st.last_update_sec,
        utc_day_id: st.utc_day_id,
        day_baseline_rate: st.day_baseline_rate,
    })
}

pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}
