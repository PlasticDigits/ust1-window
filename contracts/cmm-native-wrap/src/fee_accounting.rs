//! Event and query fields for **INV-MATH-002** fee split (GitLab #17).
//!
//! Wrap and unwrap apply [`ust1_common::math::apply_fee_ust1`] with governance `fee_bps`. The **total**
//! is unchanged; this module surfaces the same **chain-tax vs CMM protocol** attribution as
//! [`ust1_common::fee_split::chain_tax_and_cmm_protocol`] on responses and [`crate::msg::EffectiveWrapResponse`].

use cosmwasm_std::Response;

/// Append `fee_total_bps`, `fee_chain_tax_bps`, `fee_cmm_protocol_bps` for indexer accounting.
pub(crate) fn with_fee_split_attributes(response: Response, fee_bps: u16) -> Response {
    let (fee_chain_tax_bps, fee_cmm_protocol_bps) =
        ust1_common::fee_split::chain_tax_and_cmm_protocol(fee_bps);
    response
        .add_attribute("fee_total_bps", fee_bps.to_string())
        .add_attribute("fee_chain_tax_bps", fee_chain_tax_bps.to_string())
        .add_attribute("fee_cmm_protocol_bps", fee_cmm_protocol_bps.to_string())
}
