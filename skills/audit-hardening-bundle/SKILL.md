---
name: audit-hardening-bundle
description: >-
  Implement or review the issue #25 audit hardening bundle (M-2/M-3/M-8/M-12–14/M-19,
  L-8–10/L-16/L-20/L-21): dust zero-output guards, decimal instantiate invariant,
  cw20-mintable UpdateMinter(None) map cleanup, oracle-service timeouts/gas/healthz/
  SIGTERM, and related tests/docs. Use when changing window deposit/withdraw guards,
  oracle-service reliability, or gitleaks/.env hygiene tied to audits/INTERNAL_KIMIK3_*.
---

# Audit hardening bundle (#25)

Source findings: [`audits/INTERNAL_KIMIK3_1786162831.md`](../../audits/INTERNAL_KIMIK3_1786162831.md).
Issue: [ust1-window#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25).
Coverage backfill: [ust1-window#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28).
Ops/env: [`docs/DEPLOYMENT.md`](../../docs/DEPLOYMENT.md) (incl. [TEST-16](../../docs/DEPLOYMENT.md#test-16--localterra-e2e-status-28)). Invariant index: [`README.md`](../../README.md).
Companion withdraw skill: [`window-instant-withdraw-cw20`](../window-instant-withdraw-cw20/SKILL.md).
External fork: [PlasticDigits/cw20-mintable](https://github.com/PlasticDigits/cw20-mintable) (`INV-MINTER-001`).

## Invariants (must hold)

| ID | Rule | Where |
|----|------|--------|
| **INV-SWAP-003** | Deposit reverts when `ust1_out == 0` before treasury forward / mint | `ust1-window` `deposit` |
| **INV-SWAP-004** | Withdraw reverts when `v_out == 0` before burn / treasury pull | `ust1-window` `withdraw` |
| **INV-DECIMALS-001** | Bridged vFDUSD decimals ≥ UST1 decimals at instantiate **and** migrate | `validate_token_decimals` |
| **INV-MINTER-001** | `UpdateMinter` clears old primary from `MINTERS` map (fork + in-repo integration) | `cw20-mintable` + `ust1-integration-tests` |
| **INV-ORACLE-TICK-001** | Hung BSC/LCD cannot block past `TICK_TIMEOUT_SECS` / `BSC_RPC_TIMEOUT_SECS`; fail tick, not process | `oracle-service` `main` / `bsc` |
| **INV-ORACLE-ACCOUNT-001** | Missing/unparseable `sequence` / `account_number` → hard error (no `0` surprise) | `terra_tx::parse_account_info` |
| **INV-ORACLE-GAS-001** | Fee uses `max(configured TERRA_GAS_PRICE, network_min)` when probe works; else configured floor | `terra_tx::resolve_gas_price` |
| **INV-ORACLE-HEALTHZ-001** | `GET /healthz` is **process-up only** — never claim on-chain rate freshness | `healthz.rs` + docs |

Dust math floors (`gross` tiny / `fee_bps=100%` → 0) live in `ust1-common::math`; the **contract** must still reject before state mutation (forked mintable allows `Mint(0)` / `Transfer(0)`).

## Code map

| Path | Role |
|------|------|
| `contracts/ust1-window/src/{contract,error}.rs` | Zero-output + decimal guards |
| `contracts/ust1-window/src/multitest.rs` | Dust, decimals, stale-withdraw, rolling reset, governance |
| `smartcontracts-terraclassic/packages/ust1-common/src/math.rs` | `u128::MAX` / proptest edges (L-20) |
| `oracle-service/src/{main,bsc,terra_tx,config,liveness,healthz}.rs` | Timeouts, gas, SIGTERM/`operator_loop`, mutex recover, `/healthz` |
| `smartcontracts-terraclassic/tests/src/cw20_minter_integration.rs` | In-repo **INV-MINTER-001** proof against pinned fork (#28) |
| `scripts/localterra_e2e_smoke.sh` | TEST-16 gated LocalTerra smoke (skip if LCD down) |
| `.gitignore` / `.gitleaks.toml` | `.env.*`; docs/target allowlists |
| Workspace `cw20-mintable` git rev | Pin after fork merge |

## Env knobs (oracle)

| Variable | Default | Notes |
|----------|---------|-------|
| `BSC_RPC_TIMEOUT_SECS` | 30 | Per-RPC HTTP timeout |
| `TICK_TIMEOUT_SECS` | 120 | Whole-tick wall clock |
| `TERRA_GAS_PRICE` | 0.015 | Floor; adaptive with LCD min when available |
| `HEALTHZ_BIND` | `0.0.0.0:8080` | Empty / `off` / `disabled` disables listener |

## Tests to run

```bash
cargo test -p ust1-window --lib
cargo test -p ust1-common --lib
cargo test -p ust1-integration-tests --lib inv_minter_001
cargo test -p ust1-oracle-service -- --test-threads=4
make test-localterra-smoke   # TEST-16; skip-clean without LocalTerra
git check-ignore -v .env.local
```

Key cases: dust deposit/withdraw unchanged balances; instantiate decimal inversion rejected; 6/6 happy path; account JSON missing sequence errors; gas `max(configured, network)`; `/healthz` → 200; poisoned liveness mutex recovers; tick timeout does not panic the process; BSC hang fails within `BSC_RPC_TIMEOUT_SECS`; `operator_loop` exits on shutdown hook; `UpdateMinter(None)` clears `MINTERS` in integration suite.

## Out of scope (tracked elsewhere)

- H-1 deposit `min_ust1_out` / fee cap
- C-1/C-2/C-3 (wire drift, circuit breaker, ConfirmTx)
- H-3 poll vs stale-age defaults
- Re-storing mainnet UST1 wasm for M-8 (fork fix applies to **future** stores unless migrated)
