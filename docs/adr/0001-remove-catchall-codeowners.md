# ADR 0001: Remove catch-all CODEOWNERS

## Status

Accepted ([#50](https://git.cl8y.com/code/ust1-window/issues/50); design
`DESIGN: APPROVE` @ `24443742464da1a3ea69f1a87387f9e153d641f8`). S1
(CODEOWNERS delete) landed on `main` via merged PR
[#50](https://git.cl8y.com/code/ust1-window/pulls/50) before S2+WP-pre;
successor branch `issue/50` completes standing docs + README pointer +
`.woodpecker.yaml`. There is no separate standing issue `#50`; the issues
URL and the original product PR share one number (`html_url` →
`pulls/50`).

Overview (product tree, merge gate, pipeline path/layout):
[`architecture.md`](../architecture.md). Do not copy that table here. Local
invariant IDs are **H50-*** (this ticket). `code/hello` architecture **H15**
and sister-repo **H3** / **H5** / **MG** tables are other trees’ copies of
the same flags, not this repo’s issue number.

Design branch `cac-design-issue-50` is transport only; it is not the product
PR. Do not open a design-only PR.

This ADR does not authorize deploy, spend, custody rotation, Coolify oracle
publish, wasm migrate, or Forgejo protection PATCH
([agent-control #297](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/297)
/ [ADR 0004](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/src/branch/main/docs/adr/0004-autonomy-policy.md)).

## Outcome

Delete the catch-all `CODEOWNERS` so Forgejo does not plant **official**
review requests on every change. Merge to `main` stays: pull request,
Woodpecker context `ci/woodpecker/pr/woodpecker`, SHA-pinned `Do: merge`, no
direct push, no `force_merge`.

**Land** is S1+S2+**WP-pre** on PR
[#50](https://git.cl8y.com/code/ust1-window/pulls/50) (default vehicle: push
the combined diff onto that existing PR). Merging that PR **closes `#50`**.
A successor is allowed only if its body contains `Closes #50` **and** the
S3 leftover issue already exists. Merging a successor does **not** close
`#50` without that footer.
**Leftover-complete** (S3: dedicated post-merge plant-check PR; Tests
item 4) is tracked on a follow-up issue / leftover checklist that survives
that merge. S3 is not a close gate for `#50`. Operator attestation may
**record** `{n}` plus the two plant-check JSON bodies of that dedicated
PR; it is not a substitute for the PR.

This is a product-tree copy of the pattern named by
[cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48).
The canary is `code/hello` [#15](https://git.cl8y.com/code/hello/issues/15)
(still open; `CODEOWNERS` still on hello `main`). The **landed product
pattern** is
[code/cl8y-dex-terraclassic#1309](https://git.cl8y.com/code/cl8y-dex-terraclassic/issues/1309)
(CODEOWNERS-only delete, pre-existing green WP). This repo has **no**
in-tree Woodpecker pipeline and empty commit statuses on `main` / PR #49 /
PR #50, so land must produce the posting context here (**WP-pre**).
Protection on `code/ust1-window` `main` already matches the intended flags
(**author-attested** GET `updated_at` 2026-09-21T07:29:26Z; **H50-8** /
**H50-9** / **H50-2** / **H50-3** / **H50-5**; unauthenticated GET is 401).
Close still re-GETs. #50 does not re-roll protection, does not implement
CAC autoland, and does not deploy, migrate, or retune oracle/window
contracts.

## Context

`5408951` (2026-09-02) added root `CODEOWNERS`:

```
.* @code/maintainers
```

Forgejo uses Go regular expressions, not GitHub globs, and searches **root**,
`docs/CODEOWNERS`, `.gitea/CODEOWNERS`, and `.forgejo/CODEOWNERS`. Combined
with historical `block_on_official_review_requests`, every non-WIP PR that
touches a matching path requests team **maintainers** (CODEOWNERS line
`@code/maintainers`; PR JSON `requested_reviewers_teams[].name` is
`maintainers`, org `code`, team id 4). That team’s usual member is the PR
author, so self-approve is 422 and merge is 405. CAC `RECOMMEND: ACCEPT` is
not a Forgejo `APPROVED` review.
[cl8y-agent-control#388](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/388)
skipped the deadlock; it did not remove this file or this repo’s protection.
Drain must not delete `CODEOWNERS` as a workaround; this designed PR is the
allowed product-tree path.

This tree has no root `.woodpecker.yaml`. Do not add `.woodpecker.yml` or
`.woodpecker/` (directory mode can ignore root YAML and post
`ci/woodpecker/pr/<other>`). There is **no** ust1-window sister ticket that
will add one before S1 (open numbers are only #50 this delete, #49 Renovate,
and #30 EOA-governance audit). Hello#15 and dex#1309 could merge a
CODEOWNERS-only delete because those trees already posted
`ci/woodpecker/pr/woodpecker`. This tree cannot: land criterion 3 /
**H50-3** requires that context, and this slice must not `force_merge` or
drop **H50-3**. Therefore the deletion PR **may and must** add the quoted
**#50 land bootstrap** (architecture **Pipeline**). Empty statuses on
`5408951` / `8f05a0b` match **missing YAML**, not a proven disabled
Woodpecker project. Combine that YAML onto `#50`; **WP-pre** is the
enablement probe. If the context never posts after that, open a **new**
local leftover (Decision 8) **plus retrigger** — that missing-enablement
case is a land blocker for `#50`, not leftover-complete and not a
pre-combine gate.

[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) and
[`.gitlab-ci.yml`](../../.gitlab-ci.yml) already run gitleaks / cargo on
GitHub and GitLab. Forgejo `has_actions=false`; those jobs do **not** post
the required context. They stay leftover hosting CI. Do not delete them in
#50. Do not port the `rust` job into Woodpecker on this PR.

[cl8y-forgejo ADR 0003](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/src/branch/main/docs/adr/0003-community-forge-and-woodpecker.md)
item 8 (“CODEOWNERS plus protected `main`”) is amended on the forge repo for
the **official-review** half only. This tree’s README is product deploy docs
and does not currently claim CODEOWNERS is the trusted-PR gate; S2 still
adds an explicit **H50** pointer so later agents do not re-add
`.* @code/maintainers`. The issue body links cl8y-forgejo
[`docs/INVARIANTS.md`](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/src/branch/main/docs/INVARIANTS.md);
that file is not in this tree. Do not invent a local copy.

Live proof that the file still plants requests (this pass):

| PR | Role | Plant |
| --- | --- | --- |
| [#50](https://git.cl8y.com/code/ust1-window/pulls/50) | This delete (open, `draft: false`, **1 file** / 6 deletions, `mergeable: true`) | `requested_reviewers_teams` = `maintainers`; `official: true` `REQUEST_REVIEW` id **216** at 2026-09-21T07:42:59Z. Head `8f05a0b` deletes root `CODEOWNERS` and nothing else. Evidence of the catch-all (Forgejo still loads `CODEOWNERS` from `main`). **Disqualified** as “no new plant” / leftover-complete. hello#15 trap: 1 file, `mergeable: true`. |
| [#49](https://git.cl8y.com/code/ust1-window/pulls/49) | Renovate onboarding (open, 1 file) | `requested_reviewers_teams` = `maintainers`; `official: true` `REQUEST_REVIEW` id **161** at 2026-09-20T23:55:51Z |

Those leftovers are **disqualified** as “no new plant” evidence. Protection
GET on `main` has official-review **block** off (**H50-8**), so leftover
requests must not be treated as merge blockers.

`#50` as it sits cannot land: S1-only, empty Woodpecker statuses, no
standing docs / README pointer. Convert #50 to **draft** as the hello#15
merge lock (`draft: false`, 1 file, `mergeable: true` is still live). That
lock is against a later **delete-only** tip that somehow gets a green
context, not a substitute for **WP-pre**, and not a CI trigger: Forgejo
`ready_for_review` is not a Woodpecker event. After the combined push, wait
on the WP-pre status JSON (Observability). If empty, retrigger with another
push or Woodpecker **manual**; then ready and SHA-pin `Do: merge`.

Drain comments on #50 (`no occupying job…`; queued `design_author` without a
Hetzner VM) are
[cl8y-agent-control#429](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/429)
/ CAC, not a #50 failure. They are not permission to merge it.

This tree has **no** vendor `CODEOWNERS`. Scope absence checks to the four
Forgejo search paths.

## Non-goals

- Forgejo protection JSON / `apply_repo_policy.py` / migrate
  `_ensure_codeowners` / templates
  ([cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48)).
- CAC autoland predicates, occupying jobs, or `DrainSkip::OfficialReview`
  cleanup ([cl8y-agent-control#429](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/429),
  leftover of #388).
- Dismissing reviewers from the controller (forbidden substitute in #388),
  including the leftover on #50 / #49.
- Path-specific CODEOWNERS, a second maintainer, or `required_approvals: 1`.
- Weakening **H50-3**. The quoted root `.woodpecker.yaml` (**#50 land
  bootstrap**, architecture **Pipeline**) **is allowed** on the deletion PR
  so **H50-3** can post; see **WP-pre**. MUST path: root `.woodpecker.yaml`.
  Do not add `.woodpecker.yml` or `.woodpecker/`. Do not `force_merge`. Do
  not post a fake commit status. Do not add Coolify, Telegram, BSC, Terra
  `terrad` / `make build-optimized`, or bind deploy secrets to
  `pull_request`. Do not copy hello’s `coolify-deploy` / `telegram-failure`.
  Do not copy dex’s `scripts/ci/gitleaks-scan-tracked.sh`. Do not port
  `.gitlab-ci.yml` `rust` or `.github/workflows/ci.yml` onto this PR.
- Rewriting `contracts/`, `oracle-service/`, `scripts/`, `skills/`,
  `Cargo.toml`, product `INV-*` narrative, or [`docs/DEPLOYMENT.md`](../DEPLOYMENT.md).
  Do not enable direct `main`. Cargo tests stay ungated by this ticket.
- Wasm migrate, oracle Coolify/Render, treasury spender, verifier keys, or
  fee-event follow-ups
  ([agent-control #297](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/297);
  product [#30](https://git.cl8y.com/code/ust1-window/issues/30), GitLab #22 /
  #23 / #33).
- Renovate onboarding ([#49](https://git.cl8y.com/code/ust1-window/pulls/49)).
- A local `docs/INVARIANTS.md` (issue body cites the forge copy).
- Editing `autonomy.rs` / HMAC, self-approval, or a founder card for this
  ordinary design.
- Opening `cac-design-issue-50` as a PR, or merging that transport ref to
  `main`.

## Decision

1. **Delete** root `CODEOWNERS`. Do not leave an empty or comments-only file
   (Forgejo still parses it).
2. **Do not add** `docs/CODEOWNERS`, `.gitea/CODEOWNERS`, or
   `.forgejo/CODEOWNERS`. After land, `test -f` fails on all four paths.
   None of those paths may contain a reviewer rule for any pattern (not
   only `.*`).
3. **Keep** the merge gate in [`architecture.md`](../architecture.md)
   **H50**. On the product PR, **add** exactly this paragraph to root
   `README.md` after the **Git hooks and secret scanning** section and
   before **License**. Keep existing product tables, address registry, and
   `INV-*` index. Do not claim maintainers review every change. Do not
   leave the Git-hooks Gitleaks-Action sentence unqualified as this host’s
   merge CI (`has_actions=false`). Architecture-only is **not** sufficient.

```markdown
## Merge gate

Merge to `main` is a pull request, Woodpecker context
`ci/woodpecker/pr/woodpecker`, and SHA-pinned `Do: merge`
([architecture **H50**](docs/architecture.md)). Official CODEOWNERS review
is not a merge gate. Do not re-add catch-all `CODEOWNERS`
(`.* @code/maintainers`); see
[ADR 0001](docs/adr/0001-remove-catchall-codeowners.md).
GitHub/GitLab leftover hosting CI (the Gitleaks Action sentence under
**Git hooks and secret scanning**, GitLab `rust`) is not the Forgejo
required context and is not a merge hold.
```

4. **One product PR** whose diff is: delete `CODEOWNERS` + ADR 0001 +
   architecture + `docs/README.md` index + README pointer + the **WP-pre**
   pipeline file. Default vehicle: push that combined diff onto
   [#50](https://git.cl8y.com/code/ust1-window/pulls/50).
   `cac-design-issue-50` is transport; do not merge it to `main` and do not
   open it as the product PR. If a successor is required, its body **must**
   contain `Closes #50`, and the S3 leftover issue **must** already exist.
5. **Leave** already-planted official requests on open PRs (including #50
   and #49). They are non-blocking under **H50-8** / **H50-9**. Do not
   dismiss them from CAC. Human dismiss is optional leftover, not AC. They
   do not count as “no new plant.”
6. **Do not** PATCH branch protection from this repository.
7. **Split land from leftover-complete.** PR `#50` (or a `Closes #50`
   successor) is S1+S2+**WP-pre** only. S3 lives on a follow-up issue
   opened before that merge (plus the leftover checklist below). Require a
   **dedicated** post-merge plant-check PR that **changes at least one
   file**, is `draft == false`, has **no `WIP` title prefix**, and is
   opened **after** the delete is on `main`. Changing a file is
   **necessary** (dex#1309 no-op) and **not sufficient**. Do not accept
   `#50`’s leftover plant, `#49`’s plant, or “the next natural PR.” A
   no-op dedicated PR is not evidence: `.*` matches every path, and
   Forgejo does not plant on a no-op even if `CODEOWNERS` is still on
   `main`. A draft/`WIP`-prefix follow-up is not evidence (Forgejo skips
   CODEOWNERS on WIP). Fail if reviewers were planted by UI /
   `POST .../requested_reviewers`. Close the probe **without merge**.
   Record `{n}` plus the two JSON bodies on the leftover issue. Operator
   attestation may **record** those GETs; it is not a substitute for
   opening the PR.
8. **WP-pre (land prerequisite, owned).** The deletion-PR head SHA must
   show authentic Woodpecker **success** for context
   `ci/woodpecker/pr/woodpecker` before SHA-pinned `Do: merge`. Predicate
   (latest status with that context; Observability / Tests item 2):
   `status == success` **and** `target_url` starts with
   `https://ci.cl8y.com/` **and** `creator.login == lifejkskla` (id 1;
   live sister posts on hello#15 / dex#1309). A missing or
   non-`ci.cl8y.com` `target_url`, or a creator other than `lifejkskla`,
   is a hand-posted fake. Do **not** check `creator == ci.cl8y.com` (that
   false-rejects a real Woodpecker post). Pending, failure, error, empty,
   a green GitLab/`find` clone, or a hand-posted status is not **WP-pre**.

**File owner:** implement of `#50` — no sister ust1-window ticket exists to
add YAML first, so put **exactly** this file at root `.woodpecker.yaml` on
the deletion PR (same bytes as architecture **Pipeline**):

```yaml
# #50 land bootstrap. Posts context `ci/woodpecker/pr/woodpecker`.
# Not this repo's product test gate. Cargo tests stay ungated.
# Any later real CI must extend this same root file.
# MUST path: repository-root `.woodpecker.yaml`.
# Do not add `.woodpecker.yml` or `.woodpecker/`.

when:
  - event: [push, manual]
    branch: main
  - event: pull_request

steps:
  - name: tree
    image: alpine:3.22@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce
    commands:
      - test -n "$(find . -type f ! -path './.git/*' | head -1)"
```

**WP-pre is the enablement probe.** Combine the quoted YAML onto `#50`. Do
not confirm an active Woodpecker project **before** that push. File owner
(`#50` implement) ≠ posting owner (forge ops) **after** a miss, not as a
pre-combine gate. Empty statuses with no YAML match **missing YAML**, not a
proven disabled project. Do not look up `ci.cl8y.com/repos/42` (Forgejo
repo id **42** is `forge_remote_id`, not the Woodpecker UI id; dex is
Forgejo **49** / WP **7**). Do not wait on
[cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48)
(404 from this tree) or closed
[dex#1247](https://git.cl8y.com/code/cl8y-dex-terraclassic/issues/1247)
(historical 44/44 `code/*` on 2026-09-12; this repo already existed).

If the YAML is on the PR and the context never posts: **land blocked**.
Open a **new** leftover issue in **this** repo (dex#1247 shape: enable
Woodpecker with `forge_remote_id=42`, Forgejo webhook
`https://ci.cl8y.com/api/hook` for `push` + `pull_request`) **plus
retrigger** (another push or Woodpecker **manual**). Do not treat
`ready_for_review` as a CI trigger. Do not wait on #48 or closed #1247.
Do not treat a missing context as leftover-complete, “out of this slice,”
or silent sister-ops. Do not drop **H50-3**. Do not `force_merge`. Do not
POST a fake status.

## Component / state / interface changes

| Surface | Change |
| --- | --- |
| `CODEOWNERS` (root) | Remove file. |
| `docs/CODEOWNERS`, `.gitea/CODEOWNERS`, `.forgejo/CODEOWNERS` | Must remain absent (no empty file). |
| Forgejo PR review interface | After land, a **dedicated** plant-check PR against `main` that **changes at least one file**, is `draft == false`, and has **no `WIP` title prefix** must not get an official CODEOWNERS team request. `requested_reviewers_teams` from this file becomes empty for those new PRs. Close that probe **without merge**. |
| Branch protection API | No write from this ticket. Operator GET must still match architecture **GET-required** (**H50-2**, **H50-3**, **H50-5**, **H50-8**, **H50-9**). Observed rows are do-not-touch. Prior `updated_at` is **author-attested**; close re-GETs. Unauthenticated GET is 401. |
| Root `.woodpecker.yaml` | **Required on the deletion PR.** **#50 land bootstrap** quoted in Decision 8 / architecture **Pipeline**. MUST that path. `steps:` + two-item `when:` + digest-pinned alpine + `find` one-liner. No secrets, Coolify, Telegram, BSC, Terra. Do not add `.yml` or `.woodpecker/`. Any later real CI extends this same file. |
| `.github/workflows/ci.yml`, `.gitlab-ci.yml` | Unchanged leftover hosting CI. Not a Forgejo required context. |
| `.gitignore` | Unchanged. `docs/` is already trackable (`DEPLOYMENT.md` on `main`). Do not add a directory ignore of `docs/`. |
| `docs/architecture.md`, `docs/adr/0001-remove-catchall-codeowners.md` | Added on the design branch; land with S1+S2. |
| `docs/README.md` | Index pointer at architecture + ADR 0001 + existing `DEPLOYMENT.md`. |
| Root `README.md` | **Edit** on the deletion PR: add the Decision 3 **Merge gate** paragraph. Keep product tables, addresses, and `INV-*`. Do not stub. |
| `contracts/`, `oracle-service/`, `scripts/`, `skills/`, `Cargo.toml`, `docs/DEPLOYMENT.md`, `code/maintainers` team | Unchanged. The team may keep existing; it simply is not planted as official review. Cargo tests stay ungated. |

No runtime contract, schema, LCD, RPC, or HTTP API change.

## Affected invariants

| ID | Kind | Rule |
| --- | --- | --- |
| **H50-1** | Non-GET | `test -f` fails on `CODEOWNERS`, `docs/CODEOWNERS`, `.gitea/CODEOWNERS`, and `.forgejo/CODEOWNERS`. Absence, not “file exists but is not requesting reviewers.” File `@code/maintainers` / JSON team `maintainers` are the same leftover plant. |
| **H50-2** | GET-required | `enable_push=false` on `main`. |
| **H50-3** | GET-required | `enable_status_check=true` and required context `ci/woodpecker/pr/woodpecker`. |
| **H50-4** | Non-GET | Merge is SHA-pinned `Do: merge`. Never document or use `force_merge`. Not a protection field; omit from GET matching. |
| **H50-5** | GET-required | `block_on_rejected_reviews` stays true. An explicit REJECT still blocks. |
| **H50-6** | Non-GET | No Coolify/Woodpecker secrets on `pull_request`. This tree does not grow a deploy step from #50. Hello `telegram-failure` is forbidden. On-chain / Coolify / Render stays #297. |
| **H50-7** | Non-GET | This tree does not expand CAC merge/deploy/spend/custody policy. |
| **H50-8** | GET-required | `block_on_official_review_requests=false`. Leftover official requests on #50/#49 are non-blocking. |
| **H50-9** | GET-required | `required_approvals=0`. |

GET-observed flags (`block_on_outdated_branch`, `dismiss_stale_approvals`,
`apply_to_admins`) are listed in architecture only. They have no
close-criterion IDs. #50 must not PATCH them.

Product `INV-*` (math, oracle, limits, schema) in the root README are
unchanged. This ADR does not weaken them.

CAC invariants 29 / 67 / #388 stay: skip official-review deadlock; never
`force_merge`; drain does not delete CODEOWNERS. Product land makes the skip
class stop firing **for this repo** once new PRs have no plant.

## Alternatives

| Option | Why not |
| --- | --- |
| Keep file, rely on `block_on_official_review_requests=false` | Requests still plant on every non-WIP PR that changes a file (see #49 and #50); drain noise; Renovate/agent PRs look like they need a human stamp; templates can re-teach the old gate. |
| Replace `.*` with path owners | No second reviewer exists; same 405/422 if official-review is ever turned on; out of scope. |
| Add a second maintainer | Founder ops, not this implement. |
| Dismiss official requests from CAC | Forbidden by #388 as a substitute for policy reversal. |
| Direct-push the delete to `main` | Violates **H50-2**. File deletes go through a PR (already [#50](https://git.cl8y.com/code/ust1-window/pulls/50)). |
| Empty or comments-only CODEOWNERS | Forgejo still parses it. Absence is the contract. |
| Wait for `code/hello` canary merge before this delete | Sister pattern, not a local iid. This repo’s protection is already rolled; the file still plants. hello#15 is still open. |
| Cite hello#15 as a landed product delete | The landed pattern is dex#1309. |
| Leave ADR only on `cac-design-issue-50` | Standing docs never reach `main`; later agents re-add `.* @code/maintainers`. |
| Merge live #50 as delete-only (S1 without S2 / WP-pre) | **H50-1** maybe true, README/ADR/architecture / posting context false; later agents have no “do not re-add” contract. hello#15 is the delete-only trap (1 file, mergeable). Empty statuses already fail **H50-3**. |
| Close #50 only after a later PR proves no new plant | Merge of #50 closes the tracker; waiting for the follow-up before merge never produces it. |
| Treat `#50` leftover plant, `#49`’s plant, an empty-diff follow-up, a `WIP`-prefix/draft probe, or a probe that was merged as S3 | Merge closes `#50` before leftover-complete. A dedicated **non-empty**, `draft == false`, no-`WIP`-prefix post-merge PR, closed **without merge**, with `{n}` + JSON, is the evidence. Changing a file is necessary and not sufficient. |
| Operator attestation instead of the dedicated plant-check PR | False-pass path. Attestation may record GETs of that PR; it is not a substitute. |
| Match plant `team.name == "code/maintainers"` | That string is the CODEOWNERS target, not the live reviews JSON (`team.name == "maintainers"`). Misses the plant. |
| Skip the 30s re-GET when both plant signals are empty | Hello pin: first-GET empty can still be a late plant. Re-GET both; pass only if the second pair is still empty. |
| Merge a successor without `Closes #50` | `#50` stays open. Body must contain `Closes #50` and the S3 leftover issue must already exist. |
| Drop the Woodpecker required context so an empty-CI repo can merge | Violates **H50-3**. Do not `force_merge`. Produce the context via **WP-pre**. Do not treat GHA/GitLab as a substitute. |
| Wait for a sister YAML-only ticket before S1 | No such ust1-window issue exists. Blocking land on an unfiled ticket re-boxes implement. Allow the quoted bootstrap on the deletion PR instead. |
| Confirm Woodpecker enablement / look up `ci.cl8y.com/repos/42` before combining YAML | Pre-combine gate implement cannot run (public `/api/repos` and hooks are 401). Forgejo id **42** ≠ Woodpecker UI id. Combine YAML first; **WP-pre** is the probe. |
| Wait on cl8y-forgejo#48 or closed dex#1247 to unblock a missing context | #48 404s from this tree; #1247 is historical 44/44 from 2026-09-12. Open a **new** local leftover (`forge_remote_id=42` + webhook) plus retrigger. |
| Copy hello’s `.woodpecker.yaml` | Includes `coolify-deploy` and `telegram-failure` `from_secret`. Telegram runs on `status: [failure]` with no event filter (**H50-6**). |
| Copy dex’s `.woodpecker.yaml` | Calls `scripts/ci/gitleaks-scan-tracked.sh`, which does not exist here. |
| Port `.gitlab-ci.yml` `rust` / GitHub gitleaks into Woodpecker | Expands this chore into product CI. Cargo tests stay ungated. Later CI extends the same root file. After S2, Decision 3 already says leftover hosting CI is not a merge hold; do not “fix” the Git-hooks Gitleaks sentence by porting it. |
| Alpine `tree` binary / `apk add tree` | Stock Alpine has no `tree`. Org stubs use the `find` one-liner. |
| Add `.woodpecker.yml` or `.woodpecker/<other>.yaml` | Directory mode can ignore root YAML and post `ci/woodpecker/pr/<other>`; **H50-3** fails. |
| Treat missing context as silent forge ops / leftover-complete | File owner ≠ posting owner, but land still blocks. New local leftover (`forge_remote_id=42` + webhook) plus retrigger. |
| Fake status POST or `force_merge` | Forbidden. |
| Treat a failing, pending, empty, or hand-posted `ci/woodpecker/pr/woodpecker` as **WP-pre** | Land requires latest `status == success`, `target_url` starting with `https://ci.cl8y.com/`, and `creator.login == lifejkskla`. Checking `creator == ci.cl8y.com` false-rejects a real post. |
| Ready `#50` to trigger Woodpecker, then wait for success | Forgejo `ready_for_review` is not a Woodpecker event. If the draft-head push posts nothing, that order deadlocks. Retrigger with another push or Woodpecker **manual**; then ready. |
| Merge `cac-design-issue-50` to `main` | That ref still has root `CODEOWNERS`; it is not a valid **H50-1** tip. Transport only. |

## Complexity added / removed

**Removed:** catch-all official-review robot on every diff; operator dismiss
step; false “CODEOWNERS is the trusted-PR gate” story if later docs grow one.

**Added:** a small standing doc (this ADR + architecture **H50**) plus a
README pointer so later agents do not re-add `.* @code/maintainers` as a
merge requirement; a leftover checklist that survives merge of `#50`; a
**#50 land bootstrap** Woodpecker file on the deletion PR so **H50-3** can
post (this repo had none). No Coolify, no new services, jobs, flags, or
contract surfaces. No `.gitignore` exceptions (`docs/` is already
trackable). Cargo tests stay ungated. GitHub/GitLab workflows stay but are
not the Forgejo merge gate.

## Migration

1. Protection is already migrated (forge #48 execute on this repo,
   **author-attested** `updated_at` 2026-09-21T07:29:26Z). Unauthenticated
   GET is 401. Re-GET at merge; do not PATCH.
2. Before merging the deletion PR, open a follow-up **leftover** issue that
   owns S3 (dedicated plant-check PR per Tests item 4). Merging
   [#50](https://git.cl8y.com/code/ust1-window/pulls/50) closes that
   number; S3 must not live only there. If a successor is used, that
   leftover issue must already exist **and** the successor body must
   contain `Closes #50`.
3. Combine onto the existing product head, then merge **one** PR (see
   slices). Design branch `cac-design-issue-50` is **not** that PR. Today
   `chore/remove-catchall-codeowners` is delete-only @ `8f05a0b`; a
   delete-only merge is not land (hello#15).
4. Open PRs created while the file existed (#50 and #49) may still show an
   official team `maintainers` request. Non-blocking under **H50-8**. No
   bulk dismiss required to close #50. Disqualified as leftover-complete
   evidence.
5. Do not restore the file from `docs/templates/CODEOWNERS` in cl8y-forgejo;
   that template is owned by #48.

## Observability

Relative reads. Do not log tokens, hosts, RPC keys, LCD URLs, or
protection-script inventories.

**Protection (close criterion 4 / test 3).**
`GET /api/v1/repos/code/ust1-window/branch_protections` — pass iff the
`main` rule equals architecture **GET-required** for `enable_push`,
`enable_status_check`, `status_check_contexts`, `required_approvals`,
`block_on_official_review_requests`, and `block_on_rejected_reviews`. Fail
if any of those five IDs differ. Do **not** treat observed flags or
**H50-4** as GET-match fields. The 2026-09-21T07:29:26Z GET is
**author-attested**; unauthenticated GET is 401. Merge still re-reads for
drift. A green `find` clone-check or GitLab `rust` job does not satisfy
this read.

**Plant-check (dedicated post-merge PR, leftover-complete).** Recipe is
Tests item 4. Fail-closed pair (jq on the two GETs):

- Pass iff `(requested_reviewers_teams // []) | length == 0`
- **and** no review with `official == true && state == "REQUEST_REVIEW" && team.name == "maintainers"` (optional extra pin: `team.id == 4`). Do **not** require `team.organization` on the reviews GET. Do **not** match `team.name == "code/maintainers"` (that string is the CODEOWNERS target, not the live JSON).

`official` alone means assigned/write-access, not “planted by CODEOWNERS”;
the conjunction is the plant signal. `REQUEST_REVIEW` is `state`, not a
sibling key. Changing at least one file is necessary (dex#1309 no-op) and
**not** sufficient.

Known-plant sample: occupying [#50](https://git.cl8y.com/code/ust1-window/pulls/50)
(`draft: false`, review id **216**; must **fail** this predicate; not S3
evidence). On `#50`, `requested_reviewers_teams[].name == "maintainers"`
(org `code`, team id 4). On reviews, `official == true`,
`state == "REQUEST_REVIEW"`, `team.name == "maintainers"`. An operator who
checks `team.name == "code/maintainers"` misses the plant.

`{n}` is the dedicated plant-check PR opened **after** the delete is on
`main`. PR GET must have `draft == false`. Title must have **no `WIP`
prefix** (case-insensitive, including `[WIP]`). GET both endpoints
**immediately after open**. If either plant signal is present, fail. If
both signals are empty, wait once **30 seconds** and re-GET both before
pass; pass only if the second pair is still empty. Fail if reviewers were
planted by UI / `POST .../requested_reviewers`. Record `{n}` **and** the
two JSON bodies (the pair used for the pass decision) on the leftover
issue, then **close without merge**. PR `#50`’s leftover plant does not
pass. PR `#49`’s plant does not pass. “The next natural PR” does not pass.
An empty-diff, draft, or `WIP`-prefix dedicated PR does not pass. Operator
attestation may **record** those two GETs; it does not replace the PR.

**CI (land prerequisite).** Latest commit-status with
`context == ci/woodpecker/pr/woodpecker` on the **combined** product-PR
head must satisfy Decision 8: `status == success`, `target_url` starts
with `https://ci.cl8y.com/`, `creator.login == lifejkskla` (id 1). Sister
success posts: hello `target_url`
`https://ci.cl8y.com/repos/1/pipeline/13/1`, dex
`https://ci.cl8y.com/repos/7/pipeline/117/1` (WP UI id ≠ Forgejo id).
Prefix-match only; do not pin this repo’s Woodpecker UI id and do not look
up `ci.cl8y.com/repos/42`. A missing or non-`ci.cl8y.com` `target_url`, or
a non-`lifejkskla` creator, is a hand-posted fake. Do **not** check
`creator == ci.cl8y.com`. Pending, failure, error, empty, a green
GitLab/`find` clone, or a hand-posted status is not **WP-pre**. Drain
comments such as `drain skip: no occupying job…` are **#429** / CAC, not a
#50 failure. Empty statuses on `5408951` / `8f05a0b` match **missing
YAML**; combine the quoted file onto `#50` and treat **WP-pre** as the
enablement probe (Decision 8). If autoland waits on #429, an operator
still SHA-pins `Do: merge` (**H50-4**).

## Failure modes

| Mode | Handling |
| --- | --- |
| File deleted on a branch but still on `main` | New non-WIP PRs that change a file keep planting official review until the product PR merges. Expected until land. Occupying #50 already demonstrates this. |
| `chore/remove-catchall-codeowners` stays delete-only @ `8f05a0b` | `#50` must not merge. Convert to draft; push S2 + WP-pre onto that head. |
| Live #50 stays `draft: false` after a **delete-only** head is pushed | hello#15 trap: 1 file, `mergeable: true`. Convert to draft **before** pushing a combined head so S1-without-S2 cannot close `#50` if a context later posts. |
| Copy left in `docs/`, `.gitea/`, or `.forgejo/` (including empty/comments-only) | Forgejo still loads the first existing path and may plant. Land fails **H50-1**; delete those paths too (none exist on current `main`). Standing `docs/adr/` and `docs/architecture.md` are not CODEOWNERS. |
| S1 (delete-only) merges without S2 / WP-pre | **H50-1** true, README/ADR/architecture / posting context absent from `main`. Forbidden. Empty statuses currently also fail **H50-3**. |
| Successor merges without `Closes #50` | `#50` stays open. Forbidden as the land vehicle. |
| cl8y-forgejo migrate/apply re-copies a template | Sister-repo race ([cl8y-forgejo#48](https://git.cl8y.com/PlasticDigits/cl8y-forgejo/issues/48) `_ensure_codeowners`). Out of this slice. If a later apply re-adds the file, delete again via PR; never direct-push `main`. |
| Official request leftover on #50 or #49 | Non-blocking (**H50-8**). Optional human dismiss. Not a rollback signal. Not leftover-complete evidence. |
| Treating merge of `#50` as leftover-complete | Merge closes `#50` before S3. Use the leftover issue / checklist. |
| Operator attestation without the dedicated PR | False-pass. Fail S3; open the dedicated PR (Tests item 4). Attestation may only record its GETs. |
| Plant-check PR has empty diff | False-pass: Forgejo does not plant on a no-op even if `CODEOWNERS` remains. Fail S3; open a new dedicated PR that changes at least one file. Changing a file is necessary and not sufficient. |
| Plant-check is `draft != false`, title has a `WIP` prefix, reviewers were requested in the UI / `POST .../requested_reviewers`, first-GET empty without the 30s re-GET, or the probe was merged | False-pass (CODEOWNERS skipped / late plant / Coolify-irrelevant `main` push) or false-fail (manual team request). Recipe fails closed; open a new probe; **close without merge**. |
| Operator matches `team.name == "code/maintainers"` | Misses the live plant (`team.name == "maintainers"`). Use the Observability fail-closed pair. |
| `ci/woodpecker/pr/woodpecker` never posts | **Land blocked** (**WP-pre** / land criterion 3). Do not `force_merge`; do not drop **H50-3**; do not POST a fake status. Diagnose: missing YAML or wrong path first (empty statuses today match **missing YAML**). If YAML is on the PR and nothing posts: open a **new** local leftover (`forge_remote_id=42`, webhook `https://ci.cl8y.com/api/hook` for `push`+`pull_request`) **plus retrigger** (another push or Woodpecker **manual**). Do not wait on cl8y-forgejo#48 or closed dex#1247. Do not look up `ci.cl8y.com/repos/42`. Do not treat `ready_for_review` as a retrigger. |
| Context present but not authentic success | **WP-pre** incomplete. Fail if latest `status` is not `success`, `target_url` is missing / does not start with `https://ci.cl8y.com/`, or `creator.login != lifejkskla`. Checking `creator == ci.cl8y.com` false-rejects a real post. Re-run or repair; do not `force_merge`. |
| Ready `#50` while WP-pre is empty, hoping undraft retriggers CI | Deadlock: Forgejo `ready_for_review` is not a Woodpecker event. Retrigger with another push or Woodpecker **manual**; **then** ready. |
| Rebase after a green SHA | Invalidates WP-pre (`block_on_outdated_branch` observed-true). Wait on the new head’s status JSON; do not PATCH the flag off. |
| `.woodpecker.yml` or `.woodpecker/<other>.yaml` added | Wrong or ignored context; **H50-3** fails. |
| GitLab/GHA treated as **H50-3** | Those workflows do not post `ci/woodpecker/pr/woodpecker`. Forgejo `has_actions=false`. Keep them; do not drop Woodpecker. |
| Protection silently reverted to official-review true | Merge 405 returns. Out of this repo; re-apply via forge policy, do not `force_merge`. Not proven by a green `find` step. |
| `enable_push` flipped true | **H50-2** regression. Refuse. |
| Re-adding CODEOWNERS “for safety” in a follow-up | Violates **H50-1**. Reviewers must reject unless a new ADR allowlists path owners. |
| CAC autoland waits on #429 | Operator `Do: merge` still closes #50. Do not implement occupying-job cleanup here. |
| `block_on_outdated_branch` blocks merge | Rebase the product head onto current `main`. Do not PATCH the flag off. Rebase after a green SHA invalidates WP-pre; wait on the new head. |
| Implement PATCHes protection or edits CAC / Coolify / contracts | Out of authority / wrong repo. |

## Ordered implementation slices

| Slice | Work | Mergeable? | Depends on |
| --- | --- | --- | --- |
| **S0** | This design on `cac-design-issue-50` (ADR 0001 + architecture **H50** + `docs/README.md` index). Transport only. | No. Do not open or merge as a product PR. | None in `code/ust1-window`. **WP-pre** is the enablement probe after YAML is on `#50`; not an S0 file dep and not a pre-combine gate. |
| **S1** | Delete root `CODEOWNERS` on a branch that differs from `main`. Confirm the other three paths are absent. Live head `8f05a0b` on `chore/remove-catchall-codeowners` is that delete. | **Draft-only** until S2+WP-pre are on the same branch. Not a landable slice. Convert live #50 to `draft` as the hello#15 merge lock (`draft: false`, 1 file, `mergeable: true` is still live). | S0 accepted. |
| **S2** | Same branch as S1: standing docs + `docs/README.md` index + README pointer (Decision 3 body, including leftover hosting CI qualifier). Add the quoted root `.woodpecker.yaml`. No contract / oracle-service / DEPLOYMENT / skill edits. Open the leftover issue that will own S3. | Only as the **combined** product PR with S1+WP-pre. | S1 on the same head. |
| **WP-pre** | **Land prerequisite (not leftover-complete).** Combine the quoted YAML onto `#50`; this slice **is** the enablement probe. Authentic **success** of `ci/woodpecker/pr/woodpecker` (Observability / Tests item 2). File owner: `#50` implement. If the context never posts: open a **new** local leftover (`forge_remote_id=42` + webhook) **plus retrigger**. Fake status POSTs and `force_merge` forbidden. Blocks merge of S1+S2. | N/A as a solo merge. | S2 file on the PR. Not a pre-combine iid. |
| **Product PR** | **One** PR: default [#50](https://git.cl8y.com/code/ust1-window/pulls/50) (`chore/remove-catchall-codeowners`) whose `git diff origin/main...HEAD` is delete + ADR 0001 + architecture + `docs/README.md` index + README pointer + root `.woodpecker.yaml`. Merging this PR **closes #50**. Successor only with `Closes #50` in the body **and** S3 leftover issue already open. | Yes, once S1+S2+WP-pre are on the head, rebased, authentic Woodpecker **success**, then ready. | S0 accepted; S1+S2+WP-pre combined. |
| **S3** | Leftover-complete “no new plant” (tests item 4). Dedicated post-merge plant-check PR (`draft == false`, no `WIP` prefix, at least one file changed, close **without merge**, record `{n}` + JSON). Operator attestation may record that PR’s GETs; it is not a substitute. **Not** a close gate for #50. | N/A | Product PR merged to `main`. |

PR `#50` (or `Closes #50` successor) ships **S1+S2+WP-pre** only.

Sister repos (not slices of #50, not local `DEPS`): forge #48
protection+templates (not a WP-enable ticket from this tree; it 404s);
CAC #429 autoland occupying job; hello#15 open canary (S3 plant-check pins);
dex#1309 landed pattern; dex#1247 historical enablement **shape** only
(closed; do not wait on it); ust1-window #49 Renovate; ust1-window #30
EOA governance.

### Combine step (S1+S2+WP-pre onto the product vehicle)

Do this on `chore/remove-catchall-codeowners`, not on `cac-design-issue-50`.
Combine the quoted YAML onto `#50`. **WP-pre is the enablement probe.** Do
not wait to confirm a Woodpecker project, do not look up
`ci.cl8y.com/repos/42`, and do not wait on cl8y-forgejo#48 or closed
dex#1247 **before** these pushes.

1. Convert [#50](https://git.cl8y.com/code/ust1-window/pulls/50) to **draft**
   as the hello#15 merge lock. Live `#50` is still `draft: false`, 1 file,
   `mergeable: true`.
2. Fetch published design: `origin/cac-design-issue-50` (this ADR,
   `docs/architecture.md`, `docs/README.md` index).
3. Copy or cherry-pick those S0 paths onto `chore/remove-catchall-codeowners`
   (keep the CODEOWNERS delete from `8f05a0b`).
4. Add the README **Merge gate** paragraph as the Decision 3 body
   (including the GitHub/GitLab leftover-hosting-CI qualifier). Do not
   stub the product README. Do not port gitleaks into this PR.
5. Add **exactly** the quoted root `.woodpecker.yaml` (Decision 8 /
   architecture **Pipeline**). No `.yml`, no `.woodpecker/`, no hello/dex
   product steps, no GitLab `rust` port.
6. Open the leftover issue that owns S3. Required **before** merge (tests
   item 6 / land criterion 6). If a successor is used instead of `#50`,
   that issue must already exist and the successor body must contain
   `Closes #50`.
7. Rebase onto current `origin/main` (`block_on_outdated_branch` is
   observed-true; do not PATCH it off). Push the product branch.
8. Wait on the WP-pre status JSON (Observability / Tests item 2). If
   empty: retrigger with another **push** or Woodpecker **manual**. Do
   **not** treat `ready_for_review` as a CI trigger (Forgejo undraft is
   not a Woodpecker event). After authentic **success**, convert to ready,
   then SHA-pin `Do: merge`. A rebase after a green SHA invalidates
   WP-pre; wait again. If YAML is present and the context never posts:
   open a **new** local leftover (`forge_remote_id=42`, webhook
   `https://ci.cl8y.com/api/hook` for `push`+`pull_request`) **plus
   retrigger**. Do not fake a status.

## Tests

In-repo CI cannot GET branch protection. Existing Cargo tests
(`make test-contracts`, GitLab `rust`, GitHub Actions `rust`) stay ungated
and must not be rewritten for this ticket. The **#50 land bootstrap** does
not run those suites. Any later real Forgejo CI extends the same root
`.woodpecker.yaml`. Do not add a contract test solely for file absence.

1. **Absence** (on the product tip / after land). Scope to the four Forgejo
   CODEOWNERS paths; do not `git grep` the whole tree (this ADR quotes
   `.* @code/maintainers`).

   ```
   test ! -e CODEOWNERS
   test ! -e docs/CODEOWNERS
   test ! -e .gitea/CODEOWNERS
   test ! -e .forgejo/CODEOWNERS
   git grep -n '^\.\* @' -- CODEOWNERS docs/CODEOWNERS .gitea/CODEOWNERS .forgejo/CODEOWNERS
   # pass: grep exit 1 (no matches / no files)
   ```

2. **PR pipeline (land / WP-pre).** Root `.woodpecker.yaml` on the combined
   head is the Decision 8 / architecture **Pipeline** file (`steps:`,
   two-item `when:`, digest-pinned alpine, `find` one-liner, no secrets).
   Latest commit-status with `context == ci/woodpecker/pr/woodpecker` on
   that head has `status == success`, `target_url` starting with
   `https://ci.cl8y.com/`, and `creator.login == lifejkskla` (id 1). A
   missing or non-`ci.cl8y.com` `target_url`, or a non-`lifejkskla`
   creator, is a hand-posted fake. Do **not** check
   `creator == ci.cl8y.com`. A green GitLab/`find` clone is not this
   check. Cargo / GitHub gitleaks stay ungated.

3. **Protection (operator read).** Same list as close criterion 4: GET
   `main` still equals **H50-2**, **H50-3**, **H50-5**, **H50-8**, **H50-9**.
   Fail if `enable_push` is true, official-review block is true,
   `required_approvals` is not 0, reject-block is false, or the Woodpecker
   context is missing. Do **not** treat observed flags or **H50-4** as
   GET-match fields. Prior timestamp is **author-attested**; this test is
   a close re-GET. Unauthenticated GET is 401.

4. **No new plant** (leftover-complete; **not** a #50 close gate). After
   S1+S2+WP-pre are on `main`, on the leftover issue run this recipe:

   1. Open a **dedicated** plant-check PR **into `main`**. PR GET must have
      `draft == false`. Title must have **no `WIP` prefix** (case-insensitive,
      including `[WIP]`). At least one changed file (necessary; dex#1309
      no-op; **not** sufficient).
   2. Do not request users or teams in the UI or via
      `POST .../requested_reviewers`.
   3. GET `.../pulls/{n}` and `.../pulls/{n}/reviews` **immediately after
      open**.
   4. Pass iff Observability’s fail-closed pair holds. If either plant
      signal is present, fail. If both signals are empty, wait once
      **30 seconds** and re-GET both; pass only if the second pair is
      still empty.
   5. Record `{n}` **and** the two JSON bodies (the pair used for the pass
      decision) on the leftover issue, then **close without merge**.

   Fail if `draft != false`, if the title has a `WIP` prefix, if the PR
   has no changed file, if reviewers were requested manually, or if either
   GET signal is present after the wait. **Disqualify** leftover
   `pulls/50` and leftover `pulls/49`. Do not use “the next natural PR.”
   Operator attestation may record those GETs; it is not a substitute
   for this PR.

5. **Reject still blocks (doc-level).** Do not turn off
   `block_on_rejected_reviews` to “make autoland easier.”

6. **Leftover issue exists before merge** (same as land criterion 6). The
   S3-owning follow-up issue is open before SHA-pinned `Do: merge` of `#50`
   or a `Closes #50` successor. Merging `#50` closes that number; S3 must
   not live only there.

7. **Product untouched.** `git diff origin/main...HEAD` on the implement PR
   has no `contracts/` / `oracle-service/` / `scripts/` / `skills/` /
   `Cargo.toml` / `docs/DEPLOYMENT.md` edits.

## Rollout

- Merge vehicle: **one** PR, existing
  [#50](https://git.cl8y.com/code/ust1-window/pulls/50), after the combine
  step. Diff must be delete + ADR 0001 + architecture + `docs/README.md`
  index + README pointer (Decision 3) + root `.woodpecker.yaml`
  (Decision 8). Design-only `cac-design-issue-50` must not be opened or
  merged as the product PR. Successor only with `Closes #50` in the body
  and the S3 leftover issue already open.
- Order: protection already live (re-GET at close; operator-only, 401
  here) → convert #50 to draft (hello#15 merge lock) → combine S1+S2+YAML
  on `chore/remove-catchall-codeowners` → leftover issue remains open →
  rebase → wait on authentic WP-pre **success** (if empty: another push or
  Woodpecker **manual**; do not treat ready as a CI trigger) → **then**
  ready → SHA-pin `Do: merge` → **#50 closes**. If the context never
  posts: new local leftover (`forge_remote_id=42` + webhook) plus
  retrigger; do not wait on #48 or closed #1247. Then leftover-complete
  test 4 on a dedicated post-merge plant-check PR (`draft == false`, no
  `WIP` prefix, close without merge, `{n}` + JSON). Operator attestation
  may record that PR’s GETs; it is not leftover-complete.
- Other `code/*` catch-all deletions may copy this pattern; this ADR does
  not merge those repos. Repos that already post the context (hello#15,
  dex#1309) do not need a new YAML on the delete PR; this tree does. Do
  not copy hello or dex YAML contents.
- [#297](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/297):
  no deploy, spend, custody, or CAC policy expansion. Landing #50 does not
  authorize wasm migrate, Coolify oracle publish, InstantWithdraw, or
  autonomy changes.

## Rollback

Restore the previous `CODEOWNERS` **via PR**, not direct `main`. That
re-plants official requests. It does **not** by itself re-enable
merge-block (`block_on_official_review_requests`); restoring the 405 gate
is a forge-policy revert, founder-scoped, and is not a `ust1-window`
rollback step.

File-only rollback that leaves this ADR and architecture on `main` (still
saying CODEOWNERS must be absent) is **intended**: standing docs keep
telling later agents not to treat the restored file as a merge gate. Do not
delete the docs as part of restoring the file unless a new ADR reverses
this decision.

Do **not** delete the Woodpecker file as part of a CODEOWNERS rollback
(that re-boxes **H50-3**). WP rollback is a separate PR and is not required
to undo the catch-all. Contract / oracle-service / DEPLOYMENT files stay
untouched. Do **not** delete `.github/workflows/ci.yml` or `.gitlab-ci.yml`
as part of rollback.

## Integration completion criteria

### Land (S1+S2+WP-pre) — merge of PR `#50` or `Closes #50` successor

All must be true on the merged tip. This is what merging `#50` completes. It
does **not** wait for S3.

1. `main` has no CODEOWNERS file at the four Forgejo paths: `test -f` fails
   on `CODEOWNERS`, `docs/CODEOWNERS`, `.gitea/CODEOWNERS`,
   `.forgejo/CODEOWNERS` (**H50-1**).
2. Standing docs on `main`: ADR 0001 + architecture **H50** +
   `docs/README.md` index, **and** root README pointer (Decision 3 body;
   product tables kept). Architecture-only is not sufficient.
3. The deletion-PR head has authentic **success** of
   `ci/woodpecker/pr/woodpecker` (**WP-pre** / **H50-3** / Tests item 2):
   latest `status == success`, `target_url` starts with
   `https://ci.cl8y.com/`, `creator.login == lifejkskla`. No deploy secrets
   ran on that PR event; mergeable under **H50-4** (SHA-pinned
   `Do: merge`). Ready only **after** that success. A failing, empty, or
   hand-posted status is not this criterion.
4. Protection GET still matches **H50-2**, **H50-3**, **H50-5**, **H50-8**,
   **H50-9** (same list as test 3; operator re-read at merge; not inferred
   from a green `find` step; not **H50-4**; not observed flags; prior
   `updated_at` was **author-attested**).
5. No `force_merge`, no direct `main`, no CAC dismiss-as-merge, no
   Coolify/HMAC/`autonomy.rs`/contract/oracle-service/DEPLOYMENT edits in
   the #50 diff.
6. A leftover issue exists that owns S3 (because merging `#50` closes that
   number). Same as tests item 6. Required before merge, including on a
   successor.

A delete-only `#50` is not land. Missing YAML, or YAML on the PR with no
authentic WP-pre post (open a **new** local leftover plus retrigger), is
not land. A successor without `Closes #50` is not land of `#50`.

### Leftover-complete (S3) — follow-up issue; survives merge of `#50`

Keep S3 off the `#50` close gate.

1. A **dedicated** plant-check PR following Tests item 4 and
   Observability’s fail-closed pair: opened **after** the delete landed,
   `draft == false`, title has no `WIP` prefix, at least one file changed
   (necessary, not sufficient), no UI/`POST .../requested_reviewers`
   plant, 30s re-GET if both signals are empty, reviews match
   `official == true && state == "REQUEST_REVIEW" && team.name == "maintainers"`
   (not `code/maintainers`). Record `{n}` and the two JSON bodies, then
   **close without merge**. `#50`’s leftover plant does not count. `#49`’s
   plant does not count. “The next natural PR” does not count. An
   empty-diff, draft, `WIP`-prefix, manually-requested, or **merged**
   dedicated PR does not count. Operator attestation of those GETs is not
   leftover-complete without this PR.

Forgejo#48 leftovers, CAC#429, ust1-window #49 (Renovate), and #30 (EOA
governance) may stay open; they are not land or leftover-complete gates
for this tree. A **new** local enablement leftover, opened only if WP-pre
never posts after YAML is on `#50`, **is** a land gate (Decision 8), not
S3 and not a wait on #48 or closed dex#1247.

## Authority

[cl8y-agent-control#297](https://git.cl8y.com/PlasticDigits/cl8y-agent-control/issues/297):
this design does not grant deploy, spend, custody, or agent-permission
expansion. Relaxing official-review as a **forge merge gate** is already
executed on this repo by #48 (`updated_at` 2026-09-21T07:29:26Z); this ADR
only removes the in-tree file that plants requests and adds the minimal
pipeline so **H50-3** can post. Independent review of this proposal is a
later gate. Design author must not write `DESIGN: APPROVE`.
