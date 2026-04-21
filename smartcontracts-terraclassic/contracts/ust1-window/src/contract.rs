//! Swap window: vFDUSD ↔ UST1 using oracle rate and fee on the UST1 leg.

use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, QueryRequest, Response, StdResult,
    Uint128, WasmMsg, WasmQuery,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, Cw20HookMsg, EffectiveSwapResponse, ExecuteMsg, InstantiateMsg, MigrateMsg,
    QueryMsg,
};
use crate::state::{
    Config, PendingGovernance, RollingVolume, CONFIG, CONTRACT_NAME, CONTRACT_VERSION,
    PENDING_GOVERNANCE, ROLLING,
};

fn query_oracle_state(
    deps: Deps,
    oracle: &cosmwasm_std::Addr,
) -> StdResult<ust1_oracle::msg::StateResponse> {
    let q = ust1_oracle::msg::QueryMsg::State {};
    let bin = to_json_binary(&q)?;
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: oracle.to_string(),
        msg: bin,
    }))
}

fn ensure_limits(
    env: &Env,
    rolling: &mut RollingVolume,
    cfg: &Config,
    ust1_notional: Uint128,
) -> Result<(), ContractError> {
    if ust1_notional > cfg.per_tx_ust1_limit {
        return Err(ContractError::PerTxLimit {});
    }
    let now = env.block.time.seconds();
    if rolling.window_start_sec == 0 || now >= rolling.window_start_sec + 86_400 {
        rolling.window_start_sec = now;
        rolling.volume_ust1 = Uint128::zero();
    }
    let next = rolling.volume_ust1.checked_add(ust1_notional)?;
    if next > cfg.rolling_24h_ust1_limit {
        return Err(ContractError::RollingLimit {});
    }
    rolling.volume_ust1 = next;
    Ok(())
}

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
    let cfg = Config {
        governance: deps.api.addr_validate(&msg.governance)?,
        oracle: deps.api.addr_validate(&msg.oracle)?,
        vfdusd_token: deps.api.addr_validate(&msg.vfdusd_token)?,
        ust1_token: deps.api.addr_validate(&msg.ust1_token)?,
        fee_bps: msg.fee_bps,
        per_tx_ust1_limit: msg.per_tx_ust1_limit,
        rolling_24h_ust1_limit: msg.rolling_24h_ust1_limit,
        paused: false,
    };
    CONFIG.save(deps.storage, &cfg)?;
    ROLLING.save(
        deps.storage,
        &RollingVolume {
            window_start_sec: 0,
            volume_ust1: Uint128::zero(),
        },
    )?;
    Ok(Response::new().add_attribute("action", "instantiate"))
}

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::SetLimits {
            per_tx_ust1_limit,
            rolling_24h_ust1_limit,
        } => exec_set_limits(deps, info, per_tx_ust1_limit, rolling_24h_ust1_limit),
        ExecuteMsg::SetPaused { paused } => exec_set_paused(deps, info, paused),
        ExecuteMsg::SetFeeBps { fee_bps } => exec_set_fee_bps(deps, info, fee_bps),
        ExecuteMsg::ProposeGovernance { address } => exec_propose_gov(deps, info, address),
        ExecuteMsg::AcceptGovernance {} => exec_accept_gov(deps, info),
    }
}

fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;
    if cfg.paused {
        return Err(ContractError::Paused {});
    }
    let hook: Cw20HookMsg = cosmwasm_std::from_json(&cw20_msg.msg)?;
    let sender = deps.api.addr_validate(&cw20_msg.sender)?;
    let amount = cw20_msg.amount;

    if info.sender == cfg.vfdusd_token {
        return deposit(deps, env, cfg, sender, amount, hook);
    }
    if info.sender == cfg.ust1_token {
        return withdraw(deps, env, cfg, sender, amount, hook);
    }
    Err(ContractError::InvalidCw20Hook {})
}

fn deposit(
    deps: DepsMut,
    env: Env,
    cfg: Config,
    beneficiary: cosmwasm_std::Addr,
    amount_vfdusd: Uint128,
    hook: Cw20HookMsg,
) -> Result<Response, ContractError> {
    if !matches!(hook, Cw20HookMsg::Deposit {}) {
        return Err(ContractError::InvalidCw20Hook {});
    }
    let rate = query_oracle_state(deps.as_ref(), &cfg.oracle)?.rate;
    let ust1_out = ust1_common::math::deposit_vfdusd_to_ust1(amount_vfdusd, rate, cfg.fee_bps)?;

    let mut rolling = ROLLING.load(deps.storage)?;
    ensure_limits(&env, &mut rolling, &cfg, ust1_out)?;
    ROLLING.save(deps.storage, &rolling)?;

    let mint = WasmMsg::Execute {
        contract_addr: cfg.ust1_token.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Mint {
            recipient: beneficiary.to_string(),
            amount: ust1_out,
        })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(mint)
        .add_attribute("action", "deposit")
        .add_attribute("ust1_out", ust1_out))
}

fn withdraw(
    deps: DepsMut,
    env: Env,
    cfg: Config,
    user: cosmwasm_std::Addr,
    gross_ust1: Uint128,
    hook: Cw20HookMsg,
) -> Result<Response, ContractError> {
    let min_out = match hook {
        Cw20HookMsg::Withdraw { min_vfdusd_out } => min_vfdusd_out,
        _ => return Err(ContractError::InvalidCw20Hook {}),
    };

    let rate = query_oracle_state(deps.as_ref(), &cfg.oracle)?.rate;
    let v_out = ust1_common::math::withdraw_gross_ust1_to_vfdusd(gross_ust1, rate, cfg.fee_bps)?;
    if v_out < min_out {
        return Err(ContractError::BelowMinimum {});
    }

    let bal: cw20::BalanceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: cfg.vfdusd_token.to_string(),
        msg: to_json_binary(&cw20::Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        })?,
    }))?;
    if bal.balance < v_out {
        return Err(ContractError::InsufficientVfdusd {});
    }

    let mut rolling = ROLLING.load(deps.storage)?;
    ensure_limits(&env, &mut rolling, &cfg, gross_ust1)?;
    ROLLING.save(deps.storage, &rolling)?;

    let burn = WasmMsg::Execute {
        contract_addr: cfg.ust1_token.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Burn { amount: gross_ust1 })?,
        funds: vec![],
    };

    let send_v = WasmMsg::Execute {
        contract_addr: cfg.vfdusd_token.to_string(),
        msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
            recipient: user.to_string(),
            amount: v_out,
        })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(burn)
        .add_message(send_v)
        .add_attribute("action", "withdraw")
        .add_attribute("vfdusd_out", v_out))
}

fn exec_set_limits(
    deps: DepsMut,
    info: MessageInfo,
    per_tx: Uint128,
    rolling: Uint128,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.governance {
        return Err(ContractError::Unauthorized {});
    }
    cfg.per_tx_ust1_limit = per_tx;
    cfg.rolling_24h_ust1_limit = rolling;
    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::new().add_attribute("action", "set_limits"))
}

fn exec_set_paused(
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

fn exec_set_fee_bps(
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
    let old_fee_bps = cfg.fee_bps;
    cfg.fee_bps = fee_bps;
    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::new()
        .add_attribute("action", "set_fee_bps")
        .add_attribute("old_fee_bps", old_fee_bps.to_string())
        .add_attribute("new_fee_bps", fee_bps.to_string()))
}

fn exec_propose_gov(
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

fn exec_accept_gov(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
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
        QueryMsg::EffectiveSwap {} => to_json_binary(&query_effective_swap(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let c = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        governance: c.governance.to_string(),
        oracle: c.oracle.to_string(),
        vfdusd_token: c.vfdusd_token.to_string(),
        ust1_token: c.ust1_token.to_string(),
        fee_bps: c.fee_bps,
        per_tx_ust1_limit: c.per_tx_ust1_limit,
        rolling_24h_ust1_limit: c.rolling_24h_ust1_limit,
        paused: c.paused,
    })
}

fn query_effective_swap(deps: Deps) -> StdResult<EffectiveSwapResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let rolling = ROLLING.load(deps.storage)?;
    let oracle = query_oracle_state(deps, &cfg.oracle)?;
    Ok(EffectiveSwapResponse {
        fee_bps: cfg.fee_bps,
        per_tx_ust1_limit: cfg.per_tx_ust1_limit,
        rolling_24h_ust1_limit: cfg.rolling_24h_ust1_limit,
        paused: cfg.paused,
        rolling_window_start_sec: rolling.window_start_sec,
        rolling_volume_ust1: rolling.volume_ust1,
        oracle,
    })
}

pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}
