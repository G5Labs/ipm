# UX v2 workbench — Phase 0 review

Scope: commits `4ea35445`…`e52f551d` on `ux-v2/implementation-plan` (Phase 0 of the
[plan](ontology-plan.md)). Self-review by the coordinating agent; an independent read-only
Codex run on the same diff is recorded below once harvested (its findings were not read
before this section was saved).

## Inventory (from `Ontology changes and governance`)

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| `g5.workbench.gate` — visibility only from the resolved layout; backend never branches | `routes/instance.rs` advertises `alignment_ui` from `G5_ALIGNMENT_UI_ENABLED` (fail-closed); `lib/topPanelFlags.ts::resolveWorkspaceLayout(topPanel, alignment, stored, default)`; `WorkspaceSettingsContent` shows the option only when enabled; `WorkspacePage` derives `alignmentLayout` from the resolved layout only | Inspection + unit tests (`topPanelFlags.test.ts` 13, `WorkspaceSettingsContent.test.tsx` 20, `instance::` 14); grep: no backend read of `alignment_ui` outside instance advertisement | Conforms. Stored `alignment` with the feature off resolves to the instance default (or `advanced` when the default itself is `alignment`). |
| `g5.workbench.phases.state` — explicit, owner-moved, durable event | Migration `059_workspace_phase.sql`; `WorkspaceService::set_workspace_phase` (validate → Base refused → no-op skip → UPDATE + `workspace_phase_events` INSERT in the request transaction); `update_workspace` route behind `ensure_workspace_owner` | API tests `api_workspace_phase.rs` (7: owner move + event row + no-op, non-owner 403, invalid 422, Base 409) | Conforms. **Finding F1** (fixed): the row read was not `FOR UPDATE`, so two concurrent moves could both read the same `from_phase` and record contradictory events; added `FOR UPDATE`, tests re-run (7 passed). |
| `g5.workbench.phases.state` — fork rule | `workspace_fork_service.rs`: `target_phase` resolved at admission (`be` for Base source, else source phase), frozen in `ForkRepoManifest` (`serde(default)` → `be` for older manifests), bound at finalization | Unit test (legacy default + round trip) and API tests (ordinary source copies, Base → `be`, source moved during fork keeps admitted phase) | Conforms to `g5.fork.replay`: no re-read during recovery; not in the request digest. |
| `g5.workspace.base-ui.*` — Base read-only | `WorkspacePage`: `workbenchPhase = "be"` for Base, chevrons `readOnly` for Base and non-owners; service refuses Base with `WORKSPACE_IS_BASE` | `WorkspacePageAlignment.test.tsx` (Base read-only at BE; non-owner read-only); API test | Conforms. |
| `g5.workbench.phases` — chevrons replace status band and repos bar under alignment | `statusBandEligible`/`reposBarVisible` both exclude `alignmentLayout`; `PhaseChevrons`, `ActionRibbon` (all actions disabled, `workbenchDefaults.ts` V1 set) | `WorkspacePageAlignment.test.tsx` (shell present, band/ReposBar absent; `both` unchanged), `ActionRibbon.test.tsx`, `PhaseChevrons.test.tsx` | Partial by design (no meters, inert ribbon). |
| `g5.workbench.actions` — ribbon is data | `lib/workbenchDefaults.ts` typed constant with the plan's V1 button set | Inspection | Phase 0 scope only; no committed-config loader yet (Phase 2). |
| Existing layouts unchanged | `simple|advanced|both` paths untouched except the two `!alignmentLayout` guards; `TreeNode` default-order logic now treats a persisted order equal to any canonical default as uncustomised (needed because the default is auto-persisted on first mount) | Full vitest suite 4305/4306 (the one failure is a pre-existing order-dependent `ChatPanel.test.tsx` case, passes in isolation and fails without this change too) | No regression found. The TreeNode generalisation is a small behaviour change: a user who manually restored the exact default order now follows the preset — acceptable, since the result is the default either way. |
| Migration rules (`system_design_principles` migrations and evidence) | Additive DDL; new table mirrors `workspace_pull_request_events` RLS/FORCE/grants with an added `WITH CHECK`; checksum appended; `check-migration-checksums.py` passes | Executed by the API tests against local Postgres via the SQLx migrator | `check-migration-version-uniqueness.sh` not run (needs bash ≥4). |

## Findings

- **F1 (defect, fixed)** — `set_workspace_phase` read without a row lock; see inventory. Governing: `g5.workbench.phases.state` (durable record must reflect the actual transition).
- **F2 (note)** — the branch chip shows `allSortedBranches[0].branchName`; acceptable for Phase 0, Phase 2 should show the member branch the ribbon acts on.
- **F3 (note)** — 422 (`ValidationError`) instead of the plan's 400 for an invalid phase; matches neighbouring field validation. Plan wording updated in this review.

## Unverified

- Playwright `e2e/flows/alignment-shell.spec.ts` is written but not executed: no e2e stack in this environment (`just e2e-up` needs the CI runner and Playwright images and `.env.test`). Run with `G5_ALIGNMENT_UI_ENABLED=true just e2e-up && G5_ALIGNMENT_UI_ENABLED=true just test-playwright`.
- Migration not exercised under the restricted migrator role on populated tenant tables (additive DDL only; no data section).

## Independent Codex review (read-only, gpt-6-astra, same diff; harvested after the sections above were saved)

Verdict CHANGES_NEEDED, no blockers. Triage against code:

| # | Finding | Verified | Action |
|---|---|---|---|
| 1 | MAJOR `useUpdateWorkspacePhase` rolls back/invalidates with the render-time `detailKey`; A→B navigation during a pending PATCH corrupts B's cache | Yes (`useWorkspace.ts:306–343`) | Fix: key bound in the mutation context; restore only `phase`; test with delayed rejection + navigation |
| 2 | MAJOR `FOR SHARE` in `ensure_workspace_owner` then UPDATE/`FOR UPDATE` is a lock-upgrade cycle; concurrent PATCHes can deadlock | Yes, but pre-existing for every `update_workspace` field | Out of Phase 0 scope; comment added; recorded as shared debt below |
| 3 | MAJOR event table lacks the same-org composite FK its neighbour has | Yes (`fk_wpre_pull_request_same_org`); `workspaces` has no `(org_id, id)` unique key | Fix in 059: `uq_workspaces_org_id_id` + composite FK; checksum recomputed; FK-violation test |
| 4 | MINOR failed phase move rolls back silently | Yes | Fix: inline error in `PhaseChevrons` + test |
| 5 | MINOR Base renders the (disabled) ribbon; plan says no template actions on Base | Yes | Fix: omit ribbon for Base; test |
| 6 | MINOR clone list DTO serializes `phase: null`; zod rejects null | Yes (`workspaces.rs` list DTO lacks `skip_serializing_if`) | Fix: skip attribute + contract test |
| 7 | MINOR `workbenchDefaults.ts` introduces `stage: planning\|finalizing`, a new consumer of the model Phase 3 removes | Yes | Fix: `phase?: WorkspacePhase` |
| 8 | MAJOR verification gaps: rollback/navigation, `simple`/`advanced` integration cases, sidebar preset vs custom order, event-insert failure, Playwright unexecuted | Yes | Tests added for the first three; event-insert failure and the Playwright flow remain unverified (below) |

Codex also confirmed: single transaction for write + event, ownership and Base checks, frozen manifest phase with legacy default, unchanged request digest, fail-closed layout resolution, no backend branch on the UI flag, no weakened or deleted tests.

## Remaining gaps

| Governing ontology | Gap | Proposed action | Uncertainty / alternatives |
|---|---|---|---|
| `g5.workbench.phases.state` | Concurrent PATCHes on one workspace can deadlock (lock-upgrade after `FOR SHARE`); one request fails and the client rolls back | Pre-existing pattern shared by all `update_workspace` fields; address once for the route (take the write lock first or retry on 40P01) outside this change | Could be folded into Phase 3, which already touches workspace mutation paths |
| `g5.workbench.gate` | Playwright `alignment-shell.spec.ts` unexecuted | Run against the test stack with `G5_ALIGNMENT_UI_ENABLED=true` before merge | Needs the CI runner/Playwright images locally or CI |
| `g5.workbench.phases.state` | Event-insert failure rollback not forced in a test | Covered by the single transaction by construction; a fault-injection test would need a failing constraint | Low value for Phase 0 |

## Rechecks (fix commit `a1c1f1b1`)

- Findings 1, 3–8 fixed as proposed; finding 2 left as recorded debt with a comment at the read. Deviation: `uq_workspaces_org_id_id` already exists in the baseline (`001_lean_baseline.sql:6050`), so only the composite FK was added to 059 and its checksum replaced.
- Re-run by the coordinator after the commit: `cargo test -p g5-api --test api_workspace_phase` 8 passed (incl. cross-org FK rejection); `python3 scripts/check-migration-checksums.py` matches; `npx tsc --noEmit` clean; `cargo fmt --all --check` clean; vitest `WorkspacePageAlignment` 9, `useUpdateWorkspacePhase` 2 (delayed 403 + A→B rerender: A rolled back with newer fields kept, B untouched), `TreeNode.sectionOrder` 4, `contract` 35, `PhaseChevrons` 5, `ActionRibbon` 5, `useWorkspace` 20, `topPanelFlags` 13 — all passed. Implementer's full `npx vitest run`: 4315/4316, the one failure pre-existing and order-dependent (`ChatPanel.test.tsx` account case; fails on main too).
- Still unexecuted: Playwright `alignment-shell.spec.ts` (both flag states) — needs the e2e stack; the migration under the restricted migrator role on populated tables.

## Post-push review (PR #332, `codex review --base main`)

One P2: `mutationFn` still closed over the render-time `workspaceId`, so a move paused offline and resumed after navigating A→B would PATCH B (React Query resumes a paused mutation with the latest `mutationFn`). Fixed: the target workspace rides in the mutation variables (`{workspaceId, phase}`), bound at `mutate()` time; the public `mutate(phase)` shape is unchanged. Regression test added (`onlineManager` offline → mutate on A → rerender B → online ⇒ PATCH hits A, B untouched). `useUpdateWorkspacePhase` 3, `WorkspacePageAlignment` 9, `useWorkspace` 20, workbench components 10 — passed; `tsc` clean.

## Metadata

`g5.workbench.gate` 100%, `g5.workbench.phases.state` 85%, `g5.workbench.phases` 35% as recorded by the implementation commit remain accurate after the fixes (locations unchanged; the composite FK and the keyed rollback close the integrity and cache gaps noted above; the browser flow remains the unverified part).

---

# Phase 1 review — timeline landing

Scope: commits `381ed842`…`37ebd6a3` on `ux-v2/phase-1-timeline` (stacked on Phase 0 at `aa71cb53`).
Self-review by the coordinator plus an independent read-only Codex run on the same diff (findings
below were harvested after the self-review notes were written).

## Inventory

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| `g5.workbench.landing` — timeline is the landing page under alignment | `WorkspacePage.tsx` landing effect (once per workspace after tab restoration; `?view=`/`?session=`/doc/file/job deep links win), Activity tab pinned first | `WorkspacePageAlignment.test.tsx` (+3), classic layouts asserted unchanged | Conforms; **F1** review-copy effects ran after it and re-selected Review — policy made explicit (Review first on review copies, Activity second, both pinned). |
| `g5.workbench.landing` — every entry from a durable record | `workspace_activity.rs` `PhaseChanged` ← `workspace_phase_events`, `PullRequest` ← `workspace_pull_request_events` ⋈ `workspace_pull_requests` (source workspace), same per-source `(created_at, key) < cursor` + LIMIT and merged sort | `api_workspace_timeline.rs` 4 (phase move → entry with actor; seeded PR event → entry; interleaving; other org excluded); unit serialization test | Conforms. **F2** multi-repo PRs attributed every event to the first provider PR — provider fields now only when exactly one provider repo. **F3** no multi-page mixed-source pagination test — added. |
| `g5.workbench.landing` — status header | `TimelineStatusHeader` (phase hue/label, Base read-only, primary issue, purpose: issue body → description → README via existing `useFileContent` committed read → none); no approval state, no `approvalStages.ts` | `TimelineStatusHeader.test.tsx` 8 | **F4** header was inside the feed scroller which scrolls to bottom on load — moved outside. **F5** README read could fire before Activity resolved; detail (comments) failure discarded title/body — gated on Activity resolution, detail error falls through. |
| `g5.workbench.gate` — visibility from the resolved layout only | header and landing derive from `workspaceLayout === "alignment"`; backend unconditional | Inspection | Conforms. Codex flagged the two feed kinds rendering under classic layouts; judged a plan clarification, not a defect: they are durable data and now stated as every-layout in the plan. |
| External issues — discussion stays at the primary issue | feed references issue/PR records, links out; no new comment store | Inspection | Conforms. |
| Justify each new mechanism | no new route/store/flag; two feed variants, one component, one helper | Inspection | Conforms. |

## Findings (Codex run, triaged)

| # | Finding | Verified | Action |
|---|---|---|---|
| 1 | MAJOR new entries visible under classic layouts | Behaviour confirmed; plan wording was ambiguous | Plan clarified: feed kinds are data under every layout |
| 2 | MAJOR review-copy effects override Activity ordering/selection | Yes (`WorkspacePage.tsx` ~9475 vs ~9595) | Fixed: explicit policy + tests |
| 3 | MAJOR header scrolls out of view on populated feeds | Yes (`scrollTop = scrollHeight` on load; header inside scroller) | Fixed: header outside the scroller + test |
| 4 | MAJOR multi-repo provider events attributed to the first provider PR | Yes (lateral join LIMIT 1; lifecycle writes per repo) | Fixed: provider fields only with one provider repo; aggregate link otherwise + API test |
| 5 | MINOR duplicate tracker read / premature README request | Yes | Fixed: README gated on Activity resolution; detail error falls through |
| 6 | MAJOR pagination/navigation coverage | Yes | Multi-page mixed-source API test and review-ordering tests added; Playwright still unexecuted |

Codex also confirmed: tenant scoping/RLS, actor-name batching, serialization, phase-PATCH invalidation, page-size-plus-one merge by `(timestamp, key)`, source-only PR projection, README committed read without a VM, no stage dependency, no competing discussion store, no weakened tests.

## Rechecks (fix commit `c74b9487`)

Re-run by the coordinator after the commit: `cargo test -p g5-api --test api_workspace_timeline` 6 passed
(incl. two-provider-repo PR → null provider fields; 63-entry mixed-source walk across 2 pages with no
duplicates or gaps), `--test api_workspace_phase` 8 passed; `npx tsc --noEmit` clean; vitest
`TimelineStatusHeader` 9, `WorkspaceActivityPanel` 12 (header outside the scroller, DOM order),
`WorkspacePageAlignment` 13 (review copy → Review first + Activity pinned second), `WorkspacePageReviewTab` 7,
workbench components 10 — 51 passed. Implementer's full `npx vitest run`: 4331/4332, the one failure the
known order-dependent `ChatPanel.test.tsx` case. Still unexecuted: Playwright `alignment-shell.spec.ts`.

## Metadata

`g5.workbench.landing` 55% as recorded by the implementation commit remains accurate after the fixes
(header, landing, two feed kinds; approval state and the Phase 2 Analyze-menu route to the Progress tab
outstanding; browser flow unverified). `g5.workbench.phases.state` gains its first reader (the feed).

---

# Phase 2 review — templated actions and personas

Scope: commits `7cec9c77`…`9a45ec8a` on `ux-v2/phase-2-actions` (off main `621e842e`; Phases 0 and 1
merged). Self-review by the coordinator plus an independent read-only Codex run on the same diff
(findings harvested after the self-review notes were written).

## Inventory

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| Durable Git boundaries — `.g5/workbench/` is repository content | `g5_paths.rs` allow-list (`actions.yaml`, `personas/<name>.md` only), SCM exclude rules, exact-tree commit acceptance | unit tests (9 + 2), committed read in `api_workbench.rs` | Conforms for the classifier; **F7** native exclude patterns unignore the directory's other children (dormant: no production consumer of the constants) — tightened + `git check-ignore` test. |
| `g5.workbench.actions` — templates as data, user-configurable, one committed snapshot | `workbench_config.rs` built-ins + committed override (replace per phase, inherit by id, invalid ⇒ built-ins + error, `revision = docs head . builtins hash`), `GET /workspaces/:id/workbench`; built-in texts in `workbench_builtins/` | `api_workbench.rs` 6, unit 7 incl. frontend-fallback drift | Conforms. **F6** inherit-by-id resolved only within the same menu — phase-wide lookup. **F3** action resolved by persona-preferred phase, ids repeat across phases — resolve exactly `(phase, actionId)`, phase in identity. |
| `g5.conversation.inbox.reservation` — one exact input revision; identity claimed before effects | `WorkspaceMessenger::claim` / `send_claimed`; start route: claim → load config at `configRevision` (409 `WORKBENCH_CONFIG_STALE`) → expand → send, one transaction so a failure rolls the claim back; legacy digest unchanged when workbench fields absent | `api_workbench_chat.rs` 7 (incl. stale revision, replay without re-reading config) | Conforms for ordinary starts. **F2** follow-view resolved the transcript boundary before the claim (identity depended on mutable source state; empty sources collided) — claim first, freeze the decision, replay stored result. **F5** browser generated the request key inside `mutationFn` — key and payload frozen per submission. |
| `g5.conversation.inbox.prompt` — frozen bounded prompt, 512 KiB | `persona_instructions` frozen on the session; `bootstrap_persona_prefix` at reservation prefixes the first delivered input when no delivered/reserved input or transcript row carries it | inspection + API tests (withdraw-then-resend) | **F1 (blocker)** an undelivered input edited to just under the cap overflows when prefixed; the reservation error left the task unsettled and aborted the tenant's dispatch pass, re-selected every pass — expanded size validated at enqueue/edit, overflow at reservation settled as an isolated visible failure. |
| `g5.workbench.persona` — one persona per session; follow view carries context | `chat_sessions.persona`/`persona_instructions` (migration 060, paired-null CHECK); fork copies or sets; follow view = fork + persona + introduction input in one transaction | `api_workbench_chat.rs` | Conforms (see F2). Design note from the implementer: the queued introduction input costs an acknowledgement turn; the frozen-instructions + delivery bootstrap alone would also hold the invariant — left for the human. |
| `g5.workbench.mock` — Preview mock is the ordinary preview with the `mock` profile | `PreviewControl` `fixedTarget` (same → open, different → "Close and start", none → start); enablement from the preview-profile list at page load; docs-only disabled | `PreviewControl.test.tsx` 19 | **F4** with a matching preview already running, an ordinary Close triggered an automatic restart — auto-start only on the initial resolved state or an explicit close-and-start; `switchPending` cleared on cancel failure. |
| Ribbon dispatch reuses owners | `WorkspacePage.handleWorkbenchAction`: approvals tab (unfiltered; Phase 3), `PrepareToShipModal` for ship, PR merge path for merge (never the rebase-only handlers), doc create/upload, Progress tab; Base renders no ribbon | `WorkspacePageAlignment.test.tsx` 20 | Conforms. |
| `g5.workbench.gate` — visibility from the layout only; backend data under every layout | config route and persona fields layout-agnostic; ribbon only under alignment | inspection | Conforms. |
| Justify each new mechanism | durable path entries, one route + loader, two session columns, DTO fields, one hook, ribbon dispatch; `kind: menu` added for dropdowns | inspection | Conforms; **F8** duplicated chat-start cache handling — shared success handler. |

## Findings (Codex run, triaged)

| # | Finding | Verified | Action |
|---|---|---|---|
| 1 | BLOCKER persona-prefix overflow stalls tenant dispatch | Credible (error path at reservation; pass aborts) | Fixed: validate expanded size at enqueue/edit; isolated failure at reservation; test other session still dispatches |
| 2 | MAJOR follow-view identity computed before claim | Yes | Fixed: claim first, freeze decision, replay stored result |
| 3 | MAJOR persona override resolves another phase's prompt | Yes (ids repeat across phases) | Fixed: `(phase, actionId)` resolution, phase in identity |
| 4 | MAJOR fixed-target preview auto-restarts after Close | Yes | Fixed + tests |
| 5 | MAJOR request key generated per retry | Yes | Fixed: key + payload frozen per submission |
| 6 | MINOR inherit-by-id scoped to the same menu | Yes | Fixed: phase-wide lookup |
| 7 | MINOR exclude patterns broader than classifier | Yes (dormant) | Fixed + `git check-ignore` test |
| 8 | MINOR duplicated chat-start cache handling | Yes | Fixed: shared handler |
| 9 | MAJOR test gaps | Yes | Legacy-digest golden, carry, edited-input delivery, deep-link/restoration page tests added; Playwright and live-provider receipt of the prefix remain unexecuted |

Codex also confirmed: single-transaction claim/resolve with rollback, legacy digests unchanged, one committed snapshot with explicit invalid state, substantive fallback drift check, additive migration with persona/instruction pairing, Base chat omission, merge routing, no unrelated test weakening.

## Rechecks (fix commit `e9fea214`)

Re-run by the coordinator: `cargo test -p g5-api --test api_workbench 6 --test api_workbench_chat 12
--test api_workspace_phase 8 --test api_workspace_timeline 6 --test route_auth_static 12` passed;
`g5-contracts`/`g5-scm`/`g5-api-core` unit tests for paths, exclude patterns (`git check-ignore`),
config and digest passed; `npx tsc --noEmit` clean; vitest `useWorkbench` 7, `PreviewControl` 21,
`WorkspacePageAlignment` 25, workbench components + message builders 26 — 79 passed. Implementer's
full `npx vitest run`: 4426/4428, both failures the known order-dependent `ChatPanel.test.tsx` cases
("clamps a stale saved effort" passes in isolation). Finding 7's "installation point" was disputed and
confirmed: `G5_INTERNAL_EXCLUDE_PATTERNS` has no consumer in this repo, so the test installs the
patterns itself; the same broader-than-classifier gap pre-exists for `preview_profiles/` and
`explainer_templates/` and is out of scope. Still unexecuted: Playwright `alignment-shell.spec.ts`;
live-provider receipt of the persona prefix (verified at reservation/DB level only).

## Metadata

`g5.workbench.actions` 85%, `g5.workbench.persona` 80%, `g5.workbench.mock` 80% as recorded by the
implementation remain accurate after the fixes (per-turn adapter injection and the browser flow are the
outstanding parts).

---

# Phase 3 review — approvals bound to phases

Scope: commits `4f1517e5`…`b3dd932a` on `ux-v2/phase-3-approvals` (off main `77687360`; Phases 0–2
merged). Self-review by the coordinator plus an independent read-only Codex run on the same diff
(findings harvested after the self-review notes were written).

## Inventory

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| `g5.workbench.gates` — approvals bind to zero or more phases; stage model removed | migrations 066 (additive `phases[]`, `approval_statuses.phase`, nullable `approval_requests.phase`), 067 (owner-form data cutover: backfill per the plan's table, pending requests → superseded, active runs → cancelled, stage columns dropped), 068 (`workspaces.stages` dropped, admission columns, phase-aware validator beside the baseline one); `approval_config_service` per-phase chains, phase-aware fingerprints | `migration_066_approval_phases.rs`, `api_approval_phase_cutover.rs`, `api_org_approvals` 8, `api_workspace_approvals` 19, `approval_review_contract` 12 | Conforms; backfill verified against the plan's table (both→{be,ie}, planning-only either fallback→{be}, finalizing-only→{ie}, neither→{}; historical decisions keep their mapped phase). Split into three files because the RLS guard accepts the owner form only in a file with nothing else — justified. **F6** fixture coverage thin (one stage count, one run state) — widened. |
| `g5.governance.approval.requirements` — only IE requirements gate the merge, against the configured target and pinned revision, before any provider merge and at local application | `job_merge_service::admit_target_merge` (lock + evaluate + require) at prepare for target-advancing merges; rebase-only applications carry no evidence; application re-reads recorded evidence under the commit fences and checks it is bound to the cut (fails closed when absent); PR route `admit_pull_request_merge` before the provider call with evidence pinned on the PR; provider-import prepare evaluates afresh when nothing was recorded (an admission, not a bypass) | `api_workspace_pull_requests` 60 (+3 admission), inspection | **F1 (blocker)** the provider operation compared the PR's published heads with the current workspace heads, never with the admitted `sourceHeads`, so a push + PR refresh between admission and the provider call could merge an unapproved revision — validate the admission's full identity against the provider merge plan under the PR lock before any mutation. **F3** recorded admission reused only when provider status is exactly `merged`; `partial` acceptance or a lost acknowledgement re-evaluated and could refuse an irreversible merge — reconcile against the frozen admitted cut. |
| Plan's locking commitment — attach/retire/publish/withdraw and admission share one ordered lock protocol | `lock_merge_admission` shares locks with attach/retire only | inspection | **F2** definition publishing and decision/withdrawal insert without the locks; admission could combine different policy snapshots — one ordered protocol for every writer. |
| `g5.fork.replay` — terminal outcomes replay after the digest change | fork request digest v2 without `stages`; both v1 shapes accepted for records admitted before the cutover | `api_workspace_fork_names`, fork tests | Conforms. |
| `g5.governance.approval.version` — stale binding unchanged | `reviewed_git_heads` untouched | inspection | Conforms. |
| Frontend reads the server verdict only | `mergeAdmission` DTO on approvals state and PR summaries; ship modal, Home, PR list, activity, chevron meters, Clone dialog without the Governance-model selector, three-phase definition picker, approvals tab `?phase=` | 11 vitest files | **F4** decisions invalidate only the approvals cache; PR summaries and Activity stay stale and the ship modal prefers the cached PR verdict — invalidate all dependents. **F7** residual client readiness recomputation and "missing admission = allowed" in the PR list — unknown state instead. **F8** `?phase=` in the URL overrides the ribbon's selection — one authority. **F9** timeline header still phase-only — add the phase's approval state. |
| `g5.workbench.gate` — feature enabled everywhere | compose dev/test, both CI API steps and the Playwright env lists, helm QA, `just test-playwright`; instance default layout unchanged | grep | Conforms. **F5** `org-approvals.spec.ts` toggles phase checkboxes inside the collapsed Advanced section — flow cannot pass as written. |
| Justify each new mechanism | evaluator + recorded evidence, v2 digest, phase fields, a startup one-shot for orphaned agent-review jobs (reuse mismatch: a running attempt only receives the cancel intent; the job turns terminal via the reconciler; pass retried at next start) | inspection | Conforms; the one-shot is the smallest path to the existing task cancel; recorded as a metadata gap per the implementer's amendment proposal. |

## Findings (Codex run, triaged)

| # | Finding | Verified | Action |
|---|---|---|---|
| 1 | BLOCKER provider merge not validated against the admitted heads (TOCTOU) | Yes | Fixed: admission identity checked against the provider merge plan under the PR lock before mutation; concurrent-update regression |
| 2 | MAJOR publish/withdraw outside the admission locks | Yes | Fixed: one ordered protocol for all writers + serialization tests |
| 3 | MAJOR partial acceptance / lost ack re-evaluates | Yes | Fixed: reconcile against the frozen admitted cut; admission preserved through `partial` |
| 4 | MAJOR stale ship verdicts and timeline after decisions | Yes | Fixed: invalidate PR summaries (both workspaces), approvals state and activity; freshest verdict wins |
| 5 | MAJOR org-approvals Playwright flow toggles hidden checkboxes | Yes | Fixed: open Advanced first |
| 6 | MAJOR failure-path coverage | Yes | Added: migration matrix, all active run states, running-attempt cancellation, stale-IE direct merge, concurrent policy change, interrupted provider recovery, UI refused merge |
| 7 | MINOR client readiness fallbacks | Yes | Fixed: unknown state, no recomputation |
| 8 | MINOR `?phase=` vs ribbon authority | Yes | Fixed: ribbon writes the URL |
| 9 | MINOR header lacks approval state | Yes | Fixed |
| 10 | NIT duplicated phase vocabulary | Yes | Fixed |

Codex also confirmed: the migration mapping, nullable historical request phases, preserved historical validators, v1 digest compatibility, admission invoked on direct/local merges and on the provider-import fallback (not a bypass), timeline entries from existing records, complete flag flips incl. Playwright environments.

## Amendment proposals (from the implementer, not applied to block text)

- `g5.workbench.gates`: a running agent review's job becomes terminal only after the reconciler delivers the cutover's cancel intent — recorded as a metadata gap.
- `g5.governance.merge`: a retry for the same source cut reuses the recorded admission rather than re-evaluating — now also implied by the F1/F3 fixes; worth one sentence in the block body at the next ontology pass.

## Rechecks (fix commit `ed9ca3e7`)

All ten findings held on verification and were fixed; while verifying F6 the implementer found one more
real gap — the legacy direct merge route (`POST /workspaces/:id/merge` →
`merge_cloned_workspace_into_parent`) performed no admission at all — and closed it (admits for the
exact cut under the policy locks and records the evidence). Re-run by the coordinator:
`cargo test -p g5-api --test api_workspace_pull_requests` 64 passed (incl. revision-changed-since-admission
refused with no provider `PUT`, lost-ack and partial-acceptance retries landing on the recorded
admission, stale-IE direct merge refused), `--test api_approval_policy_locks` 3, `--test
api_approval_phase_cutover` 4 (every active run state cancelled, late settlement refused; running attempt
ends through the reconciler), `--test api_workspace_approvals` 19, `--test migration_066_approval_phases`
1 (stage count × flags × fallback, all run states); `npx tsc --noEmit` clean; vitest across the touched
files 164 passed (CreatePullRequestDialog, PrepareToShipModal, PullRequestList, useWorkspaceApprovals,
TimelineStatusHeader, workbench components, WorkspacePageAlignment, HomePage, approvalPhases). Note: the
PR suite fails under resource contention when several DB-heavy binaries run in one `cargo test`
invocation against the shared dev Postgres (timeouts, not assertions); alone with `--test-threads 4` it is
green. Implementer's full vitest: expected 4427/4428 (known `ChatPanel` case).

Deviations accepted: partial provider acceptance is modelled through the aggregate a failed sibling
leaves, not a second mocked provider repository; no page-level "approve ⇒ Merge enabled without reload"
vitest (hook-level invalidation test + the Playwright flow); an *unknown* merge verdict now disables
Merge until refreshed (tightens the earlier "pending never gates" posture only for the unknown state —
flagged for the human). Still unexecuted: Playwright `org-approvals.spec.ts` and `alignment-shell.spec.ts`
(no CI images locally); the cutover against a production-shaped database; a live provider merge.

## Metadata

`g5.workbench.gates`, `g5.governance.approval.requirements`, `g5.governance.merge` and
`g5.governance.approval.version` metadata as recorded by the implementation remain accurate after the
fixes; the admission-before-provider-mutation and recorded-admission-reuse behaviour now has executed
API evidence. Open: the two amendment proposals above for the next ontology pass.

---

# Phase 5 review — Home stepper and badges

Scope: commit `212c65d9` on `ux-v2/phase-5-home` (stacked on Phase 3 at `678777da`). Frontend only.
Self-review by the coordinator plus an independent read-only Codex run on the same diff.

## Inventory

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| `g5.workbench.gate` — visibility from the resolved layout only | `HomePage` resolves the layout exactly as `WorkspacePage` (`isTopPanelEnabled` + `isAlignmentUiEnabled` + stored setting + instance default → `resolveWorkspaceLayout`), fail-closed while instance info is unavailable; classic rows unchanged | `HomePage.test.tsx` (classic default, feature-on + stored `both`, stored `alignment` without the feature) | Conforms. |
| `g5.workbench.phases` — stepper mirrors the chevrons | `HomePhaseStepper` (done/active/todo, phase hues, per-phase meter reusing `PhaseApprovalMeter` over the approvals state Home already fetches per owned row — no new queries) | tests; inspection | Conforms; **F5** icon mapping duplicated from `PhaseChevrons` — shared export. |
| `g5.workbench.phases.state` — owner-moved, Base read-only | chip click PATCHes via `useUpdateWorkspacePhase` only for `!isBase && owner`, then navigates | tests (owner PATCH + navigate; Base navigate only) | **F1** the Home-started mutation's pending/error state is invisible to the destination page (separate hook instance) and a second write can start before it settles — shared mutation key, `useMutationState` on both pages, overlapping writes disabled. |
| Badges — positive evidence only, no page walking | Working/Idle from `concurrentTasks`; Needs-user-input from the first loaded attention page of the active filter; nothing while loading/errored/zero | tests | Conforms; **F2** a narrowed filter changes the count without explanation — scope text on the badge. |
| Accessibility | chips are buttons with labels; reduced motion handled | inspection | **F3** Base chips announce disabled yet navigate; meter absent from the accessible name — fixed. |
| Justify each new mechanism | one component, no new state/routes/flags | inspection | Conforms. |

## Findings (Codex run, triaged)

| # | Finding | Verified | Action |
|---|---|---|---|
| 1 | MAJOR mutation state lost across navigation; overlapping writes | Yes | Fixed: `mutationKey` per workspace + `useMutationState` on both pages; clicks disabled while pending; delayed success/failure/second-selection tests |
| 2 | MINOR filtered first-page count unexplained | Yes | Fixed: scope description on the badge; filter-switch test |
| 3 | MINOR Base chip semantics / meter not announced | Yes | Fixed |
| 4 | MINOR test gaps (delayed cases, request count, `simple`/`advanced`, instance-info states, Playwright persistence) | Yes | Added; Playwright still unexecuted |
| 5 | NIT duplicated icon mapping | Yes | Fixed |

Codex also confirmed: fail-closed layout resolution, no approvals N+1, correct ownership/Base guards, all three attention kinds counted, reduced motion, classic markup unchanged, no backend branching.

## Rechecks (fix commit `009b6139`)

Re-run by the coordinator: `npx tsc --noEmit` clean; vitest `HomePage` 57, `WorkspacePageAlignment` 29,
`useUpdateWorkspacePhase` 5, workbench components 24 — 115 passed. The shared per-workspace mutation key
(`workspacePhaseMutationKey`) with `useMutationState` replaces the per-instance state; one pre-existing
hook test was re-targeted accordingly (an instance re-rendered for workspace B no longer reports A's
refusal — intended: B's chevrons must not show A's error; the cache rollback assertions are unchanged).
Implementer's full `npx vitest run`: 4447/4448 (known `ChatPanel` case). Still unexecuted: the Playwright
Home flow (now awaits the PATCH, asserts the timeline entry and reloads for persistence).

Screenshots of the branch in a running stack (`~/Downloads/phase5-screenshots/`, seeded forks in BE/SE/IE,
one approval decided, one running task) confirmed the stepper, meters and badges and exposed one nit: the
classic "N of M approvals complete" line (IE merge-admission count) rendered beside a settled BE meter; hidden
under the alignment layout in `1568b100` (classic unchanged; `HomePage` 57 passed).

## Metadata

`g5.workbench.phases` and `g5.workbench.phases.state` metadata as recorded by the implementation remain
accurate after the fixes.

---

# Phase 6 review — Home issues and running-compute tiles

Scope: commits `3da67274`…`fcf051d8` on `ux-v2/phase-6-home-tiles` (stacked on Phase 5 + plan commit
`af2768e9`). Self-review by the coordinator plus an independent read-only Codex run on the same diff.

## Inventory

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| Security — tenant isolation; member reads filtered to readable workspaces | `worker_host_service::list_active_workspace_vms` (the admin inventory SQL extracted, LEFT JOIN `worker_hosts` for slot size) with `OrgWide` (org owner/admin, same rule as the workspace list) or `ReadableBy(caller)` = `visibility = 'org'` OR owner OR `workspace_members`; `GET /orgs/:id/compute/running` behind `scoped_db_for_org` + `workspaces:read`; member DTO without e-mail; owner enrichment only for authorized ids | `api_compute_running.rs` 2 (visible/invite-only/hidden/stopped/deleted/foreign excluded; cross-org, unscoped, unauthenticated refused), `route_auth_static` 12, admin suite 3 unchanged | Conforms; Codex confirmed predicate equivalence and the admin wire shape. |
| `g5.workbench.gate` — alignment-only tiles; classic Home byte-identical | `HomePage` 2×2 under alignment; `HomeTileParts` header extracted with identical defaults/testids | `HomePage.test.tsx` 64 (classic: neither tile, no compute request) | Conforms. |
| External issues — links are the only writes; one primary per workspace/issue | `HomeIssuesTile` Start (existing fork-with-issue), Link existing via `POST /issue-links`, 409 `workspace_has_primary` → explicit related link | tests | Conforms; **F7** conflict presentation diverges from `WorkspaceIssuesPanel` (`issue_has_primary` path) — shared helper. **F3/F4/F5** View-all lands on a disabled search; label trap; domain failures shown as empty — fixed. **F6** quota: D requests per filter per cold visit — reshaped to one query per domain, client-side filters, 5-min stale, explicit refresh. |
| Lifecycle — explicit owner stop; never claim capture succeeded without evidence (`g5.lifecycle.deadline.loss`) | `HomeComputeTile` Shut down via `useStopWorkspaceVm` | tests | **F1** toast claimed "stopped / flush recorded" regardless of outcome and the control re-enabled before the inventory refresh — outcome-aware copy, disabled until the refresh. No stop confirmation exists in the client (workspace page stops directly) — accepted as consistent. |
| Browser evidence | `alignment-shell.spec.ts` four-tiles test | — | **F2** the fixture clone was never started, so the compute case always skipped; ambiguous locator — fixed; still unexecuted locally. |
| Justify each new mechanism | one read over the existing query; no tables/flags/settings | inspection | Conforms. |

## Findings (Codex run, triaged)

| # | Finding | Verified | Action |
|---|---|---|---|
| 1 | MAJOR stop toast claims success/flush without evidence; control re-enabled early | Yes | Fixed: outcome-aware copy, disabled through refresh |
| 2 | MAJOR Playwright compute case always skipped; ambiguous locator | Yes | Fixed: started fixture, Shut down through Home asserted |
| 3 | MINOR View all → disabled search | Yes | Fixed: filters passed, search triggered |
| 4 | MINOR label-filter trap on assignee switch | Yes | Fixed |
| 5 | MINOR domain failures rendered as empty | Yes | Fixed: shared error notice, retry |
| 6 | MINOR GitHub quota from auto-search | Yes (D per filter per cold visit) | Reshaped: one query per domain, client-side me/none, 5-min stale, explicit refresh; auto-load on visit flagged to the human |
| 7 | MINOR conflict presentation duplicated/diverging | Yes | Fixed: shared helper |
| 8 | NIT MiB→GB label | Yes | Fixed |

Codex also confirmed: bound UUIDs, org-scoped joins, authorized-only enrichment, no e-mail in the member DTO, visibility equivalence, admin shape preserved, read correctly outside the self-hosted gate, classic markup identical, polling cadence, no synthesized records.

## Rechecks (fix commit `a1084c7b`)

Re-run by the coordinator: `npx tsc --noEmit` clean; vitest `HomePage` + `components/home` +
`components/external-issues` 161 passed; `api_compute_running` 2 passed. The stop toast is now
outcome-aware on both Home and the workspace page (`lib/workspaceStopToast.ts`); per the human decision
recorded in `g5.lifecycle.deadline.loss` metadata no loss notice is shown — the copy simply makes no
flush claim. Deviation accepted: no browser case for Link existing / Start workspace (no issue-tracker
fixture exists in the e2e stack; vitest covers both). Implementer's full `npx vitest run`: 4463/4464
(known `ChatPanel` case). Still unexecuted: the Playwright four-tiles + Shut-down-through-Home flow.
Flagged to the human: the Issues tile still auto-loads one GitHub search per connected repository on
each alignment Home visit (5-minute stale window, explicit Refresh).

## Metadata

`g5.workbench.deferred` metadata as recorded by the implementation remains accurate after the fixes.

---

# Phase 7 review — Tab shell and organization fork picker

Scope: branch `ux-v2/tab-shell` off `origin/main` `8e5f0726`. Self-review by the implementer plus two
independent read-only Codex runs on the same diff (correctness/design and code duplication), each
given only the current files.

## Inventory

| Commitment | Actual path | Check | Result |
|---|---|---|---|
| `g5.workbench.tabs` — route is the active tab; left-neighbour-else-Home close; per-user browser list; one mounted page | `hooks/useWorkspaceTabs.tsx`, `lib/workspaceTabs.ts`, `components/layout/WorkspaceTabStrip.tsx` rendered by `AppShell` | `WorkspaceTabStrip.test.tsx` (13), `workspaceTabs.test.ts`, `shellRoutes.test.ts` | **F2** right-neighbour fallback contradicted the block — fixed; **F3** another browser tab removing the routed workspace's tab left it tabless — the route re-adds it; the close-then-navigate race that re-added a just-closed tab is fenced by `closingRef`. |
| `g5.workbench.tabs` — a workspace restores its own center tabs when shown again | `WorkspacePage.tsx` tab-switch effect | `WorkspacePageAlignment.test.tsx` "restores each workspace's own center tabs" | **F1** the page cleared center tabs on every in-page switch and restored only on first mount, so a tab switch lost the destination's tabs — fixed: every switch restores the destination's own persisted record (keyed by workspace id, so nothing leaks across workspaces); the getting-started guide still opens only on first mount. Applies under every layout (the switcher navigates the same way). |
| `g5.workbench.tabs` — "+" is the organization fork picker; Fork switches the session organization, then the ordinary dialog | `pages/ForkPickerPage.tsx` at `/fork`, `hooks/useSwitchOrgThen.ts`, `CloneDialog` | `ForkPickerPage.test.tsx` (4) | Conforms; **F5** the extracted lineage helper had made the fork dialog's target tree Base-first too — now opt-in (`baseFirst`) for the picker only. |
| `g5.workbench.gate` — visibility from the resolved layout only; classic layouts unchanged | `hooks/useWorkspaceLayout.ts` (one resolution shared by Home, the workspace page and the provider) | `WorkspacePageAlignment.test.tsx` classic cases, `HomePage.test.tsx`, strip hidden cases (await instance info before the negative assertion) | Conforms. **F4** `/account/github` classified as a workspace route and would have become a tab — static top-level routes are excluded (`shellRoutes.ts`). |
| Reserved workspace names | `RESERVED_WORKSPACE_NAMES` (backend) vs `/fork` | inspection | `/fork` is a static top-level route beside `/projects`; no new reserved name. |
| Accessibility | strip tablist | lint (`jsx-a11y`), `WorkspaceTabStrip.test.tsx` arrow keys | **F6** a focusable close control inside a button — now a sibling button; Left/Right move focus along the tablist (manual activation). |
| Justify each new mechanism | no backend, no setting, no flag; one provider, one localStorage list | inspection | Conforms. Shared helpers extracted rather than duplicated (below). |

## Findings (Codex runs, triaged)

| # | Finding | Verified | Action |
|---|---|---|---|
| C1 | MAJOR switching workspaces loses their center tabs (`WorkspacePage` restore only on first mount) | Yes (pre-existing for the switcher; consequential once tabs exist) | Fixed, test added |
| C2 | MAJOR first-tab close fell to the right neighbour, not Home | Yes | Fixed, tests corrected |
| C3 | MAJOR storage sync could leave the routed workspace without a tab | Yes | Fixed (route re-adds), test added |
| C4 | MINOR `/account/github` became a fake workspace tab | Yes | Fixed, classification tests |
| C5 | MINOR tree helper changed classic fork dialog ordering | Yes | Fixed (`baseFirst` opt-in), picker fixture discriminates |
| C6 | MINOR nested interactive control; no arrow keys | Yes | Fixed, keyboard test |
| C7 | MINOR remembered organization survived a user change | Yes | Fixed (scoped to user and memberships) |
| C8 | MINOR `/fork/` lost the strip | Yes | Fixed (`matchPath`), test |
| C9 | MINOR negative tests could pass during initialization; picker fixture did not discriminate Base-first | Yes | Fixed (await instance info; `aardvark` root) |
| D1 | MAJOR test JWT builder copied from `test/render.tsx` | Yes | `createTestJwt(user, orgId?)` exported and reused |
| D2 | MINOR `workspacePagePath` existed in `reviewWorkspace.ts` | Yes | Moved to `lib/workspacePagePath.ts`, re-exported; used by tabs, picker, projects page |
| D3 | MINOR layout resolution composed three times | Yes | `useResolvedWorkspaceLayout` shared by Home, workspace page, provider |
| D4 | MINOR tolerant localStorage list read/write copied from `recentWorkspaces.ts` | Yes | `lib/storedList.ts` shared by both |
| D5 | MINOR Create Organization gate duplicated from the Account flyout | Yes | `lib/orgCreation.ts` shared; the card shell stays separate (different content) |
| D6 | MINOR switch-organization-then-continue repeated from `ProjectsPage` | Yes | `hooks/useSwitchOrgThen.ts` shared by both pages |
| D7 | MINOR e2e setup bypassed `openWorkspacePath`; clone name builder repeated | Yes | Helpers reused; `workspaceCloneName` exported |

Not taken: C9's suggestions for user-switch, Strict Mode and cross-org authorization browser cases (the
user-switch and storage cases are unit-covered; a second e2e organization does not exist in the stack).

## Rechecks (after fixes)

`npx tsc --noEmit` clean; `npm run lint` clean on touched files; prettier clean on every touched file
that was clean on `origin/main` (`CloneDialog.tsx` and `WorkspacePageAlignment.test.tsx` are not
prettier-clean on main and were edited by hand to keep the diff to the intended hunks). Vitest: the
affected suites pass (strip 13, picker 4, pure functions, alignment page 32, Home, projects, settings
panel, clone dialog, App routes); full `npx vitest run`: 4355/4356, the one failure being the
known order-sensitive `ChatPanel` account case (passes alone and on a clean `origin/main`). Playwright
tab flow written, unexecuted here (no e2e stack); runs in CI `g5-ci-e2e` with the alignment gate on.

## Final independent review (fresh `codex exec review --base origin/main` on PR #380)

| # | Finding | Verified | Action |
|---|---|---|---|
| F1 | P2 the close guard was not cleared on a route to Home, so a workspace closed as the first tab and reopened from Home got no tab | Yes (reproduced by the reviewer's regression test) | Fixed: any route away from the closed workspace clears the guard; `WorkspaceTabStrip.test.tsx` "reopening a workspace after closing its tab from Home" |
| F2 | P2 static routes excluded by first segment hid the strip for an organization slugged `projects` or `account` | Yes (`validate_slug` reserves no such slugs) | Fixed: exact static route matches only; `shellRoutes.test.ts` covers `/projects/base` and `/account/base/doc/…` |

Rechecks: `tsc` clean, lint clean, strip 14 / routes / alignment page / picker / App routes 100 passed.

## Metadata

`g5.workbench.tabs` and `g5.story.switch-open-workspaces` metadata updated with the final locations,
tests and gaps.


## Phase 8 — implementation self-review (2026-10-09)

Scope: the approved Phase 8 section in `ontology-plan.md`, its Phase 8 governing rows,
and the four workbench metadata entries requested by the human. This is a self-review;
no independent agent was used. The existing Phase 8 design/chat/ontology edits were
present before implementation and are preserved.

Initial findings and evidence: 128 targeted frontend tests and all eight backend
workbench-config tests pass. Production frontend build passes. The first full frontend
run exposed three stale `Sidebar.test.tsx` expectations for persisted orders: Phase 8
requires both new IDs in both canonical orders, while classic rendered sections stay
unchanged. Update only those storage expectations and re-run. The known ChatPanel
unavailable-account failure also appeared; it is outside Phase 8. Postgres probes at
localhost:5432 and :5433 return `Operation not permitted`, so database execution remains
unverified. The API test binary is being compiled without executing database tests.

| Commitment | Actual path | Counterexample/check | Result or gap |
|---|---|---|---|
| Alignment-only rail, Activity first, Jobs; canonical migration and classic behavior | `TreeNode.tsx`, `WorkspacePage.tsx` | Section-order tests, all three classic layouts, Activity click and keyboard in expanded/rail/hover modes | Targeted tests pass; update legacy storage expectations exposed by full suite |
| Issues are a projection; primary first, related means Subtask, native state and typed read failures | `WorkspaceIssuesSection.tsx`, shared `WorkspaceIssueLinkParts.tsx`, existing link hook | Unsorted link fixture, closed issue, unreadable issue, failed list/retry, disabled feature | Targeted tests pass; org-off cleanup reuses `WorkspaceIssuesPanel` |
| Owner-only imports use ordinary related links and shared conflict semantics | `ImportTaskPicker.tsx`, `useLinkWorkspaceIssue`, `IssueLinkConflictNotice` | Held first response proves sequential writes; conflict between two successful links; existing links checked/disabled; review-copy block; unavailable/partial search; debounce; Escape | Targeted tests pass; no provider hierarchy write |
| Implement is a bare persona start, with launch admission and pinned configuration | `WorkspacePage.tsx::handleWorkbenchChat`, `useStartWorkbenchAction`, `startMessageFromIssue` | Existing same-persona chat still causes new start, exact content/persona/phase/revision, absent actionId, Base/non-owner disabled | Targeted tests pass; live provider receipt not exercised |
| Fork reuses issue-prefilled create; detach confirms and never mutates tracker; approvals select current phase | `CreateWorkspaceDialog`, shared confirmation, `useUnlinkWorkspaceIssue`, `openWorkspaceApprovalsTab` | Primary warning/cancel/confirm, link invalidation removes actions; phase-specific approvals; fork-prefill regression | Detach/approvals pass; fork regression added in review |
| Completion remains Manual; writes need the broker | Shared tracker-write reasons, disabled checkbox/menu/quick-add | Checkbox reflects closed state and cannot mutate; disabled menu reason | Pass; close/reopen/create remain the specified V2 gap |
| IE has the exact SE Author/Analysis groups before Release; committed phase replaces whole phase | `actions.yaml`, generated fallback, `workbench_config.rs` | Serialized shared-group equality, fallback drift check, existing override unit test | Eight backend unit tests pass; API runtime blocked by sandbox Postgres access |
| No new routes, stores, flags, settings or dependencies | Diff inspection | Existing operations and transient component state only | Conforms |

Bounded ontology-to-inventory pass: the governing identity, primary ownership, completion
and broker constraints are covered above. The implementation calls only the existing
local association and persona-start operations; it introduces no external tracker writes.
Browser rail assertions are written in `alignment-shell.spec.ts` but remain unexecuted,
as specified for this environment. Database and live-provider guarantees are not inferred
from MSW tests.

Later full-suite findings: the completed run was 4,402 passed / 6 failed. Besides the
three storage expectations and the known ChatPanel failure, the fallback group-name test
still expected IE Release alone, and the raw-button guard found two missing test IDs on
the new issue Action menu. Add those IDs and update the fallback expectation before rechecking.

### Final results and handoff

All Phase 8 findings above are fixed. The second storage recheck also confirmed that
`mergeWithMissing` inserts Activity immediately after Files in a customized classic order;
the updated assertion preserves that existing insertion rule and the user's other positions.
The additional fork-prefill regression passes. No unresolved implementation defect was found
in this self-review, and no product behavior deviates from the Phase 8 plan.

Checks run in this checkout:

| Check | Exact result |
|---|---|
| `cd frontend && npx tsc --noEmit -p tsconfig.json` | Pass, exit 0; no diagnostics |
| `cd frontend && npm run lint` | Pass, exit 0; 0 errors, 546 warnings. `TreeNode.tsx` and `WorkspacePage.tsx` retain their main-branch warning counts (21 and 96); new issue components have no lint findings |
| `cd frontend && npx prettier --check` on all 17 touched frontend files | Exit 1 only for `src/pages/WorkspacePageAlignment.test.tsx`, already unclean on `origin/main`. The generated fallback is ignored by the frontend Prettier configuration and kept byte-identical to its Rust generator. An earlier direct Prettier invocation from the repository root also reported the generated file (already unclean on main) |
| `npx prettier --check` on all 14 main-clean/new frontend files | Pass, exit 0; the separately checked `TreeNode.sectionOrder.test.tsx` is also now clean |
| Frontend production build, `npm run build -- --config .phase8.vite.config.ts --configLoader runner` | Pass, exit 0; Vite built in 21.72 seconds. Existing stale Browserslist-data and large-chunk notices remain |
| Final targeted Vitest run (12 suites below) | Pass, exit 0; 195 tests passed |
| Final full Vitest run | Exit 1; 277 files passed, 1 failed; 4,408 tests passed, 1 failed out of 4,409. Sole failure: `ChatPanel.test.tsx` → “shows a no-longer-visible account as Unavailable and lets the user move away from it”, the known account-field failure |
| `cd backend && UPDATE_WORKBENCH_BUILTINS=1 cargo test -p g5-api-core frontend_fallback` | Pass, exit 0; 1 test passed; fallback regenerated |
| `cd backend && cargo test -p g5-api-core frontend_fallback` after regeneration | Pass, exit 0; 1 test passed, 1,026 filtered out |
| `cd backend && cargo test -p g5-api-core workbench_config` | Pass, exit 0; all 8 tests passed, including exact IE/SE shared-group equality and whole-phase override replacement |
| `cd backend && cargo fmt --all --check` | Pass, exit 0 after formatting the two changed Rust test files |
| `cd backend && cargo test -p g5-api --test api_workbench --no-run` | Pass, exit 0; API integration-test executable compiled |
| `cargo test -p g5-api --test api_workbench` execution | Not run: local Postgres connection probes for both configured fallback ports (5432 and 5433) were rejected with `Operation not permitted` by the sandbox |
| Playwright `frontend/e2e/flows/alignment-shell.spec.ts` | Added rail-order and Activity-to-timeline assertions; not run, as specified for this environment |
| `git diff --check` | Pass, exit 0 |

The final targeted suite set was:
`components/Sidebar.test.tsx`, `pages/WorkspacePageAlignment.test.tsx`,
`components/tree/TreeNode.sectionOrder.test.tsx`, `components/workbench/ActionRibbon.test.tsx`,
`components/external-issues/{WorkspaceIssuesSection,ImportTaskPicker,ExternalIssueDetailPanel,WorkspaceIssuesPanel}.test.tsx`,
`lib/workbenchDefaults.test.ts`, `test/design-tokens.test.ts`,
`test/design-token-contrast.test.ts`, and `test/button-testid-coverage.test.ts` (all under `frontend/src`).

Tooling deviation only: ordinary `npx vitest run` failed before tests because this
worktree's `node_modules` symlink points into the read-only main checkout; Vite attempted
to create its bundled-config temporary file there. `--configLoader runner` alone then
failed on the existing config's `__dirname`. Tests and build therefore used a temporary
copy of the same Vite config with its directory made explicit and its cache in `/tmp`.
The successful test invocation was `npx vitest run --config .phase8.vite.config.ts
--configLoader runner --no-cache` (with the listed paths for the targeted run).
The temporary config has been removed. No dependencies, persistent configuration,
routes, stores, feature flags or settings were added. Early checks exposed TypeScript
missing test-id props, one ambiguous Activity test selector, and the full-suite findings
recorded above; their final checks all pass.

Files changed by this implementation (paths are repository-relative):

- `frontend/src/components/tree/TreeNode.tsx` and `TreeNode.sectionOrder.test.tsx`;
  `frontend/src/components/Sidebar.test.tsx`.
- New `frontend/src/components/external-issues/WorkspaceIssuesSection.tsx`,
  `WorkspaceIssuesSection.test.tsx`, `ImportTaskPicker.tsx`, `ImportTaskPicker.test.tsx`,
  and `WorkspaceIssueLinkParts.tsx`; updated `WorkspaceIssuesPanel.tsx`,
  `ExternalIssueDetailPanel.tsx`, and `ExternalIssueDetailPanel.test.tsx` in that directory.
- `frontend/src/pages/WorkspacePage.tsx` and `WorkspacePageAlignment.test.tsx`;
  `frontend/e2e/flows/alignment-shell.spec.ts`.
- `backend/g5-api-core/src/services/agents/workbench_builtins/actions.yaml`,
  `backend/g5-api-core/src/services/agents/workbench_config.rs`,
  `backend/g5-api/tests/api_workbench.rs`, `frontend/src/lib/workbenchBuiltins.generated.ts`,
  `frontend/src/lib/workbenchDefaults.test.ts`, and
  `frontend/src/components/workbench/ActionRibbon.test.tsx`.
- Four metadata entries only in `.g5/ontology/features/workbench.g5.md`;
  the review-summary addition to `.g5/pr_ontology_diff_design/ux-v2-workbench/ontology-plan.md`;
  this review record. Pre-existing Phase 8 plan/ontology/chat changes were preserved;
  `.g5/chat_intent/ux-v2-workbench/request.md` was not edited by this implementation.

Remaining limits: real browser rail behavior, database-backed issue/workbench operations
and live provider/agent receipt are not established by these checks. Tracker writes remain
the explicitly disabled V2 backlog. No git write operation, commit, or push was performed.
