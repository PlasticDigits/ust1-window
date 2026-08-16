---
name: oracle-liveness-confirm
description: >-
  Confirm Terra DeliverTx wasm events (or State fallback) before recording
  ust1-oracle-service liveness success (INV-ORACLE-LIVENESS-001 / audit C-3 /
  GitLab #23 / #32). Same-rate heartbeat UpdateRate after the 4h throttle is a
  confirmed include when events (or State) match.
---

# Oracle liveness confirmation (DeliverTx events + State fallback)

Use this skill when changing oracle-service broadcast, LCD confirmation, silence
alerting, heartbeat same-rate refresh, or anything that calls
`LivenessTracker::record_successful_broadcast`.

Companion: [`oracle-ops-poll-silence`](../oracle-ops-poll-silence/SKILL.md) (poll ≪
max age; heartbeat uses the 4h–6h gap). Operator docs: [`docs/DEPLOYMENT.md`](../../docs/DEPLOYMENT.md).

## Invariant

**INV-ORACLE-LIVENESS-001** ([GitLab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23),
[#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28),
[#32](https://gitlab.com/PlasticDigits/ust1-window/-/issues/32),
audit C-3 in `audits/INTERNAL_KIMIK3_1786162831.md`):

1. `BROADCAST_MODE_SYNC` with `tx_response.code == 0` means **CheckTx only**.
2. Liveness success requires DeliverTx inclusion (`GET /cosmos/tx/v1beta1/txs/{hash}`,
   `code == 0`, hash `eq_ignore_ascii_case` the broadcast hash) **and**:
   - **Preferred:** wasm events on **that same tx** with `_contract_address == ORACLE_CONTRACT`,
     `action == update_rate`, and `rate` equal to this tick's proposed `Uint128`
     ([`confirm::oracle_tx_events_match_update`](../../oracle-service/src/confirm.rs)).
   - **Fallback:** if structured wasm attrs are **absent** (stripped LCD), retried
     oracle `State` with `last_update_sec` advanced and `rate` equal to proposed
     ([`confirm::oracle_state_matches_intended_update`](../../oracle-service/src/confirm.rs)).
3. After event match, a lagged LCD `State` is **`warn!` only**
   (`oracle_state_lag_after_event_ok`) — do **not** fail-close or skip the liveness record.
4. Uncertain inclusion, DeliverTx `code != 0`, confirmation timeout, hash mismatch,
   wrong `_contract_address`, missing `action`, or event `rate != proposed`
   ⇒ **fail closed** (do **not** record liveness). Do not treat `raw_log` substring
   matching as sufficient when structured `events` exist.
5. On account sequence mismatch, refresh account and retry broadcast **once**.
6. Never log full LCD URLs with embedded API keys (use redacted LCD base).
7. Oracle-paused skip, policy skip (`check_rate_update` Err), and equal-rate skip
   **inside** the 4h throttle must **not** call `record_successful_broadcast`.
   Equal-rate **after** throttle is a heartbeat `Submit` and **does** record liveness
   after confirm (`event=oracle_heartbeat` / `tick_kind=heartbeat`).
8. Heartbeat uses this tick's multi-RPC Venus `proposed`, never a cached last-success
   rate. No governance txs. No auto-unpause.

## Code map

| Concern | Path |
|---------|------|
| Event match + State matcher | `oracle-service/src/confirm.rs` |
| SYNC broadcast + sequence retry + DeliverTx poll (returns wasm events) | `oracle-service/src/terra_tx.rs` |
| Heartbeat routing + success gating | `oracle-service/src/main.rs` (`decide_tick_action`, `submit_and_confirm_oracle_update`) |
| Silence tracker semantics | `oracle-service/src/liveness.rs` |
| `ORACLE_TX_CONFIRM_*` / silence env | `oracle-service/src/config.rs` |
| Same-rate-after-throttle policy | `ust1-common` `oracle_policy.rs`; `ust1-oracle` multitest |
| TEST-16 LocalTerra gate | `scripts/localterra_e2e_smoke.sh`, `docs/DEPLOYMENT.md` § TEST-16 |
| Operator docs | `docs/DEPLOYMENT.md`, root `README.md` |
| Poll / heartbeat timing | [`oracle-ops-poll-silence`](../oracle-ops-poll-silence/SKILL.md) |

## Env

| Variable | Default | Meaning |
|----------|---------|---------|
| `ORACLE_TX_CONFIRM_TIMEOUT_SECS` | `90` | Max wait for DeliverTx after CheckTx |
| `ORACLE_TX_CONFIRM_POLL_INTERVAL_MS` | `2000` | Poll interval (+ small jitter) |
| `ORACLE_MAX_SILENCE_SECS` | `21600` | Alert if no **confirmed** include (heartbeat counts) |

No `ORACLE_HEARTBEAT_ENABLE` knob: heartbeat is always on when policy `Ok`.

## Tests to keep green

```bash
cargo test -p ust1-oracle-service
cargo test -p ust1-common
cargo test -p ust1-oracle
make test-localterra-smoke   # optional; skip-clean without LocalTerra (#28 / TEST-16)
```

Covered paths: confirm success, DeliverTx fail, timeout, state mismatch (events absent),
event match + lagged State (liveness recorded), wrong contract, event rate mismatch,
sequence retry, hash mismatch, URL redaction, equal-rate skip inside throttle (no
liveness), heartbeat after throttle, policy-skip (no liveness), paused skip, EVM
disagreement, BSC hang timeout. Prefer `wiremock` LCD/BSC fixtures over live chain.

## Out of scope (related issues)

- **H-3 / #24**: silence / poll timing vs window staleness budget — see
  [`oracle-ops-poll-silence`](../oracle-ops-poll-silence/SKILL.md).
- **M-1**: local clock vs block time for off-chain policy (separate).
- On-chain oracle policy changes (same-rate is already `Ok` after throttle; no migrate).
- DEX runbook **O2** / UI `isOracleStale`: do not disable window age checks; heartbeat
  is the operator fix ([cl8y-dex-terraclassic#503](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/503)).
