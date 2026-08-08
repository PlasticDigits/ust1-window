# Internal Security Audit — UST1 Stack (`ust1-window`)

| | |
|---|---|
| **Report type** | Internal security audit |
| **Primary scope** | `glab mr !1` (commit `41c2032` — window withdraw path → treasury `InstantWithdrawCw20`) plus the full UST1 stack |
| **Related issues** | #19, #20 (MR context); #5, #6–#14 (prior security review & fixes, regression-verified) |
| **Date (UTC+9)** | 2026-08-08 |
| **Commit audited** | `41c2032` (MR head), tree `@1550603392` |
| **Method** | Full-codebase manual read + 4 parallel deep-analysis passes (smart contracts; economics/oracle; Rust server/infra/supply-chain; test coverage/e2e) + `cargo audit` + `gitleaks` + `git log -S/-G` history sweep + dependency source review (`cw20-mintable` fork, `ustr-cmm` checkout) |
| **Tools actually run** | `cargo audit` (3 vulns, 5 warnings), `gitleaks` (no secrets, 2 false positives), `pre-commit` hooks all pass |

> This is a point-in-time internal review, not an external audit. Findings marked "verify on columbus-5" need mainnet confirmation before deployment.

---

## 1. Executive summary

The codebase is unusually well-hardened for its size: fail-closed math, two-step governance, an oracle with throttle/daily-cap/monotonicity, HTTPS-only RPC, secret redaction, rev-pinned git dependencies, and a comprehensive unit-test suite (129 window tests, 79 oracle tests, 42 common tests, 95 oracle-service tests, 50 shared integration tests, plus a LocalTerra e2e path that was verified to pass end-to-end). The prior security review (#5–#14) was verified to be genuinely fixed and is covered by regression tests.

That said, this audit found **3 critical, 7 high, 19 medium** issues and a set of low/info items. The three critical findings all sit at the **cross-boundary trust layer** that the existing unit tests structurally cannot cover:

1. **Cross-repo wire-format drift** — the window now calls treasury `InstantWithdrawCw20`, whose schema is only asserted against a *local* stub with `deny_unknown_fields` explicitly removed to keep tests passing. If the final `ustr-cmm#6` treasury schema differs in any way (snake_case, field type, enum layout), every withdraw on mainnet bricks (fail-closed, no theft — but a dead redemption path).
2. **Monotonic-only oracle + Venus collateral-collapse tail risk** — the oracle hard-rejects rate decreases. If vFDUSD is exploited/collapses (the historic pattern for Venus-fork vTokens), the protocol is locked into the pre-collapse rate and the attacker can mint UST1 with near-worthless vFDUSD and redeem it for the treasury's honest vFDUSD inventory, bounded only by the rolling limit (~10k UST1/day).
3. **`BROADCAST_MODE_SYNC` treated as on-chain success** — the oracle service records liveness "success" after mempool CheckTx only; DeliverTx failures (policy drift, paused oracle, wrong operator) are invisible for up to `ORACLE_MAX_SILENCE_SECS` (default 8h) while the window is already rejecting swaps after 6h staleness.

Notable high findings: **deposits have no `min_ust1_out` and `fee_bps` can be set to 100%** (a malicious/compromised governance — or a mempool observer racing a `SetFeeBps` — can zero out an in-flight depositor), **single-EOA governance can mint unbacked UST1 and drain the treasury through its own window**, **oracle poll interval equals `max_oracle_age_sec`** (one missed tick = window halt), and **GitLab CI runs secret-detection only** (no build/test on the primary merge host).

---

## 2. Risk dashboard

| Severity | Count | Headline items |
|---|---|---|
| Critical | 3 | Wire-format drift (C-1); monotonic-oracle collapse drain (C-2); SYNC-broadcast false success (C-3) |
| High | 7 | Deposit no min-out / 100% fee (H-1); governance single-EOA mint→drain (H-2); poll==stale-age & alert misaligned (H-3); single-instance race (H-4); GitLab CI gap (H-5); RPC URL/API-key leakage (H-6); fixed-window burst + shared-bucket griefing (H-7) |
| Medium | 19 | Dust zero-output swaps, burn-tax unmodeled in wrap contract, UpdateMinter(None) leaves minters, consensus uses ≤3 URLs, optimizer tag-pin, no network tests for oracle-service, … |
| Low / Info | ~25 | Validation-before-ACL info leak, pending-governance no expiry, zero limits allowed, local-clock policy, mutex panic, gitleaks FPs, stale artifacts, economics disclosures, … |

---

## 3. Scope & architecture recap

| Component | Files | Role |
|---|---|---|
| `ust1-window` (code 11550) | `contracts/ust1-window/` | vFDUSD ↔ UST1 swap window. Deposit: pull vFDUSD, forward to treasury, mint UST1. Withdraw (this MR): burn UST1, call treasury `InstantWithdrawCw20`. |
| `ust1-oracle` (code 11549) | `contracts/ust1-oracle/` | Rate store. Operator updates; monotonic + 2% daily cap + 30min throttle. |
| `ust1-common` | `smartcontracts-terraclassic/packages/ust1-common/` | Pure math + oracle policy shared by contracts and service. |
| `cmm-native-wrap` | `contracts/cmm-native-wrap/` | USTC ↔ cwUSTC wrap/unwrap with fee accrual. |
| `cw20-mintable` (fork) | git dep `73a206b…` | UST1 token = cw20-base + minter API; **zero-amount guards removed**; additional-minter bookkeeping diverges from upstream. |
| `ustr-cmm` treasury (external) | git dep `9623780…` (cached rev `d8b0afd` read) | Holds vFDUSD; new `InstantWithdrawCw20` + `SetCw20Spender` proposed in `ustr-cmm#6` (not in cached checkout). |
| `ust1-oracle-service` | `oracle-service/` | Rust worker: BSC Venus `exchangeRateStored` (2-of-3 RPC consensus) → Terra `UpdateRate` via LCD. |

Cross-chain surface: BSC Venus vFDUSD → CL8Y lock/mint bridge → Terra vFDUSD (CW20, decimals 8) → window → UST1 (decimals 6) → optionally wrap/unwrap to native USTC. Each arrow is a trust/peg assumption.

---

## 4. Critical findings

### C-1 — Unverified `instant_withdraw_cw20` wire format; local stub is the only schema authority
**Where:** `contracts/ust1-window/src/treasury.rs:15-27`, `contracts/ust1-window/src/multitest.rs:53-68`, `smartcontracts-terraclassic/tests/src/stub_treasury.rs:20-41`
**What:** The MR replaces the withdraw path with a call to `TreasuryExecuteMsg::InstantWithdrawCw20`. The only schema check in this repo is the window's own `cw_serde` serialization test (`treasury.rs:39-52`) — it compares the message against **itself**. The test stub (`stub_treasury.rs`) was created by copying the window's `treasury.rs` and **explicitly removing `deny_unknown_fields`** (see the note in the MR issue: *"Removed deny_unknown_fields from TreasuryExecuteMsg to make the stub forwards-compatible"*), so unknown/misspelled/renamed fields would still pass. The cached `ustr-cmm` checkout (`d8b0afd`) contains **no** `InstantWithdrawCw20` — it only exists in the not-yet-merged `ustr-cmm#6`.
**Impact:** If the final treasury schema differs in any wire-visible way (field casing, `Uint128` string-vs-number, enum variant layout), every withdraw reverts on mainnet: redemption path dead, UST1 unmints impossible, users exit only via secondary markets at a likely discount. Fail-closed (no fund loss) but a liveness catastrophe discovered only at deploy time. There is also a bootstrap dependency: until treasury governance executes `SetCw20Spender { window }`, all withdraws revert.
**Also flagged as:** TEST-1, SC-7 (stub fidelity).
**Recommendation:**
1. Before any mainnet migration, generate the treasury's JSON schema from the *final* `ustr-cmm#6` branch and add a cross-repo contract test asserting `window::TreasuryExecuteMsg` serializes to it byte-for-byte (CI job that clones `ustr-cmm`).
2. Restore `deny_unknown_fields` on the stub to match the real treasury; stub should be *exactly* as strict as production.
3. Run the integration suite against the **real** `cmm-treasury` wasm artifact built from `ustr-cmm#6`, not the stub.
4. Post-migration runbook: one live withdraw probe before announcing.

### C-2 — Monotonic-only oracle locks in stale rate on vFDUSD collateral collapse → treasury drain
**Where:** `smartcontracts-terraclassic/packages/ust1-common/src/oracle_policy.rs:56-58`, `contracts/ust1-oracle/src/contract.rs:61-67`, `contracts/ust1-window/src/contract.rs:155-170`
**What:** The oracle policy hard-rejects any rate decrease (`MonotonicViolation`) both on-chain and off-chain. Rationale (issue #5): the Venus exchange rate only grows under normal operation. But the vFDUSD rate is a proxy for the health of Venus's FDUSD market on BSC. If that market is exploited (bad debt, oracle manipulation on BSC, FDUSD depeg, bridge depeg), the *economic* value of vFDUSD collapses while `exchangeRateStored` stays at its last (or frozen) value.
**Attack:** (1) Venus/FDUSD incident occurs; vFDUSD trades at e.g. 20% of face. (2) Oracle legitimately cannot lower R (hard rejection); governance would need an emergency path that **does not exist** (no rate-reset, no oracle-level pause — only the *window* can pause). (3) Attacker buys vFDUSD cheap, deposits D units at stale R → mints ≈ `D·R·0.99` UST1. (4) Immediately withdraws that UST1 → treasury `InstantWithdrawCw20` pays out ≈ `D·R·0.99·0.99/R = D·0.9801` vFDUSD of the treasury's *honest* inventory. Net: attacker converts D near-worthless vFDUSD into ~0.98D full-value vFDUSD, repeatedly. Bounded by the rolling 24h limit (default 10_000 UST1) and treasury balance — so it is a **slow drain at up to ~$10k/day** until governance pauses the window, not an instant rug, but it is the single largest *economic* attack surface in the design.
**Recommendation:**
1. Add an oracle-level `Pause`/circuit breaker (governance, effective immediately, blocks `UpdateRate` *and* causes window reads to fail stale-check fast) — currently only the window can pause, and only its own swaps.
2. Add a governance "emergency rate reset" with strict guardrails (e.g., timelock + monotonic bypass allowed once per N days), or accept decreases only when accompanied by a Venus bad-debt signal.
3. Off-chain: alert when BSC spot price of vFDUSD diverges >X% from `exchangeRateStored`-implied value (secondary-source sanity check).
4. Document the assumption explicitly in `docs/DEPLOYMENT.md` risk section: *UST1 is only as safe as Venus FDUSD collateral + the bridge; monotonicity trades "impossible to manipulate down" for "impossible to mark down".*

### C-3 — Oracle service treats `BROADCAST_MODE_SYNC` (CheckTx) success as on-chain success
**Where:** `oracle-service/src/terra_tx.rs:208-241`, `oracle-service/src/main.rs:113-120`, `oracle-service/src/liveness.rs:20-22`
**What:** With `BROADCAST_MODE_SYNC`, the LCD response `code == 0` means the tx passed **CheckTx** (mempool admission) only. DeliverTx failures — contract policy rejection (chain-time drift vs local policy), paused oracle, operator rotated on-chain, out-of-gas during wasm execution — return `code == 0` at broadcast and fail in the block. The service then calls `record_successful_broadcast()` and logs *"submitted oracle update"*. Compounding: the liveness alerter keys off that same false success.
**Impact:** The operator's only health signal (log alert after 8h of silence, log-drain dependent) can stay green while the on-chain rate goes stale; the window starts rejecting all swaps after 6h (`DEFAULT_MAX_ORACLE_AGE_SECS`). A misconfigured operator could run for hours believing it is healthy during an outage.
**Recommendation:**
1. After broadcast, poll `GET /cosmos/tx/v1beta1/txs/{hash}` until included; require block-result `code == 0`.
2. Then query the oracle `State` and verify `last_update_sec`/`rate` actually changed; only then record liveness success.
3. On `account sequence mismatch`, re-query the account and retry once; add bounded retries with jitter for transient LCD errors.

---

## 5. High findings

### H-1 — Deposit has no `min_ust1_out`, and `fee_bps` may be set to 100%
**Where:** `contracts/ust1-window/src/msg.rs:26` (`Deposit {}` — no fields), `contract.rs:100-101, 326-327` (`SetFeeBps` rejects only `> BPS_DENOM`)
**What:** Withdraw exposes `min_vfdusd_out` slippage protection; deposit does not. Governance can set `fee_bps = 10_000` (100%). A deposit racing a `SetFeeBps` in the mempool — or a malicious/compromised governance — yields `ust1_out = 0`. Because the `cw20-mintable` fork removed zero-amount guards, `Mint(0)` **succeeds** and the full vFDUSD principal is still forwarded to the treasury: the depositor loses 100% with a *successful* transaction. (On vanilla cw20-base the mint would revert and refund; the fork removes that accidental seatbelt.)
**Recommendation:** Add `min_ust1_out` to `Deposit` mirroring withdraw; cap `fee_bps` at a sane maximum (e.g., 500 bps) in `SetFeeBps`; add a zero-output guard (`ust1_out == 0 → error`) in deposit, mirroring SC-10/M-2.

### H-2 — Single-EOA governance can mint unbacked UST1 and drain the treasury through its own window
**Where:** `docs/DEPLOYMENT.md:149` (UST1 `minter = governance`, then `add_minter(window)`), `contracts/ust1-window/src/contract.rs` (all admin paths behind one `governance` address)
**What:** Governance is one address controlling: window config + pause + fees + limits, oracle operator rotation, the UST1 token's minter set (including *itself* as minter), and contract migration. With mint rights, governance can mint arbitrary unbacked UST1 and redeem it through the window for treasury vFDUSD — the two governance checks (`only registered minter`, `only governance`) are the **same key**. A single key compromise = full treasury loss. This is a documented design trade-off (testnet→mainnet bootstrap), but it is the largest centralization risk in the system. Related: no timelocks anywhere (SC-22), no two-step on `instantiate` admin fields (SC-5), pause also freezes *withdrawals* (SC-1 — arguably correct for oracle-risk pause, but it means pause = funds frozen, and there is no separate deposit-only pause).
**Recommendation:** Move governance to a multisig (e.g., 3-of-5) before meaningful TVL; drop the governance self-mint right after bootstrap (`update_minter(governance, None)` — but see M-9); consider a timelock for fee/limit/minter changes; document pause semantics (deposits+withdraws both frozen) for users.

### H-3 — Oracle poll interval equals window staleness budget; silence alert fires too late
**Where:** `oracle-service/src/config.rs:141-148` (`ORACLE_POLL_INTERVAL_SECS` default 21600; `ORACLE_MAX_SILENCE_SECS` default 28800), `ust1-common` `DEFAULT_MAX_ORACLE_AGE_SECS = 21600`
**What:** A single missed/failed tick leaves zero margin before the window rejects swaps (6h staleness vs 6h poll). The liveness alert default (8h) is 2h *later* than the window's 6h cutoff. In addition, the 30-min on-chain throttle means ~23 of 24 daily ticks are no-ops by design, so "silence" is ambiguous without the C-3 fix.
**Recommendation:** Default poll to ≤ 1h (still policy-gated by throttle when within band), set `ORACLE_MAX_SILENCE_SECS ≤ max_oracle_age + poll` (e.g., 21600+3600), and after C-3 the alert keys off confirmed on-chain updates.

### H-4 — No single-instance enforcement for the oracle operator
**Where:** `oracle-service/src/main.rs:47-66`, `docs/DEPLOYMENT.md:450-476`
**What:** Two workers with the same mnemonic (a plausible Render misconfiguration) produce sequence collisions, duplicate `UpdateRate` attempts, and nondeterministic failures. No lock, no leader election, no on-chain nonce pre-check.
**Recommendation:** Document/enforce exactly-one replica; add a startup advisory lock or detect repeated sequence-mismatch errors and page.

### H-5 — GitLab CI runs secret-detection only; no build/test on the primary merge host
**Where:** `.gitlab-ci.yml:8-16` vs `.github/workflows/ci.yml`
**What:** The repo's `repository` URL is GitLab; MRs there run `secret_detection` only. `cargo test`/`clippy`/`fmt` run solely in GitHub Actions on GitHub push/PR. A GitLab MR can merge code that doesn't compile or breaks tests.
**Recommendation:** Mirror the Rust job into `.gitlab-ci.yml` (or gate GitLab merges on the mirrored GitHub check).

### H-6 — RPC/LCD URLs (with embedded API keys) leak into logs via error strings
**Where:** `oracle-service/src/config.rs:67,78`, `oracle-service/src/bsc.rs:41-44,53,110`, `oracle-service/src/main.rs:63`
**What:** Validation and request errors interpolate full URLs; `reqwest::Error` Display also includes request URLs. One malformed `https://mainnet.infura.io/v3/<KEY>` entry permanently writes the key into Render log drains. `warn!("tick failed")` propagates these.
**Recommendation:** Redact credentials/query strings before formatting errors; log provider index/host only.

### H-7 — Fixed-window limits allow 2× burst at boundary; single shared bucket enables griefing
**Where:** `contracts/ust1-window/src/contract.rs:81-84` (reset when `now ≥ start + 86400`), `198, 261` (same `ROLLING` item for both directions)
**What:** (a) Fixed (non-sliding) window: 10k at T+86399 and 10k at T+86401 → 20k in 2s. (b) Deposits and withdraws share one bucket: an attacker cycling their own funds (paying 2% round-trip) can exhaust the bucket and DoS honest users; cheap on a 10k/day default. (c) Accounting asymmetry: deposits count *post-fee* (`ust1_out`), withdraws count *pre-fee* (`gross`) — small but arbitrary. These bounds also cap C-2's drain rate, so any redesign should preserve an absolute daily ceiling.
**Recommendation:** Sliding-window or per-direction buckets; document the accepted 2× burst and asymmetry if retained.

---

## 6. Medium findings (abridged)

| ID | Where | Finding | Fix |
|---|---|---|---|
| M-1 | `treasury.rs` client / `main.rs:93-100,136-141` | Off-chain policy uses local `SystemTime`, not Terra block time → off-chain/on-chain allow/deny diverge under clock skew (feeds C-3 false confidence). | Query block time for policy checks. |
| M-2 | `contract.rs:226-269` + fork | **Dust withdraw**: `v_out == 0` still burns UST1 and (with the fork's missing zero-guards) treasury `Transfer(0)` *succeeds* → user gets nothing for a successful burn. No zero-output guard; `min_vfdusd_out` defaults to 0. | Revert when `v_out == 0`. |
| M-3 | `contract.rs:186-224` + fork | **Dust deposit**: `ust1_out == 0` still forwards full vFDUSD to treasury; `Mint(0)` succeeds on the fork. Compounds H-1. | Revert when `ust1_out == 0`. |
| M-4 | `contract.rs:257-268` | Withdraw pre-checks treasury balance/allowance via 2 queries — TOCTOU (state can change before execution; submsg error handling makes it fail-closed, so UX-DoS only). | Accept + document; or drop pre-check and rely on submsg error. |
| M-5 | `cmm-native-wrap/src/contract.rs:131-170` | **Terra burn tax unmodeled** on unwrap native sends: `BankMsg::Send` of native_out from the contract will attract the columbus-5 burn tax (mechanics: sender-side surcharge vs recipient deduction depend on module version — **verify on columbus-5**); either way `balance == sum(expected)` breaks (insufficient balance → DoS, or short-paid recipients). Fees accrued can mask it temporarily. | Model tax explicitly (query tax rate / tax-exempt list), add tax to the amount withheld, integration-test on LocalTerra with tax enabled. |
| M-6 | `cmm-native-wrap/src/contract.rs:99-104` | `expected[payer] -= amount` silently no-ops (saturating) when payer lacks credit; over-returned native effectively comes from the fee pool — no error surfaces the anomaly. | Return an error on insufficient internal credit; add reconciliation query. |
| M-7 | `cmm-native-wrap` | No fee sweep/withdraw: accrued `accrued_fees_native` is permanently locked, only queryable. | Governance fee-sweep msg. |
| M-8 | `cw20-mintable` fork `73a206b` | `execute_update_minter(addr, None)` leaves the address in the `MINTERS` map (upstream deletes the entry); stale minter metadata persists. | Port upstream delete semantics. |
| M-9 | fork + DEPLOYMENT | `InstantiateMsg.mint` with `cap: None` → uncapped mint; combined with H-2 the UST1 supply cap is policy-only. | Set a cap at instantiate or document as intentional. |
| M-10 | `evm_rpc.rs:49-63` | Consensus only uses `urls[0..3]`; extra configured URLs ignored; **first-two-agree wins** — two correlated providers (same operator behind two hostnames) defeat the 2-of-3 intent. | Document; prefer distinct operators; optionally require 3rd as tiebreak; rotate sample. |
| M-11 | `bsc.rs:33-48` | `eth_chainId` cached forever after first tick; a post-start DNS/provider hijack behind the same URL isn't re-detected. | Re-verify periodically or on consensus mismatch. |
| M-12 | `bsc.rs:75,87` | No explicit timeout on the Alloy BSC provider (Terra client has 30s); a hung RPC stalls the whole tick (no tick-level timeout either). | Transport timeouts + `tokio::time::timeout` around `run_once`. |
| M-13 | `terra_tx.rs:91-108` | Account JSON parsing falls back to `0` for missing/unparseable `sequence`/`account_number` → rejected txs or invalid signatures with no operator-visible error. | Fail hard; support BaseAccount/vesting variants explicitly. |
| M-14 | `terra_tx.rs:20-21` | Fixed `0.015 uluna/gas` — if columbus-5 min gas price rises, every tick fails with no adaptive pricing. | Query node min gas price; use max(configured, network). |
| M-15 | `scripts/optimize.sh:11` | Optimizer image pinned by **tag** (`cosmwasm/optimizer:0.16.1`), not digest — mutable supply-chain input for production bytecode (code IDs 11549/11550). | Pin `@sha256:…`; record digest in DEPLOYMENT.md. |
| M-16 | `.github/workflows/ci.yml:15,23` | Floating `gitleaks-action@v2` and `rust-toolchain@stable`; no `cargo audit`/`cargo deny` in CI; no wasm reproducibility/code-ID ↔ source check in CI. | Pin SHAs; add audit + reproducible-build job. |
| M-17 | `config.rs:103` | `dotenvy::dotenv()` unconditionally loads `.env` from CWD, production included. | Gate behind debug flag / explicit env opt-in. |
| M-18 | Oracle contract `contract.rs:61-67` | UTC-day-boundary double-dip: +2% at 23:59:59 then +2% at 00:00:00 ⇒ up to ~+4.04% in 2s. Bounded, but contradicts the "2% per day" doc shorthand. | Sliding 24h window, or document the boundary case. |
| M-19 | Tests | Oracle-service network paths (`terra_tx`, `evm_rpc`, `bsc`) have **zero** automated tests — 3 of 8 source files untested, including all signing/broadcast/consensus code (TEST-18). No LocalTerra e2e in CI (TEST-16); no window governance tests (TEST-11); withdraw-path stale-oracle & rolling-window edges untested (TEST-6/7). | Wiremock-based HTTP tests; e2e job in CI; backfill listed cases. |

---

## 7. Low / informational (selection)

- **L-1** `SetFeeBps`/`SetMaxOracleAgeSec` validate *before* the governance ACL → config bounds oracle for unauthenticated callers (`contract.rs:326-331, 351-353`). Check ACL first.
- **L-2** `PendingGovernance` never expires → mistaken `ProposeGovernance` bricks rotation until the same key accepts/cancels (`contract.rs:277-324`, oracle likewise). Add expiry or allow overwrite.
- **L-3** Zero limits allowed at instantiate/`SetLimits`; `per_tx > rolling` not rejected → accidental full-pause config. Cross-validate.
- **L-4** Window bootstrap possible with a stale oracle (no age check at instantiate) — swaps fail-closed until first update; document.
- **L-5** Oracle `UpdateRate` with unchanged rate still bumps `last_update_sec` (state-touch refresh). Acceptable; document.
- **L-6** Oracle `SetPaused` is state-touch (no-op pause is a silent success). Fine; test exists.
- **L-7** BIP39 seed/`SigningKey` not zeroized (`terra_tx.rs:54-59`) — hot-wallet memory hygiene; `SecretString` wraps only the mnemonic string. Prefer `zeroize`/KMS long-term.
- **L-8** `.gitignore` covers `.env` only — add `.env.*`. Gitleaks `--no-git` hits `target/` + a public address in docs (allowlist).
- **L-9** Liveness mutex `.expect("poisoned")` can abort the loop; use `into_inner()`/`parking_lot`.
- **L-10** No graceful shutdown (`tokio::signal` feature enabled but unused) — SIGTERM mid-tick.
- **L-11** `latest - confirmation_blocks` computed per-provider → benign consensus churn near the 0.01% tolerance; use a shared `min(latest)-conf` block.
- **L-12** `uint256→u128` overflow fails the tick (correct fail-closed; add a metric).
- **L-13** `docker run -v` root-owned artifacts (documented); add `--user` flag.
- **L-14** `artifacts/*.wasm` in-repo are stale vs current source — regenerate or gitignore to avoid operator confusion.
- **L-15** Workspace `Cargo.toml` self-references `ust1-cmm` by git rev while the repo *is* ustr-cmm — sharp edge when bumping the pin (CI did catch it once, per issue #20).
- **L-16** README.md:72-73 implies HTTP health checks exist; liveness is log-only. Fix docs or add `/healthz`.
- **L-17** DEPLOYMENT.md should warn against pasting env blocks into tickets/chat; document keyring-password hygiene.
- **L-18** All three `migrate` entry points lack version guards (`contract.rs:391-395`, oracle `227-230`, wrap `178-182`) — chain admin-gated, but add `cw2` version checks. **Verified: MR !1 changes no storage layout (doc-comments only), so the 11550→new migration is low-risk; the legacy-state migrate test gap (TEST-3) is accordingly Low.**
- **L-19** Treasury address immutability: a malicious later treasury code migration is out of window's control; mitigated by fail-closed withdraw pre-checks. Document assumption.
- **L-20** `ust1-common` has no proptest/fuzz differential vs the on-chain U256 impl (TEST-19); math edge coverage at `u128::MAX` scale is thin (TEST-21).
- **L-21** Chain-registry only checks Cosmos assets, not the bridged vFDUSD CW20 (ECON-12): vFDUSD Terra (8dp) vs UST1 (6dp) `10^(D-U)` guard holds only if D≥U — assert in instantiate.
- **L-22** Mempool front-running of oracle updates: withdrawing at the old lower R just before a rate increase yields more vFDUSD — bounded by 2%/day and fees; inherent to public mempools; document (ECON-14).
- **L-23** Deposit-before-rate-increase is *unprofitable* (verified direction) — no action (ECON-9 resolved).
- **L-24** UST1 is a depreciating asset in vFDUSD terms (per-UST1 redemption ∝ 1/R, R monotonic↑) — expected by design, but user-facing docs should state it plainly (ECON-11).
- **L-25** Stuck funds in wrap contract (direct native sends) have no recovery path (SC-15) — acceptable, document.

---

## 8. Prior security review (#5 → #6–#14) — regression verification

All items from the earlier review were re-checked in current code and confirmed fixed with test coverage:

| Prior item | Status in `41c2032` |
|---|---|
| #6 monotonic oracle + comments | Enforced on-chain + off-chain (`oracle_policy.rs:56-58`); violation test exists |
| #7 oracle staleness checks in window | Deposit + withdraw both check (`contract.rs:200-202, 263-265`); tests exist (deposit side; withdraw side untested → TEST-6) |
| #8 two-step governance | Window + oracle both implement propose/accept/cancel; tests exist |
| #9 event attributes | `swap_*` events emitted on deposit/withdraw; tested |
| #10 dependency pinning | git deps rev-locked in `Cargo.lock` (verified `1500/4505`); LocalTerra digest-pinned |
| #11 limits (`per_tx`, rolling) | Enforced both directions; tests exist (boundary/reset edges thin → TEST-7) |
| #12 treasury allowance pre-check | Present (TOCTOU caveat, M-4) |
| #13 oracle-service 2-of-3 consensus | Present (M-10/M-11 caveats) |
| #14 fee bounds | `SetFeeBps` rejects >100% — but see H-1 (100% itself is too permissive) |

No regressions found. New findings in this report are largely in areas the prior review did not cover (cross-repo wire format, economic tail risk, ops broadcast semantics, CI/CD, wrap-contract tax).

---

## 9. Coverage vs. the audit brief

| Requested area | Verdict |
|---|---|
| **Test coverage** | Strong unit/integration breadth (~395 tests). Gaps: oracle-service network layer (0 tests), withdraw-path edge cases, governance msg tests on window, e2e not in CI, stub-only treasury integration, no proptest for `ust1-common`. |
| **Common DeFi attacks** | Sandwich (L-22 bounded), fee/manipulation (H-1), limits abuse (H-7), donated-balance (L-25), dust (M-2/M-3), front-running oracle (L-22). Reentrancy: N/A (CosmWasm actor model) — verified no callback surface. |
| **Common smart-contract attacks** | Overflow (Uint128/Uint256 checked math — clean), zero-address (Addr validated), unauthorized (ACL tests), replay (chain-level), schema confusion (**C-1**), migrate safety (L-18), allowance race (MR removes allowance path; legacy `TransferFrom` support dropped — confirmed no code depends on it). |
| **Database leaks** | No database in the system — chain state is public and msgs carry no secrets (positive). The nearest analogues (logs, env, key material) covered by H-6, L-7, L-8. |
| **e2e testing** | LocalTerra e2e exists and was verified passing historically; **not wired into CI**; e2e covers happy path only; stub treasury substitutes for the real one (C-1). |
| **Happy/bad path** | Both covered well in unit tests (paused, stale oracle, limit exceed, slippage, unauthorized, atomic-revert on treasury failure). Bad-path gaps enumerated in M-19/L-20. |
| **Missing security features** | Circuit breaker at oracle level (C-2), deposit min-out (H-1), tx confirmation (C-3), single-instance lock (H-4), fee sweep (M-7), timelocks/multisig (H-2), metrics endpoint (L-16). |
| **Access control / privileges** | Two-step governance correct; single-EOA centralization (H-2); minter bookkeeping bug in fork (M-8); instantiate admin fields not two-step (H-2). |
| **Rust server code** | See C-3, H-4, H-6, M-1, M-10–M-14, L-7/L-9/L-10/L-12. Overall solid structure (fail-closed config validation, redaction tests, HTTPS enforcement). |
| **Smart contract design** | Fail-closed posture is consistent and commendable; main design risks are C-1 (external interface), C-2 (oracle philosophy), H-1/H-7 (window economics), M-5 (wrap tax). |
| **Tokenomic / economic attacks** | C-2 (collateral-collapse drain), H-7 (limits gaming), ECON-17/18 (bridge & peg dependency chains — UST1 inherits Venus, FDUSD, CL8Y bridge, and Terra tax risks; documented but worth restating to users), L-24 (depreciation disclosure). |
| **Oracle manipulation** | On-chain: monotonic + 2%/day cap + 30min throttle + staleness bound manipulation to ≤2%/day (positive). Off-chain: 2-of-3 consensus with 0.01% tolerance, canonical vToken pin on mainnet, HTTPS-only (positives); weaknesses M-10/M-11/M-12 and Venus-rate-as-price proxy risk (C-2). |

---

## 10. Dependency & supply-chain (verified output)

`cargo audit` (run locally, 2026-08-08):

| ID | Crate | Path | Note |
|---|---|---|---|
| RUSTSEC-2024-0344 | `curve25519-dalek` 3.2.0 | via `cosmwasm-std` 1.5.11 | timing variability (elliptic-curve) |
| RUSTSEC-2026-0220 | `ruint` 1.17.2 | via `alloy-primitives` 0.8.x | shift-overflow bugs |
| RUSTSEC-2026-0104 | `rustls-webpki` 0.103.12 | via `reqwest` TLS | CRL parsing panic |

Plus 5 warnings (unmaintained `derivative`, `paste`, `proc-macro-error2`; `anyhow` RUSTSEC-2026-0190; `lru` RUSTSEC-2026-0002). None are directly exploitable in this deployment context, but **no CI gate exists** to catch the next one (M-16).

Positives: git deps rev-pinned and consistent (`ustr-cmm 9623780…`, `cw20-mintable 73a206b…`); LocalTerra digest-pinned; `gitleaks` clean (2 FPs: a public registry address in docs, doc-test strings in `target/`); no committed mnemonics in sampled history (`git log -S 'TERRA_MNEMONIC'`, BIP39 `-G` sweep); `Config` `Debug` redaction enforced by test.

---

## 11. Extended areas analyzed beyond the brief

1. Cross-repo wire-format drift (C-1) — the MR's defining risk.
2. Venus-fork vToken collapse semantics vs monotonic oracle (C-2).
3. Cosmos broadcast-mode semantics vs liveness (C-3).
4. Terra Classic burn-tax interaction with contract bank sends (M-5).
5. Fixed-window limit mathematics and shared-bucket griefing (H-7).
6. Forked `cw20-mintable` divergence from upstream (zero-guards, minter bookkeeping) and its interplay with dust swaps (M-2/M-3/M-8).
7. UTC-day-boundary double-dip in the daily cap (M-18).
8. Render/ops topology: single-instance, log-drain-only alerting, env-file handling (H-4, L-8, M-17).
9. Supply chain of the *build* (optimizer tag-pinning, floating CI actions) not just deps (M-15/M-16).
10. Bootstrap/migration runbook ordering (treasury `SetCw20Spender` before window migrate; stale-artifact trap).
11. Peg/depreciation disclosures (L-24) and bridge decimal invariants (L-21).
12. Mempool-level oracle front-running (L-22/L-23).

---

## 12. Prioritized recommendations

**Before mainnet migration (blocking):**
1. C-1 — cross-repo schema conformance test vs final `ustr-cmm#6`; restore `deny_unknown_fields` on the stub; run integration suite against the real treasury wasm; live withdraw probe post-migration.
2. C-3 — confirm DeliverTx + on-chain state change before recording liveness success.
3. H-1 — add `min_ust1_out` to `Deposit`; cap `fee_bps`; zero-output guards on both swap directions (M-2/M-3).
4. H-3 — lower poll default; align `ORACLE_MAX_SILENCE_SECS` with window staleness budget.
5. M-5 — model/verify burn tax for `cmm-native-wrap` unwraps on columbus-5 before enabling wrap flows.

**Soon (pre-TVL):**
6. C-2 — oracle circuit breaker + documented emergency rate-reset governance path; vFDUSD spot-vs-rate divergence alert.
7. H-2 — multisig governance; drop governance self-mint after bootstrap (mind M-8); consider timelocks.
8. H-5 — build/test/clippy in GitLab CI; add `cargo audit` + wasm reproducibility job (M-15/M-16).
9. H-6 — redact URLs in logs. H-4 — enforce single oracle instance.
10. M-8/M-9 — port upstream minter-delete semantics; decide on a UST1 supply cap.

**Backlog:** H-7 sliding windows; M-10–M-14 RPC/broadcast hardening; M-19/TEST-* backfills (oracle-service network tests, governance tests, withdraw stale/rolling edges, proptest for `ust1-common`, CI e2e); L-* doc and hygiene items.

---

## 13. Notable positives (keep doing)

- Consistent fail-closed design: stale oracle, paused, zero-liquidity, and policy violations all revert rather than guess.
- Oracle defense-in-depth: monotonic + daily cap + throttle enforced **both** off-chain (skip broadcast) and on-chain (reject) — the off-chain copy cannot drift from the on-chain copy because both use `ust1-common`.
- Two-step governance transfers with cancel, tested on both contracts.
- Atomic revert of burn+withdraw in one response (burn first) with explicit tests, including failure propagation from the treasury submessage.
- Secrets: `SecretString` redaction with a dedicated test; presence-only preflight script; HTTPS-only endpoints; no secrets in broadcast payloads; clean gitleaks/history.
- Pinned git deps, digest-pinned LocalTerra, pre-commit hooks (fmt/clippy/gitleaks/shellcheck) all green.
- Deployment docs are genuinely operational (addresses, code IDs, runbooks, troubleshooting).

*Report compiled from independent verification plus four parallel analysis passes; every Critical/High item above was re-confirmed against the source at `41c2032` before inclusion.*
