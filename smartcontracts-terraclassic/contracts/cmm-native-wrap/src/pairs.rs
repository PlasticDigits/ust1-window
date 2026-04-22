//! Instantiate-time validation for the two supported native denoms.

use crate::error::ContractError;
use crate::state::{DenomPair, LUNC_DENOM, USTC_DENOM};

pub fn validate_pairs(pairs: &[DenomPair]) -> Result<(), ContractError> {
    if pairs.len() != 2 {
        return Err(ContractError::InvalidPairCount { expected: 2 });
    }
    let d0 = pairs[0].native_denom.as_str();
    let d1 = pairs[1].native_denom.as_str();
    let ok = (d0 == LUNC_DENOM && d1 == USTC_DENOM) || (d0 == USTC_DENOM && d1 == LUNC_DENOM);
    if !ok {
        return Err(ContractError::InvalidPairDenoms {
            a: LUNC_DENOM.to_string(),
            b: USTC_DENOM.to_string(),
        });
    }
    if pairs[0].wrapped_token == pairs[1].wrapped_token {
        return Err(ContractError::DuplicateWrappedToken {});
    }
    Ok(())
}
