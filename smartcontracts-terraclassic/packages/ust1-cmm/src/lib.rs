//! Canonical CMM protocol constants shared across CMM contracts.
//!
//! This crate is also published as a standalone repository:
//! <https://gitlab.com/PlasticDigits/ust1-cmm> (consume via Git dependency there when available).
//!
//! Withdraw wire format for this treasury lives in `ust1-window::treasury`
//! (`InstantWithdrawCw20` — Option 3 / [ust1-window#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20)).
//! Ops: treasury gov `SetCw20Spender` (+ `limit_24h`) after window migrate; see
//! [`docs/DEPLOYMENT.md`](../../../../docs/DEPLOYMENT.md) Phase 5 and
//! [`skills/window-instant-withdraw-cw20`](../../../../skills/window-instant-withdraw-cw20/SKILL.md).

/// Mainnet CMM treasury (vFDUSD custody for UST1 swaps). GitLab issue #17.
pub const CMM_TREASURY_MAINNET: &str =
    "terra16j5u6ey7a84g40sr3gd94nzg5w5fm45046k9s2347qhfpwm5fr6sem3lr2";
