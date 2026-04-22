//! Fee accounting: total swap fee (`fee_bps`) split into chain-tax coverage vs CMM protocol (GitLab #17).
//!
//! Used for **UST1-leg** amounts in `ust1-window` and for **symmetric** wrap/unwrap fee attribution in
//! `cmm-native-wrap` (wLUNC / wUSTC). The on-chain debit/credit is still a single `apply_fee_ust1` step;
//! this split is for **events, queries, and indexers**.

/// Split `fee_total_bps` into (chain_tax_coverage_bps, cmm_protocol_fee_bps).
///
/// Uses floor half for chain tax and the remainder for CMM so the components always sum to `fee_total_bps`.
pub fn chain_tax_and_cmm_protocol(fee_total_bps: u16) -> (u16, u16) {
    let chain = fee_total_bps / 2;
    let cmm = fee_total_bps - chain;
    (chain, cmm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sums_to_total() {
        for fee in [0u16, 1, 50, 100, 9999, 10_000] {
            let (a, b) = chain_tax_and_cmm_protocol(fee);
            assert_eq!(a + b, fee);
        }
    }

    #[test]
    fn split_100_is_50_50() {
        assert_eq!(chain_tax_and_cmm_protocol(100), (50, 50));
    }
}
