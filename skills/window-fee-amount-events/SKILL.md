---
name: window-fee-amount-events
description: >-
  Implement or review ust1-window deposit/withdraw wasm fee_amount + fee_asset
  (INV-FEE-EVENT-001) so the DEX indexer can census CMM window fees. Use when
  changing window events, apply_fee_ust1 / fee_amount_ust1, DEPLOYMENT event
  table, or GitLab issue #33 / DEX #614 ingest.
---

# Window fee_amount events (#33)

DEX ingest ([cl8y-dex-terraclassic#614](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/614) / [!411](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/merge_requests/411)) fail-closes unless window wasm names `fee_amount` plus a token identity (`fee_asset` preferred; `ust1_token` also accepted). Live columbus-5 window **11566** (`terra1zxwpzpzpleatqn39r00grau4yt29sld8pw78s7ktvjafnj5nsaxq0h3rh2`) does **not** emit them until this code is stored and **migrated onto the same address**.

Issue: [ust1-window#33](https://gitlab.com/PlasticDigits/ust1-window/-/issues/33). Event table + migrate: [docs/DEPLOYMENT.md § Window fee events](../../docs/DEPLOYMENT.md#window-fee-events-deposit--withdraw-33). Invariant index: [README.md](../../README.md). Wrap-mapper `fee_amount` is a **different** contract ([DEX #613](https://gitlab.com/PlasticDigits/cl8y-dex-terraclassic/-/issues/613)) — do not fold.

## Invariants (must hold)

1. **INV-FEE-EVENT-001**: `fee_amount` is raw UST1 withheld (`gross − apply_fee_ust1`). Deposit: pre-fee UST1 − `ust1_out`. Withdraw: CW20 `send.amount` − after-fee UST1. Helper: `ust1_common::math::fee_amount_ust1`.
2. **`fee_asset`** is config `ust1_token` (UST1 CW20 bech32). Never vFDUSD (PFee-7 / P550-11). DEX prices hub UST1.
3. **Additive only.** Keep `action` (`deposit` / `withdraw`), `ust1_out`, `vfdusd_out`, `vfdusd_to_treasury`, `fee_total_bps`, `fee_chain_tax_bps`, `fee_cmm_protocol_bps`.
4. **Do not change fee math** (`INV-SWAP-001/002/003/004`, chain-tax / CMM split, treasury forward, pause, oracle freshness, rolling limits, `min_vfdusd_out`).
5. **Do not emit a derived USD.** Indexer stamps `fee_usd` from hub UST1.
6. **Do not reconstruct** `fee_amount` from `ust1_out × fee_total_bps` or `vfdusd_to_treasury × fee_cmm_protocol_bps`. Deposit `ust1_out` is post-fee (needs oracle rate to invert). Withdraw gross UST1 is the CW20 send, not on the window event. `fee_*_bps` are attribution, not transfers.
7. **Zero fee:** emit `fee_amount=0` (or omit). Indexer skips non-positive. Never invent a fake amount.
8. **Failed txs** (window pause, oracle pause/stale, dust `INV-SWAP-003/004`, below min, limits) must not emit a success `deposit`/`withdraw` event.
9. **Migrate, do not instantiate** a second window. DEX `UST1_WINDOW_ADDRESS` stays `terra1zxwp…`. Record the replacement code id in `docs/DEPLOYMENT.md` after columbus-5 store+migrate.

## Code map

| Path | Role |
|------|------|
| `smartcontracts-terraclassic/packages/ust1-common/src/math.rs` | `fee_amount_ust1`; INV-SWAP-001/002 unchanged |
| `contracts/ust1-window/src/contract.rs` | `with_fee_event_attrs`; deposit/withdraw emit keys |
| `contracts/ust1-window/src/state.rs` | Config invariant note |
| `contracts/ust1-window/src/multitest.rs` | `inv_fee_event_001_*` attr locks |
| `smartcontracts-terraclassic/tests/src/integration_tests.rs` | Integration attr lock |
| `docs/DEPLOYMENT.md` | Event table + migrate checklist |
| Companion redeem | [`window-instant-withdraw-cw20`](../window-instant-withdraw-cw20/SKILL.md) |

## Tests to run

```bash
cargo test -p ust1-common --lib inv_fee_event_001
cargo test -p ust1-common --lib inv_swap_001 inv_swap_002
cargo test -p ust1-window --lib inv_fee_event_001
cargo test -p ust1-integration-tests --lib inv_fee_event_001
cargo test -p ust1-window --lib
cargo test -p ust1-integration-tests --lib
```

Expect: `fee_bps=100` deposit `fee_amount` = 1% of pre-fee UST1 (`ust1_out` still net); withdraw `fee_amount` = 1% of gross UST1 (`vfdusd_out` still net); `fee_bps=0` → `0`; `fee_asset` is UST1.

## Out of scope

- Changing `fee_bps`, DEX parser, `/mint` faucet, wrap-mapper (#613), or live columbus-5 store/migrate (ops; document the new code id when done).
- InstantWithdrawCw20 wire: [`window-instant-withdraw-cw20`](../window-instant-withdraw-cw20/SKILL.md).
