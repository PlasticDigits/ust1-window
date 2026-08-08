# Deployment: BSC + Terra Classic (UST1 stack)

This document is the **operator-facing deployment path** for production: Terra Classic wasm (UST1 swap window + oracle + tokens), CL8Y bridge registration for **vFDUSD**, BSC read-only integration for the oracle service, and hosting the oracle **without** a `render.yaml` file (Render dashboard only).

**Related code:**

| Area | Location |
|------|----------|
| LocalTerra helper | [`scripts/deploy_local.py`](../scripts/deploy_local.py) |
| LocalTerra / TEST-16 smoke | [`scripts/localterra_e2e_smoke.sh`](../scripts/localterra_e2e_smoke.sh), `make test-localterra-smoke` ([#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28)) |
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
- **INV-ORACLE-PAUSE-001** — when oracle `paused=true`, `UpdateRate` is blocked and **all** windows reading that oracle reject deposit/withdraw immediately (circuit breaker; do not wait for `max_oracle_age_sec`). See [Emergency pause](#emergency-pause-oracle-circuit-breaker-vs-window) ([GitLab #22](https://gitlab.com/PlasticDigits/ust1-window/-/issues/22); audit C-2 #1). Agent skill: [`skills/oracle-circuit-breaker`](../skills/oracle-circuit-breaker/SKILL.md).
- **INV-ORACLE-LIVENESS-001** — oracle-service silence/liveness success only after DeliverTx `code == 0` **and** oracle `State` reflects the intended update (not CheckTx alone). Equal-rate and policy-skip paths must **not** call `record_successful_broadcast`. See [`skills/oracle-liveness-confirm/SKILL.md`](../skills/oracle-liveness-confirm/SKILL.md) ([GitLab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23), [#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28), audit C-3).
- **INV-MINTER-001** — pinned `cw20-mintable` `UpdateMinter` clears the old primary from `MINTERS` (`None` or rotation). In-repo proof: `ust1-integration-tests` `cw20_minter_integration` ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25)/[#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28); skill [`skills/audit-hardening-bundle`](../skills/audit-hardening-bundle/SKILL.md)).

---

## TEST-16 / LocalTerra e2e status ([#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28))

| Layer | Status | Where |
|-------|--------|--------|
| DeliverTx-reject → no liveness | **Always-on** (wiremock) | `ust1-oracle-service` `deliver_tx_failure_does_not_allow_liveness` (+ equal-rate / policy-skip / BSC hang) |
| Oracle pause fail-closed | **Always-on** (multitest + integration) | `ust1-window` multitest + `ust1-integration-tests` `oracle_paused_blocks_deposit_and_withdraw_while_rate_fresh` |
| Full LocalTerra wasm e2e | **Optional / gated** | `make test-localterra-smoke` → [`scripts/localterra_e2e_smoke.sh`](../scripts/localterra_e2e_smoke.sh); CI jobs `localterra-e2e` (GitLab **manual** + `allow_failure`, GitHub `continue-on-error`) |

**Ownership:** PlasticDigits. The gated job **skips cleanly** (exit 0) when LCD/RPC is down so default pipelines stay fast. Promote to required only after [`scripts/deploy_local.py`](../scripts/deploy_local.py) can store/instantiate optimized wasm without manual `terrad` steps. Mainnet ops probes (live withdraw, staging restart logs) remain runbook items under Phase 5 / [#19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19) — not TEST-16.

Agent skills: [`oracle-liveness-confirm`](../skills/oracle-liveness-confirm/SKILL.md), [`oracle-circuit-breaker`](../skills/oracle-circuit-breaker/SKILL.md), [`audit-hardening-bundle`](../skills/audit-hardening-bundle/SKILL.md).

---

## Emergency pause (oracle circuit breaker vs window)

Prefer the **oracle** pause to freeze every window that trusts that oracle. Use the **window** pause only for a single market.

| Action | Who | Effect | When |
|--------|-----|--------|------|
| Oracle `SetPaused { paused: true }` | **Governance only** | Blocks `UpdateRate`; `State.paused=true`; windows fail with `OraclePaused` on deposit **and** withdraw (even if rate is still age-fresh) | Suspected vFDUSD / Venus / bridge economic collapse; oracle-service detects a rate **decrease** it cannot mark on-chain |
| Oracle `SetPaused { paused: false }` | **Governance only** | Resumes updates + window swaps (subject to freshness / limits) | After incident review; rate policy still monotonic — no emergency rate-down in this path |
| Window `SetPaused` | Window governance | Pauses that window only | Local maintenance / single-window incident |

**Ops steps (oracle circuit breaker):**

```bash
# Confirm pause surface (after migrate that includes State.paused)
terrad query wasm contract-state smart "$ORACLE_ADDR" '{"config":{}}' \
  --chain-id columbus-5 --node "$TERRA_RPC"
terrad query wasm contract-state smart "$ORACLE_ADDR" '{"state":{}}' \
  --chain-id columbus-5 --node "$TERRA_RPC"
# expect paused: true/false on both

# Trip breaker (governance key)
terrad tx wasm execute "$ORACLE_ADDR" \
  '{"set_paused":{"paused":true}}' \
  --from "$GOVERNANCE_KEY" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 10000000uluna \
  --keyring-backend file --broadcast-mode sync -y

# Smoke: window deposit/withdraw should revert with oracle paused (not wait ~6h staleness)
```

**Migrate note:** Deploy/migrate **oracle then window** (or same release) so `State` includes `paused` and the window enforces `OraclePaused`. Storage layout is unchanged (pause already lived in oracle `Config`); `State.paused` is an additive query field.

**Out of scope here:** emergency rate reset / monotonic bypass; operator auto-trip; spot-vs-`exchangeRateStored` alerter (remaining C-2 items).

Ops timing vs window staleness ([glab #24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24) / audit H-3; skill [`skills/oracle-ops-poll-silence`](../skills/oracle-ops-poll-silence/SKILL.md)):

- **INV-ORACLE-OPS-POLL-001** — default poll (**3600 s**) ≪ window `DEFAULT_MAX_ORACLE_AGE_SECS` (**21600 s**).
- **INV-ORACLE-OPS-SILENCE-001** — default silence (**21600 s**) ≤ window max oracle age (chosen formula: `silence = max_oracle_age`; audit also allows `max_age + poll` only when `poll ≪ max_age`).
- Prefer: `poll < max_oracle_age` and `silence ≤ max_oracle_age` (page at or before user impact).
- On-chain throttle / daily / mono policy are **unchanged**; frequent polls are mostly no-ops when within band.
- The service does **not** read live on-chain `max_oracle_age_sec`; if governance changes it, retune env. Startup logs `ORACLE_OPS_TIMING_MISCONFIG` for common footguns (does not hard-fail).
- Silence keys off **confirmed** on-chain updates (DeliverTx + matching `State`, C-3 / [#23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23)), not mempool CheckTx acceptance alone.

Window swap guards (audit hardening, [issue #25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25); see [`audits/INTERNAL_KIMIK3_1786162831.md`](../audits/INTERNAL_KIMIK3_1786162831.md)):

- **INV-DECIMALS-001** — at `ust1-window` instantiate, vFDUSD token decimals must be **≥** UST1 token decimals (`validate_token_decimals`; atom scaling assumes D≥U).
- **INV-SWAP-003** — deposit reverts when computed `ust1_out == 0` (no treasury forward / `Mint(0)`).
- **INV-SWAP-004** — withdraw reverts when computed `v_out == 0` (no burn-for-nothing).

Agent skill for these invariants and ops knobs: [`skills/audit-hardening-bundle`](../skills/audit-hardening-bundle/SKILL.md).

---

## Terra Classic CLI (`terrad`): gas, fees, and common mistakes

Terra Classic is easy to get wrong if you copy “generic Cosmos” snippets. Prefer a **fixed pattern** for every `terrad tx`:

1. **Always pin the node:** `--node https://…:443` (or your provider’s Tendermint RPC). Queries and broadcasts must hit a node you trust.
2. **Always set chain id:** `--chain-id columbus-5` for mainnet.
3. **Use automatic gas *with* headroom:** `--gas auto --gas-adjustment 1.5` (instantiate) or **`1.8`–`2.5`** for `wasm store` of large binaries when simulation underestimates.
4. **Prefer `--gas-prices 28.325uluna` with `--gas auto`** for store/migrate (matches mainnet min-gas-price). Fixed `--fees` also works but under-budget fails hard: e.g. `80000000uluna` was rejected when estimate needed ~`91894401uluna`. Executes often use **`--fees 10000000uluna`**; large **`wasm store`** may need **`120000000uluna`+** if not using gas-prices.
5. **Keyring:** use the same `--keyring-backend` as your imports (this ops setup uses **`file`** — see `~/.terra/config/client.toml`). Do not pass `os` if keys live under `keyring-file`. Password-locked keys: pipe the passphrase twice (sim + sign), e.g. `printf '%s\n%s\n' "$PASS" "$PASS" | terrad tx …`, or set `TERRA_KEYRING_PASSWORD` when using ustr-cmm `treasury-migrate-wrap-wire.sh`.
6. **Broadcast mode:** `--broadcast-mode sync` (or `block`) so you can inspect the tx result immediately. Sync responses often have empty `events` — query the `txhash` (or LCD) for `code_id` / migrate confirmation.

**Wasm `store` example:**

```bash
terrad tx wasm store artifacts/ust1_oracle.wasm \
  --from "$TERRA_KEY_NAME" \
  --chain-id columbus-5 \
  --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 \
  --gas-prices 28.325uluna \
  --keyring-backend file \
  --broadcast-mode sync -y
```

After each tx, record **`code_id`** (for store) or **`_contract_address`** (for instantiate) from the confirmed tx events (`terrad query tx <hash>` or LCD).

**Queries** use the same `--node` (RPC URLs work for `query` in current Terra Classic tooling). If `terrad query wasm contract` fails with a proto decode error (`wiretype end group for non-group`), use LCD instead:

```bash
terrad query wasm contract-state smart "$ORACLE_ADDR" '{"state":{}}' \
  --chain-id columbus-5 --node "$TERRA_RPC"

# Fallback for contract metadata (code_id / admin):
curl -sS "$TERRA_LCD/cosmwasm/wasm/v1/contract/$WINDOW_ADDR" | jq .
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
export GOVERNANCE_KEY="cl8y2_admin"          # same address; migrate / add_minter / SetCw20Spender
export KEYRING_BACKEND="file"               # matches ~/.terra/config/client.toml

export TERRA_BRIDGE_ADDRESS="terra18m02l2f43c2dagqnz3kfccpgz9pzzz5hk9l5mh5wvr6dcvv47zfqdfs7la"
export CW20_MINTABLE_CODE_ID="10184"
export ORACLE_ADDR="terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n"
export WINDOW_ADDR="terra1zxwpzpzpleatqn39r00grau4yt29sld8pw78s7ktvjafnj5nsaxq0h3rh2"
export CMM_TREASURY="terra16j5u6ey7a84g40sr3gd94nzg5w5fm45046k9s2347qhfpwm5fr6sem3lr2"
export UST1_ORACLE_CODE_ID="11549"
export UST1_WINDOW_CODE_ID="11566"          # InstantWithdrawCw20 (was 11550 at instantiate)

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

- [x] `make build-optimized` for InstantWithdrawCw20 window wasm (sha256 `469e0b9f…ebc9`).
- [x] `wasm store` InstantWithdraw `ust1_window.wasm` → code id **11566** (tx `AA40BE6A…037E`); oracle store remains **11549**.
- [x] Instantiate **`ust1-oracle`** — `terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n` (code **11549**).
- [x] Instantiate **`ust1-window`** — `terra1zxwpzpzpleatqn39r00grau4yt29sld8pw78s7ktvjafnj5nsaxq0h3rh2` (originally code **11550**; approved fee/limits; default CMM treasury).
- [x] **Migrate window** to InstantWithdrawCw20 code **11566** (tx `5C2A5CAF…1227`; admin `cl8y2_admin` / `terra1xsecn…`).
- [x] **UST1 minters:** window `terra1zxwp…` is in UST1 `minters` (governance self-mint cleanup / INV-MINTER-001 still optional).
- [x] **Treasury / withdraw inventory (Option 3):** treasury code **11564**; `SetCw20Spender` + `limit_24h=10000000000` for vFDUSD → window (see Phase 5). Schema pin / CI: [#21](https://gitlab.com/PlasticDigits/ust1-window/-/issues/21).
- [ ] **Pre-announce:** schema conformance green at pin rev; **live withdraw probe** tx recorded (Phase 5 §2 step 4) — needs treasury vFDUSD inventory + first `UpdateRate`.
- [ ] **Governance handoff:** if required, run `ProposeGovernance` / `AcceptGovernance` on oracle and window.
- [ ] **First oracle commit:** `oracle_operator` sends `UpdateRate` consistent with policy (service will continue updates).

### Optional: `cmm-native-wrap`

- [ ] If wrapping **uluna/uusd**, deploy `cmm-native-wrap` per [`cmm-native-wrap`](../contracts/cmm-native-wrap) and your governance playbook (no oracle).

### Oracle service (Coolify Dockerfile or Render)

- [ ] Create Coolify app from root [`Dockerfile`](../Dockerfile) (see [Coolify / Render](#oracle-host-coolify-dockerfile-or-render)) — or Render Background Worker.
- [ ] Set **all** env vars; run `scripts/verify_oracle_operator_env.sh` locally with the same values before applying. `TERRA_MNEMONIC` → `terra1hm3ph…`.

- [ ] Confirm `POLL_INTERVAL_SECS` ≪ on-chain / intended `max_oracle_age_sec` (default poll **3600**, window default age **21600**).
- [ ] Confirm `ORACLE_MAX_SILENCE_SECS` ≤ `max_oracle_age` (default **21600**); avoid silence ≫ `max_age + poll`.
- [ ] Confirm logs show `poll_interval_secs` / `max_silence_since_broadcast_secs` at startup, then periodic polls and successful broadcasts after deploy.
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

Mainnet: window is already an UST1 minter. Historical instantiate used governance as the primary minter first, then Phase 5 `add_minter(window)`.

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

Mainnet InstantWithdraw window wasm is already stored as code id **11566** (see [address registry](#address-registry-template)). Prefer `--gas-prices` (fixed `--fees 80000000uluna` underpays at ~28.325 uluna/gas).

```bash
terrad tx wasm store artifacts/ust1_oracle.wasm \
  --from "$TERRA_KEY_NAME" --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna \
  --keyring-backend file --broadcast-mode sync -y

terrad tx wasm store artifacts/ust1_window.wasm \
  --from "$GOVERNANCE_KEY" --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna \
  --keyring-backend file --broadcast-mode sync -y
# mainnet: code 11566 — tx AA40BE6AD52295F8D1ABF4352ECEDBB8D99F3BBE23AC3F4BC609E62B27BD037E
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

**Bootstrap / INV-MINTER-001 ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25)/[#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28)):** after the window is an additional minter, drop governance self-mint so a stale `MINTERS` entry cannot mint:

```bash
# Clear primary minter (also removes dual-listed primary from MINTERS on pinned fork)
terrad tx wasm execute "$TERRA_UST1" \
  '{"update_minter":{"new_minter":null}}' \
  --from "$GOVERNANCE_KEY" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 10000000uluna \
  --keyring-backend file --broadcast-mode sync -y

# Or rotate primary to a dedicated admin, then AddMinter(window) from the new primary:
# '{"update_minter":{"new_minter":"'"$NEW_PRIMARY"'"}}'
terrad query wasm contract-state smart "$TERRA_UST1" \
  '{"minters":{}}' --chain-id columbus-5 --node "$TERRA_RPC"
# expect governance address absent from minters list
```

In-repo regression: `cargo test -p ust1-integration-tests --lib inv_minter_001`.

### 2) Withdraw inventory — Option 3 (`InstantWithdrawCw20`)

Default `cmm_treasury` is the **ustr-cmm Treasury** contract:

`terra16j5u6ey7a84g40sr3gd94nzg5w5fm45046k9s2347qhfpwm5fr6sem3lr2`

Governance: `terra1xsecn4snv94ezcez0z3vq8an9j4h4kxxcydp8l`.

**Chosen model ([#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20)):** window redeem calls treasury **`InstantWithdrawCw20`** (registered spender). Deposits still `Transfer` vFDUSD to treasury. **Do not** use EOA `increase_allowance` / CW20 `TransferFrom` against this treasury.

Treasury half: [ustr-cmm#6](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/6) (spender registry) + [#7](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/7) (24h pull limit; **fail-closed** until limit is set). Cross-repo wire: [#21](https://gitlab.com/PlasticDigits/ust1-window/-/issues/21). Agent skill: [`skills/window-instant-withdraw-cw20`](../skills/window-instant-withdraw-cw20/SKILL.md); treasury skill: ustr-cmm `skills/treasury-cw20-instant-withdraw`.

**Schema pin (INV-SCHEMA-001 / [#21](https://gitlab.com/PlasticDigits/ust1-window/-/issues/21)):** Window `InstantWithdrawCw20` JSON must match ustr-cmm treasury at git rev **`e6c4b7cf33f2f56d21c0e9fb2828efe87f032ded`** (`ust1_window::treasury::USTR_CMM_TREASURY_SCHEMA_REV`). Golden vectors: [`contracts/ust1-window/testdata/instant_withdraw_cw20_golden.json`](../contracts/ust1-window/testdata/instant_withdraw_cw20_golden.json). Verify / refresh: `scripts/verify_treasury_wire_schema.sh` (CI also runs `treasury_schema` + `real_treasury_integration` against the pinned `cmm-treasury` git dep).

**Ops sequence (mainnet status 2026-08-08):**

1. ~~Confirm treasury bytecode exposes `InstantWithdrawCw20` / `SetCw20Spender`~~ — treasury code **11564**.
2. ~~Migrate window `terra1zxwp…`~~ — code **11566** (empty `MigrateMsg`; config preserved). Store + migrate signed by **`cl8y2_admin`** (`GOVERNANCE_KEY`):

```bash
# UST1_WINDOW_CODE_ID / WINDOW_ADDR from suggested exports (11566 / terra1zxwp…)

# Store (if uploading a newer build):
# terrad tx wasm store artifacts/ust1_window.wasm --from "$GOVERNANCE_KEY" \
#   --chain-id columbus-5 --node "$TERRA_RPC" \
#   --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna \
#   --keyring-backend file --broadcast-mode sync -y

terrad tx wasm migrate "$WINDOW_ADDR" "$UST1_WINDOW_CODE_ID" '{}' \
  --from "$GOVERNANCE_KEY" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --gas-prices 28.325uluna \
  --keyring-backend file --broadcast-mode sync -y
# done: tx 5C2A5CAF60C2CC90FD1ED897E938A2350DE129CB110BA601EF6E3B382FC11227
```

Verify via LCD if `terrad query wasm contract` hits a proto decode error:

```bash
curl -sS "$TERRA_LCD/cosmwasm/wasm/v1/contract/$WINDOW_ADDR" | jq '.contract_info.code_id'
# expect "11566"
```

3. ~~Treasury gov registers the window **with a 24h limit**~~ — `limit_24h=10000000000` live. Re-run pattern if rotating spenders:

```bash
# Example: align with window rolling inventory policy (~10_000 vFDUSD = 10000000000 base units)
terrad tx wasm execute "$CMM_TREASURY" \
  '{"set_cw20_spender":{"token":"'"$TERRA_VFDUSD"'","spender":"'"$WINDOW_ADDR"'","limit_24h":"10000000000"}}' \
  --from "$GOVERNANCE_KEY" \
  --chain-id columbus-5 --node "$TERRA_RPC" \
  --gas auto --gas-adjustment 1.5 --fees 10000000uluna \
  --keyring-backend file --broadcast-mode sync -y
```

Query limit:

```bash
terrad query wasm contract-state smart "$CMM_TREASURY" \
  '{"cw20_spender_limit":{"token":"'"$TERRA_VFDUSD"'","spender":"'"$WINDOW_ADDR"'"}}' \
  --chain-id columbus-5 --node "$TERRA_RPC"
```

4. Run a **small** UST1→vFDUSD withdraw smoke (**live probe — required before public redeem announcement**). Finder should show treasury CW20 **`Transfer`** (not `TransferFrom`). `allowance(treasury, window)` remains unused (0). Record the probe tx hash in the [address registry](#address-registry-template) notes. Blocked until treasury holds vFDUSD and oracle `last_update_sec` is non-zero.

**Live withdraw probe checklist (pre-announce):**

- [x] Schema pin rev recorded and CI `verify_treasury_wire_schema` / `treasury_schema` green on the deploy branch
- [x] Window migrated to InstantWithdrawCw20 code **11566**; treasury exposes spender API (code **11564**)
- [x] `SetCw20Spender` + `limit_24h` executed for vFDUSD → window
- [ ] Small smoke withdraw succeeds; events show CW20 `Transfer` from treasury
- [ ] Unhappy path sanity (optional): confirm unregistered spender cannot pull (gov test on staging / LocalTerra)

**Policy note:** Window per-tx / 24h UST1 limits remain the user-facing product caps; treasury `limit_24h` is a hard ceiling (defense in depth).

### 3) First oracle rate

Until `last_update_sec` is set by a successful `UpdateRate`, window swaps may reject stale oracle state. Bring rate on-chain from **`ORACLE_BOT_ADDR`** (or rely on the service once live).

---

## Oracle host: Coolify (Dockerfile) or Render

`ust1-oracle-service` is a **long-running process** with structured logs. It exposes a minimal **liveness** HTTP endpoint (`GET /healthz`) for platform health checks; there is no metrics API.

**Mainnet operator** (must match `TERRA_MNEMONIC`): `terra1hm3ph0jevtkuc9efj9q3ld3ktk3g6la3ruhqna`  
**Oracle contract:** `terra1fmht0t6svq3n24zx03nkfja0m40zhfyyxkdcvlrkl6u7gfe6aagq4gch8n` (code **11549**).

### Coolify (recommended)

Repo root [`Dockerfile`](../Dockerfile) builds `ust1-oracle-service` (Rust **1.88** bookworm → slim runtime). [`.dockerignore`](../.dockerignore) trims build context.

1. **New Resource → Application** (Dockerfile).
2. Connect this GitLab repo; branch `main` (or a deploy tag).
3. **Dockerfile** path: `/Dockerfile`; build context: repository root.
4. **Port:** `8080` (health only — not a public API).
5. **Healthcheck:** `GET /healthz` on port `8080` (or rely on the image `HEALTHCHECK`).
6. **Environment:** paste the [table below](#oracle-service-environment). Mark `TERRA_MNEMONIC` as secret.
7. **Deploy.** Confirm logs show startup + `check_rate_update`; after the first confirmed `UpdateRate`, oracle `state.last_update_sec` is non-zero.

Local image smoke:

```bash
docker build -t ust1-oracle-service .
docker run --rm -p 8080:8080 --env-file .env ust1-oracle-service
curl -fsS http://127.0.0.1:8080/healthz
```

### Render dashboard (no `render.yaml`)

1. **New → Background Worker** (recommended) or **Private Service** if your org uses them.
2. Connect this repository (GitLab/GitHub).
3. **Branch:** production branch or tag.
4. **Build command:**  
   `cargo build --release -p ust1-oracle-service`
5. **Start command:**  
   `./target/release/ust1-oracle-service`
6. **Instance type:** smallest that keeps steady CPU for periodic RPC polling; scale if you see RPC timeouts.
7. **Environment → Environment Variables:** paste the [table below](#oracle-service-environment) (production values, never commit secrets). Set `RUST_VERSION` to match CI if needed (`1.88`).
8. **Deploy.** Watch **Logs** for `check_rate_update` / `sign_and_broadcast_execute` outcomes.

### Secrets

- Store `TERRA_MNEMONIC` in Coolify/Render **Secret** fields only (seed for `terra1hm3ph…`).
- Rotate operator keys via on-chain `SetOracleOperator` if compromised.

### Health / alerting

- Enable host **notifications** for **crashes and deploy failures**.
- **HTTP health check:** `GET /healthz` on `HEALTHZ_BIND` (default `0.0.0.0:8080`). This is **liveness only** (process up) — it does **not** prove a fresh on-chain oracle rate.
- The binary emits **`error!`** if no **confirmed on-chain oracle update** exceeds `ORACLE_MAX_SILENCE_SECS` (default **21600** s) — forward logs to your SIEM or log drain and page on that pattern (`LIVENESS_ORACLE_NO_BROADCAST`). Also watch startup **`ORACLE_OPS_TIMING_MISCONFIG`** warnings.
- **Silence tracking means confirmed updates** (**INV-ORACLE-LIVENESS-001**, [GitLab #23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23)): after `BROADCAST_MODE_SYNC` CheckTx, the service waits for DeliverTx `code == 0` and verifies oracle `State` (`last_update_sec` advanced, `rate` matches the proposed update) before recording liveness. Mempool admission alone does **not** reset the silence timer. See [`skills/oracle-liveness-confirm/SKILL.md`](../skills/oracle-liveness-confirm/SKILL.md).
- Combine `/healthz` with log-based silence alerts; neither alone guarantees rate freshness.

---

## Oracle service environment

Loaded in [`Config::from_env`](../oracle-service/src/config.rs). Timing defaults implement **INV-ORACLE-OPS-POLL-001** / **INV-ORACLE-OPS-SILENCE-001** ([#24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24)); agent skill: [`skills/oracle-ops-poll-silence`](../skills/oracle-ops-poll-silence/SKILL.md).

| Variable | Purpose |
|----------|---------|
| `BSC_RPC_URLS` | Comma-separated HTTPS RPC URLs (**≥ 2**). |
| `BSC_ALLOWED_CHAIN_IDS` | Default `56`; widen only for non-mainnet testing. |
| `BSC_CONFIRMATION_BLOCKS` | Reorg depth (default 15). |
| `BSC_RPC_TIMEOUT_SECS` | Per-RPC HTTP timeout for BSC reads (default 30). |
| `VENUS_VTOKEN_ADDRESS` | On BSC-mainnet-only allowlist, must be canonical vFDUSD vToken (see `config.rs`). |
| `TERRA_LCD_URL` | HTTPS LCD base URL (used for queries + broadcast). |
| `TERRA_CHAIN_ID` | `columbus-5` on mainnet. |
| `TERRA_MNEMONIC` | Oracle operator seed (**secret**). |
| `ORACLE_CONTRACT` | `ust1-oracle` address. |
| `POLL_INTERVAL_SECS` | Default **3600** s (1h). Keep ≪ window `max_oracle_age_sec` (default 21600). Do not set to 21600 — zero missed-tick margin (H-3 / #24). |
| `ORACLE_MAX_SILENCE_SECS` | Loud log if no **confirmed** on-chain oracle update (DeliverTx + matching `State`; default **21600** s). Prefer ≤ window max oracle age; documented grace ≤ `max_age + poll`. |
| `ORACLE_TX_CONFIRM_TIMEOUT_SECS` | Max wait for DeliverTx after SYNC broadcast (default 90). |
| `ORACLE_TX_CONFIRM_POLL_INTERVAL_MS` | Inclusion poll interval (default 2000). |
| `HEALTHZ_BIND` | Liveness HTTP bind (`host:port`, default `0.0.0.0:8080`). Set `off`/`disabled`/empty to disable. Probe is process-up only (`GET /healthz`). |
| `TICK_TIMEOUT_SECS` | Wall-clock cap per poll tick (BSC + Terra paths); default 120. |
| `TERRA_GAS_PRICE` | Configured gas floor in uluna/gas (default `0.015`; alias `TERRA_GAS_PRICE_ULUNA`). Service uses `max(configured, network_min)` when LCD `/cosmos/base/node/v1beta1/config` probe succeeds; otherwise the configured floor. |

**Production relationship:**

```text
poll < max_oracle_age                 # default 3600 < 21600
silence ≤ max_oracle_age              # preferred (default 21600)
silence ≤ max_oracle_age + poll       # documented grace ceiling
```

Env overrides remain supported; mis-sets that violate the relationship log `ORACLE_OPS_TIMING_MISCONFIG` at startup (and advisories from `scripts/verify_oracle_operator_env.sh`) but do not hard-fail — operator responsibility if on-chain `max_oracle_age_sec` was customized.

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
| `ust1-window` | live | **11566** | `terra1zxwpzpzpleatqn39r00grau4yt29sld8pw78s7ktvjafnj5nsaxq0h3rh2` | InstantWithdrawCw20; fee_bps=100; per-tx 1000 / 24h 10000 UST1; instantiate tx `9F078327…224C` (code 11550); store `AA40BE6A…037E`; migrate `5C2A5CAF…1227`; wasm sha256 `469e0b9f…ebc9` |
| CMM treasury (ustr-cmm) | live | **11564** | `terra16j5u6ey7a84g40sr3gd94nzg5w5fm45046k9s2347qhfpwm5fr6sem3lr2` | Contract; window spender + `limit_24h=10000000000`; gov `terra1xsecn…` |
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
| 2026-08-08 | Root [`Dockerfile`](../Dockerfile) + [`.dockerignore`](../.dockerignore) for `ust1-oracle-service` Coolify/Docker deploy; Coolify runbook in this doc ([#19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)). |
| 2026-08-08 | Mainnet window migrate to InstantWithdrawCw20 code **11566** (store `AA40BE6A…037E`, migrate `5C2A5CAF…1227`); registry/README; ops note for `--gas-prices 28.325uluna` + password-locked `cl8y2_admin` ([#19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19) Phase 5 / [#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20)). |
| 2026-08-08 | Post-merge coverage gaps ([#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28)): M-8 minter integration, BSC hang, SIGTERM hook, equal-rate/policy-skip liveness, pause integration, TEST-16 LocalTerra gated smoke + docs. |
| 2026-08-08 | Oracle circuit breaker: `State.paused` + window fail-closed (`OraclePaused`); emergency pause runbook ([issue #22](https://gitlab.com/PlasticDigits/ust1-window/-/issues/22); audit C-2 #1). |
| 2026-08-08 | Audit hardening bundle ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25)): `/healthz` liveness, tick/gas/RPC timeouts, INV-SWAP-003/004 dust guards, INV-DECIMALS-001, adaptive Terra gas price; skill [`skills/audit-hardening-bundle`](../skills/audit-hardening-bundle/SKILL.md). |
| 2026-08-08 | Cross-repo InstantWithdrawCw20 schema pin + golden + real-treasury multitest; strict stubs; live probe checklist ([issue #21](https://gitlab.com/PlasticDigits/ust1-window/-/issues/21) / audit C-1); ustr-cmm rev `e6c4b7cf…`. |
| 2026-08-08 | Window withdraw = treasury `InstantWithdrawCw20` (Option 3); Phase 5 ops = migrate window + `SetCw20Spender`/`limit_24h` ([issue #20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20); depends on ustr-cmm #6/#7). |
| 2026-07-30 | Mainnet CW20s live: **vFDUSD** `terra1mnl9…svj3`, **UST1** `terra1f0eq…fy72` (code **10184**); registry + README updated ([issue #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)). |
| 2026-07-30 | Phase 2/3 operator runbook: code id **10184**, known deployer/gov/BSC addresses, Venus vFDUSD **LockUnlock** + Terra **mint_burn**, decimals Terra 6 / BSC 8 ([issue #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)). |
| 2026-07-30 | Window instantiate example + defaults: `fee_bps=100`, per-tx **1,000** / rolling 24h **10,000** UST1 ([issue #19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19)). |
| 2026-04-23 | Full mainnet runbook: `terrad` fees/gas, cw20-mintable + CL8Y vFDUSD wiring, UST1 contracts, Render dashboard worker instructions ([issue #15](https://gitlab.com/PlasticDigits/ust1-window/-/issues/15)). |
| 2026-04-22 | Initial deployment doc and registry. |
