---
name: oracle-ops-poll-silence
description: >-
  Align ust1-oracle-service POLL_INTERVAL_SECS and ORACLE_MAX_SILENCE_SECS with
  window DEFAULT_MAX_ORACLE_AGE_SECS (audit H-3 / glab #24). Same-rate heartbeat
  UpdateRate after the 4h throttle keeps last_update_sec inside the 6h window
  (glab #32). Use when changing oracle-service defaults, heartbeat routing,
  ops timing warnings, liveness silence thresholds, DEPLOYMENT env table, or
  verify_oracle_operator_env advisories.
---

# Oracle ops poll / silence timing (H-3) + heartbeat (#32)

Issue: [ust1-window#24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24). Audit: `audits/INTERNAL_KIMIK3_1786162831.md` § H-3.

Heartbeat / event-confirm: [ust1-window#32](https://gitlab.com/PlasticDigits/ust1-window/-/issues/32) (incident [#31](https://gitlab.com/PlasticDigits/ust1-window/-/issues/31)).

Related: confirm-before-liveness **C-3** / [#23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23) / [#32](https://gitlab.com/PlasticDigits/ust1-window/-/issues/32) — silence keys off **confirmed** includes (DeliverTx wasm events, or State fallback). Skill: [`oracle-liveness-confirm`](../oracle-liveness-confirm/SKILL.md).

Cross-links: [docs/DEPLOYMENT.md](../../docs/DEPLOYMENT.md) § Oracle service environment, [README.md](../../README.md) § Oracle service, `oracle-service/src/{config,liveness,main}.rs`, `ust1_common::DEFAULT_MAX_ORACLE_AGE_SECS`.

## Invariants (must hold)

1. **INV-ORACLE-OPS-POLL-001**: Default `POLL_INTERVAL_SECS` ≤ **3600** and **strictly below** window `DEFAULT_MAX_ORACLE_AGE_SECS` (21600). A missed tick must not exhaust the entire staleness budget.
2. **INV-ORACLE-OPS-SILENCE-001**: Default `ORACLE_MAX_SILENCE_SECS` ≤ window max oracle age (default **21600**). Prefer page at or before user impact; documented grace ceiling is `max_age + poll`. `LIVENESS_ORACLE_NO_BROADCAST` means **no confirmed include** (not “Venus unchanged”).
3. **On-chain policy unchanged**: do not alter `MIN_ORACLE_UPDATE_INTERVAL_SECS` (4h), daily cap, or monotonicity when editing ops timing. Do **not** widen `max_oracle_age_sec` to “fix” staleness (DEX **O2**).
4. **Heartbeat (same-rate `UpdateRate`)**: when Venus `proposed == state.rate` and `check_rate_update` is `Ok` (age ≥ 4h, not paused), submit `UpdateRate` with **this tick’s** consensus `proposed`. Inside the throttle window, equal rate stays a cheap `SkipEqualRate` (no gas). Log `event=oracle_heartbeat` / `tick_kind=heartbeat`.
5. **Env overrides allowed**: misconfig emits `ORACLE_OPS_TIMING_MISCONFIG` warnings (and verify-script advisories); do not hard-fail load unless `poll`/`silence` are invalid for other reasons.
6. **C-3 / #32**: `record_successful_broadcast` only after DeliverTx `code == 0` plus wasm event match (or State fallback). CheckTx / SYNC is not success.

## Recommended relationship

```text
poll < max_oracle_age                 # default poll=3600, max_age=21600
silence ≤ max_oracle_age              # preferred (default silence=21600)
silence ≤ max_oracle_age + poll       # documented grace ceiling
throttle (4h) < max_age (6h)          # 2h gap for heartbeat; ~two 1h polls
```

While the process is healthy, max silence between confirmed updates should be
**≤ 4h + one poll + confirm timeout** (~5h), inside the 6h window budget.

Footgun: `POLL_INTERVAL_SECS=21600` + `ORACLE_MAX_SILENCE_SECS=86400` restores H-3 (zero tick margin + late paging).

Do **not** skip EVM consensus on heartbeat ticks (≥2 BSC RPCs, 0.01% agreement).

## Code map

| Path | Role |
|------|------|
| `oracle-service/src/config.rs` | Defaults, `resolve_*`, `ops_timing_warnings` |
| `oracle-service/src/liveness.rs` | `should_alert` / silence tracker |
| `oracle-service/src/main.rs` | Heartbeat routing, startup warnings, `LIVENESS_ORACLE_NO_BROADCAST` |
| `oracle-service/src/confirm.rs` | Event confirm vs State fallback |
| `scripts/verify_oracle_operator_env.sh` | Preflight advisories |
| `docs/DEPLOYMENT.md` | Operator env table + formula + `oracle_heartbeat` log examples |
| `ust1-common` `DEFAULT_MAX_ORACLE_AGE_SECS` / `MIN_ORACLE_UPDATE_INTERVAL_SECS` | Window staleness + throttle (leave unless separate issue) |

## Tests to run

```bash
cargo test -p ust1-oracle-service
cargo clippy -p ust1-oracle-service -- -D warnings
```

Key cases: default poll/silence constants; env override resolve; `ops_timing_warnings` on legacy 21600/28800; liveness `should_alert` boundary; equal-rate inside throttle (no tx); heartbeat after 4h; paused / mono / daily-cap skips.

## Out of scope

- Changing on-chain throttle / daily / mono policy (same-rate after 4h is already `Ok`).
- DeliverTx event-confirm details — [`oracle-liveness-confirm`](../oracle-liveness-confirm/SKILL.md).
- Window `max_oracle_age_sec` governance changes (operators must keep env aligned if they change it).
- DEX UI gates / `check-ust1-wrap-ops-health.sh` (follow-up pointers on DEX #503).
