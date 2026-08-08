---
name: window-instant-withdraw-cw20
description: >-
  Implement, review, or ops-wire ust1-window redeem via CMM treasury
  InstantWithdrawCw20 (Option 3). Use when changing withdraw messages,
  treasury.rs client, InsufficientVfdusd / allowance removal, stub treasury
  multitest, DEPLOYMENT Phase 5 SetCw20Spender, or GitLab issue #20.
---

# Window InstantWithdrawCw20 (Option 3 redeem)

Companion consumer for [ustr-cmm#6](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/6) / [#7](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/7). Issue: [ust1-window#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20). Parent deploy track: [#19](https://gitlab.com/PlasticDigits/ust1-window/-/issues/19) Phase 5.

Cross-links: [docs/DEPLOYMENT.md](../../docs/DEPLOYMENT.md) Phase 5 §2, [README.md](../../README.md), `contracts/ust1-window/src/{contract,treasury,state}.rs`, treasury skill in ustr-cmm: `skills/treasury-cw20-instant-withdraw`.

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

Client module: `ust1_window::treasury::TreasuryExecuteMsg` / `instant_withdraw_cw20_msg`.

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
3. **Deposit unchanged**: vFDUSD `Transfer` → `cmm_treasury` after mint.
4. **Guards preserved**: oracle freshness, fee math (`INV-SWAP-*` including **INV-SWAP-003/004** zero-output rejects — see [audit-hardening-bundle](../audit-hardening-bundle/SKILL.md) / [#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25)), per-tx / rolling limits (`INV-LIMIT-001`), pause, `min_vfdusd_out`. Prefer keep treasury balance check → `InsufficientVfdusd`.
5. **Recipient**: cw20 Send `sender` only (not attacker-controlled hook field).
6. **`cmm_treasury` instantiate-only** unless a separate issue adds `SetTreasury`.
7. **No public pull entry** on the window — only via UST1 `Receive` withdraw hook.
8. Window limits are primary UX caps; treasury `limit_24h` is a hard ceiling (defense in depth). Do not assume treasury caps replace window limits.

## Code map

| Path | Role |
|------|------|
| `contracts/ust1-window/src/treasury.rs` | Minimal execute client |
| `contracts/ust1-window/src/contract.rs` → `withdraw` | Burn + InstantWithdrawCw20 |
| `contracts/ust1-window/src/error.rs` | No `InsufficientTreasuryAllowance` |
| `contracts/ust1-window/src/multitest.rs` | Stub treasury (no `IncreaseAllowance`) |
| `smartcontracts-terraclassic/tests/src/stub_treasury.rs` | Integration stub |
| `docs/DEPLOYMENT.md` Phase 5 §2 | Migrate + `SetCw20Spender` ops |

## Tests to run

```bash
cargo test -p ust1-window --lib
cargo test -p ust1-integration-tests --lib
```

Key cases: deposit→withdraw round trip with **zero** allowance; treasury reject atomic (UST1 not burned); insufficient treasury balance; migrate preserves config; wire JSON contains `instant_withdraw_cw20`.

## Mainnet ops checklist

1. Store + migrate treasury (ustr-cmm) if CW20 InstantWithdraw not live.
2. Optimize/store new `ust1_window` wasm; **migrate** `terra1zxwp…` (prefer keep address).
3. `SetCw20Spender { token: vFDUSD, spender: WINDOW, limit_24h: … }`.
4. Small withdraw smoke; confirm treasury `Transfer` (not `TransferFrom`); allowance unused.
5. Mark #19 Phase 5 withdraw AC when smoke + oracle wiring complete.

## Out of scope

- Implementing treasury CW20 API (ustr-cmm).
- Changing fee/limit params, oracle service, bridge, or `cmm-native-wrap`.
