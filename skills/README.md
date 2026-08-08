# Agent skills (ust1-window)

Project skills for third-party / Cursor agents working in this repo.

| Skill | When to use |
|-------|-------------|
| [window-instant-withdraw-cw20](./window-instant-withdraw-cw20/SKILL.md) | Window redeem via treasury `InstantWithdrawCw20`, stub-treasury tests, migrate + `SetCw20Spender` ops ([#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20); depends on [ustr-cmm#6](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/6) / [#7](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/7)) |
| [audit-hardening-bundle](./audit-hardening-bundle/SKILL.md) | Audit hardening ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25)): INV-SWAP-003/004 dust guards, INV-DECIMALS-001, oracle `/healthz` + tick/gas/RPC env, cw20 `UpdateMinter(None)` delete semantics |

Human-facing deploy docs: [docs/DEPLOYMENT.md](../docs/DEPLOYMENT.md). Companion treasury skill (other repo): `ustr-cmm/skills/treasury-cw20-instant-withdraw`.
