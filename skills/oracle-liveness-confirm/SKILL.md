---
name: oracle-liveness-confirm
description: >-
  Confirm Terra DeliverTx + oracle State before recording ust1-oracle-service
  liveness success (INV-ORACLE-LIVENESS-001 / audit C-3 / GitLab #23).
---

# Oracle liveness confirmation (DeliverTx + State)

Use this skill when changing oracle-service broadcast, LCD confirmation, silence
alerting, or anything that calls `LivenessTracker::record_successful_broadcast`.

## Invariant

**INV-ORACLE-LIVENESS-001** ([GitLab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23),
audit C-3 in `audits/INTERNAL_KIMIK3_1786162831.md`):

1. `BROADCAST_MODE_SYNC` with `tx_response.code == 0` means **CheckTx only**.
2. Liveness success requires **DeliverTx** inclusion (`GET /cosmos/tx/v1beta1/txs/{hash}`,
   `code == 0`) **and** oracle `State` with `last_update_sec` advanced and `rate`
   equal to the proposed update.
3. Uncertain inclusion, DeliverTx failure, confirmation timeout, or state mismatch
   ⇒ **fail closed** (do **not** record liveness).
4. Bind confirmation to the **exact txhash** returned by broadcast (reject hash mismatch).
5. On account sequence mismatch, refresh account and retry broadcast **once**.
6. Never log full LCD URLs with embedded API keys (use redacted LCD base).

## Code map

| Concern | Path |
|---------|------|
| State match helper | `oracle-service/src/confirm.rs` |
| SYNC broadcast + sequence retry + DeliverTx poll | `oracle-service/src/terra_tx.rs` |
| Success gating before liveness | `oracle-service/src/main.rs` (`submit_and_confirm_oracle_update`) |
| Silence tracker semantics | `oracle-service/src/liveness.rs` |
| `ORACLE_TX_CONFIRM_*` / silence env | `oracle-service/src/config.rs` |
| Operator docs | `docs/DEPLOYMENT.md`, root `README.md` |

## Env

| Variable | Default | Meaning |
|----------|---------|---------|
| `ORACLE_TX_CONFIRM_TIMEOUT_SECS` | `90` | Max wait for DeliverTx after CheckTx |
| `ORACLE_TX_CONFIRM_POLL_INTERVAL_MS` | `2000` | Poll interval (+ small jitter) |
| `ORACLE_MAX_SILENCE_SECS` | `28800` | Alert if no **confirmed** update |

## Tests to keep green

```bash
cargo test -p ust1-oracle-service
```

Covered paths: confirm success, DeliverTx fail, timeout, state mismatch, sequence
retry, hash mismatch, URL redaction. Prefer `wiremock` LCD fixtures over live chain.

## Out of scope (related issues)

- **H-3**: silence / poll timing vs window staleness budget (separate).
- **M-1**: local clock vs block time for off-chain policy (separate).
- On-chain oracle policy changes (not part of #23).
