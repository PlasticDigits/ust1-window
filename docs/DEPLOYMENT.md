# Deployment: BSC + Terra Classic (UST1 stack)

This document is the **operator-facing deployment path** for production: Terra Classic wasm (UST1 swap window + oracle + tokens), CL8Y bridge registration for **vFDUSD**, BSC read-only integration for the oracle service, and hosting the oracle **without** a `render.yaml` file (Render dashboard only).

**Related code:**

| Area | Location |
|------|----------|
| LocalTerra helper | [`scripts/deploy_local.py`](../scripts/deploy_local.py) |
| Optimized wasm (UST1 contracts) | [`make build-optimized`](../Makefile), [`scripts/optimize.sh`](../scripts/optimize.sh) |
| Oracle service config / canonical BSC token | [`oracle-service/src/config.rs`](../oracle-service/src/config.rs) |
| On-chain oracle policy (must match service) | [`ust1-common` oracle_policy](../smartcontracts-terraclassic/packages/ust1-common/src/oracle_policy.rs), [`ust1-oracle`](../contracts/ust1-oracle) |
| Swap math & limits | [`ust1-common` math](../smartcontracts-terraclassic/packages/ust1-common/src/math.rs), [`ust1-window`](../contracts/ust1-window) |
| Native wrap (optional) | [`cmm-native-wrap`](../contracts/cmm-native-wrap) |

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
5. **Keyring:** use the same `--keyring-backend` as your imports (this ops setup uses **`file`** — see `~/.terra/config/client.toml`). Do not pass `os` if keys live under `keyring-file`.
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
  --keyring-backend file \
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
| **CMM treasury** | ustr-cmm **Treasury contract** (not an EOA): holds inventory; window redeem via `InstantWithdrawCw20` + `SetCw20Spender` (see Phase 5 / [#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20)) |

### Known mainnet operator addresses ([issue #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19))

| Role | Address | Notes |
|------|---------|-------|
| Terra deployer key (`cl8ydeploy`) | `terra1hu4zggf3f8yw6jw3rxrjxn2drwad675gq5k2lv` | `--from` for CW20 / UST1 stack instantiate + store; creator of code id **10184** |
| Terra admin / governance / bridge admin | `terra1xsecn4snv94ezcez0z3vq8an9j4h4kxxcydp8l` | CW20 `--admin`; UST1 initial minter (`GOVERNANCE_ADDR`); CL8Y Terra bridge admin (`cl8y2_admin` keyring) |
| BSC deployer | `0xD699EbC6930F593f0725D2a7dC58ACC65b41a08e` | Historical CL8Y deployer (not required for token register) |
| BSC admin / TokenRegistry owner | `0xcd4eb82cfc16d5785b4f7e3bfc255e735e79f39c` | Signs `registerToken` / destination / incoming mapping |
| Venus vFDUSD (BSC) | `0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba` | **8 decimals**; 3rd-party → bridge as **LockUnlock** (`0`), not MintBurn |
| CL8Y Terra bridge | `terra18m02l2f43c2dagqnz3kfccpgz9pzzz5hk9l5mh5wvr6dcvv47zfqdfs7la` | Minter of Terra bridged vFDUSD CW20 |
| CL8Y BSC TokenRegistry | `0x3d8820ec93748fd4df8eee6b763834a23938b207` | |
| CL8Y BSC LockUnlock | `0xd7b3bf05987052009c350874e810df98da95d258` | Handler for Venus vFDUSD |
| cw20-mintable code id | `10184` | Already stored on `columbus-5` — **do not re-store** |

Phase 2 signs instantiate with **`cl8ydeploy`** (`terra1hu4z…`) but sets contract admin / UST1 minter to **`terra1xsecn…`**. Phase 3 Terra bridge txs use keyring **`cl8y2_admin`** (same `terra1xsecn…` address).

Suggested exports:

```bash
export TERRA_RPC="https://terra-classic-rpc.publicnode.com:443"
export TERRA_LCD="https://terra-classic-lcd.publicnode.com:443"
export TERRA_CHAIN_ID="columbus-5"

# Keyring names (import keys that match the addresses below)
export TERRA_KEY_NAME="cl8ydeploy"          # terra1hu4zggf3f8yw6jw3rxrjxn2drwad675gq5k2lv
export TERRA_ADMIN="terra1xsecn4snv94ezcez0z3vq8an9j4h4kxxcydp8l"
export GOVERNANCE_ADDR="terra1xsecn4snv94ezcez0z3vq8an9j4h4kxxcydp8l"
export TERRA_BRIDGE_ADMIN_KEY="cl8y2_admin"  # terra1xsecn4snv94ezcez0z3vq8an9j4h4kxxcydp8l
export KEYRING_BACKEND="file"               # matches ~/.terra/config/client.toml

export TERRA_BRIDGE_ADDRESS="terra18m02l2f43c2dagqnz3kfccpgz9pzzz5hk9l5mh5wvr6dcvv47zfqdfs7la"
export CW20_MINTABLE_CODE_ID="10184"

export BSC_RPC="https://bsc-dataseed1.binance.org"
export BSC_TOKEN_REGISTRY="0x3d8820ec93748fd4df8eee6b763834a23938b207"
export BSC_VFDUSD_ERC20="0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba"  # Venus vFDUSD, 8 decimals
export BSC_ADMIN="0xcd4eb82cfc16d5785b4f7e3bfc255e735e79f39c"

# Bridge-internal chain ids (CL8Y mainnet)
export BSC_CHAIN_ID_BYTES4="0x00000038"   # 56
export TERRA_CHAIN_ID_BYTES4="0x00000001"
export BSC_CHAIN_B64="AAAAOA=="
export TERRA_CHAIN_B64="AAAAAQ=="

# Decimals: Terra CW20 vFDUSD = 6 (issue #19); BSC Venus vFDUSD = 8 (on-chain)
export TERRA_VFDUSD_DECIMALS=6
export BSC_VFDUSD_DECIMALS=8
```

Derive **Terra CW20 → bytes32 / base64** for mappings using the Python snippets below (same as CL8Y §6.0 / §6.3) so EVM `setTokenDestinationWithDecimals` and Terra `set_incoming_token_mapping` agree.

---

## Master checklist

Use this end-to-end; record every address and code id in the [registry](#address-registry-template).

### Prerequisites

- [ ] Rust + Docker (for `cosmwasm/workspace-optimizer`), `terrad`, funded wallets, HTTPS RPC/LCD endpoints.
- [ ] **Two or more** HTTPS BSC JSON-RPC URLs for `BSC_RPC_URLS` (oracle service consensus).
- [ ] CL8Y bridge deployed and **admin keys** available on Terra and BSC (see CL8Y `deployment-guide.md`).
- [ ] Decide **decimals** for vFDUSD on Terra and BSC; they must match what you register on `TokenRegistry` / Terra bridge (mis-matched decimals brick amounts).

### Tokens (cw20-mintable)

- [x] **cw20-mintable code id `10184`** already on `columbus-5` — skip `wasm store`.
- [x] Instantiate **vFDUSD** (decimals **6**) — `terra1mnl9azefrqpmu888ar2u6zrcwr80hxlt3avf4300r576cw5ar7esvxsvj3` (minter = bridge).
- [x] Instantiate **UST1** (decimals **6**) — `terra1f0eqgy9w7e5e7up97vjudqwx38tesf8ylx75x2lv3nwm0clry0pqmgfy72` (minter = governance).
- [x] Query `{"token_info":{}}` on both; record **`TERRA_VFDUSD`**, **`TERRA_UST1`** (see [address registry](#address-registry-template)).

### CL8Y bridge: list and connect vFDUSD (BSC ↔ Terra)

Venus vFDUSD on BSC is **LockUnlock** (`registerToken` type **`0`**). Terra CW20 is **mint_burn**. See [Phase 3](#phase-3--cl8y-bridge-registration-for-vfdusd-bsc-lockunlock--terra-mint_burn).

- [ ] **BSC:** `registerToken(vFDUSD, 0)` then destination + incoming mapping (Terra decimals **6** / BSC **8**).
- [ ] **Terra:** `add_token` mint_burn → `set_token_destination` → `set_incoming_token_mapping` (bridge admin key).
- [ ] **Smoke:** minimal BSC→Terra lock/mint before public announcement.
- [ ] **Frontend / “listed on CL8Y”:** Bridge UI token lists are driven by **on-chain registry + off-chain config** for your environment. After mappings exist, confirm the asset appears in the CL8Y app you expose to users (refresh operator env / token matrix / caches per your CL8Y ops runbook). If the chain is correct but the UI is empty, treat it as an **operator or frontend config** issue, not a UST1 contract issue.

### UST1 stack (this repo)

- [ ] `make build-optimized` at the git revision you intend to ship; record git SHA.
- [ ] `wasm store` `artifacts/ust1_oracle.wasm`, `artifacts/ust1_window.wasm` → code IDs.
- [x] Instantiate **`ust1-oracle`** — `terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n` (code **11549**).
- [x] Instantiate **`ust1-window`** — `terra1zxwpzpzpleatqn39r00grau4yt29sld8pw78s7ktvjafnj5nsaxq0h3rh2` (code **11550**; approved fee/limits; default CMM treasury).
- [ ] **UST1 minters:** from governance, execute cw20-mintable **`add_minter`** for **`ust1-window`** (see integration tests in `ust1-integration-tests`).
- [ ] **Treasury / withdraw inventory (Option 3):** migrate window to InstantWithdrawCw20 code; treasury gov `SetCw20Spender` (+ `limit_24h`) for vFDUSD → window (see Phase 5). Depends on [ustr-cmm#6](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/6) / [#7](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/7) and [ust1-window#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20).
- [ ] **Governance handoff:** if required, run `ProposeGovernance` / `AcceptGovernance` on oracle and window.
- [ ] **First oracle commit:** `oracle_operator` sends `UpdateRate` consistent with policy (service will continue updates).

### Optional: `cmm-native-wrap`

- [ ] If wrapping **uluna/uusd**, deploy `cmm-native-wrap` per [`cmm-native-wrap`](../contracts/cmm-native-wrap) and your governance playbook (no oracle).

### Oracle service (Render, no YAML)

- [ ] Create Render resources via **dashboard** (see [Render dashboard setup](#render-dashboard-setup-no-renderyaml)).
- [ ] Set **all** env vars; run `scripts/verify_oracle_operator_env.sh` locally with the same values before applying.
- [ ] Confirm logs show periodic polls and successful broadcasts after deploy.
- [ ] External alerting on process restarts / log errors (Render notifications + optional log drain).

---

## Phase 1 — Build UST1 wasm artifacts

Needed for **Phase 4** (`ust1-oracle` / `ust1-window`), **not** for Phase 2 CW20 instantiate (that uses existing code id **10184**).

From repo root:

```bash
make build-optimized
ls -l artifacts/ust1_oracle.wasm artifacts/ust1_window.wasm
# expect non-empty files; if artifacts/ is empty/root-owned, fix Docker perms and re-run
```

Ship **only** artifacts built from a **tagged** revision you have tested.

---

## Phase 2 — Instantiate vFDUSD and UST1 (cw20-mintable **10184**)

**Skip `wasm store`.** Code id `10184` is already on `columbus-5` (creator `terra1hu4z…`). Confirm:

```bash
terrad query wasm code-info 10184 --chain-id columbus-5 --node "$TERRA_RPC"
```

Ensure deployer key is funded and imported:

```bash
terrad keys show "$TERRA_KEY_NAME" -a   # expect terra1hu4zggf3f8yw6jw3rxrjxn2drwad675gq5k2lv
terrad keys show "$TERRA_BRIDGE_ADMIN_KEY" -a   # expect terra1xsecn4snv94ezcez0z3vq8an9j4h4kxxcydp8l
```

### Instantiate vFDUSD (bridge is minter)

Terra CW20 **decimals = 6** (issue #19). BSC Venus token is **8 decimals** — bridge mappings in Phase 3 scale between them. Minter = CL8Y Terra bridge (mints on BSC→Terra). Contract **admin** = UST1 governance.

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
  --keyring-backend file \
  --broadcast-mode sync -y
```

Mainnet (2026-07-30):

```bash
export TERRA_VFDUSD="terra1mnl9azefrqpmu888ar2u6zrcwr80hxlt3avf4300r576cw5ar7esvxsvj3"
# tx 48D01D2DBEDFC46603B37C7F62FE9207CDB0683277E7E0778D94AF4091C51F02
terrad query wasm contract-state smart "$TERRA_VFDUSD" '{"token_info":{}}' \
  --chain-id columbus-5 --node "$TERRA_RPC"
# expect symbol vFDUSD, decimals 6
```

### Instantiate UST1 (governance minter first)

Window is **not** a minter yet — add it in Phase 5 after `ust1-window` exists.

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
  --keyring-backend file \
  --broadcast-mode sync -y

export TERRA_UST1="terra1f0eqgy9w7e5e7up97vjudqwx38tesf8ylx75x2lv3nwm0clry0pqmgfy72"
# tx 2A5970A8F1F74FF5970F2241B77A07BB1148B2C2BFD4FCB9290D776568E63EAF
terrad query wasm contract-state smart "$TERRA_UST1" '{"token_info":{}}' \
  --chain-id columbus-5 --node "$TERRA_RPC"
```

Record both addresses in the [address registry](#address-registry-template) (already filled for mainnet).

---

## Phase 3 — CL8Y bridge registration for vFDUSD (BSC LockUnlock ↔ Terra mint_burn)

Venus vFDUSD is a **3rd-party** ERC20: on BSC register with handler **`0` = LockUnlock** (lock on deposit / unlock on withdraw). Do **not** use MintBurn (`1`) on BSC — the bridge cannot mint/burn Venus. On Terra the bridged CW20 uses **`mint_burn`** with minter = Terra bridge (already set at instantiate).

### 3a — Derive encoding helpers (after `TERRA_VFDUSD` is set)

```bash
# Terra CW20 → bytes32 (for EVM dest token) and base64 (for Terra incoming src_token)
export TERRA_VFDUSD_BYTES32=0x$(python3 -c "import bech32; _, data = bech32.bech32_decode('$TERRA_VFDUSD'); raw = bytes(bech32.convertbits(data, 5, 8, False)); print('00' * (32 - len(raw)) + raw.hex())")
export TERRA_VFDUSD_HASH_B64=$(python3 -c "import bech32, base64; _, data = bech32.bech32_decode('$TERRA_VFDUSD'); raw = bytes(bech32.convertbits(data, 5, 8, False)); print(base64.b64encode(bytes(32 - len(raw)) + raw).decode())")

# BSC token → bytes32 hex (for Terra set_token_destination dest_token)
export BSC_VFDUSD_B32=$(cast abi-encode "f(address)" "$BSC_VFDUSD_ERC20")
```

### 3b — BSC TokenRegistry (signed by BSC admin `0xcd4e…`)

`registerToken` second arg: **`0` = LockUnlock**, **`1` = MintBurn**.

```bash
# 1) Register Venus vFDUSD as LockUnlock
cast send --interactive --rpc-url "$BSC_RPC" \
  "$BSC_TOKEN_REGISTRY" "registerToken(address,uint8)" \
  "$BSC_VFDUSD_ERC20" 0

# 2) Outgoing BSC → Terra (dest decimals = Terra CW20 decimals = 6)
cast send --interactive --rpc-url "$BSC_RPC" \
  "$BSC_TOKEN_REGISTRY" "setTokenDestinationWithDecimals(address,bytes4,bytes32,uint8)" \
  "$BSC_VFDUSD_ERC20" "$TERRA_CHAIN_ID_BYTES4" "$TERRA_VFDUSD_BYTES32" "$TERRA_VFDUSD_DECIMALS"

# 3) Incoming Terra → BSC (src decimals = Terra = 6)
cast send --interactive --rpc-url "$BSC_RPC" \
  "$BSC_TOKEN_REGISTRY" "setIncomingTokenMapping(bytes4,address,uint8)" \
  "$TERRA_CHAIN_ID_BYTES4" "$BSC_VFDUSD_ERC20" "$TERRA_VFDUSD_DECIMALS"
```

Verify (expect `true` / type `0`):

```bash
cast call "$BSC_TOKEN_REGISTRY" "tokenRegistered(address)(bool)" "$BSC_VFDUSD_ERC20" --rpc-url "$BSC_RPC"
```

### 3c — Terra bridge (signed by **bridge admin** key)

```bash
# 1) Add Terra CW20 as mint_burn (bridge already minter on the CW20)
terrad tx wasm execute "$TERRA_BRIDGE_ADDRESS" \
  '{"add_token":{"token":"'"$TERRA_VFDUSD"'","is_native":false,"token_type":"mint_burn","terra_decimals":'"$TERRA_VFDUSD_DECIMALS"'}}' \
  --from "$TERRA_BRIDGE_ADMIN_KEY" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 10000000uluna \
  --keyring-backend file --broadcast-mode sync -y

sleep 10

# 2) Outgoing Terra → BSC (dest decimals = Venus = 8)
terrad tx wasm execute "$TERRA_BRIDGE_ADDRESS" \
  '{"set_token_destination":{"token":"'"$TERRA_VFDUSD"'","dest_chain":"'"$BSC_CHAIN_B64"'","dest_token":"'"$BSC_VFDUSD_B32"'","dest_decimals":'"$BSC_VFDUSD_DECIMALS"'}}' \
  --from "$TERRA_BRIDGE_ADMIN_KEY" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 10000000uluna \
  --keyring-backend file --broadcast-mode sync -y

sleep 10

# 3) Incoming BSC → Terra (src_token = Terra CW20 bytes32/base64; src_decimals = BSC = 8)
terrad tx wasm execute "$TERRA_BRIDGE_ADDRESS" \
  '{"set_incoming_token_mapping":{"src_chain":"'"$BSC_CHAIN_B64"'","src_token":"'"$TERRA_VFDUSD_HASH_B64"'","local_token":"'"$TERRA_VFDUSD"'","src_decimals":'"$BSC_VFDUSD_DECIMALS"'}}' \
  --from "$TERRA_BRIDGE_ADMIN_KEY" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 10000000uluna \
  --keyring-backend file --broadcast-mode sync -y
```

Smoke with a **minimal** BSC→Terra lock/mint before announcing. Full CL8Y patterns: `cl8y-bridge-monorepo` `docs/deployment-guide.md` §6.

---

## Phase 4 — Deploy `ust1-oracle` and `ust1-window`

### Store

```bash
terrad tx wasm store artifacts/ust1_oracle.wasm \
  --from "$TERRA_KEY_NAME" --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 2.0 --fees 80000000uluna \
  --keyring-backend file --broadcast-mode sync -y

terrad tx wasm store artifacts/ust1_window.wasm \
  --from "$TERRA_KEY_NAME" --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 2.0 --fees 80000000uluna \
  --keyring-backend file --broadcast-mode sync -y
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
  --keyring-backend file --broadcast-mode sync -y
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
  --keyring-backend file --broadcast-mode sync -y
```

Schemas: [`ust1-oracle` msg](../contracts/ust1-oracle/src/msg.rs), [`ust1-window` msg](../contracts/ust1-window/src/msg.rs).

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
  --keyring-backend file --broadcast-mode sync -y
```

### 2) Withdraw inventory — Option 3 (`InstantWithdrawCw20`)

Default `cmm_treasury` is the **ustr-cmm Treasury** contract:

`terra16j5u6ey7a84g40sr3gd94nzg5w5fm45046k9s2347qhfpwm5fr6sem3lr2`

Governance: `terra1xsecn4snv94ezcez0z3vq8an9j4h4kxxcydp8l`.

**Chosen model ([#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20)):** window redeem calls treasury **`InstantWithdrawCw20`** (registered spender). Deposits still `Transfer` vFDUSD to treasury. **Do not** use EOA `increase_allowance` / CW20 `TransferFrom` against this treasury.

Treasury half: [ustr-cmm#6](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/6) (spender registry) + [#7](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/7) (24h pull limit; **fail-closed** until limit is set). Agent skill: [`skills/window-instant-withdraw-cw20`](../skills/window-instant-withdraw-cw20/SKILL.md); treasury skill: ustr-cmm `skills/treasury-cw20-instant-withdraw`.

**Ops sequence (after window wasm with InstantWithdrawCw20 is stored):**

1. Confirm treasury bytecode exposes `InstantWithdrawCw20` / `SetCw20Spender` (migrate treasury in place if still on pre-#6 code).
2. Prefer **migrate** existing window `terra1zxwp…` (admin = governance) to the new code id so the address stays stable. `MigrateMsg` is empty; config (treasury, oracle, tokens, limits) is preserved.
3. Treasury gov registers the window **with a 24h limit** (pulls fail without a limit):

```bash
# Example: align with window rolling inventory policy (~10_000 vFDUSD = 10000000000 base units)
terrad tx wasm execute "$CMM_TREASURY" \
  '{"set_cw20_spender":{"token":"'"$TERRA_VFDUSD"'","spender":"'"$WINDOW_ADDR"'","limit_24h":"10000000000"}}' \
  --from "$GOVERNANCE_KEY" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 10000000uluna \
  --keyring-backend file --broadcast-mode sync -y
```

4. Query spenders / limit, then run a **small** UST1→vFDUSD withdraw smoke. Finder should show treasury CW20 **`Transfer`** (not `TransferFrom`). `allowance(treasury, window)` remains unused (0).

**Policy note:** Window per-tx / 24h UST1 limits remain the user-facing product caps; treasury `limit_24h` is a hard ceiling (defense in depth).

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
| CL8Y Terra bridge | | — | `terra18m02l2f43c2dagqnz3kfccpgz9pzzz5hk9l5mh5wvr6dcvv47zfqdfs7la` | Existing deployment |
| cw20-mintable | | **10184** | — | Already stored |
| vFDUSD cw20 | live | 10184 | `terra1mnl9azefrqpmu888ar2u6zrcwr80hxlt3avf4300r576cw5ar7esvxsvj3` | Minter = bridge; decimals 6; tx `48D01D2D…1F02` |
| UST1 cw20 | live | 10184 | `terra1f0eqgy9w7e5e7up97vjudqwx38tesf8ylx75x2lv3nwm0clry0pqmgfy72` | Minter = governance `terra1xsecn…`; decimals 6; tx `2A5970A8…3EAF` |
| `ust1-oracle` | live | **11549** | `terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n` | Operator `terra1hm3ph0jevtkuc9efj9q3ld3ktk3g6la3ruhqna`; tx `EFA79773…355E` |
| `ust1-window` | live | **11550** | `terra1zxwpzpzpleatqn39r00grau4yt29sld8pw78s7ktvjafnj5nsaxq0h3rh2` | fee_bps=100; per-tx 1000 / 24h 10000 UST1; tx `9F078327…224C` |
| CMM treasury (ustr-cmm) | live | see ustr-cmm (migrate for #6/#7) | `terra16j5u6ey7a84g40sr3gd94nzg5w5fm45046k9s2347qhfpwm5fr6sem3lr2` | Contract; CW20 pulls via `InstantWithdrawCw20` + `SetCw20Spender`; gov `terra1xsecn…` |
| Terra deployer (`cl8ydeploy`) | — | — | `terra1hu4zggf3f8yw6jw3rxrjxn2drwad675gq5k2lv` | Code **10184** creator |
| Terra admin / gov / bridge admin (`cl8y2_admin`) | — | — | `terra1xsecn4snv94ezcez0z3vq8an9j4h4kxxcydp8l` | CW20 admin + UST1 minter + bridge admin |
| BSC vFDUSD (Venus) | — | — | `0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba` | LockUnlock; 8 decimals; registered on TokenRegistry |
| BSC TokenRegistry | — | — | `0x3d8820ec93748fd4df8eee6b763834a23938b207` | Owner `0xcd4eb8…` |
| BSC admin | — | — | `0xcd4eb82cfc16d5785b4f7e3bfc255e735e79f39c` | |
| BSC deployer | — | — | `0xD699EbC6930F593f0725D2a7dC58ACC65b41a08e` | |

### BSC (oracle path)

| Item | Mainnet |
|------|---------|
| Chain ID (EVM) | `56` |
| Venus vFDUSD vToken (same as bridged ERC20) | `0xC4eF4229FEc74Ccfe17B2bdeF7715fAC740BA0ba` |

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
| 2026-08-08 | Window withdraw = treasury `InstantWithdrawCw20` (Option 3); Phase 5 ops = migrate window + `SetCw20Spender`/`limit_24h` ([issue #20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20); depends on ustr-cmm #6/#7). |
| 2026-07-30 | Mainnet CW20s live: **vFDUSD** `terra1mnl9…svj3`, **UST1** `terra1f0eq…fy72` (code **10184**); registry + README updated ([issue #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)). |
| 2026-07-30 | Phase 2/3 operator runbook: code id **10184**, known deployer/gov/BSC addresses, Venus vFDUSD **LockUnlock** + Terra **mint_burn**, decimals Terra 6 / BSC 8 ([issue #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)). |
| 2026-07-30 | Window instantiate example + defaults: `fee_bps=100`, per-tx **1,000** / rolling 24h **10,000** UST1 ([issue #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)). |
| 2026-04-23 | Full mainnet runbook: `terrad` fees/gas, cw20-mintable + CL8Y vFDUSD wiring, UST1 contracts, Render dashboard worker instructions ([issue #15](https://gitlab.com/PlasticDigits/ust1-window/-/issues/15)). |
| 2026-04-22 | Initial deployment doc and registry. |
