//! Per-denom rolling window and per-transaction caps (**INV-LIMIT-NATIVE-001**).

use cosmwasm_std::{Env, Uint128};

use crate::error::ContractError;
use crate::state::{DenomPair, RollingVolume};

pub fn ensure_limits(
    env: &Env,
    rolling: &mut RollingVolume,
    pair: &DenomPair,
    notional: Uint128,
) -> Result<(), ContractError> {
    if notional > pair.per_tx_wrap_limit {
        return Err(ContractError::PerTxLimit {});
    }
    let now = env.block.time.seconds();
    if rolling.window_start_sec == 0 || now >= rolling.window_start_sec + 86_400 {
        rolling.window_start_sec = now;
        rolling.volume_wrap = Uint128::zero();
    }
    let next = rolling.volume_wrap.checked_add(notional)?;
    if next > pair.rolling_24h_wrap_limit {
        return Err(ContractError::RollingLimit {});
    }
    rolling.volume_wrap = next;
    Ok(())
}
