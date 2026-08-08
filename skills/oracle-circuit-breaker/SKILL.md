---
name: oracle-circuit-breaker
description: >-
  Implement, review, or ops-trip the ust1-oracle pause circuit breaker so
  ust1-window deposit/withdraw fail closed immediately when oracle paused
  (INV-ORACLE-PAUSE-001). Use when changing State.paused, ensure_oracle_usable,
  OraclePaused, SetPaused ACL, DEPLOYMENT emergency pause, or GitLab issue #22.
---

# Oracle circuit breaker (pause → window fail-closed)

Implements **only** audit C-2 recommendation #1 from [`audits/INTERNAL_KIMIK3_1786162831.md`](../../audits/INTERNAL_KIMIK3_1786162831.md). Issue: [ust1-window#22](https://gitlab.com/PlasticDigits/ust1-window/-/issues/22).

Cross-links: [docs/DEPLOYMENT.md](../../docs/DEPLOYMENT.md) [Emergency pause](../../docs/DEPLOYMENT.md#emergency-pause-oracle-circuit-breaker-vs-window), [README.md](../../README.md) invariants, companion skill [`window-instant-withdraw-cw20`](../window-instant-withdraw-cw20/SKILL.md) (guards must keep pause checks).

## Why this exists

Monotonicity blocks marking the rate **down**. Without an oracle-level kill switch, windows keep minting/redeeming at the last high rate until `max_oracle_age_sec` (~6h) — a slow treasury drain bounded only by rolling limits. Governance `SetPaused` on the oracle must freeze **all** readers immediately.

## Invariants (must hold)

1. **INV-ORACLE-PAUSE-001**: When oracle `Config.paused` / `State.paused` is true:
   - `UpdateRate` rejects (`Paused`).
   - Window **deposit and withdraw** both reject with `OraclePaused` **before** age/staleness checks (fresh `last_update_sec` must not bypass).
2. Pause **and** unpause are **governance-only** (no operator trip / auto-unpause in this skill unless a later issue adds it).
3. Queries (`Config`, `State`) remain readable while paused (monitoring).
4. Do **not** weaken monotonicity, daily cap, throttle, or rolling limits.
5. No emergency rate-decrease / reset in this path (tracked separately under remaining C-2).

## Code map

| Path | Role |
|------|------|
| `contracts/ust1-oracle/src/msg.rs` | `StateResponse.paused` |
| `contracts/ust1-oracle/src/contract.rs` | `query_state` merges `CONFIG.paused`; `SetPaused` gov-only |
| `contracts/ust1-oracle/src/state.rs` | Config invariant docs |
| `contracts/ust1-window/src/contract.rs` | `ensure_oracle_usable` (pause then stale) |
| `contracts/ust1-window/src/error.rs` | `OraclePaused` |
| `contracts/ust1-window/src/multitest.rs` | pause→halt→unpause→resume; ACL; stale still stale |
| `oracle-service/src/main.rs` | Skip broadcast when `state.paused` |
| `docs/DEPLOYMENT.md` | Emergency pause runbook |

## Tests to run

```bash
cargo test -p ust1-oracle --lib
cargo test -p ust1-window --lib
cargo test -p ust1-integration-tests --lib
```

Key cases: `state_surfaces_paused_flag_for_circuit_breaker`; `oracle_paused_blocks_deposit_and_withdraw_while_rate_fresh`; unpaused+stale → `OracleStale`; window pause vs oracle pause; unauthorized `SetPaused`.

## Ops checklist

1. Prefer oracle pause over per-window pause when collapse is detected at the oracle trust boundary.
2. Migrate **oracle then window** (additive `State.paused`; no storage layout break).
3. Trip: governance `{"set_paused":{"paused":true}}` on oracle; confirm `state.paused` and failed window swaps.
4. Clear: governance unpause only after incident review.

## Out of scope

- Emergency rate reset / monotonic bypass
- Operator `TripCircuitBreaker` (optional in #22; not implemented)
- Off-chain spot-vs-`exchangeRateStored` divergence alerter
