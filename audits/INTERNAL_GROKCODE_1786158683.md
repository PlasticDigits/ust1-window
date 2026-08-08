# Internal Security Audit — ust1-window

| Field | Value |
|-------|--------|
| **ID** | `INTERNAL_GROKCODE_1786158683` |
| **Date** | 2026-08-08 |
| **Auditor** | Cursor Grok 4.5 (internal) |
| **Primary scope** | GitLab MR [`!1`](https://gitlab.com/PlasticDigits/ust1-window/-/merge_requests/1) (merged) — InstantWithdrawCw20 redeem |
| **Related issues** | `#20` (closed), `#19` (open Phase 2 deploy), prior security `#5`–`#14`, product `#1`–`#4`, `#16`–`#18` |
| **Repo HEAD at audit** | `main` @ `5b1e719` (merge of `feat/instant-withdraw-cw20`) |
| **External deps in trust boundary** | ustr-cmm Treasury (`InstantWithdrawCw20` / `SetCw20Spender`), CL8Y bridge, Venus vToken on BSC, cw20-mintable |

---

## 1. Executive summary

MR `!1` correctly removes the broken CW20 `TransferFrom` + allowance redeem model and replaces it with treasury `InstantWithdrawCw20`, with multitest coverage for zero-allowance happy path and atomic reject (**INV-WITHDRAW-001/002**). That is a **security improvement** for mainnet custody.

Residual risk is dominated by **ops incompleteness** (#19 Phase 5: migrate + `SetCw20Spender` + smoke still open), **trust in external treasury bytecode** (not tested against real ustr-cmm), **concentrated privileged keys**, **oracle monotonic / single-operator economics**, and **test gaps** around economic attacks, deposit slippage, rolling-window boundary games, and true e2e/fork testing.

**Overall posture (contracts + oracle-service in this repo):** Mature relative to the closed `#5`–`#14` remediation track; **not yet production-complete** until Phase 5 ops and treasury wire parity are verified on-chain.

---

## 2. Attack-surface inventory (expanded beyond request)

Beyond the areas requested (DeFi / SC attacks, DB leaks, e2e, ACL, Rust server, tokenomics, oracles), the following surfaces were identified and reviewed:

| # | Surface | Location | Why it matters |
|---|---------|----------|----------------|
| A1 | Treasury execute client wire drift | `contracts/ust1-window/src/treasury.rs` | Wrong JSON enum → permanent redeem DoS or unexpected treasury behavior |
| A2 | CW20 Receive spoofing | window + native-wrap | Fake token could mint/burn if token identity not checked |
| A3 | UST1 minter privilege | cw20-mintable `AddMinter(window)` | Compromised window admin/migrate ⇒ unbounded mint |
| A4 | CosmWasm contract admin / migrate | empty `MigrateMsg` | Admin can swap code; migrate cannot repair wrong addresses |
| A5 | Hardcoded mainnet treasury default | `ust1_cmm::CMM_TREASURY_MAINNET` | Testnet/mis-instantiate risk |
| A6 | Bridge ↔ decimals (6 vs 8) | DEPLOYMENT Phase 3 | Wrong scale ⇒ economic drain / stuck funds |
| A7 | Venus rate semantics vs `RATE_SCALE` | oracle-service → oracle → window | Unit mismatch ⇒ systematic mispricing |
| A8 | Off-chain vs on-chain time | `now_unix()` vs `env.block.time` | Policy pre-check can disagree with chain |
| A9 | RPC collusion / confirmation=0 | `evm_rpc.rs`, `BSC_CONFIRMATION_BLOCKS` | Consensus theater; reorg reads |
| A10 | Key concentration | `terra1xsecn…` gov+admin+bridge | Single compromise ⇒ full stack |
| A11 | Fee attribution vs custody | fee_split attributes only | Fees may accrue without extract path (wrap) |
| A12 | Rolling volume “day” semantics | fixed 86400 from first swap | Burst at window edge |
| A13 | Deposit without `min_out` | window `Deposit {}` | Oracle move / MEV sandwich on mint |
| A14 | Stub vs real treasury | multitest stubs | False confidence on spender/limit enforcement |
| A15 | Supply chain | git deps + CI secret detection only | No SAST/dependency scan in GitLab CI beyond secrets |
| A16 | LocalTerra volume / LCD exposure | `docker-compose.yml` | Dev env; not prod DB |
| A17 | Attribute / event injection | response attributes | Indexer phishing (low) |
| A18 | Governance fee/limit griefing | `SetFeeBps(10000)`, zero limits | Economic DoS without pause |
| A19 | Cross-contract atomicity order | mint→transfer; burn→pull | Correct if both in same `Response` |
| A20 | No traditional SQL/NoSQL DB | oracle-service in-memory only | “DB leak” maps to env secrets + public chain state |

---

## 3. Scope of MR `!1` and related issues

### 3.1 MR `!1` (merged)

**Title:** `feat(window): redeem via treasury InstantWithdrawCw20 (#20)`

**Intended security delta:**

- Remove allowance / `TransferFrom` dependency on treasury (EOA-style allowance invalid for contract custody).
- Emit `InstantWithdrawCw20 { recipient, token, amount }` to `cmm_treasury`.
- Keep burn + pull atomic; document **INV-WITHDRAW-001/002**.
- Ops docs: migrate window + treasury `SetCw20Spender` / `limit_24h`.

**Unchecked MR test-plan items (ops / external verification):**

- [ ] Wire JSON matches live ustr-cmm treasury
- [ ] Mainnet migrate `terra1zxwp…`
- [ ] Treasury gov `SetCw20Spender` + `limit_24h`
- [ ] Small mainnet withdraw smoke (`Transfer` not `TransferFrom`)

### 3.2 Related issues

| Issue | State | Security relevance |
|-------|-------|--------------------|
| #20 | closed | Spec for InstantWithdraw path — implemented in `!1` |
| #19 | **open** | Phase 5 withdraw still blocked on ops; primary production gate |
| #5 | closed | Top-20 CW + EVM checklist parent |
| #6 | closed | Oracle staleness on window — mitigated (`max_oracle_age_sec`) |
| #7–#13 | closed | Oracle-service: consensus, reorg depth, Venus allowlist, HTTPS, liveness, Debug mnemonic, chainId |
| #14 | closed | `cw20-mintable` now pinned to `rev` in workspace `Cargo.toml` |
| #16–#18 | closed | Native wrap + fee split + reverse swap vectors |

---

## 4. Findings

Severity: **Critical / High / Medium / Low / Informational**.  
Status: **Open** unless noted remediated in-repo.

---

### F-01 — Mainnet redeem still ops-gated (spender + migrate)

| | |
|--|--|
| **Severity** | **High** (availability / funds stuck until ops; mis-ops can enable over-pull) |
| **Status** | Open (#19) |
| **Component** | Deployment / ustr-cmm / window |

**Description:** Code on `main` implements InstantWithdraw, but live redeem requires (1) window wasm migrate, (2) treasury `SetCw20Spender` with `limit_24h` (fail-closed per ustr-cmm#7), (3) UST1 `add_minter(window)`. Until then, users cannot redeem via the intended path; if spender is set with an oversized `limit_24h` without window limits, treasury inventory risk rises.

**Evidence:** `docs/DEPLOYMENT.md` Phase 5; MR `!1` unchecked ops boxes; README “post-deploy wiring … still pending”.

**Recommendation:** Complete Phase 5 checklist; set treasury `limit_24h` ≤ product rolling cap (e.g. 10_000 UST1-equivalent); document dual-cap monitoring.

---

### F-02 — Stub treasury tests do not enforce real spender / limit policy

| | |
|--|--|
| **Severity** | **High** (test gap → false assurance) |
| **Status** | Open |
| **Component** | `multitest.rs`, `stub_treasury.rs`, missing ustr-cmm integration |

**Description:** Stubs accept any `InstantWithdrawCw20` (optional hard reject) and emit CW20 `Transfer`. They do **not** model:

- `info.sender` must be registered spender
- per-token `limit_24h` accounting / fail-closed unset limit
- token allowlist
- pause on treasury

**Impact:** INV-WITHDRAW-* proven only against a toy treasury. Wire-name unit test (`instant_withdraw_cw20` snake_case) does not prove field parity with ustr-cmm.

**Recommendation:** Add multitest/integration against published ustr-cmm treasury wasm (or shared msg crate + golden JSON from ustr-cmm CI). Keep a contract-level test that unregistered caller cannot pull.

---

### F-03 — Privileged key concentration (gov + CW20 admin + bridge admin)

| | |
|--|--|
| **Severity** | **High** |
| **Status** | Open (ops / architecture) |
| **Component** | Mainnet key layout (`terra1xsecn…`) |

**Description:** Same address is documented as Terra admin, governance, and bridge admin. Compromise or malicious governance enables: migrate contracts, change UST1 minters, pause/oracle operator, bridge mint mappings, treasury spender registration (via treasury gov).

**Recommendation:** Split roles (multisig per role); time-locks on migrate / `SetCw20Spender` / `AddMinter`; hardware isolation for oracle operator key.

---

### F-04 — Oracle monotonicity + single operator → directional economic risk

| | |
|--|--|
| **Severity** | **High** (economic / oracle design) |
| **Status** | Open (by design; mitigated partially) |
| **Component** | `ust1-oracle`, `oracle_policy.rs`, window math |

**Description:** On-chain rate is **non-decreasing** (`INV-ORACLE-MONO-001`), throttled (4h), capped (+2%/UTC day). If Venus exchange rate falls while the on-chain rate stays high:

- **Deposit** mints **more** UST1 per vFDUSD than fair real rate.
- **Withdraw** returns **less** vFDUSD per UST1.

Net: UST1 can become over-issued relative to fair Venus claim; treasury may be drained over time via preferential deposits (bounded by per-tx / 24h limits and fees). Compromised `oracle_operator` can also walk the rate up to the daily cap repeatedly (slow rug via over-mint).

**Mitigations present:** throttle, daily cap, pause, window staleness (`DEFAULT_MAX_ORACLE_AGE_SECS` = 6h), volume limits.

**Gaps:** No decrease path / emergency rate reset; no deviation circuit-breaker vs last Venus read on-chain; no multi-operator.

**Recommendation:** Governance emergency `SetRate` or bounded decrease under pause; alert on `|venus - onchain|` divergence; consider dual-operator / multisig operator; economic fuzz tests (see T-gaps).

---

### F-05 — Deposit path lacks slippage protection (`min_ust1_out`)

| | |
|--|--|
| **Severity** | **Medium** |
| **Status** | Open |
| **Component** | `Cw20HookMsg::Deposit {}` (`msg.rs`, `contract.rs`) |

**Description:** Withdraw has `min_vfdusd_out`; deposit has none. Between user quote and inclusion, oracle `UpdateRate` (or stale→fresh jump) can worsen mint output. Not classic AMM sandwich, but still adverse selection / griefing.

**Recommendation:** Add `Deposit { min_ust1_out: Uint128 }` (breaking hook change) or document that depositors must size/accept rate risk and use tight off-chain checks.

---

### F-06 — Rolling 24h limits are not sliding windows

| | |
|--|--|
| **Severity** | **Medium** |
| **Status** | Open |
| **Component** | `ensure_limits` in window + native-wrap |

**Description:** Volume resets when `now >= window_start_sec + 86400` on the **next** swap. An attacker can consume nearly a full rolling cap just before reset and again immediately after → up to ~2× advertised 24h notional in a short interval.

**Recommendation:** Document as known; or use sliding buckets / epoch rings; optionally lower treasury `limit_24h` as hard ceiling.

---

### F-07 — Empty migrate cannot fix misconfigured addresses

| | |
|--|--|
| **Severity** | **Medium** |
| **Status** | Open (design) |
| **Component** | `migrate` in window / oracle / wrap |

**Description:** `MigrateMsg {}` only bumps cw2 version. Wrong `oracle`, `cmm_treasury`, or token addresses require new instantiate (or a future migrate that mutates `CONFIG`). Admin who can migrate can still replace code with a malicious binary that ignores this — migrate privilege is the real control plane.

**Recommendation:** Keep admin on multisig; consider explicit `MigrateMsg` fields for emergency retarget with events; never set admin to a hot key.

---

### F-08 — Governance can set `fee_bps = 10000` or zero limits (griefing)

| | |
|--|--|
| **Severity** | **Medium** |
| **Status** | Open |
| **Component** | `SetFeeBps`, `SetLimits` |

**Description:** Validation allows `fee_bps <= 10000`. 100% fee zeroes user out; zero limits DoS deposits/withdraws without `paused` (harder for indexers to spot). Trusted-gov assumption.

**Recommendation:** Bound fee (e.g. ≤ 500 bps) and reject zero limits unless paired with pause; emit clear events (already partial for fee).

---

### F-09 — Oracle-service residual off-chain risks

| | |
|--|--|
| **Severity** | **Medium** (aggregate) |
| **Status** | Partially remediated (#7–#13); residuals open |
| **Component** | `oracle-service/` |

| Residual | Detail |
|----------|--------|
| Liveness | Log-only `LIVENESS_ORACLE_NO_BROADCAST`; no pager/metrics sink |
| Confirmations | `BSC_CONFIRMATION_BLOCKS` defaults 15 but can be set to `0` |
| Consensus theater | Two RPCs under one operator still “agree” |
| Broadcast | `BROADCAST_MODE_SYNC` — not waiting for commit/inclusion proof |
| Time skew | Off-chain `now_unix()` for policy pre-check vs chain time |
| Secrets | Mnemonic via env/`SecretString` (Debug redacted — good); still host compromise = operator |
| No HTTP API | No public HTTP attack surface (good) |
| No DB | No SQL leak class; process memory + env only |

**Recommendation:** Enforce min confirmation blocks in code for mainnet; require distinct RPC ASNs/providers in runbook; switch to block inclusion wait; wire liveness to alerting; HSM/KMS for mnemonic.

---

### F-10 — Native wrap fee custody with no sweep path

| | |
|--|--|
| **Severity** | **Medium** (if wrap deployed from this repo) |
| **Status** | Open / deprioritized (#19 says use ustr-cmm wrap-mapper) |
| **Component** | `cmm-native-wrap` |

**Description:** Wrap keeps native in contract and mints after fee; unwrap burns and sends after fee. Fee remainder accrues in-contract with **no** `WithdrawFees` / treasury forward. Funds are not stolen by users but can be stuck or silently used as unwrap liquidity. Round-trip pays fee twice.

**Recommendation:** If this contract is ever mainnetted: explicit fee destination; if not, keep marked obsolete vs ustr-cmm.

---

### F-11 — Hardcoded mainnet treasury default on instantiate

| | |
|--|--|
| **Severity** | **Low / Medium** |
| **Status** | Open |
| **Component** | `instantiate` + `CMM_TREASURY_MAINNET` |

**Description:** Omitting `cmm_treasury` binds to mainnet treasury address. Local/test instantiations that forget `Some(treasury)` silently point at mainnet custody address (harmless on LocalTerra for transfers that fail, dangerous if somehow used with real tokens on wrong network assumptions).

**Recommendation:** Require explicit treasury in instantiate (no default) for non-mainnet; or gate default on chain-id (not available cleanly in CosmWasm without env).

---

### F-12 — CI security coverage thin

| | |
|--|--|
| **Severity** | **Low** |
| **Status** | Open |
| **Component** | `.gitlab-ci.yml`, `.github/workflows/ci.yml` |

**Description:** GitLab CI enables Secret Detection only. No in-repo job visible here for `cargo audit`, wasm reproduciblity attestation, or full checklist automation from #5. Pre-commit has gitleaks.

**Recommendation:** Add `cargo deny`/`audit`, pinned optimizer digests, and a CI job running contract multitests.

---

### F-13 — Deposit message ordering / reentrancy (CosmWasm)

| | |
|--|--|
| **Severity** | **Informational** (acceptable) |
| **Status** | Accept risk |
| **Component** | window `deposit` / `withdraw` |

**Description:** Classic EVM reentrancy is limited by CosmWasm’s atomic submessage execution. Deposit: mint UST1 then `Transfer` vFDUSD to treasury (window already received vFDUSD via `Send`). Withdraw: burn then InstantWithdraw. Failure of either submsg reverts storage (rolling volume included) — covered by `withdraw_treasury_reject_is_atomic_no_ust1_burn`.

**Residual:** Malicious CW20 token (if governance set wrong token address) could behave adversarially; mitigated by instantiate-time token binding + CW20 sender checks.

---

### F-14 — Prior findings #6–#14 appear addressed in current tree

| Issue | Topic | Current evidence |
|-------|-------|------------------|
| #6 | Oracle staleness | `ensure_oracle_fresh`, `SetMaxOracleAge`, tests |
| #7 | Multi-RPC consensus | `evm_rpc::run_with_evm_rpc_rate_consensus` (≥2 URLs) |
| #8 | Reorg / latest | `confirmation_blocks` + historical block read |
| #9 | Venus allowlist | Canonical address when chain allowlist is only `56` |
| #10 | HTTPS URLs | `validate_https_url` + `DEV_ALLOW_HTTP` localhost escape |
| #11 | Liveness | `LivenessTracker` + error log (no external sink) |
| #12 | chainId | `verify_bsc_rpc_chain_id` / startup verify all URLs |
| #13 | Debug mnemonic | `secrecy::SecretString` + unit test |
| #14 | cw20-mintable pin | `rev = "73a206b5…"` in workspace |

---

## 5. Access control & privileges matrix

| Actor | Powers | Notes |
|-------|--------|-------|
| Window governance | limits, pause, fee, max oracle age, propose gov | Two-step accept |
| Oracle governance | operator, pause, propose gov | Two-step accept |
| Oracle operator | `UpdateRate` under policy | Hot key; economic leverage |
| CW20 admin | migrate token code, mint config | = gov address on mainnet docs |
| UST1 minters | mint UST1 | gov + (pending) window |
| vFDUSD minter | bridge | Bridge compromise ⇒ unbacked vFDUSD |
| Treasury gov (external) | `SetCw20Spender`, inventory policy | Critical for F-01 |
| Contract admin | migrate wasm | Empty MigrateMsg today |
| Anyone | CW20 Send deposit/withdraw if not paused | Subject to limits/oracle |

**Missing controls:** No on-chain timelock; no guardian role separate from gov; no rate circuit-breaker; no deposit slippage; no `SetTreasury`/`SetOracle` (immutability vs repair tradeoff).

---

## 6. Smart-contract / DeFi attack checklist

| Attack class | Assessment |
|--------------|------------|
| Reentrancy (EVM-style) | Low risk under CosmWasm atomicity; burn-then-pull / mint-then-forward tested |
| CW20 spoofing | Mitigated (`info.sender` must be configured token) — tested |
| Unauthorized mint | Requires minter role; window only mints on valid deposit |
| Allowance drain | **Removed** by MR `!1` (positive) |
| Flash-loan oracle | On-chain rate not flash-updatable (4h + daily cap); Venus read uses lagged block |
| Sandwich / slippage | Withdraw protected; **deposit not** (F-05) |
| Donation / inflation attacks | N/A AMM; treasury inventory separate |
| Pause bypass | Receive checks `paused` first — tested |
| Limit bypass | Per-tx + rolling checked; boundary game F-06 |
| Governance takeover | Two-step; no delay |
| Migrate malicious code | Admin trust (F-03/F-07) |
| Integer overflow | `checked_*` / `Uint256` paths in math |
| Rounding theft | Floor favors protocol; fee split is attribution-only |
| Economic round-trip | Fee + floor loss; no arb invariant test suite |
| Oracle stuck / stale | Window blocks; liveness log-only |
| Native denom spoofing (wrap) | Exactly one coin; denom must match pair — tested |
| Cross-chain bridge mint | Out of repo; high trust (decimals 6↔8) |

---

## 7. “Database leaks” and secrets

There is **no application database**. Analogues:

| Asset | Risk | Mitigation / gap |
|-------|------|------------------|
| `TERRA_MNEMONIC` | Full oracle operator compromise | `SecretString`; do not log; host secrets |
| LCD / RPC responses | Integrity of rate & account sequence | HTTPS required; multi-RPC; still trust LCD for broadcast |
| CosmWasm state | Public by design | No private user PII |
| LocalTerra volume | Dev chain state | Not production |
| CI secrets | Leak via logs | gitleaks + GitLab secret detection |

---

## 8. Tokenomic / economic attack notes

1. **Fee on UST1 leg both ways** — round-trip always loses; reduces closed-loop arb unless external UST1 price diverges.
2. **External UST1 secondary market** (future DEX) — if market price ≠ oracle window, inventory arb against treasury up to limits (intended design; limits are the safety valve).
3. **Monotonic oracle lag** — see F-04.
4. **Treasury insolvency** — withdraw checks treasury vFDUSD balance; users fail closed (`InsufficientVfdusd`); UST1 can trade illiquid if inventory empty.
5. **Spender limit vs window limit** — defense in depth only after ops (F-01).
6. **Bridge-minted vFDUSD** — unbacked Terra vFDUSD if bridge broken ⇒ deposits mint unbacked UST1.

---

## 9. Test coverage assessment

### 9.1 Inventory (automated)

| Package | Approx. tests | Focus |
|---------|---------------|-------|
| `ust1-window` multitest | 13 | InstantWithdraw, limits, pause, spoof CW20, stale oracle, atomic reject, migrate |
| `ust1-integration-tests` | 8 | Round-trip, fee gov, oracle throttle, EffectiveSwap, daily cap property |
| `ust1-oracle` multitest | 8 | Policy INV-*, ACL, two-step gov |
| `ust1-common` | 11 | Math vectors INV-SWAP-002, fee props, oracle policy |
| `cmm-native-wrap` multitest | 12 | Wrap/unwrap, limits, spoof, pause, fee split attrs |
| `ust1-oracle-service` | ~9 unit | Venus validation, Debug redact, RPC agree math |

### 9.2 Happy / bad paths (window redeem — MR `!1`)

| Path | Covered? |
|------|----------|
| Deposit → withdraw, zero allowance | Yes |
| Wire shape `instant_withdraw_cw20` | Yes (string contains) |
| Treasury reject atomic (no burn) | Yes (stub) |
| Insufficient treasury balance | Yes |
| Below `min_vfdusd_out` | Yes |
| Per-tx / rolling limit | Yes |
| Pause | Yes |
| Fake CW20 | Yes |
| Stale oracle | Yes (deposit) |
| Unauthorized gov | Partial (`SetMaxOracleAge` only) |
| Real ustr-cmm spender/limit | **No** |
| Mainnet / LocalTerra e2e smoke | **No** (ops pending) |
| Deposit slippage | **No** |
| Oracle economic attack sequences | **No** |
| Rolling boundary 2× burst | **No** |
| Fee 100% grief | **No** |
| Stale oracle on withdraw | Implicit same helper; no dedicated test |
| Concurrent txs / sequence races (service) | **No** |

### 9.3 E2E

- **cw-multi-test** = in-process e2e substitute (good unit/integration hybrid).
- **No** dockerized LocalTerra CI job exercising InstantWithdraw against real treasury wasm.
- **No** mainnet fork test harness.

---

## 10. Missing security features (summary)

1. Deposit `min_out` / deadline  
2. On-chain oracle deviation / emergency decrease  
3. Timelocked governance & migrate  
4. Role separation (admin ≠ bridge ≠ gov ≠ operator)  
5. Automated pager for oracle silence  
6. Enforcement of min BSC confirmations on mainnet  
7. Integration tests vs real treasury  
8. Fee recipient path for native wrap (if used)  
9. Sliding or multi-bucket rate limits  
10. Broader CI (audit, wasm verify)

---

## 11. Positive controls observed (MR `!1` and prior remediations)

- InstantWithdraw client isolates treasury msg surface (`treasury.rs`).
- Recipient bound to CW20 `Send` sender (not hook-attacker field).
- Burn-then-pull ordering + atomic reject test.
- Allowance path removed from happy path / errors.
- Oracle freshness + min age ≥ throttle interval.
- Checked arithmetic; proptest on fee monotonicity.
- Two-step governance on oracle (and window).
- Oracle-service: multi-RPC consensus, chainId, Venus allowlist, HTTPS, secrecy, confirmation depth default.
- CW20 dependency pinned by commit SHA.

---

## 12. Priority remediation plan

| Priority | Action | Tracks |
|----------|--------|--------|
| P0 | Finish #19 Phase 5: migrate window, `SetCw20Spender`+`limit_24h`, `add_minter`, smoke withdraw | F-01 |
| P0 | Golden-vector / integration test vs ustr-cmm treasury messages & ACL | F-02 |
| P1 | Monitor Venus vs on-chain rate; design emergency rate procedure | F-04 |
| P1 | Split privileged keys / multisig per role | F-03 |
| P2 | Deposit `min_ust1_out`; document rolling-window edge | F-05, F-06 |
| P2 | Harden oracle-service (min confirmations, inclusion wait, real alerts) | F-09 |
| P3 | Fee/limit bounds; CI audit; require explicit treasury | F-08, F-11, F-12 |

---

## 13. Areas for follow-on audit (out of this repo’s binary)

1. **ustr-cmm Treasury** `InstantWithdrawCw20` / `SetCw20Spender` / fail-closed limits (trust root for F-01/F-02).  
2. **CL8Y bridge** mint_burn + BSC LockUnlock + decimal scaling.  
3. **cw20-mintable** multi-minter semantics and admin migrate.  
4. **Venus vToken** `exchangeRateStored` economic assumptions (interest accrual only upward typical — aligns with mono, but not guaranteed forever).  
5. **Frontend** `dex.cl8y.com/ust1` quote/slippage UX (not in repo).

---

## 14. Conclusion

MR `!1` is directionally correct and improves the custody model by eliminating treasury CW20 allowances. Automated tests in this monorepo adequately cover **local** InstantWithdraw happy/bad paths against stubs, but **do not** yet prove production safety: that still depends on ustr-cmm registration, mainnet migrate/smoke (#19), and ongoing oracle/key operational security. Highest-value next work is P0 ops + real-treasury test parity, then economic/oracle operator controls.

---

*End of report `INTERNAL_GROKCODE_1786158683`.*
