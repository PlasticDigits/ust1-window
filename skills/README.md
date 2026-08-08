# Agent skills (ust1-window)

Project skills for third-party / Cursor agents working in this repo.

| Skill | When to use |
|-------|-------------|
| [window-instant-withdraw-cw20](./window-instant-withdraw-cw20/SKILL.md) | Window redeem via treasury `InstantWithdrawCw20`, stub-treasury tests, migrate + `SetCw20Spender` ops ([#20](https://gitlab.com/PlasticDigits/ust1-window/-/issues/20); depends on [ustr-cmm#6](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/6) / [#7](https://gitlab.com/PlasticDigits2/ustr-cmm/-/issues/7)) |
| [oracle-ops-poll-silence](./oracle-ops-poll-silence/SKILL.md) | Oracle service poll / silence defaults vs window staleness (**INV-ORACLE-OPS-***; audit H-3 / [#24](https://gitlab.com/PlasticDigits/ust1-window/-/issues/24)) |
| [oracle-circuit-breaker](./oracle-circuit-breaker/SKILL.md) | Oracle pause → window fail-closed (`State.paused` / `OraclePaused`, **INV-ORACLE-PAUSE-001**); emergency pause ops ([#22](https://gitlab.com/PlasticDigits/ust1-window/-/issues/22); audit C-2 #1) |
| [oracle-liveness-confirm](./oracle-liveness-confirm/SKILL.md) | Oracle-service DeliverTx + `State` confirmation before liveness success (**INV-ORACLE-LIVENESS-001**, audit C-3, [#23](https://gitlab.com/PlasticDigits/ust1-window/-/issues/23)) |
| [audit-hardening-bundle](./audit-hardening-bundle/SKILL.md) | Audit hardening ([#25](https://gitlab.com/PlasticDigits/ust1-window/-/issues/25)): INV-SWAP-003/004 dust guards, INV-DECIMALS-001, oracle `/healthz` + tick/gas/RPC env, cw20 `UpdateMinter(None)` delete semantics |

Human-facing deploy docs: [docs/DEPLOYMENT.md](../docs/DEPLOYMENT.md) (incl. [emergency pause](../docs/DEPLOYMENT.md#emergency-pause-oracle-circuit-breaker-vs-window)). Companion treasury skill (other repo): `ustr-cmm/skills/treasury-cw20-instant-withdraw`.
