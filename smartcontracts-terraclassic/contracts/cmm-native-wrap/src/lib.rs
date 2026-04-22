//! Treasury-style **native wrap** for Terra Classic **LUNC** (`uluna`) and **USTC** (`uusd`).
//!
//! - **Wrap**: send one native coin to [`ExecuteMsg::Wrap`]; the contract holds the native balance and
//!   **mints** the configured cw20-mintable wrapped token (1:1 atoms after `fee_bps`).
//! - **Unwrap**: cw20 **`Send`** with [`Cw20HookMsg::Unwrap`]; the contract **burns** wrapped tokens and
//!   **`BankMsg::Send`**s native to the user.
//!
//! There is **no oracle**. CW20 assets other than the two configured wrapped tokens are **out of scope**
//! (see GitLab issue #16).

pub mod contract;
pub mod error;
pub mod gov;
pub mod limits;
pub mod msg;
pub mod pairs;
pub mod state;
pub mod unwrap;
pub mod wrap;

#[cfg(test)]
mod multitest;

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
