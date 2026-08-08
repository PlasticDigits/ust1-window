# ust1-window

Monorepo for Terra Classic **UST1** swap tooling against bridged Venus **vFDUSD** (cw20-mintable), with a **rate oracle** (BSC Venus exchange rate) and a **swap window** contract, plus a separate **native wrap** contract for treasury-backed **wLUNC** / **wUSTC** (**uluna** / **uusd**) with **no oracle** (1:1 atoms after `fee_bps`).

## Mainnet status (`columbus-5`)

Tokens + oracle/window live on mainnet ([GitLab #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)). Window migrated to InstantWithdrawCw20 code **11566**; UST1 `add_minter(window)` and treasury `SetCw20Spender` (+ `limit_24h`) done. Still pending: first `UpdateRate`, oracle service, live withdraw probe — full operator registry in [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md). Redeem path: [GitLab #20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20). Wire-format pin vs ustr-cmm: [#21](https://gitlab.com/PlasticDigits/ust1-window/-/issues/21).

### Contracts & tokens

| Asset | Address | Code ID | Notes |
|-------|---------|---------|-------|
| cw20-mintable | — | **10184** | Shared CW20 code |
| **vFDUSD** (bridged CW20) | `terra1mnl9azefrqpmu888ar2u6zrcwr80hxlt3avf4300r576cw5ar7esvxsvj3` | 10184 | Decimals **6**; minter = CL8Y Terra bridge |
| **UST1** | `terra1f0eqgy9w7e5e7up97vjudqwx38tesf8ylx75x2lv3nwm0clry0pqmgfy72` | 10184 | Decimals **6**; minters include window `terra1zxwp…` |
| **ust1-oracle** | `terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n` | **11549** | Operator `terra1hm3ph0jevtkuc9efj9q3ld3ktk3g6la3ruhqna`; initial rate `1e18` |
| **ust1-window** | `terra1zxwpzpzpleatqn39r00grau4yt29sld8pw78s7ktvjafnj5nsaxq0h3rh2` | **11566** | InstantWithdrawCw20; `fee_bps=100`; per-tx **1000** / rolling 24h **10000** UST1; migrated from **11550** |
| CMM treasury (ustr-cmm) | `terra16j5u6ey7a84g40sr3gd94nzg5w5fm45046k9s2347qhfpwm5fr6sem3lr2` | **11564** | **Contract**; window registered spender + `limit_24h=10000000000` ([#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20)) |
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
| `contracts/ust1-oracle` | On-chain rate `R`, 4h min interval, UTC daily +2% cap, monotonic; governance pause surfaces on `State.paused` (circuit breaker, [#22](https://gitlab.com/PlasticDigits/ust1-window/-/issues/22)) |
| `contracts/ust1-window` | cw20 receive: vFDUSD→mint UST1 + forward vFDUSD to CMM treasury; UST1→burn + treasury `InstantWithdrawCw20` (registered spender; no CW20 allowance); governance-set fee on UST1 leg (`fee_bps`, default 1.0% with 50/50 chain-tax vs CMM accounting). Skill: [`skills/window-instant-withdraw-cw20`](skills/window-instant-withdraw-cw20/SKILL.md) |
| `contracts/cmm-native-wrap` | Native `Wrap` + cw20 `Receive` unwrap: **uluna**↔wLUNC, **uusd**↔wUSTC only; governance `fee_bps` (default **1%** recommended per GitLab #17) with **50/50** chain-tax vs CMM attribution on events / `EffectiveWrap`; per-denom limits; **no** `ust1-oracle` (GitLab #16) |
| `oracle-service` | Polls BSC `exchangeRateStored`, applies same policy as chain, broadcasts `UpdateRate`, confirms DeliverTx + `State` before liveness (poll/silence defaults: [`skills/oracle-ops-poll-silence`](skills/oracle-ops-poll-silence/SKILL.md); confirm semantics: [`skills/oracle-liveness-confirm`](skills/oracle-liveness-confirm/SKILL.md) ([#23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23))) |
| `scripts/` | Python 3 deploy helpers (no business logic) |

## Invariants (index)

- **INV-MATH-001 / INV-SWAP-001 / INV-SWAP-002** — `ust1-common/src/math.rs` (reverse path: `inv_swap_002_*` vector tests lock the fee-then-rate floor semantics)
- **INV-SWAP-003 / INV-SWAP-004** — `ust1-window/src/contract.rs` (deposit/withdraw revert on zero output; math dust floors in `ust1-common` before contract guard) ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25))
- **INV-DECIMALS-001** — `ust1-window/src/contract.rs` (`validate_token_decimals`: vFDUSD decimals ≥ UST1 decimals at instantiate/migrate) ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25))
- **INV-MINTER-001** — `cw20-mintable` fork: `UpdateMinter` clears old primary from `MINTERS` ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25)/[#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28); [cw20-mintable#1](https://github.com/PlasticDigits/cw20-mintable/pull/1); in-repo: `ust1-integration-tests` `cw20_minter_integration`)
- **INV-MATH-002** — `ust1-common/src/fee_split.rs` + `ust1-window` / `cmm-native-wrap` event attributes and `Effective*` queries (GitLab #17)
- **INV-ORACLE-THROTTLE-001 / INV-ORACLE-DAILY-001 / INV-ORACLE-MONO-001** — `ust1-common/src/oracle_policy.rs` + `ust1-oracle`
- **INV-ORACLE-OPS-POLL-001 / INV-ORACLE-OPS-SILENCE-001** — oracle-service poll ≪ / silence ≤ window `DEFAULT_MAX_ORACLE_AGE_SECS` ([#24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24); skill [`skills/oracle-ops-poll-silence`](skills/oracle-ops-poll-silence/SKILL.md))
- **INV-ORACLE-PAUSE-001** — oracle `State.paused` + window `ensure_oracle_usable` fail-closed on deposit/withdraw ([#22](https://gitlab.com/PlasticDigits/ust1-window/-/issues/22); skill [`skills/oracle-circuit-breaker`](skills/oracle-circuit-breaker/SKILL.md))
- **INV-ORACLE-LIVENESS-001** — oracle-service confirms DeliverTx + matching oracle `State` before silence/liveness success (not CheckTx alone); `oracle-service/src/{confirm,terra_tx,liveness,main}.rs`, skill [`skills/oracle-liveness-confirm`](skills/oracle-liveness-confirm/SKILL.md) ([#23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23), audit C-3)
- **INV-ORACLE-TICK-001 / INV-ORACLE-ACCOUNT-001 / INV-ORACLE-GAS-001 / INV-ORACLE-HEALTHZ-001** — `oracle-service` tick timeout, fail-hard account parse, adaptive gas, process-up `/healthz` ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25); [`skills/audit-hardening-bundle`](skills/audit-hardening-bundle/SKILL.md))
- **INV-LIMIT-001** — `ust1-window/src/state.rs`, enforced in `contract.rs`
- **INV-WITHDRAW-001 / INV-WITHDRAW-002** — `ust1-window/src/state.rs` + `treasury.rs` / `contract.rs` (InstantWithdrawCw20; burn-then-pull atomicity) ([#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20))
- **INV-SCHEMA-001** — `ust1-window/src/treasury.rs` + golden `testdata/instant_withdraw_cw20_golden.json` + integration `treasury_schema` / `real_treasury_integration` (pinned ustr-cmm wire format) ([#21](https://gitlab.com/PlasticDigits/ust1-window/-/issues/21))
- **INV-LIMIT-NATIVE-001** — `cmm-native-wrap/src/state.rs` / `limits.rs`, enforced in `wrap.rs` and `unwrap.rs`

Operator checklist, BSC + Terra address registry, and mainnet/testnet deployment notes: [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md) ([GitLab #15](https://gitlab.com/PlasticDigits/ust1-window/-/issues/15)).

## Local development

```bash
make start          # LocalTerra (Docker)
make wait-healthy
make test-contracts
make test-localterra-smoke   # TEST-16 / #28: skip-clean if LCD down; see docs/DEPLOYMENT.md
# After deploy (see scripts/):
# export BSC_RPC_URLS=..., VENUS_VTOKEN_ADDRESS=..., TERRA_LCD_URL=..., ORACLE_CONTRACT=...
# cargo run -p ust1-oracle-service
```

## Oracle service: observability and deployment

The `ust1-oracle-service` binary is intentionally **lightweight**: it uses **structured `tracing` logs only** (no Prometheus or in-process metrics server). Every `check_rate_update` and broadcast/confirm outcome is logged at `info` (policy / confirmed update) or `warn` (CheckTx / DeliverTx / state confirmation failure). **`BROADCAST_MODE_SYNC` CheckTx success alone does not count as success** (**INV-ORACLE-LIVENESS-001**): the service polls for DeliverTx inclusion and verifies oracle `State` (`last_update_sec` advanced, `rate` matches) before recording liveness. If there has been **no confirmed on-chain oracle update** for longer than **`ORACLE_MAX_SILENCE_SECS`** (default **21600**, i.e. 6 hours — aligned with window `DEFAULT_MAX_ORACLE_AGE_SECS`), the process emits a **high-visibility `error!` liveness alert** on each poll tick. Default **`POLL_INTERVAL_SECS`** is **3600** (1h) so a missed tick does not consume the entire 6h staleness budget; on-chain throttle / policy still skip most broadcasts (**INV-ORACLE-OPS-POLL-001** / **INV-ORACLE-OPS-SILENCE-001**, [glab #24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24)).

**Production-style deployment** (Terra Classic wasm + BSC oracle path + operator checklist + address registry) is documented in [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md). After exporting env vars, run `make verify-oracle-env` to confirm required keys are present before starting the service. Agent skills: [`skills/oracle-ops-poll-silence`](skills/oracle-ops-poll-silence/SKILL.md), [`skills/oracle-liveness-confirm`](skills/oracle-liveness-confirm/SKILL.md) ([#23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23)).

**Deployment** (e.g. [Render](https://render.com)): the oracle service exposes a **liveness-only** HTTP probe — `GET /healthz` returns 200 when the process is up (bind via `HEALTHZ_BIND`, default `0.0.0.0:8080`; set `off` to disable). This does **not** imply on-chain rate freshness or a recent successful `UpdateRate`; pair it with log alerts (`ORACLE_MAX_SILENCE_SECS`) and off-platform paging. Tick-level timeouts and gas pricing knobs (`TICK_TIMEOUT_SECS`, `TERRA_GAS_PRICE`, `BSC_RPC_TIMEOUT_SECS`) are documented in [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md). Agent notes: [`skills/audit-hardening-bundle`](skills/audit-hardening-bundle/SKILL.md) ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25)), [`skills/oracle-ops-poll-silence`](skills/oracle-ops-poll-silence/SKILL.md) ([#24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24)).

Relevant environment variables: `ORACLE_MAX_SILENCE_SECS`, `ORACLE_TX_CONFIRM_TIMEOUT_SECS`, `ORACLE_TX_CONFIRM_POLL_INTERVAL_MS`, `POLL_INTERVAL_SECS`, `HEALTHZ_BIND`, plus the oracle env vars listed under Local development above and in `docs/DEPLOYMENT.md`.

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
