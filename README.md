# ust1-window

Monorepo for Terra Classic **UST1** swap tooling against bridged Venus **vFDUSD** (cw20-mintable), with a **rate oracle** (BSC Venus exchange rate) and a **swap window** contract.

## Packages

| Path | Role |
|------|------|
| `smartcontracts-terraclassic/packages/ust1-common` | Fixed-point math, oracle policy (`INV-*` in source), shared with contracts + service |
| `smartcontracts-terraclassic/contracts/ust1-oracle` | On-chain rate `R`, 4h min interval, UTC daily +2% cap, monotonic |
| `smartcontracts-terraclassic/contracts/ust1-window` | cw20 receive: vFDUSD→mint UST1, UST1→burn + vFDUSD; governance-set fee on UST1 leg (`fee_bps`, default 0.5%); governance limits |
| `oracle-service` | Polls BSC `exchangeRateStored`, applies same policy as chain, broadcasts `UpdateRate` |
| `scripts/` | Python 3 deploy helpers (no business logic) |

## Invariants (index)

- **INV-MATH-001 / INV-SWAP-001 / INV-SWAP-002** — `ust1-common/src/math.rs`
- **INV-ORACLE-THROTTLE-001 / INV-ORACLE-DAILY-001 / INV-ORACLE-MONO-001** — `ust1-common/src/oracle_policy.rs` + `ust1-oracle`
- **INV-LIMIT-001** — `ust1-window/src/state.rs`, enforced in `contract.rs`

## Local development

```bash
make start          # LocalTerra (Docker)
make wait-healthy
make test-contracts
# After deploy (see scripts/):
# export BSC_RPC_URLS=..., VENUS_VTOKEN_ADDRESS=..., TERRA_LCD_URL=..., ORACLE_CONTRACT=...
# cargo run -p ust1-oracle-service
```

## Build optimized Wasm

```bash
make build-optimized   # Docker workspace-optimizer → artifacts/
```

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
