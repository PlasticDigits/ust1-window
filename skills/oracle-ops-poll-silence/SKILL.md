---
name: oracle-ops-poll-silence
description: >-
  Align ust1-oracle-service POLL_INTERVAL_SECS and ORACLE_MAX_SILENCE_SECS with
  window DEFAULT_MAX_ORACLE_AGE_SECS (audit H-3 / glab #24). Use when changing
  oracle-service defaults, ops timing warnings, liveness silence thresholds,
  DEPLOYMENT env table, or verify_oracle_operator_env advisories.
---

# Oracle ops poll / silence timing (H-3)

Issue: [ust1-window#24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24). Audit: `audits/INTERNAL_KIMIK3_1786162831.md` § H-3.

Related: confirm-before-liveness **C-3** / [#23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23) — silence must eventually key off **confirmed** updates; H-3 only fixes poll/silence defaults vs window staleness.

Cross-links: [docs/DEPLOYMENT.md](../../docs/DEPLOYMENT.md) § Oracle service environment, [README.md](../../README.md) § Oracle service, `oracle-service/src/{config,liveness,main}.rs`, `ust1_common::DEFAULT_MAX_ORACLE_AGE_SECS`.

## Invariants (must hold)

1. **INV-ORACLE-OPS-POLL-001**: Default `POLL_INTERVAL_SECS` ≤ **3600** and **strictly below** window `DEFAULT_MAX_ORACLE_AGE_SECS` (21600). A missed tick must not exhaust the entire staleness budget.
2. **INV-ORACLE-OPS-SILENCE-001**: Default `ORACLE_MAX_SILENCE_SECS` ≤ window max oracle age (default **21600**). Prefer page at or before user impact; documented grace ceiling is `max_age + poll`.
3. **On-chain policy unchanged**: do not alter `MIN_ORACLE_UPDATE_INTERVAL_SECS`, daily cap, or monotonicity when editing ops timing.
4. **Env overrides allowed**: misconfig emits `ORACLE_OPS_TIMING_MISCONFIG` warnings (and verify-script advisories); do not hard-fail load unless `poll`/`silence` are invalid for other reasons.
5. **C-3 dependency**: until #23, `record_successful_broadcast` after SYNC/CheckTx is not full liveness correctness — document, do not claim otherwise.

## Recommended relationship

```text
poll < max_oracle_age                 # default poll=3600, max_age=21600
silence ≤ max_oracle_age              # preferred (default silence=21600)
silence ≤ max_oracle_age + poll       # documented grace ceiling
```

Footgun: `POLL_INTERVAL_SECS=21600` + `ORACLE_MAX_SILENCE_SECS=86400` restores H-3 (zero tick margin + late paging).

## Code map

| Path | Role |
|------|------|
| `oracle-service/src/config.rs` | Defaults, `resolve_*`, `ops_timing_warnings` |
| `oracle-service/src/liveness.rs` | `should_alert` / silence tracker |
| `oracle-service/src/main.rs` | Startup warnings + `LIVENESS_ORACLE_NO_BROADCAST` |
| `scripts/verify_oracle_operator_env.sh` | Preflight advisories |
| `docs/DEPLOYMENT.md` | Operator env table + formula |
| `ust1-common` `DEFAULT_MAX_ORACLE_AGE_SECS` | On-chain window staleness default (leave unless separate issue) |

## Tests to run

```bash
cargo test -p ust1-oracle-service
cargo clippy -p ust1-oracle-service -- -D warnings
```

Key cases: default poll/silence constants; env override resolve; `ops_timing_warnings` on legacy 21600/28800; liveness `should_alert` boundary.

## Out of scope

- Changing on-chain throttle / daily / mono policy.
- DeliverTx confirmation semantics (C-3 / #23).
- Window `max_oracle_age_sec` governance changes (operators must keep env aligned if they change it).
