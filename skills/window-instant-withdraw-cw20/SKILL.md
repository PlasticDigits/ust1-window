---
name: window-instant-withdraw-cw20
description: >-
  Implement, review, or ops-wire ust1-window redeem via CMM treasury
  InstantWithdrawCw20 (Option 3). Use when changing withdraw messages,
  treasury.rs client, schema pin / golden JSON, InsufficientVfdusd /
  allowance removal, stub treasury multitest, real ustr-cmm treasury
  integration, DEPLOYMENT Phase 5 SetCw20Spender, or GitLab issues #20 / #21.
---

# Window InstantWithdrawCw20 (Option 3 redeem)

Companion consumer for [ustr-cmm#6](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/6) / [#7](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/7). Issues: [ust1-window#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20) (path), [#21](https://gitlab.com/PlasticDigits/ust1-window/-/issues/21) (cross-repo schema / audit C-1). Parent deploy track: [#19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19) Phase 5.

Cross-links: [docs/DEPLOYMENT.md](../../docs/DEPLOYMENT.md) Phase 5 §2 (schema pin + live probe), [README.md](../../README.md), `contracts/ust1-window/src/{contract,treasury,state}.rs`, golden [`testdata/instant_withdraw_cw20_golden.json`](../../contracts/ust1-window/testdata/instant_withdraw_cw20_golden.json), `scripts/verify_treasury_wire_schema.sh`, treasury skill in ustr-cmm: `skills/treasury-cw20-instant-withdraw`.

## Why this exists

Default `cmm_treasury` is the ustr-cmm **Treasury contract**, which cannot `IncreaseAllowance` as an EOA. Allowance + `TransferFrom` is invalid for mainnet redeem. Option 3: window (registered spender) asks treasury to `Transfer` vFDUSD to the user via `InstantWithdrawCw20`.

## Wire format (do not drift from ustr-cmm)

```json
{
  "instant_withdraw_cw20": {
    "recipient": "<user>",
    "token": "<vfdusd>",
    "amount": "<v_out>"
  }
}
```

**Schema pin (INV-SCHEMA-001):** `ust1_window::treasury::USTR_CMM_TREASURY_SCHEMA_REV` = `e6c4b7cf33f2f56d21c0e9fb2828efe87f032ded` (`PlasticDigits2/ustr-cmm`). Workspace Cargo dep `cmm-treasury` and the golden file must use the same rev. Bump only with intentional review + `scripts/verify_treasury_wire_schema.sh --regen`.

Client module: `ust1_window::treasury::TreasuryExecuteMsg` / `instant_withdraw_cw20_msg` (minimal subset — do not import full treasury into window wasm).

Ops (treasury gov, after window migrate):

```json
{
  "set_cw20_spender": {
    "token": "<TERRA_VFDUSD>",
    "spender": "<WINDOW_ADDR>",
    "limit_24h": "<quota>"
  }
}
```

Pulls are **fail-closed** without `limit_24h` (ustr-cmm#7). Align quota with window inventory policy (e.g. ~10_000 vFDUSD).

## Invariants (must hold)

1. **INV-WITHDRAW-001**: Happy-path withdraw does **not** call CW20 `TransferFrom` or query allowance. It calls treasury `InstantWithdrawCw20`.
2. **INV-WITHDRAW-002**: Response messages are ordered **Burn UST1 → InstantWithdrawCw20**; same tx; either failure reverts (no partial burn).
3. **INV-SCHEMA-001**: Window InstantWithdrawCw20 JSON is byte-compatible with pinned ustr-cmm treasury `ExecuteMsg` (golden + `treasury_schema` + real treasury multitest). Stubs keep `cw_serde` / `deny_unknown_fields` — never loosen for “forward compat”.
4. **Deposit unchanged**: vFDUSD `Transfer` → `cmm_treasury` after mint.
5. **Guards preserved**: oracle usability (`INV-ORACLE-PAUSE-001` then freshness), fee math (`INV-SWAP-*` including **INV-SWAP-003/004** zero-output rejects — see [audit-hardening-bundle](../audit-hardening-bundle/SKILL.md) / [#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25)), per-tx / rolling limits (`INV-LIMIT-001`), window pause, `min_vfdusd_out`. Prefer keep treasury balance check → `InsufficientVfdusd`. See [`oracle-circuit-breaker`](../oracle-circuit-breaker/SKILL.md).
6. **Recipient**: cw20 Send `sender` only (not attacker-controlled hook field).
7. **`cmm_treasury` instantiate-only** unless a separate issue adds `SetTreasury`.
8. **No public pull entry** on the window — only via UST1 `Receive` withdraw hook.
9. Window limits are primary UX caps; treasury `limit_24h` is a hard ceiling (defense in depth). Do not assume treasury caps replace window limits.

## Code map

| Path | Role |
|------|------|
| `contracts/ust1-window/src/treasury.rs` | Minimal execute client + pin constants + golden unit tests |
| `contracts/ust1-window/testdata/instant_withdraw_cw20_golden.json` | Canonical wire vectors + negative fixtures |
| `contracts/ust1-window/src/contract.rs` → `withdraw` | Burn + InstantWithdrawCw20 |
| `contracts/ust1-window/src/error.rs` | No `InsufficientTreasuryAllowance` |
| `contracts/ust1-window/src/multitest.rs` | Stub treasury (`deny_unknown_fields`) |
| `smartcontracts-terraclassic/tests/src/stub_treasury.rs` | Fast integration stub (strict) |
| `smartcontracts-terraclassic/tests/src/treasury_schema.rs` | Cross-crate byte-for-byte vs pinned `cmm-treasury` |
| `smartcontracts-terraclassic/tests/src/real_treasury_integration.rs` | Real treasury ACL / withdraw / atomic revert |
| `scripts/verify_treasury_wire_schema.sh` | CI / regen golden from pin |
| `docs/DEPLOYMENT.md` Phase 5 §2 | Migrate + `SetCw20Spender` + live probe |

## Tests to run

```bash
cargo test -p ust1-window --lib
cargo test -p ust1-integration-tests --lib
scripts/verify_treasury_wire_schema.sh
```

Key cases: golden / cross-crate schema match; stub rejects unknown fields; deposit→withdraw round trip with **zero** allowance against **real** treasury; unregistered spender atomic revert; migrate preserves config.

## Mainnet ops checklist

1. ~~Store + migrate treasury (ustr-cmm)~~ — treasury code **11564**.
2. ~~Optimize/store + migrate window~~ — `terra1zxwp…` → code **11566** (store `AA40BE6A…037E`, migrate `5C2A5CAF…1227`).
3. ~~`SetCw20Spender { token: vFDUSD, spender: WINDOW, limit_24h: … }`~~ — `limit_24h=10000000000` live.
4. Small withdraw smoke (**live probe before announce**); confirm treasury `Transfer` (not `TransferFrom`); allowance unused. Needs inventory + first `UpdateRate`.
5. Mark #19 Phase 5 withdraw AC when smoke + oracle wiring complete.

## Out of scope

- Implementing treasury CW20 API (ustr-cmm).
- Changing fee/limit params, oracle service, bridge, or `cmm-native-wrap`.
- Mainnet live probe itself (ops / runbook — documented in DEPLOYMENT.md).
- Oracle poll/silence vs window staleness: [`skills/oracle-ops-poll-silence`](../oracle-ops-poll-silence/SKILL.md) / [#24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24).
