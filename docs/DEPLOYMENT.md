# Deployment: BSC + Terra Classic (UST1 stack)

This document is the **operator-facing deployment path** for production: Terra Classic wasm (UST1 swap window + oracle + tokens), CL8Y bridge registration for **vFDUSD**, BSC read-only integration for the oracle service, and hosting the oracle **without** a `render.yaml` file (Render dashboard only).

**Related code:**

| Area | Location |
|------|----------|
| LocalTerra helper | [`scripts/deploy_local.py`](../scripts/deploy_local.py) |
| Optimized wasm (UST1 contracts) | [`make build-optimized`](../Makefile), [`scripts/optimize.sh`](../scripts/optimize.sh) |
| Oracle service config / canonical BSC token | [`oracle-service/src/config.rs`](../oracle-service/src/config.rs) |
| On-chain oracle policy (must match service) | [`ust1-common` oracle_policy](../smartcontracts-terraclassic/packages/ust1-common/src/oracle_policy.rs), [`ust1-oracle`](../smartcontracts-terraclassic/contracts/ust1-oracle) |
| Swap math & limits | [`ust1-common` math](../smartcontracts-terraclassic/packages/ust1-common/src/math.rs), [`ust1-window`](../smartcontracts-terraclassic/contracts/ust1-window) |
| Native wrap (optional) | [`cmm-native-wrap`](../smartcontracts-terraclassic/contracts/cmm-native-wrap) |

**External runbook (CL8Y bridge):** Cross-chain registration examples use the same `terrad` fee pattern as `cl8y-bridge-monorepo` **`docs/deployment-guide.md`** (§5–6). Keep that document open in your CL8Y checkout when wiring **vFDUSD** on BSC ↔ Terra.

---

## Invariants and off-chain parity

The oracle service applies the **same** rate policy as the chain before broadcasting `UpdateRate`:

- **INV-ORACLE-THROTTLE-001** — minimum interval between on-chain updates (4h).
- **INV-ORACLE-DAILY-001** — UTC calendar-day increase cap (2%).
- **INV-ORACLE-MONO-001** — monotonic non-decreasing on-chain rate.

---

## Terra Classic CLI (`terrad`): gas, fees, and common mistakes

Terra Classic is easy to get wrong if you copy “generic Cosmos” snippets. Prefer a **fixed pattern** for every `terrad tx`:

1. **Always pin the node:** `--node https://…:443` (or your provider’s Tendermint RPC). Queries and broadcasts must hit a node you trust.
2. **Always set chain id:** `--chain-id columbus-5` for mainnet.
3. **Use automatic gas *with* headroom:** `--gas auto --gas-adjustment 1.5` (instantiate) or **`1.8`–`2.5`** for `wasm store` of large binaries when simulation underestimates.
4. **Always attach an explicit fee budget in `uluna`:** `--fees <N>uluna`.  
   Naive `--gas auto` without a sufficient fee cap often yields `insufficient fees` or flaky simulation. The CL8Y deployment guide uses **`--fees 10000000uluna`** for many executes; **`wasm store`** commonly needs **more** (try `50000000uluna`–`150000000uluna` and increase if the node still rejects).
5. **Keyring:** `--keyring-backend os` (or `file`) consistent with how you imported keys.
6. **Broadcast mode:** `--broadcast-mode sync` (or `block`) so you can inspect the tx result immediately.

**Wasm `store` example:**

```bash
export FEES_ULUNA=80000000   # fee *amount* in uluna, not gas units; raise if needed

terrad tx wasm store artifacts/ust1_oracle.wasm \
  --from "$TERRA_KEY_NAME" \
  --chain-id columbus-5 \
  --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 2.0 \
  --fees "${FEES_ULUNA}uluna" \
  --keyring-backend os \
  --broadcast-mode sync -y
```

After each tx, record **`code_id`** (for store) or **`_contract_address`** (for instantiate) from events.

**Queries** use the same `--node` (RPC URLs work for `query` in current Terra Classic tooling):

```bash
terrad query wasm contract-state smart "$ORACLE_ADDR" '{"state":{}}' \
  --chain-id columbus-5 --node "$TERRA_RPC"
```

---

## Roles and shell exports

Define once and reuse:

| Role | Responsibility |
|------|------------------|
| **Deployer key** | `wasm store`, token/oracle/window `instantiate` |
| **Bridge admin key** | CL8Y bridge `add_token`, `set_token_destination`, `set_incoming_token_mapping` |
| **EVM admin key** | BSC `TokenRegistry` owner calls (`registerToken`, destinations, incoming mappings) |
| **Governance (multisig)** | `ust1-*` governance messages; optional contract admin |
| **Oracle operator** | Sole caller of `ust1-oracle` `UpdateRate`; seed in `TERRA_MNEMONIC` for the service |
| **CMM treasury** | Holds bridged **vFDUSD**; signs `IncreaseAllowance` so `ust1-window` can pull on withdraw |

Suggested exports:

```bash
export TERRA_RPC="https://terra-classic-rpc.publicnode.com:443"   # or your provider
export TERRA_LCD="https://terra-classic-lcd.publicnode.com:443" # oracle *service* uses LCD
export TERRA_KEY_NAME="ust1_deployer"
export TERRA_CHAIN_ID="columbus-5"

# After CL8Y deploy (record real addresses):
export TERRA_BRIDGE_ADDRESS="terra1…"
export BSC_TOKEN_REGISTRY="0x…"
export BSC_VFDUSD_ERC20="0x…"   # BSC side FDUSD / bridged representation you register

# Bridge-internal chain ids (from your CL8Y deployment; typical mainnet values — confirm against your registry):
export BSC_CHAIN_ID_BYTES4="0x00000038"   # 56 as 4-byte bridge id — VERIFY on your deployment
export TERRA_CHAIN_ID_BYTES4="0x00000001" # VERIFY
# Base64 form for Terra execute JSON (example for BSC — must match your bridge’s registered id):
export BSC_CHAIN_B64="AAAAOA=="
```

Derive **Terra CW20 → bytes32 / base64** for mappings using the Python snippets in the CL8Y guide (§6.0 / §6.3) so EVM `setTokenDestinationWithDecimals` and Terra `set_incoming_token_mapping` agree.

---

## Master checklist

Use this end-to-end; record every address and code id in the [registry](#address-registry-template).

### Prerequisites

- [ ] Rust + Docker (for `cosmwasm/workspace-optimizer`), `terrad`, funded wallets, HTTPS RPC/LCD endpoints.
- [ ] **Two or more** HTTPS BSC JSON-RPC URLs for `BSC_RPC_URLS` (oracle service consensus).
- [ ] CL8Y bridge deployed and **admin keys** available on Terra and BSC (see CL8Y `deployment-guide.md`).
- [ ] Decide **decimals** for vFDUSD on Terra and BSC; they must match what you register on `TokenRegistry` / Terra bridge (mis-matched decimals brick amounts).

### Tokens (cw20-mintable)

- [ ] Obtain **`cw20_mintable.wasm`** (build from [`PlasticDigits/cw20-mintable`](https://github.com/PlasticDigits/cw20-mintable) with the same optimizer toolchain you trust for production, or your release process).
- [ ] `wasm store` cw20-mintable → record `CW20_MINTABLE_CODE_ID`.
- [ ] Instantiate **vFDUSD** on Terra with `mint.minter` = **`TERRA_BRIDGE_ADDRESS`** (bridge mints on incoming EVM→Terra). Use `--admin` per your upgrade policy.
- [ ] Instantiate **UST1** on Terra with `mint.minter` = **governance** (or a dedicated admin) **not** the bridge. Decimals typically **6** (match your economics); symbol **`UST1`** satisfies cw20-mintable validation.
- [ ] Query `{"token_info":{}}` on both contracts; record **`TERRA_VFDUSD`**, **`TERRA_UST1`**.

### CL8Y bridge: list and connect vFDUSD (BSC ↔ Terra)

Complete cross-chain registration so the token is routable and mint/burn lines up with the bridge. **Follow CL8Y §6** with your real addresses; below is the *shape* only.

- [ ] **BSC `TokenRegistry`:** `registerToken(BSC_VFDUSD_ERC20, handler_code)` per CL8Y.
- [ ] **Outgoing BSC → Terra:** `setTokenDestinationWithDecimals(BSC_VFDUSD, TERRA_CHAIN_ID_BYTES4, terra_vfdusd_as_bytes32, terra_decimals)`.
- [ ] **Incoming Terra → BSC:** `setIncomingTokenMapping(TERRA_CHAIN_B64, BSC_VFDUSD_ERC20, src_decimals_from_terra)` (exact args per ABI — mirror CL8Y test token section).
- [ ] **Terra bridge `add_token`:** `vFDUSD` cw20 with `token_type: "mint_burn"` and `terra_decimals` matching the cw20.
- [ ] **Terra `set_token_destination`:** Terra vFDUSD → BSC erc20 (dest token as 32-byte hex, dest decimals = BSC side).
- [ ] **Terra `set_incoming_token_mapping`:** BSC → Terra vFDUSD (src_token = base64-encoded 32-byte form of Terra cw20 address, matching EVM-side bytes32).
- [ ] **Smoke:** small test lock/mint path on testnet first; on mainnet, verify with minimal amounts and internal tooling before public announcement.
- [ ] **Frontend / “listed on CL8Y”:** Bridge UI token lists are driven by **on-chain registry + off-chain config** for your environment. After mappings exist, confirm the asset appears in the CL8Y app you expose to users (refresh operator env / token matrix / caches per your CL8Y ops runbook). If the chain is correct but the UI is empty, treat it as an **operator or frontend config** issue, not a UST1 contract issue.

### UST1 stack (this repo)

- [ ] `make build-optimized` at the git revision you intend to ship; record git SHA.
- [ ] `wasm store` `artifacts/ust1_oracle.wasm`, `artifacts/ust1_window.wasm` → code IDs.
- [ ] Instantiate **`ust1-oracle`** with `governance`, `oracle_operator`, `initial_rate` (`"1000000000000000000"` for 1:1 at `RATE_SCALE`; see `ust1-common`).
- [ ] Instantiate **`ust1-window`** with `oracle`, `vfdusd_token`, `ust1_token`, limits, `fee_bps`, optional `cmm_treasury` (omit to use [`CMM_TREASURY_MAINNET`](../smartcontracts-terraclassic/packages/ust1-cmm/src/lib.rs)).
- [ ] **UST1 minters:** from governance, execute cw20-mintable **`add_minter`** for **`ust1-window`** (see integration tests in `ust1-integration-tests`).
- [ ] **Treasury allowance:** from CMM treasury, **`increase_allowance`** on **vFDUSD** for **window** (spender = window contract; amount per policy).
- [ ] **Governance handoff:** if required, run `ProposeGovernance` / `AcceptGovernance` on oracle and window.
- [ ] **First oracle commit:** `oracle_operator` sends `UpdateRate` consistent with policy (service will continue updates).

### Optional: `cmm-native-wrap`

- [ ] If wrapping **uluna/uusd**, deploy `cmm-native-wrap` per [`cmm-native-wrap`](../smartcontracts-terraclassic/contracts/cmm-native-wrap) and your governance playbook (no oracle).

### Oracle service (Render, no YAML)

- [ ] Create Render resources via **dashboard** (see [Render dashboard setup](#render-dashboard-setup-no-renderyaml)).
- [ ] Set **all** env vars; run `scripts/verify_oracle_operator_env.sh` locally with the same values before applying.
- [ ] Confirm logs show periodic polls and successful broadcasts after deploy.
- [ ] External alerting on process restarts / log errors (Render notifications + optional log drain).

---

## Phase 1 — Build UST1 wasm artifacts

From repo root:

```bash
make build-optimized
ls -l artifacts/ust1_oracle.wasm artifacts/ust1_window.wasm
```

Ship **only** artifacts built from a **tagged** revision you have tested.

---

## Phase 2 — Store and instantiate vFDUSD and UST1 (cw20-mintable)

### Store cw20-mintable once

```bash
terrad tx wasm store cw20_mintable.wasm \
  --from "$TERRA_KEY_NAME" \
  --chain-id columbus-5 \
  --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 2.0 \
  --fees 80000000uluna \
  --keyring-backend os \
  --broadcast-mode sync -y
# export CW20_MINTABLE_CODE_ID=<from tx result>
```

### Instantiate vFDUSD (bridge is minter)

Governance/upgrade **`--admin`** should be your multisig or policy account.

```bash
terrad tx wasm instantiate "$CW20_MINTABLE_CODE_ID" \
  '{"name":"Venus FDUSD (bridged)","symbol":"vFDUSD","decimals":6,"initial_balances":[],"mint":{"minter":"'"$TERRA_BRIDGE_ADDRESS"'","cap":null},"marketing":null}' \
  --label "vfdusd-cw20" \
  --admin "$TERRA_ADMIN" \
  --from "$TERRA_KEY_NAME" \
  --chain-id columbus-5 \
  --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 \
  --fees 10000000uluna \
  --keyring-backend os \
  --broadcast-mode sync -y
```

**Decimals:** Use the decimals that match your **BSC** registration and bridge math (6 and 18 are both common — they must agree across `TokenRegistry`, Terra `add_token`, and the cw20 metadata).

### Instantiate UST1 (governance minter first)

```bash
terrad tx wasm instantiate "$CW20_MINTABLE_CODE_ID" \
  '{"name":"UST1","symbol":"UST1","decimals":6,"initial_balances":[],"mint":{"minter":"'"$GOVERNANCE_ADDR"'","cap":null},"marketing":null}' \
  --label "ust1-cw20" \
  --admin "$TERRA_ADMIN" \
  --from "$TERRA_KEY_NAME" \
  --chain-id columbus-5 \
  --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 \
  --fees 10000000uluna \
  --keyring-backend os \
  --broadcast-mode sync -y
```

Record `TERRA_VFDUSD` and `TERRA_UST1` contract addresses.

---

## Phase 3 — CL8Y bridge registration for vFDUSD

Execute the **symmetric** registration:

- On **BSC**: register token + outgoing destination (to Terra cw20 as bytes32) + incoming mapping (from Terra).
- On **Terra**: `add_token` + `set_token_destination` + `set_incoming_token_mapping`.

Copy the exact `cast send` / `terrad tx wasm execute` blocks from **CL8Y `docs/deployment-guide.md` §6.3** (EVM token registry) and the **Terra Side — Add Tokens / destinations / incoming** subsections, substituting:

- `TERRA_TESTA_*` → your **`TERRA_VFDUSD`** equivalents.
- EVM test token addresses → **`BSC_VFDUSD_ERC20`**.
- Decimals in each call → your agreed **BSC / Terra** decimals.

**Do not guess** `BSC_CHAIN_B64` / bytes4 ids: read the live `ChainRegistry` / bridge config for the deployment you integrate with.

---

## Phase 4 — Deploy `ust1-oracle` and `ust1-window`

### Store

```bash
terrad tx wasm store artifacts/ust1_oracle.wasm \
  --from "$TERRA_KEY_NAME" --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 2.0 --fees 80000000uluna \
  --keyring-backend os --broadcast-mode sync -y

terrad tx wasm store artifacts/ust1_window.wasm \
  --from "$TERRA_KEY_NAME" --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 2.0 --fees 80000000uluna \
  --keyring-backend os --broadcast-mode sync -y
```

### Instantiate oracle

```bash
terrad tx wasm instantiate "$UST1_ORACLE_CODE_ID" \
  '{"governance":"'"$GOVERNANCE_ADDR"'","oracle_operator":"'"$ORACLE_BOT_ADDR"'","initial_rate":"1000000000000000000"}' \
  --label "ust1-oracle" \
  --admin "$TERRA_ADMIN" \
  --from "$TERRA_KEY_NAME" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 10000000uluna \
  --keyring-backend os --broadcast-mode sync -y
```

### Instantiate window

`per_tx_ust1_limit` / `rolling_24h_ust1_limit` are **raw base units** (for 6 decimals, multiply whole UST1 by `1_000_000`). Approved mainnet defaults ([issue #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)): `fee_bps=100` (1%), **1,000** UST1 per tx (`"1000000000"`), **10,000** UST1 rolling 24h (`"10000000000"`). Same constants live in `ust1-common` (`DEFAULT_FEE_BPS`, `DEFAULT_PER_TX_UST1_LIMIT`, `DEFAULT_ROLLING_24H_UST1_LIMIT`). Limits are governance-updatable after deploy via `SetLimits` (no code change / remigrate required).

```bash
terrad tx wasm instantiate "$UST1_WINDOW_CODE_ID" \
  '{"governance":"'"$GOVERNANCE_ADDR"'","oracle":"'"$ORACLE_ADDR"'","vfdusd_token":"'"$TERRA_VFDUSD"'","cmm_treasury":null,"ust1_token":"'"$TERRA_UST1"'","fee_bps":100,"per_tx_ust1_limit":"1000000000","rolling_24h_ust1_limit":"10000000000","max_oracle_age_sec":null}' \
  --label "ust1-window" \
  --admin "$TERRA_ADMIN" \
  --from "$TERRA_KEY_NAME" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 15000000uluna \
  --keyring-backend os --broadcast-mode sync -y
```

Schemas: [`ust1-oracle` msg](../smartcontracts-terraclassic/contracts/ust1-oracle/src/msg.rs), [`ust1-window` msg](../smartcontracts-terraclassic/contracts/ust1-window/src/msg.rs).

---

## Phase 5 — Post-deploy wiring

### 1) Add `ust1-window` as UST1 minter

Governance (current minter) executes cw20-mintable:

```bash
terrad tx wasm execute "$TERRA_UST1" \
  '{"add_minter":{"minter":"'"$WINDOW_ADDR"'"}}' \
  --from "$GOVERNANCE_KEY" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 10000000uluna \
  --keyring-backend os --broadcast-mode sync -y
```

### 2) Treasury vFDUSD allowance for withdraw path

From **CMM treasury** (holder of vFDUSD):

```bash
terrad tx wasm execute "$TERRA_VFDUSD" \
  '{"increase_allowance":{"spender":"'"$WINDOW_ADDR"'","amount":"340282366920938463463374607431768211455","expires":{"never":{}}}}' \
  --from "$TREASURY_KEY" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 10000000uluna \
  --keyring-backend os --broadcast-mode sync -y
```

Use your policy for `amount` / `expires`. The amount above is **u128::max** as a JSON string (matches integration tests); prefer an explicit cap if your treasury policy requires it.

### 3) First oracle rate

Until `last_update_sec` is set by a successful `UpdateRate`, window swaps may reject stale oracle state. Bring rate on-chain from **`ORACLE_BOT_ADDR`** (or rely on the service once live).

---

## Render dashboard setup (no `render.yaml`)

`ust1-oracle-service` is a **long-running process** with structured logs and **no HTTP API**. On Render, prefer a **Background Worker** (not a public Web Service) unless you add a separate health HTTP sidecar.

### Create the service (UI)

1. **New → Background Worker** (recommended) or **Private Service** if your org uses them.
2. Connect this repository (GitLab/GitHub).
3. **Branch:** production branch or tag.
4. **Build command:**  
   `cargo build --release -p ust1-oracle-service`
5. **Start command:**  
   `./target/release/ust1-oracle-service`
6. **Instance type:** smallest that keeps steady CPU for periodic RPC polling; scale if you see RPC timeouts.
7. **Environment → Environment Variables:** paste the [table below](#oracle-service-environment) (production values, never commit secrets). Set `RUST_VERSION` to match your local test toolchain if Render’s default is too old (e.g. `1.85.0`).
8. **Deploy.** Watch **Logs** for `check_rate_update` / `sign_and_broadcast_execute` outcomes.

### Secrets

- Store `TERRA_MNEMONIC` in Render **Secret** fields only.
- Rotate operator keys via on-chain `SetOracleOperator` if compromised.

### Health / alerting

- Enable Render **notifications** for **crashes and deploy failures**.
- The binary emits **`error!`** if no successful broadcast exceeds `ORACLE_MAX_SILENCE_SECS` — forward logs to your SIEM or log drain and page on that pattern.
- Do **not** rely on Render’s HTTP health check for this binary unless you add an HTTP probe.

---

## Oracle service environment

Loaded in [`Config::from_env`](../oracle-service/src/config.rs):

| Variable | Purpose |
|----------|---------|
| `BSC_RPC_URLS` | Comma-separated HTTPS RPC URLs (**≥ 2**). |
| `BSC_ALLOWED_CHAIN_IDS` | Default `56`; widen only for non-mainnet testing. |
| `BSC_CONFIRMATION_BLOCKS` | Reorg depth (default 15). |
| `VENUS_VTOKEN_ADDRESS` | On BSC-mainnet-only allowlist, must be canonical vFDUSD vToken (see `config.rs`). |
| `TERRA_LCD_URL` | HTTPS LCD base URL (used for queries + broadcast). |
| `TERRA_CHAIN_ID` | `columbus-5` on mainnet. |
| `TERRA_MNEMONIC` | Oracle operator seed (**secret**). |
| `ORACLE_CONTRACT` | `ust1-oracle` address. |
| `POLL_INTERVAL_SECS` | Default 21600 s. |
| `ORACLE_MAX_SILENCE_SECS` | Loud log if no successful broadcast (default 28800 s). |

**HTTPS:** production must use `https://`. Local-only: `DEV_ALLOW_HTTP=1` for loopback (see `config.rs`).

**Preflight (local shell):**

```bash
export BSC_RPC_URLS="https://…,https://…"
# … all required vars …
bash scripts/verify_oracle_operator_env.sh
```

---

## Deterministic vs recorded addresses

Treat mainnet addresses as **recorded-at-deploy**: store code id, tx hashes, and contract addresses. The oracle reads a **known** Venus vToken on BSC; you do not deploy EVM contracts for the oracle path.

---

## Address registry (template)

| Contract / role | Mainnet (`columbus-5`) | Code ID | Address | Notes |
|-----------------|------------------------|---------|---------|-------|
| CL8Y Terra bridge | | | | Existing deployment |
| vFDUSD cw20 | | | | Minter = bridge |
| UST1 cw20 | | | | Minter includes window |
| `ust1-oracle` | | | | |
| `ust1-window` | | | | |
| CMM treasury | `terra16j5u6ey7a84g40sr3gd94nzg5w5fm45046k9s2347qhfpwm5fr6sem3lr2` | — | — | Default `cmm_treasury` if omitted |
| BSC vFDUSD ERC20 | | | | Registered in CL8Y `TokenRegistry` |

### BSC (oracle path)

| Item | Mainnet |
|------|---------|
| Chain ID (EVM) | `56` |
| Venus vFDUSD vToken | `0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba` |

---

## Keys and funding

- **Deployer / admin:** fund with enough **LUNC** (`uluna`) for wasm store and setup.
- **Oracle operator:** must pay Terra fees on each `UpdateRate`; keep a buffer (service uses gas limit 500k × price 0.015 uluna/gas in code — monitor real spend).

---

## Smoke checks

```bash
terrad query wasm contract-state smart "$ORACLE_ADDR" '{"state":{}}' --chain-id columbus-5 --node "$TERRA_RPC"
terrad query wasm contract-state smart "$WINDOW_ADDR" '{"effective_swap":{}}' --chain-id columbus-5 --node "$TERRA_RPC"
```

---

## Changelog

| Date | Change |
|------|--------|
| 2026-07-30 | Window instantiate example + defaults: `fee_bps=100`, per-tx **1,000** / rolling 24h **10,000** UST1 ([issue #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)). |
| 2026-04-23 | Full mainnet runbook: `terrad` fees/gas, cw20-mintable + CL8Y vFDUSD wiring, UST1 contracts, Render dashboard worker instructions ([issue #15](https://gitlab.com/PlasticDigits/ust1-window/-/issues/15)). |
| 2026-04-22 | Initial deployment doc and registry. |
