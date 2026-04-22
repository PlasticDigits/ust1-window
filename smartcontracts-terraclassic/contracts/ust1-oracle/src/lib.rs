//! UST1 oracle: stores fixed-point FDUSD-per-vFDUSD rate with governance + operator roles.

pub mod contract;
pub mod error;
pub mod msg;
pub mod state;

pub use msg::{
    ExecuteMsg as OracleExecuteMsg, InstantiateMsg as OracleInstantiateMsg,
    QueryMsg as OracleQueryMsg, StateResponse,
};

#[cfg(test)]
mod multitest;

// `crate-type` includes `rlib`; entry points are only referenced by the Wasm host, so the `rlib`
// build sees them as unused (CosmWasm `#[entry_point]` does not satisfy `dead_code` for that target).
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
