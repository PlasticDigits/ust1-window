# ust1-window

Monorepo for Terra Classic **UST1** swap tooling against bridged Venus **vFDUSD** (cw20-mintable), with a **rate oracle** (BSC Venus exchange rate) and a **swap window** contract, plus a separate **native wrap** contract for treasury-backed **wLUNC** / **wUSTC** (**uluna** / **uusd**) with **no oracle** (1:1 atoms after `fee_bps`).

## Packages

| Path | Role |
|------|------|
| `smartcontracts-terraclassic/packages/ust1-common` | Fixed-point math, oracle policy (`INV-*` in source), shared with contracts + service |
| `smartcontracts-terraclassic/packages/ust1-cmm` | CMM constants (e.g. mainnet treasury address); `ust1-window` depends on crate `ust1-cmm` via **Git** (see root `Cargo.toml`, `Cargo.lock`). Change this tree, then `cargo update -p ust1-cmm` and commit the lockfile. Optional later: dedicated [`ust1-cmm`](https://gitlab.com/PlasticDigits/ust1-cmm) repo. |
| `contracts/ust1-oracle` | On-chain rate `R`, 4h min interval, UTC daily +2% cap, monotonic |
| `contracts/ust1-window` | cw20 receive: vFDUSD→mint UST1 + forward vFDUSD to CMM treasury, UST1→burn + `TransferFrom` vFDUSD from treasury; governance-set fee on UST1 leg (`fee_bps`, default 1.0% with 50/50 chain-tax vs CMM accounting); treasury must `IncreaseAllowance` for the window on vFDUSD |
| `contracts/cmm-native-wrap` | Native `Wrap` + cw20 `Receive` unwrap: **uluna**↔wLUNC, **uusd**↔wUSTC only; governance `fee_bps` (default **1%** recommended per GitLab #17) with **50/50** chain-tax vs CMM attribution on events / `EffectiveWrap`; per-denom limits; **no** `ust1-oracle` (GitLab #16) |
| `oracle-service` | Polls BSC `exchangeRateStored`, applies same policy as chain, broadcasts `UpdateRate` |
| `scripts/` | Python 3 deploy helpers (no business logic) |

## Invariants (index)

- **INV-MATH-001 / INV-SWAP-001 / INV-SWAP-002** — `ust1-common/src/math.rs` (reverse path: `inv_swap_002_*` vector tests lock the fee-then-rate floor semantics)
- **INV-MATH-002** — `ust1-common/src/fee_split.rs` + `ust1-window` / `cmm-native-wrap` event attributes and `Effective*` queries (GitLab #17)
- **INV-ORACLE-THROTTLE-001 / INV-ORACLE-DAILY-001 / INV-ORACLE-MONO-001** — `ust1-common/src/oracle_policy.rs` + `ust1-oracle`
- **INV-LIMIT-001** — `ust1-window/src/state.rs`, enforced in `contract.rs`
- **INV-LIMIT-NATIVE-001** — `cmm-native-wrap/src/state.rs` / `limits.rs`, enforced in `wrap.rs` and `unwrap.rs`

Operator checklist, BSC + Terra address registry, and mainnet/testnet deployment notes: [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md) ([GitLab #15](https://gitlab.com/PlasticDigits/ust1-window/-/issues/15)).

## Local development

```bash
make start          # LocalTerra (Docker)
make wait-healthy
make test-contracts
# After deploy (see scripts/):
# export BSC_RPC_URLS=..., VENUS_VTOKEN_ADDRESS=..., TERRA_LCD_URL=..., ORACLE_CONTRACT=...
# cargo run -p ust1-oracle-service
```

## Oracle service: observability and deployment

The `ust1-oracle-service` binary is intentionally **lightweight**: it uses **structured `tracing` logs only** (no Prometheus or in-process metrics server). Every `check_rate_update` and `sign_and_broadcast_execute` outcome is logged at `info` (policy result) or `info`/`warn` (broadcast). If there has been **no successful on-chain broadcast** for longer than **`ORACLE_MAX_SILENCE_SECS`** (default **28800**, i.e. 8 hours), the process emits a **high-visibility `error!` liveness alert** on each poll tick.

**Production-style deployment** (Terra Classic wasm + BSC oracle path + operator checklist + address registry) is documented in [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md). After exporting env vars, run `make verify-oracle-env` to confirm required keys are present before starting the service.

**Deployment** (e.g. [Render](https://render.com)): the platform should provide an **HTTP health check** and **uptime / failure alerting** on the service URL or process, similar to Render’s built-in health checks and notifications. Whatever host you use must offer **comparable external monitoring** so silent process hangs or repeated crashes are surfaced; the in-process log alert is not a substitute for off-platform paging.

Relevant environment variables: `ORACLE_MAX_SILENCE_SECS`, `POLL_INTERVAL_SECS`, plus the oracle env vars listed under Local development above and in `docs/DEPLOYMENT.md`.

## Build optimized Wasm

```bash
make build-optimized   # Docker cosmwasm/optimizer → artifacts/
```

Contract crates live under top-level `contracts/` so CosmWasm `bob` picks them up (it only builds workspace members with that path prefix).

## Git hooks and secret scanning

- **Gitleaks** config: [`.gitleaks.toml`](.gitleaks.toml). CI runs [gitleaks/gitleaks-action](https://github.com/gitleaks/gitleaks-action) on every push/PR.
- **Pre-commit** ([`.pre-commit-config.yaml`](.pre-commit-config.yaml)): merge-conflict / YAML / EOF / whitespace checks, Gitleaks, ShellCheck on `scripts/**/*.sh`, `cargo fmt`, `cargo check`, `cargo clippy` (warnings denied), and `python3 -m compileall` on `scripts/`.

```bash
python3 -m venv .venv && . .venv/bin/activate   # recommended on PEP 668–managed systems
pip install -r requirements-dev.txt
make install-hooks                    # pre-commit install
make precommit                        # pre-commit run --all-files
```

## License

MIT OR Apache-2.0 (see individual crates).
