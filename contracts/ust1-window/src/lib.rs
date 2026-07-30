//! UST1 swap window: cw20 receive hooks for vFDUSD deposit and UST1 withdrawal.

pub mod contract;
pub mod error;
pub mod msg;
pub mod state;

#[cfg(test)]
mod multitest;

// See `ust1-oracle` `entry` module: `rlib` + Wasm entry points trigger `dead_code` without this.
#[cfg(not(feature = "library"))]
#[allow(dead_code)]
mod entry {
    use cosmwasm_std::{entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};

    use crate::contract;
    use crate::error::ContractError;
    use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};

    #[entry_point]
    pub fn instantiate(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: InstantiateMsg,
    ) -> Result<Response, ContractError> {
        contract::instantiate(deps, env, info, msg)
    }

    #[entry_point]
    pub fn execute(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: ExecuteMsg,
    ) -> Result<Response, ContractError> {
        contract::execute(deps, env, info, msg)
    }

    #[entry_point]
    pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
        contract::query(deps, env, msg)
    }

    #[entry_point]
    pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
        contract::migrate(deps, env, msg)
    }
}
