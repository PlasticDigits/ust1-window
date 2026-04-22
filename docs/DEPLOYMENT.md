# Deployment: BSC + Terra Classic (UST1 stack)

This document is the **operator-facing deployment path** for production-like environments: Terra Classic wasm contracts, BSC read-only integration for the oracle service, and runtime configuration. It satisfies the deliverables tracked in [GitLab issue #15](https://gitlab.com/PlasticDigits/ust1-window/-/issues/15).

**Related code (no deployment business logic in scripts beyond orchestration hints):**

| Area | Location |
|------|----------|
| LocalTerra helper | [`scripts/deploy_local.py`](../scripts/deploy_local.py) |
| Optimized wasm build | [`make build-optimized`](../Makefile), [`scripts/optimize.sh`](../scripts/optimize.sh) |
| Oracle service config / canonical BSC token | [`oracle-service/src/config.rs`](../oracle-service/src/config.rs) |
| On-chain oracle policy (must match service) | [`smartcontracts-terraclassic/packages/ust1-common/src/oracle_policy.rs`](../smartcontracts-terraclassic/packages/ust1-common/src/oracle_policy.rs), [`smartcontracts-terraclassic/contracts/ust1-oracle`](../smartcontracts-terraclassic/contracts/ust1-oracle) |
| Swap math & limits | [`smartcontracts-terraclassic/packages/ust1-common/src/math.rs`](../smartcontracts-terraclassic/packages/ust1-common/src/math.rs) (**INV-MATH-001**, **INV-SWAP-001**, **INV-SWAP-002**), [`ust1-window` state](../smartcontracts-terraclassic/contracts/ust1-window/src/state.rs) (**INV-LIMIT-001**) |
| Native wrap limits | [`cmm-native-wrap`](../smartcontracts-terraclassic/contracts/cmm-native-wrap) (**INV-LIMIT-NATIVE-001**; no oracle — [issue #16](https://gitlab.com/PlasticDigits/ust1-window/-/issues/16)) |

## Invariants and off-chain parity

The oracle service applies the **same** rate policy as the chain before broadcasting `UpdateRate`:

- **INV-ORACLE-THROTTLE-001** — minimum interval between on-chain updates (`MIN_ORACLE_UPDATE_INTERVAL_SECS`, 4h).
- **INV-ORACLE-DAILY-001** — UTC calendar-day increase cap (`MAX_DAILY_INCREASE_BPS`, 2%).
- **INV-ORACLE-MONO-001** — monotonic non-decreasing on-chain rate.

See source references above; integration tests in `ust1-integration-tests` further lock behavior.

---

## Operator checklist

Use this as a literal runbook; record outputs in the [address registry](#address-registry-template) below.

| Phase | Step | Done |
|-------|------|------|
| **Pre-deploy** | Toolchain: Rust stable, `cargo`, Python 3 (for `scripts/`), Terra Classic CLI (`terrad`) for your target network, Docker (for `make build-optimized`). | ☐ |
| **Pre-deploy** | Decide environment: **mainnet** vs **testnet**; note `TERRA_CHAIN_ID` (e.g. `columbus-5` for Terra Classic mainnet). | ☐ |
| **Pre-deploy** | Obtain HTTPS **LCD** (REST) base URL and RPC if needed; confirm connectivity. | ☐ |
| **Pre-deploy** | Obtain **two or more** HTTPS **BSC JSON-RPC** endpoints (`BSC_RPC_URLS` — required for multi-provider consensus in `ust1-oracle-service`). | ☐ |
| **Pre-deploy** | Build wasm: `make build-optimized` (or use CI-produced artifacts from the same git revision). | ☐ |
| **Pre-deploy** | Keys: deployer wallet(s), governance addresses, **oracle operator** hot wallet (must match `oracle_operator` on `ust1-oracle`); secure `TERRA_MNEMONIC` for the operator. | ☐ |
| **Deploy** | Ensure **cw20** addresses exist or deploy: **vFDUSD** (bridged), **UST1** (mintable per protocol). Record both. | ☐ |
| **Deploy** | `wasm store` / `instantiate` **ust1-oracle** with `governance`, `oracle_operator`, `initial_rate` (typically `10^18` fixed-point; see `RATE_SCALE` in `ust1-common`). Record code id + contract address. | ☐ |
| **Deploy** | `wasm store` / `instantiate` **ust1-window** with `oracle`, `vfdusd_token`, `ust1_token`, limits, `fee_bps`, optional `cmm_treasury` (defaults to mainnet constant in `ust1-cmm` if omitted). Record addresses. | ☐ |
| **Deploy** | (Optional) **cmm-native-wrap** for wLUNC/wUSTC: instantiate with governance, fee, and **both** pairs (`uluna` / `uusd`). | ☐ |
| **Deploy** | Post-instantiate: governance / multisig steps (e.g. `ProposeGovernance` / `AcceptGovernance`) per your playbook. | ☐ |
| **Post-deploy** | Terra: query `ust1-oracle` `State` and `Config`; query `ust1-window` `Config` and `EffectiveSwap`. | ☐ |
| **Post-deploy** | Treasury: CMM must `IncreaseAllowance` on **vFDUSD** for the window contract (withdraw path). | ☐ |
| **Post-deploy** | Oracle service: set env (see [Oracle service environment](#oracle-service-environment)); run `scripts/verify_oracle_operator_env.sh`; start `cargo run -p ust1-oracle-service` (or release binary). | ☐ |
| **Post-deploy** | Observability: confirm structured logs; configure **external** HTTP health / uptime on the service host (see [README](../README.md) — in-process liveness is not a substitute for paging). | ☐ |
| **Post-deploy** | Tune `ORACLE_MAX_SILENCE_SECS` (default 8h) and `POLL_INTERVAL_SECS` for your ops model. | ☐ |

---

## Deterministic vs recorded addresses

### Terra Classic

Contract addresses are **not** automatically predictable across chains unless you deliberately use a fixed deployer account and a documented nonce/sequence schedule, or a chain-supported predictable instantiation scheme (e.g. `instantiate2` where available and chosen by the project). **Default recommendation:** treat addresses as **recorded-at-deploy**: store code id, instantiate tx hashes, and contract addresses in the registry for each environment.

### BSC

The oracle reads **Venus `exchangeRateStored`** from the configured vToken. On **BSC mainnet** with `BSC_ALLOWED_CHAIN_IDS=56` (default), `ust1-oracle-service` **requires** the canonical Venus vFDUSD vToken address (see `CANONICAL_VENUS_VFDUSD_BSC_MAINNET` in `oracle-service/src/config.rs`). On testnets / additional allowed ids, you may supply a different vToken address (e.g. mock). No project contract is deployed on BSC by this repo; determinism is “**verified bytecode / known integration address**,” not CREATE2 for our contracts.

---

## Address registry (template)

Copy this block into your internal runbook and fill per environment.

### BSC

| Item | Mainnet | Testnet / dev | Notes |
|------|---------|---------------|--------|
| Chain ID | `56` | e.g. `97` (Chapel) | Must match `eth_chainId` from RPC; allowlist via `BSC_ALLOWED_CHAIN_IDS`. |
| Venus vFDUSD vToken | `0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba` | _record mock / test token_ | Mainnet value enforced when allowlist is only `56` ([`config.rs`](../oracle-service/src/config.rs)). |
| Our deployed EVM contracts | — | — | None required for oracle read path. |

### Terra Classic

| Contract / role | Mainnet (`columbus-5`) | Testnet | Code ID | Contract address | Notes |
|-----------------|------------------------|---------|---------|------------------|--------|
| CMM treasury (vFDUSD custody) | `terra16j5u6ey7a84g40sr3gd94nzg5w5fm45046k9s2347qhfpwm5fr6sem3lr2` ([`ust1-cmm`](../smartcontracts-terraclassic/packages/ust1-cmm/src/lib.rs)) | | | | Used as default `cmm_treasury` in `ust1-window` if not overridden. |
| vFDUSD cw20 | | | | | Bridged asset. |
| UST1 cw20 | | | | | Mintable per protocol. |
| `ust1-oracle` | | | | | `oracle_operator` must sign `UpdateRate`. |
| `ust1-window` | | | | | References oracle + tokens. |
| `cmm-native-wrap` | | | | | Optional; **no** oracle. |

---

## Terra Classic: suggested `terrad` shape

Exact flags depend on your CLI version and keyring. **Ordering:** deploy **oracle** first, then **window** (window holds oracle address). CW20s must exist before window instantiation.

**Store example** (adjust `--from`, fees, `--chain-id`, `--node`):

```bash
terrad tx wasm store artifacts/ust1_oracle.wasm --from deployer --chain-id columbus-5 --gas auto --gas-adjustment 1.3 --broadcast-mode sync -y
terrad tx wasm store artifacts/ust1_window.wasm --from deployer --chain-id columbus-5 --gas auto --gas-adjustment 1.3 --broadcast-mode sync -y
```

**Instantiate oracle** (JSON illustrative — replace addresses and code id):

```json
{
  "governance": "terra1…",
  "oracle_operator": "terra1…",
  "initial_rate": "1000000000000000000"
}
```

**Instantiate window** (illustrative):

```json
{
  "governance": "terra1…",
  "oracle": "terra1…",
  "vfdusd_token": "terra1…",
  "cmm_treasury": null,
  "ust1_token": "terra1…",
  "fee_bps": 100,
  "per_tx_ust1_limit": "…",
  "rolling_24h_ust1_limit": "…",
  "max_oracle_age_sec": null
}
```

Verification queries:

```bash
terrad query wasm contract-store <ORACLE_ADDR> '{"state":{}}' --chain-id columbus-5
terrad query wasm contract-store <WINDOW_ADDR> '{"effective_swap":{}}' --chain-id columbus-5
```

Schema source: [`ust1-oracle` msg](../smartcontracts-terraclassic/contracts/ust1-oracle/src/msg.rs), [`ust1-window` msg](../smartcontracts-terraclassic/contracts/ust1-window/src/msg.rs).

---

## Oracle service environment

Required variables are loaded in [`Config::from_env`](../oracle-service/src/config.rs). Summary:

| Variable | Purpose |
|----------|---------|
| `BSC_RPC_URLS` | Comma-separated HTTPS RPC URLs (≥ 2). |
| `BSC_ALLOWED_CHAIN_IDS` | Default `56`; use e.g. `56,97,31337` for test + Anvil. |
| `BSC_CONFIRMATION_BLOCKS` | Reorg protection depth (default 15). |
| `VENUS_VTOKEN_ADDRESS` | Venus vToken holding `exchangeRateStored`. |
| `TERRA_LCD_URL` | HTTPS LCD base URL. |
| `TERRA_CHAIN_ID` | e.g. `columbus-5`. |
| `TERRA_MNEMONIC` | Oracle operator seed (**secret**; matches on-chain `oracle_operator`). |
| `ORACLE_CONTRACT` | `ust1-oracle` address. |
| `POLL_INTERVAL_SECS` | Default 21600 s. |
| `ORACLE_MAX_SILENCE_SECS` | Liveness log threshold (default 28800 s). |

**HTTPS:** Production URLs must use `https://`. For local development only, `DEV_ALLOW_HTTP=1` permits `http://` on localhost / loopback (see `config.rs`).

**Preflight:** run `scripts/verify_oracle_operator_env.sh` after exporting variables.

---

## Keys and funding

### Keys

- **Deployer:** submits `store` / `instantiate` (fund with enough L1 for fees).
- **Governance:** contract admin transitions (`ProposeGovernance` / `AcceptGovernance`, parameter updates).
- **Oracle operator:** Terra address registered in `ust1-oracle` as `oracle_operator`; `TERRA_MNEMONIC` in the service must derive to this address (default path `m/44'/330'/0'/0/0` in [`terra_tx.rs`](../oracle-service/src/terra_tx.rs)).

### Gas / funding

- **Terra Classic:** Maintain a balance on the **oracle operator** account sufficient for periodic `UpdateRate` txs (default gas limit order 500k; gas price set in code — monitor actual fee token spend). Top up when broadcasts fail due to insufficient funds or when LCD returns account errors.
- **BSC:** The oracle service only performs **JSON-RPC reads**; it does **not** submit BSC transactions. Budget **BNB** only if your infra provider bills per request differently; typically operational cost is RPC quota, not on-chain gas.

---

## Smoke and health

- **On-chain:** oracle `State` reflects successful updates; window `EffectiveSwap` returns coherent `oracle` nested state and limits.
- **Service:** logs show periodic ticks; no sustained `LIVENESS_ORACLE_NO_BROADCAST` errors unless policy or upstream RPC legitimately blocks updates.
- **External:** README expects a platform health check (e.g. Render-style) in addition to log-based alerts.

---

## Changelog

| Date | Change |
|------|--------|
| 2026-04-22 | Initial in-repo deployment doc, checklist, and address registry ([issue #15](https://gitlab.com/PlasticDigits/ust1-window/-/issues/15)). |
