# Internal Security Audit — ust1-window (MR !12 / #28)

| Field | Value |
|-------|--------|
| **ID** | `INTERNAL_GROKCODE_1786183770` |
| **Date (UTC)** | 2026-08-08 |
| **Auditor** | Cursor Grok 4.5 (internal) + Composer 2.5 parallel review passes |
| **Primary scope** | GitLab MR [`!12`](https://gitlab.com/PlasticDigits/ust1-window/-/merge_requests/12) (merged) — post-merge coverage gaps for [#28](https://gitlab.com/PlasticDigits/ust1-window/-/issues/28) |
| **Related issues** | Closed: `#28`, `#22`, `#23`, `#25`, `#21`, `#20`; product rejects: `#26` (H-1), `#27` (remaining C-2); **open:** `#19` (mainnet Phase 2/5 ops) |
| **Repo HEAD at audit** | `main` @ `cf2c31a130a788b77e00575e2419edf4110cfc67` |
| **Method** | Full-codebase read; `glab` MR/issue review (not MCP); 5× Composer 2.5 parallel passes (surface inventory, contracts, oracle-service, economics/tests, ACL/CI); `cargo audit`; `gitleaks detect` |
| **Tools** | `glab`, `cargo audit` (3 vulns / 5 warnings), `gitleaks` (no leaks) |

> Point-in-time internal review, not an external audit. Mainnet bytecode / treasury spender wiring must be verified on `columbus-5` before production decisions (#19).

---

## 1. Executive summary

MR `!12` is a **test-and-docs-only** merge: it does not change on-chain product behavior. It correctly closes the automated coverage gaps that remained after `#22` (oracle pause), `#23` (DeliverTx liveness), and `#25` (hardening bundle): M-8 minter map delete, BSC hang timeout, SIGTERM/`operator_loop` exit, equal-rate/policy-skip liveness regressions, oracle-pause integration, and a gated TEST-16 LocalTerra smoke.

**Verdict:** Regression posture for fail-closed oracle ops is materially stronger after `!12`. Residual risk for mainnet is dominated by **ops incompleteness (#19)**, **trusted single-EOA governance / minter privilege**, **monotonic-oracle economic assumptions** (pause is human-gated; `#27` rejected), **asymmetric deposit slippage** (`#26` rejected), and a **false-confidence gap in TEST-16** (when LCD is up the script re-runs wiremock cargo tests — it does not deploy wasm or exercise Live DeliverTx).

**Overall posture:** Mature relative to the closed `#5`–`#25` remediation track; **not production-complete** until Phase 5 ops (`add_minter`, treasury `SetCw20Spender`/`limit_24h`, live withdraw smoke, oracle-service host) land under `#19`.

---

## 2. Attack-surface inventory (expanded beyond request)

Beyond the areas requested (DeFi / SC attacks, DB leaks, e2e, ACL, privileges, Rust server, tokenomics, oracle manipulation), the following surfaces were identified and reviewed:

| # | Surface | Location | Why it matters |
|---|---------|----------|----------------|
| A1 | Treasury InstantWithdraw wire drift | `contracts/ust1-window/src/treasury.rs` | Wrong JSON ⇒ permanent redeem DoS |
| A2 | CW20 Receive spoofing | window + native-wrap | Fake token mint/burn if identity unchecked |
| A3 | UST1 minter privilege / stale `MINTERS` | `cw20-mintable` pin + `cw20_minter_integration.rs` | Unbounded mint if map/gov wrong |
| A4 | CosmWasm contract admin / empty `MigrateMsg` | all contracts `migrate` | Admin can swap code; migrate cannot retarget addresses |
| A5 | Hardcoded mainnet treasury default | `ust1_cmm::CMM_TREASURY_MAINNET` | Testnet/mis-instantiate risk |
| A6 | Bridge decimals (Terra 6 vs BSC 8) | `docs/DEPLOYMENT.md` Phase 3 | Scale mismatch ⇒ economic drain / stuck funds |
| A7 | Venus `exchangeRateStored` vs `RATE_SCALE` | oracle-service → oracle → window | Unit mismatch ⇒ systematic mispricing |
| A8 | Off-chain wall clock vs chain time | `main.rs` `now_unix()` vs `env.block.time` | Policy pre-check can disagree with chain |
| A9 | RPC collusion / `BSC_CONFIRMATION_BLOCKS=0` | `evm_rpc.rs`, `bsc.rs`, `config.rs` | Consensus theater; reorg reads |
| A10 | Key concentration | README `terra1xsecn…` gov+admin+bridge | Single compromise ⇒ full stack |
| A11 | Fee attribution vs custody | `fee_split` attributes only | Fees may accrue without extract path (wrap) |
| A12 | Rolling volume “day” (fixed 86400) | `ust1-window` `ensure_limits` | Burst at window edge (~2× advertised) |
| A13 | Deposit without `min_ust1_out` | `Cw20HookMsg::Deposit {}` | Oracle/fee race / adverse selection |
| A14 | Stub vs real treasury | multitest stubs + `real_treasury_integration` | False confidence if only stubs used |
| A15 | Supply chain | git deps, optimizer image, no `cargo audit` in CI | Vuln/drift land unnoticed |
| A16 | LocalTerra LCD/RPC exposure | `docker-compose.yml` `:1317`/`:26657` | Dev env exposure (not prod DB) |
| A17 | Attribute / event injection | response attributes | Indexer phishing (low) |
| A18 | Governance fee/limit griefing | `SetFeeBps(10000)`, zero limits | Economic DoS without pause |
| A19 | Cross-contract atomicity order | mint→transfer; burn→pull | Correct if both in same `Response` |
| A20 | No traditional SQL/NoSQL DB | oracle-service in-memory only | “DB leak” ⇒ env secrets + public chain |
| A21 | `/healthz` process-up only | `healthz.rs`, default `0.0.0.0:8080` | Ops may treat 200 as rate-fresh |
| A22 | `dotenvy::dotenv()` unconditional | `oracle-service/src/config.rs` | Planted `.env` can override prod secrets |
| A23 | HTTPS URL SSRF to private/metadata IPs | `validate_https_url` | Env write ⇒ lateral HTTP from worker |
| A24 | Equal-rate on-chain refresh | oracle accepts `new_rate == old_rate` | Keeps window “fresh” with stale economics |
| A25 | Shared deposit/withdraw rolling bucket | single `ROLLING` | Fee-paid cycling griefs honest users |
| A26 | UTC midnight +2% double-dip | `oracle_policy::roll_utc_day` | ~+4.04% in seconds at day boundary |
| A27 | Native wrap burn tax / dust wrap | `cmm-native-wrap` | Classic tax + zero-mint wrap loss |
| A28 | TEST-16 “e2e” false confidence | `scripts/localterra_e2e_smoke.sh` | LCD-up path still wiremock-only |
| A29 | Immutable window addresses | window `Config` / empty migrate | Wrong oracle/treasury needs redeploy |
| A30 | Duplicate oracle replicas | no leader election | Sequence races / delayed liveness |

---

## 3. Scope of MR `!12` and related issues

### 3.1 MR `!12` (merged)

**Title:** `test: close post-merge coverage gaps (#28)`  
**SHA:** `695e577` / merge `cf2c31a`  
**Intended security delta:** Automate proofs for already-shipped fail-closed behavior — **no on-chain product changes**.

| Gap closed | Evidence in tree |
|------------|------------------|
| M-8 `UpdateMinter(None)` clears `MINTERS` | `smartcontracts-terraclassic/tests/src/cw20_minter_integration.rs` |
| BSC hang ≤ timeout | `oracle-service/src/bsc.rs` `read_exchange_rate_stored_times_out_on_hanging_rpc` |
| SIGTERM / graceful loop exit | `operator_loop` + `shutdown_signal_with_hook` tests in `main.rs` |
| Equal-rate / policy-skip ≠ liveness success | `run_once_*_does_not_record_liveness`, `decide_tick_action_*` |
| Oracle pause → window fail-closed (integration) | `oracle_paused_blocks_deposit_and_withdraw_while_rate_fresh` in `integration_tests.rs` |
| TEST-16 LocalTerra gated smoke | `scripts/localterra_e2e_smoke.sh`, CI `localterra-e2e` (`when: manual`, `allow_failure: true`) |

### 3.2 Related issue status

| Issue | State | Security relevance |
|-------|-------|--------------------|
| `#28` | closed (by !12) | Coverage backfill — **this audit’s primary scope** |
| `#22` | closed | Oracle pause circuit breaker (INV-ORACLE-PAUSE-001) |
| `#23` | closed | DeliverTx + State before liveness (INV-ORACLE-LIVENESS-001) |
| `#25` | closed | Dust/decimals/timeouts/healthz/minter pin |
| `#21` | closed | InstantWithdrawCw20 schema pin (INV-SCHEMA-001) |
| `#24` | closed | Poll/silence vs max oracle age |
| `#20` | closed | InstantWithdraw redeem path |
| `#26` | **closed — rejected** | H-1 deposit `min_ust1_out` + fee cap — *“mitigated by admin mint to affected accounts”* |
| `#27` | **closed — rejected** | Remaining C-2 (rate reset / spot alerter / operator auto-trip) — *“pause sufficient; upgrade after evaluate”* |
| `#19` | **open** | Mainnet deploy/wiring — primary production gate |

---

## 4. Privilege matrix

| Role | Capabilities | Notes |
|------|--------------|-------|
| **Window / oracle / wrap governance** | Pause, fees, limits, max oracle age, operator rotation, propose gov | Two-step accept; **no timelock**; pending never expires |
| **Oracle operator** | `UpdateRate` only | Hot mnemonic in oracle-service; cannot pause |
| **CosmWasm contract admin** | `migrate` (empty msg) | Can replace code; cannot rewrite immutable addresses via migrate |
| **UST1 minter set** | Mint UST1 | Bootstrap: gov then `add_minter(window)` + drop gov minter (ops) |
| **vFDUSD minter** | Bridge only | CL8Y Terra bridge |
| **Treasury governance** | `SetCw20Spender`, `limit_24h` | External ustr-cmm; **mainnet incomplete (#19)** |
| **BSC / bridge admin** | TokenRegistry, LockUnlock | Separate EVM key |
| **Any user** | Deposit/withdraw/wrap hooks | Subject to pause, staleness, limits, fees |

Documented mainnet key concentration: `terra1xsecn…` holds admin/governance/bridge-admin roles (README / DEPLOYMENT).

---

## 5. Findings

Severity: **Critical / High / Medium / Low / Informational**.  
Status: **Open** unless noted remediated or **Accepted** (product rejected issue).

---

### F-01 — Mainnet redeem / minter wiring still ops-gated (#19)

| | |
|--|--|
| **Severity** | **High** |
| **Status** | Open |
| **Location** | `docs/DEPLOYMENT.md` Phase 5; README mainnet status |
| **Description** | Tokens/oracle/window instantiated, but `add_minter(window)`, window migrate + treasury `SetCw20Spender`/`limit_24h`, first confirmed `UpdateRate`, and live withdraw smoke remain pending. |
| **Impact** | Redeem dead or over-privileged until ops complete; mis-ops (spender limit too high / wrong window) enables over-pull. |
| **Recommendation** | Complete #19 checklist; keep `limit_24h` ≤ window rolling cap; one live withdraw probe before announcing. |
| **Tests** | In-repo schema + real-treasury multitest exist; **mainnet probe missing**. |

---

### F-02 — Single-EOA governance + minter privilege = full treasury drain

| | |
|--|--|
| **Severity** | **High** |
| **Status** | Open (design / ops) |
| **Location** | README roles; window/oracle gov paths; UST1 mint bootstrap |
| **Description** | One key path can pause, set fees/limits, rotate operator, migrate contracts, and (until minter dropped) mint unbacked UST1 redeemable via window. |
| **Impact** | Key compromise ⇒ complete inventory loss (bounded only by external treasury spender limit if set). |
| **Recommendation** | Multisig governance before meaningful TVL; complete `UpdateMinter(None)` on gov after `add_minter(window)`; consider timelock on fee/limit/minter. |
| **Tests** | ACL unit tests present; no multisig/timelock tests (N/A). |

---

### F-03 — Monotonic oracle + collateral collapse (human-gated pause only)

| | |
|--|--|
| **Severity** | **High** (economic) — partially mitigated |
| **Status** | **Accepted residual** (#27 rejected; #22 pause shipped) |
| **Location** | `ust1-common/src/oracle_policy.rs:56-58`; `ust1-oracle` `UpdateRate`; window `ensure_oracle_usable` |
| **Description** | Rate decreases hard-rejected. Pause (#22) fails window closed immediately when governance trips it. Oracle-service **does not** auto-`SetPaused` on observed decrease. No emergency rate reset / spot divergence alerter (#27 rejected: pause + upgrade path). |
| **Impact** | Until a human pauses, attacker can mint/redeem at stale high R using devalued vFDUSD — slow drain bounded by rolling limits (~10k UST1/day defaults) + treasury balance. After pause, markets freeze; reopen at toxic rate without upgrade. |
| **Recommendation** | Keep emergency pause runbook hot; monitor Venus/bridge/spot off-repo; document accepted risk in user-facing materials; reconsider operator auto-trip if TVL grows. |
| **Tests** | Pause fail-closed covered (multitest + integration). **No** economic collapse simulation. |

---

### F-04 — Deposit path lacks `min_ust1_out`; `fee_bps=10000` still legal

| | |
|--|--|
| **Severity** | **Medium** (High if governance untrusted) |
| **Status** | **Accepted** (#26 rejected) |
| **Location** | `contracts/ust1-window/src/msg.rs` `Deposit {}`; `exec_set_fee_bps` allows `<= 10000` |
| **Description** | Withdraw has `min_vfdusd_out`; deposit has none. Dust zero-output revert (INV-SWAP-003) blocks exact-zero mint, but not “user expected N, got dust.” Product rejection: *mitigate by admin mint to affected accounts*. |
| **Impact** | Mempool race with `SetFeeBps` / rate jump; compromised gov can grief depositors (recovery assumes honest admin). |
| **Recommendation** | If TVL grows, reopen H-1; until then document that depositors must accept rate/fee risk and that recovery is discretionary mint. |
| **Tests** | Dust deposit covered; **no** min-out / fee-cap tests. |

---

### F-05 — TEST-16 LocalTerra smoke is not a true wasm e2e

| | |
|--|--|
| **Severity** | **Medium** (process / coverage) |
| **Status** | Open (documented) |
| **Location** | `scripts/localterra_e2e_smoke.sh:31-55`; `.gitlab-ci.yml` `localterra-e2e` |
| **Description** | When LCD is down → SKIP exit 0. When up → re-runs **wiremock** oracle-service tests + prints a **manual** wasm checklist. `deploy_local.py` remains a stub. Job is `manual` + `allow_failure: true`. |
| **Impact** | Checklist may be marked “done” without ever exercising LCD DeliverTx or on-chain pause against a live LocalTerra wasm deploy. |
| **Recommendation** | Rename/clarify as “LCD-gated wiremock smoke”; promote to required only after automated store/instantiate; add real pause/DeliverTx probe. |
| **Tests** | Wiremock DeliverTx-reject covered always-on; live wasm path **missing**. |

---

### F-06 — Rolling fixed-window burst + shared deposit/withdraw bucket

| | |
|--|--|
| **Severity** | **Medium** |
| **Status** | Open |
| **Location** | `ust1-window/src/contract.rs` `ensure_limits` (~L106-125); deposit uses `ust1_out`, withdraw uses gross |
| **Description** | Window resets when `now >= window_start + 86400` on next swap (not sliding). Near-boundary burst can approach ~2× advertised daily notional. Single bucket shared across directions enables fee-paid cycling DoS. |
| **Impact** | Limit DoS / faster drain under F-03; contradicts “10k/day” shorthand. |
| **Recommendation** | Sliding window or document; consider per-direction buckets; align treasury `limit_24h` as hard ceiling. |
| **Tests** | Reset after 86400 covered; **boundary burst / grief cycle missing**. |

---

### F-07 — Equal-rate updates can refresh on-chain freshness without economic change

| | |
|--|--|
| **Severity** | **Medium** |
| **Status** | Open |
| **Location** | `ust1-oracle` `execute_update_rate` accepts equal rate; service skips equal-rate off-chain (`TickAction::SkipEqualRate`) |
| **Description** | Honest service skips broadcast when BSC rate equals stored rate. A compromised operator can still submit equal-rate `UpdateRate` after throttle to advance `last_update_sec`, keeping the window open during a collateral event. |
| **Impact** | Extends mispricing window while appearing fresh; compounds F-03. |
| **Recommendation** | On-chain reject `new_rate == old_rate`, or require minimum delta. |
| **Tests** | Service skip covered; **on-chain equal-rate refresh behavior not asserted as error**. |

---

### F-08 — UTC day-boundary +2% “double dip”

| | |
|--|--|
| **Severity** | **Medium** |
| **Status** | Open |
| **Location** | `ust1-common/src/oracle_policy.rs` `roll_utc_day` |
| **Description** | Daily baseline resets at UTC midnight; sequential updates across midnight can compound ~+4.04% within seconds while each update is in-policy. |
| **Impact** | Faster upward walk than “2%/day” docs imply (still needs operator). |
| **Recommendation** | Document boundary; or sliding 24h cap; add boundary vector test. |
| **Tests** | Same-day cap covered; **midnight double-dip missing**. |

---

### F-09 — Oracle-service env SSRF / `.env` override / chain-id not probed

| | |
|--|--|
| **Severity** | **Medium** |
| **Status** | Open |
| **Location** | `config.rs` `validate_https_url`, `dotenvy::dotenv().ok()`, `TERRA_CHAIN_ID` |
| **Description** | HTTPS enforced but private/metadata IPs not denied. Unconditional dotenv can override Render secrets. LCD chain-id not compared to configured `TERRA_CHAIN_ID`. |
| **Impact** | With env write / planted `.env`: wrong key, lateral HTTP, wrong-network confusion / prolonged outage. |
| **Recommendation** | Gate dotenv; deny RFC1918/link-local; fail-fast LCD chain-id mismatch. |
| **Tests** | HTTPS/dev-http partially covered; private-IP / dotenv / chain-id **missing**. |

---

### F-10 — Dependency vulnerabilities (`cargo audit`)

| | |
|--|--|
| **Severity** | **Medium** (supply chain) |
| **Status** | Open |
| **Location** | `Cargo.lock` |
| **Description** | `cargo audit` reports **3 vulnerabilities**: `curve25519-dalek` (RUSTSEC-2024-0344 via cosmwasm-crypto), `ruint` (RUSTSEC-2026-0220 via alloy), `rustls-webpki` (RUSTSEC-2026-0104); plus 5 unmaintained/unsound warnings (`derivative`, `paste`, `proc-macro-error2`, `anyhow`, `lru`). **Neither GitLab nor GitHub CI runs `cargo audit`/`cargo deny`.** |
| **Impact** | Mostly transitive; CosmWasm/alloy paths need periodic bump discipline. Silent drift without CI gate. |
| **Recommendation** | Add `cargo audit` (or `cargo deny`) to CI; track bumps for CosmWasm / alloy stacks. |
| **Tests** | None in CI. |

---

### F-11 — `/healthz` default bind + process-up semantics

| | |
|--|--|
| **Severity** | **Low** |
| **Status** | Open (documented) |
| **Location** | `healthz.rs`; `HEALTHZ_BIND` default `0.0.0.0:8080` |
| **Description** | Returns 200 with empty body; correctly **not** readiness for rate freshness. Public bind enables cheap probe/DoS if exposed. |
| **Impact** | Ops misread; reconnaissance. |
| **Recommendation** | Prefer `127.0.0.1` behind platform proxy; always pair with `LIVENESS_ORACLE_NO_BROADCAST` log paging. |
| **Tests** | `healthz_returns_200` present. |

---

### F-12 — SIGTERM does not cancel in-flight tick / healthz

| | |
|--|--|
| **Severity** | **Low** |
| **Status** | Open |
| **Location** | `main.rs` `operator_loop` docs + `healthz::spawn_healthz_server` |
| **Description** | Shutdown breaks the select loop; in-flight tick may finish; healthz task not gracefully stopped. Real OS SIGTERM not exercised in CI (hook only). |
| **Impact** | Extra broadcast on deploy; brief dual-instance races. |
| **Recommendation** | `CancellationToken` + axum graceful shutdown; optional real-signal smoke. |
| **Tests** | Hook exit covered (`operator_loop_exits_on_shutdown_hook`). |

---

### F-13 — `cmm-native-wrap` dust wrap / burn tax / locked fees

| | |
|--|--|
| **Severity** | **Medium** (wrap path; Phase 3 scope) |
| **Status** | Open |
| **Location** | `wrap.rs`, `unwrap.rs`, fee accounting |
| **Description** | Unwrap has `min_native_out`; wrap lacks zero-output / min-out. Terra Classic burn tax unmodeled on `BankMsg::Send`. Fees accrue in-contract with no sweep. |
| **Impact** | User loss / unwrap DoS / unrecoverable fee dust — relevant if wrap is deployed from this repo. |
| **Recommendation** | Mirror INV-SWAP-003; model tax; add `SweepFees`. |
| **Tests** | Happy wrap/unwrap present; dust/tax/sweep **missing**. |

---

### F-14 — Immutable window oracle/treasury/token addresses

| | |
|--|--|
| **Severity** | **Low–Medium** (ops) |
| **Status** | Open (by design) |
| **Location** | window `Config`; `migrate` only bumps cw2 + revalidates decimals |
| **Description** | Cannot retarget oracle/treasury/tokens in place. |
| **Impact** | Misconfigure ⇒ redeploy + user migration. |
| **Recommendation** | Document playbook; optional timelocked `SetOracle`/`SetTreasury` if product allows. |
| **Tests** | `migrate_preserves_config`. |

---

### F-15 — Pending governance has no expiry / cancel

| | |
|--|--|
| **Severity** | **Low** |
| **Status** | Open |
| **Location** | window/oracle/wrap `ProposeGovernance` / `AcceptGovernance` |
| **Description** | Mistaken proposal sticks until overwritten or accepted. |
| **Impact** | Rotation bricking if address wrong/lost. |
| **Recommendation** | Expiry or `CancelGovernance`. |
| **Tests** | Happy propose/accept only. |

---

## 6. Remediated / regression-closed since prior audits

| Prior ID | Topic | Current state | MR / issue |
|----------|--------|---------------|------------|
| C-1 | InstantWithdraw wire drift | **Remediated** — pin `e6c4b7cf…`, golden, real treasury, CI script | `#21` / !10 |
| C-2 #1 | Oracle pause fail-closed | **Remediated** — `State.paused` + window check first | `#22` / !7 |
| C-3 | CheckTx-only liveness | **Remediated** — DeliverTx + State confirm | `#23` / !2 |
| H-3 | Poll == max age / late silence | **Remediated** — poll 1h, silence 6h | `#24` / !4 |
| H-5 | GitLab secrets-only CI | **Remediated** — `rust` job fmt/clippy/test | `.gitlab-ci.yml` |
| M-2/M-3 | Dust zero-output | **Remediated** — INV-SWAP-003/004 | `#25` |
| M-8 | `UpdateMinter(None)` map | **Remediated** + **in-repo tested** | `#25`/`#28` / !12 |
| M-12/M-19 | BSC hang / network tests | **Remediated** + hang test | `#25`/`#28` / !12 |
| L-10 | SIGTERM | **Remediated** (hook; not real OS signal) | `#25`/`#28` / !12 |
| L-16 | `/healthz` claim | **Remediated** (process-up, documented) | `#25` |
| L-21 | Decimal inversion | **Remediated** — INV-DECIMALS-001 | `#25` |
| Skip-path liveness lie | Equal-rate/policy skip | **Remediated** | `#28` / !12 |
| Pause integration gap | multitest-only | **Remediated** — integration suite | `#28` / !12 |

---

## 7. Attack applicability matrix

| Attack class | Applies? | Notes |
|--------------|----------|-------|
| Reentrancy | **No** | CosmWasm atomic submessages; burn→pull same `Response` |
| Flash loan price oracle | **No** | Stored rate, not intra-tx AMM |
| Share inflation / first depositor | **N/A** | Fixed-rate mint/burn, not LP shares |
| Donation attack | **Low** | Treasury donation ≠ mint; helps withdrawors |
| Sandwich / front-run | **Partial** | Withdraw has min-out; **deposit does not** |
| Governance rug | **Yes** | Single EOA; fee/limits/migrate/mint |
| Infinite mint | **Gov-trusted** | Window mints on deposit; gov minter until dropped |
| Oracle manipulation (on-chain) | **Partial** | Operator + policy caps |
| Oracle manipulation (RPC/Venus) | **Yes** | Consensus 2-of-N, reorg depth, Venus trust |
| Monotonic rate trap / depeg drain | **Yes (design)** | Pause mitigates after human action |
| Rolling limit bypass | **Yes (edge)** | Fixed-window burst / shared bucket |
| Pause bypass | **Mitigated** | Pause checked before staleness |
| CheckTx false liveness | **Mitigated** | DeliverTx + State |
| Schema wire drift | **Mitigated** | INV-SCHEMA-001 |
| Minter map stale entry | **Mitigated** | INV-MINTER-001 + !12 tests |
| Dust zero-output theft | **Mitigated** | INV-SWAP-003/004 |
| Decimal mismatch | **Mitigated** | INV-DECIMALS-001 |
| SQL/DB leak | **N/A → env secrets** | No DB; mnemonic/RPC keys in env |
| Cross-chain bridge exploit | **Out of repo** | Trust boundary |
| Signature replay | **No** | Cosmos sequence |

---

## 8. Test coverage matrix (INV / attack)

| ID / attack | Covered? | How |
|-------------|----------|-----|
| INV-MATH-001 / SWAP-001/002 | Yes | `ust1-common` math + vectors |
| INV-SWAP-003/004 | Yes | window multitest dust cases |
| INV-LIMIT-001 | Yes | per-tx / rolling multitest |
| INV-WITHDRAW-001/002 | Yes | InstantWithdraw + atomic reject |
| INV-SCHEMA-001 | Yes | treasury unit + golden + CI script + real treasury |
| INV-ORACLE-THROTTLE/DAILY/MONO | Yes | common + oracle multitest |
| INV-ORACLE-PAUSE-001 | Yes | multitest **and** integration (!12) |
| INV-ORACLE-LIVENESS-001 | Yes | DeliverTx fail / state mismatch / timeout / skip paths (!12) |
| INV-ORACLE-OPS-POLL/SILENCE | Partial | Default/config tests; no live misconfig e2e |
| INV-DECIMALS-001 | Yes | instantiate/migrate |
| INV-MINTER-001 | Yes | `cw20_minter_integration` (!12) |
| INV-ORACLE-TICK-001 | Yes | BSC hang + tick timeout |
| INV-ORACLE-HEALTHZ-001 | Yes | healthz 200 + docs |
| Deposit slippage | **No** | Accepted #26 |
| Collapse economics | **No** | |
| Rolling 2× burst / shared grief | **No** | |
| UTC midnight double +2% | **No** | |
| Equal-rate on-chain refresh | **No** | |
| LocalTerra wasm pause/DeliverTx | **No** | Checklist only |
| `cargo audit` in CI | **No** | |
| SIGTERM real OS signal | **No** | Hook only |

Approximate suite size: ~169 named tests across workspace crates (oracle-service largest).

---

## 9. Database / secrets leak class

There is **no SQL/NoSQL database** in this monorepo.

| Asset | Storage | Risk |
|-------|---------|------|
| Operator mnemonic | Env / Render secrets / optional `.env` | Host compromise = UpdateRate key |
| BSC/LCD URLs (API keys) | Env; LCD credentials redacted in errors | Log leakage partially mitigated |
| Liveness tracker | Process memory only | Lost on restart (by design) |
| Chain state | Public LCD/RPC | Expected transparency |
| `/healthz` | Empty 200 | No secret disclosure |
| LocalTerra volume | Docker volume | Dev-only |

Controls: `SecretString`, Debug redaction test, `.env*` gitignore, gitleaks + GitLab secret detection, pre-commit gitleaks.

---

## 10. E2E / CI status

| Layer | Status |
|-------|--------|
| Always-on GitLab `rust` | fmt + clippy `-D warnings` + `cargo test` + treasury schema script |
| Always-on GitHub CI | gitleaks + same rust checks (+ optional localterra) |
| Secret detection | GitLab template + gitleaks |
| `localterra-e2e` | Manual, allow_failure; SKIP if LCD down; wiremock when up |
| Mainnet ops probes | Runbook only (#19) |
| SAST / `cargo audit` / dependency scanning | **Not in CI** |

---

## 11. Positive controls observed

1. **Fail-closed oracle usability** — pause before staleness (`ensure_oracle_usable`).
2. **Atomic withdraw** — burn then InstantWithdrawCw20 in one `Response`.
3. **Strict treasury wire pin** — rev `e6c4b7cf…`, golden negatives, `deny_unknown_fields`, real treasury multitest.
4. **Shared checked math** — floors, proptests, dust guards.
5. **Oracle policy** — mono + 4h throttle + 2% UTC day; mirrored off-chain.
6. **Separation of duties** — operator ≠ governance for pause.
7. **CW20 hook hardening** — only configured tokens; beneficiary = cw20 `sender`.
8. **Oracle-service fail-closed liveness** — DeliverTx + State; skip paths do not record success.
9. **BSC timeouts + multi-RPC consensus + chainId + canonical vToken**.
10. **HTTPS URL gate** (dev loopback escape).
11. **Ops timing defaults** aligned with window max age (#24).
12. **Poisoned mutex recovery**; adaptive gas; account parse fail-hard.
13. **Skills + DEPLOYMENT invariants** cross-linked for ops/auditors.
14. **GitLab rust CI** closes prior merge-host gap.

---

## 12. Residual risks after MR !12 (priority)

| P | Item | Finding |
|---|------|---------|
| P0 | Finish #19 mainnet wiring + live withdraw smoke | F-01 |
| P0 | Multisig / drop gov minter before TVL | F-02 |
| P1 | Keep pause runbook + off-repo spot/Venus monitoring (accepted #27) | F-03 |
| P1 | Clarify/fix TEST-16 into real wasm e2e or rename | F-05 |
| P2 | Document/accept or fix deposit min-out (#26) | F-04 |
| P2 | Rolling window burst + shared bucket | F-06 |
| P2 | Reject equal-rate on-chain refresh | F-07 |
| P2 | `cargo audit` in CI + dependency bumps | F-10 |
| P2 | Dotenv gate / private-IP deny / LCD chain-id check | F-09 |
| P3 | Midnight double-dip test/docs; wrap tax/dust; gov expiry; healthz bind | F-08, F-11–F-15 |

---

## 13. MR !12 acceptance criteria — audit verification

| AC from #28 / !12 | Verified in tree? |
|-------------------|-------------------|
| M-8 minter integration | **Yes** — `inv_minter_001_*` |
| BSC hang timeout test | **Yes** — `bsc.rs` hang wiremock |
| SIGTERM / graceful shutdown coverage | **Yes** — injectable hook (not real OS signal; documented out of scope) |
| Equal-rate / policy-skip no liveness | **Yes** — `run_once_*` + `decide_tick_action_*` |
| Oracle pause integration | **Yes** — `ust1-integration-tests` |
| LocalTerra / TEST-16 documented + CI-gated | **Yes** — but **not** a full wasm e2e (F-05) |
| Skills/README updated; cargo green | Claimed in MR; not re-run as full suite in this audit pass |

---

## 14. Method notes

- Used **`glab` CLI** for MR !12 and issues `#19`–`#28` (no GitLab MCP).
- Composer 2.5 subagents: [Explore security surfaces](e52865f3-79b1-4e0a-9dfe-b5cbf824db46), [Audit smart contracts](7aa1b0bc-8646-47ff-b6f0-b403e3518348), [Audit oracle Rust service](c06d6a38-186f-4d03-a010-c6ca5f0b94b4), [Audit economics and tests](063235af-76e7-41cb-b61d-4d56d5991f40), [Audit ACL and CI/ops](ee98ec27-def8-49fa-8afc-b6b69eb9fa43).
- Prior audits consulted: `INTERNAL_GROKCODE_1786158683.md`, `INTERNAL_KIMIK3_1786162831.md`.
- No repository code was modified for this audit beyond adding this report file.

---

## 15. Addendum — supplemental items from parallel review passes

Merged after Composer 2.5 pass completion; severity calibrated to the main report (not all “Critical” labels from individual passes were retained where #22/#26/#27 already change residual risk).

| ID | Severity | Title | Location | Note |
|----|----------|-------|----------|------|
| F-16 | Medium | Duplicate `BSC_RPC_URLS` satisfies 2-of-N consensus | `config.rs`, `evm_rpc.rs` | Same URL twice “agrees”; reject duplicates at load |
| F-17 | Medium | BSC RPC URLs (API keys) leak in error strings | `bsc.rs` eyre paths; tick `warn!(error=%e)` | LCD redaction exists; BSC paths do not |
| F-18 | Medium | `eth_chainId` cached forever per URL | `bsc.rs` `CHAIN_ID_CACHE` | Re-verify on TTL or consensus mismatch |
| F-19 | Low | `ORACLE_CONTRACT` not bech32-validated at startup | `config.rs` | Fail-fast misdeploy |
| F-20 | Low | Optimizer image tag-pinned, not digest | `scripts/optimize.sh` | Pin digest like LocalTerra compose |
| F-21 | Low | `deploy_local.py` stub may print mnemonic placeholder | `scripts/deploy_local.py` | Stub-only; avoid secret-shaped stdout |
| F-22 | Info | `#19` issue body still mentions `increase_allowance` | GitLab #19 | Update to InstantWithdraw / `SetCw20Spender` wording |

---

*End of report `INTERNAL_GROKCODE_1786183770`.*
