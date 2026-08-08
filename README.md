# ust1-window

Monorepo for Terra Classic **UST1** swap tooling against bridged Venus **vFDUSD** (cw20-mintable), with a **rate oracle** (BSC Venus exchange rate) and a **swap window** contract, plus a separate **native wrap** contract for treasury-backed **wLUNC** / **wUSTC** (**uluna** / **uusd**) with **no oracle** (1:1 atoms after `fee_bps`).

## Mainnet status (`columbus-5`)

Tokens + oracle/window instantiate complete ([GitLab #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)). Post-deploy wiring (`add_minter`, window migrate + treasury `SetCw20Spender` for InstantWithdrawCw20, first `UpdateRate`, oracle service) still pending — full operator registry in [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md). Redeem path: [GitLab #20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20).

### Contracts & tokens

| Asset | Address | Code ID | Notes |
|-------|---------|---------|-------|
| cw20-mintable | — | **10184** | Shared CW20 code |
| **vFDUSD** (bridged CW20) | `terra1mnl9azefrqpmu888ar2u6zrcwr80hxlt3avf4300r576cw5ar7esvxsvj3` | 10184 | Decimals **6**; minter = CL8Y Terra bridge |
| **UST1** | `terra1f0eqgy9w7e5e7up97vjudqwx38tesf8ylx75x2lv3nwm0clry0pqmgfy72` | 10184 | Decimals **6**; minter = governance (window `add_minter` pending) |
| **ust1-oracle** | `terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n` | **11549** | Operator `terra1hm3ph0jevtkuc9efj9q3ld3ktk3g6la3ruhqna`; initial rate `1e18` |
| **ust1-window** | `terra1zxwpzpzpleatqn39r00grau4yt29sld8pw78s7ktvjafnj5nsaxq0h3rh2` | **11550** | `fee_bps=100`; per-tx **1000** / rolling 24h **10000** UST1; CMM treasury default |
| CMM treasury (ustr-cmm) | `terra16j5u6ey7a84g40sr3gd94nzg5w5fm45046k9s2347qhfpwm5fr6sem3lr2` | — | **Contract**; window redeem via `InstantWithdrawCw20` after gov `SetCw20Spender` ([#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20)) |
| CL8Y Terra bridge | `terra18m02l2f43c2dagqnz3kfccpgz9pzzz5hk9l5mh5wvr6dcvv47zfqdfs7la` | — | vFDUSD minter; BSC↔Terra registered |

### Roles & BSC

| Role / asset | Address |
|--------------|---------|
| Terra deployer (`cl8ydeploy`) | `terra1hu4zggf3f8yw6jw3rxrjxn2drwad675gq5k2lv` |
| Terra admin / governance / bridge admin (`cl8y2_admin`) | `terra1xsecn4snv94ezcez0z3vq8an9j4h4kxxcydp8l` |
| Oracle operator | `terra1hm3ph0jevtkuc9efj9q3ld3ktk3g6la3ruhqna` |
| Venus vFDUSD (BSC, 8 decimals, LockUnlock) | `0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba` |
| CL8Y BSC TokenRegistry | `0x3d8820ec93748fd4df8eee6b763834a23938b207` |
| BSC admin (TokenRegistry owner) | `0xcd4eb82cfc16d5785b4f7e3bfc255e735e79f39c` |

## Packages

| Path | Role |
|------|------|
| `smartcontracts-terraclassic/packages/ust1-common` | Fixed-point math, oracle policy (`INV-*` in source), shared with contracts + service |
| `smartcontracts-terraclassic/packages/ust1-cmm` | CMM constants (e.g. mainnet treasury address); `ust1-window` depends on crate `ust1-cmm` via **Git** (see root `Cargo.toml`, `Cargo.lock`). Change this tree, then `cargo update -p ust1-cmm` and commit the lockfile. Optional later: dedicated [`ust1-cmm`](https://gitlab.com/PlasticDigits/ust1-cmm) repo. |
| `contracts/ust1-oracle` | On-chain rate `R`, 4h min interval, UTC daily +2% cap, monotonic |
| `contracts/ust1-window` | cw20 receive: vFDUSD→mint UST1 + forward vFDUSD to CMM treasury; UST1→burn + treasury `InstantWithdrawCw20` (registered spender; no CW20 allowance); governance-set fee on UST1 leg (`fee_bps`, default 1.0% with 50/50 chain-tax vs CMM accounting). Skill: [`skills/window-instant-withdraw-cw20`](skills/window-instant-withdraw-cw20/SKILL.md) |
| `contracts/cmm-native-wrap` | Native `Wrap` + cw20 `Receive` unwrap: **uluna**↔wLUNC, **uusd**↔wUSTC only; governance `fee_bps` (default **1%** recommended per GitLab #17) with **50/50** chain-tax vs CMM attribution on events / `EffectiveWrap`; per-denom limits; **no** `ust1-oracle` (GitLab #16) |
| `oracle-service` | Polls BSC `exchangeRateStored`, applies same policy as chain, broadcasts `UpdateRate` |
| `scripts/` | Python 3 deploy helpers (no business logic) |

## Invariants (index)

- **INV-MATH-001 / INV-SWAP-001 / INV-SWAP-002** — `ust1-common/src/math.rs` (reverse path: `inv_swap_002_*` vector tests lock the fee-then-rate floor semantics)
- **INV-SWAP-003 / INV-SWAP-004** — `ust1-window/src/contract.rs` (deposit/withdraw revert on zero output; math dust floors in `ust1-common` before contract guard) ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25))
- **INV-DECIMALS-001** — `ust1-window/src/contract.rs` (`validate_token_decimals`: vFDUSD decimals ≥ UST1 decimals at instantiate/migrate) ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25))
- **INV-MINTER-001** — `cw20-mintable` fork: `UpdateMinter` clears old primary from `MINTERS` ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25); [cw20-mintable#1](https://github.com/PlasticDigits/cw20-mintable/pull/1))
- **INV-MATH-002** — `ust1-common/src/fee_split.rs` + `ust1-window` / `cmm-native-wrap` event attributes and `Effective*` queries (GitLab #17)
- **INV-ORACLE-THROTTLE-001 / INV-ORACLE-DAILY-001 / INV-ORACLE-MONO-001** — `ust1-common/src/oracle_policy.rs` + `ust1-oracle`
- **INV-ORACLE-TICK-001 / INV-ORACLE-ACCOUNT-001 / INV-ORACLE-GAS-001 / INV-ORACLE-HEALTHZ-001** — `oracle-service` tick timeout, fail-hard account parse, adaptive gas, process-up `/healthz` ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25); [`skills/audit-hardening-bundle`](skills/audit-hardening-bundle/SKILL.md))
- **INV-LIMIT-001** — `ust1-window/src/state.rs`, enforced in `contract.rs`
- **INV-WITHDRAW-001 / INV-WITHDRAW-002** — `ust1-window/src/state.rs` + `treasury.rs` / `contract.rs` (InstantWithdrawCw20; burn-then-pull atomicity) ([#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20))
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

**Deployment** (e.g. [Render](https://render.com)): the oracle service exposes a **liveness-only** HTTP probe — `GET /healthz` returns 200 when the process is up (bind via `HEALTHZ_BIND`, default `0.0.0.0:8080`; set `off` to disable). This does **not** imply on-chain rate freshness or a recent successful `UpdateRate`; pair it with log alerts (`ORACLE_MAX_SILENCE_SECS`) and off-platform paging. Tick-level timeouts and gas pricing knobs (`TICK_TIMEOUT_SECS`, `TERRA_GAS_PRICE`, `BSC_RPC_TIMEOUT_SECS`) are documented in [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md). Agent notes: [`skills/audit-hardening-bundle`](skills/audit-hardening-bundle/SKILL.md) ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25)).

Relevant environment variables: `ORACLE_MAX_SILENCE_SECS`, `POLL_INTERVAL_SECS`, `HEALTHZ_BIND`, plus the oracle env vars listed under Local development above and in `docs/DEPLOYMENT.md`.

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
