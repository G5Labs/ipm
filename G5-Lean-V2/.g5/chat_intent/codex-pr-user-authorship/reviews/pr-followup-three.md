# PR target, title and node-ID follow-up — independent review three

Initial discovery saved before reading other reviewers or the author's review explanation.
Read-only review of the frozen working tree supplied with `/tmp/g5-pr-followup-snapshot.tgz`.
No production, ontology, or test edits and no delegation. Review inputs were the original
request, the pinned `ontology-plan.md` (including its governance inventory and final
amendment), current linked ontology bodies/footers, repository instructions, and the
ontology-review skill/design guide. No supplied defect hints were used. Existing test
receipts are identified separately from checks executed by this reviewer.

## Findings

### F1 — P2: retain the known provider PR when its title update fails

- Location: `backend/g5-api-core/src/services/workspace_pull_request_provider_lifecycle.rs:1048–1052`, with the error handler at `534–545` and persistence at `1844–1849,1866–1878`.
- Governing commitments: `g5.governance.pr.title`, `g5.governance.pr.write`, and `g5.architecture.recovery`.
- The new PATCH runs after lookup has already returned the existing `GitHubPull`. On any PATCH error, `?` discards that fact and the outer handler records `pull: None`. `record_provider_status` writes NULL to the existing row's provider PR number, node ID, URL, and observed head/base SHAs. A rejected title therefore makes a still-existing remote PR disappear from stored link/identity output. Merge subsequently refuses with `Provider PR number is missing` (`:640`); retry discovery is required to recover unrelated identity state.
- This is a newly reachable use of an existing destructive failure writer. The current title-failure integration test (`api_workspace_pull_requests.rs:1581–1608`) checks failed status and the G5 title but does not assert preservation of provider identity.
- Proposed repair: handle the title-update failure while retaining the discovered pull, and persist the failed/action-required outcome alongside that known identity. Avoid clearing established identity facts on a metadata-edit failure.
- Regression check: create/adopt PR #22 with node ID and URL, reject PATCH with 422 (and separately a transport failure), then assert the returned and persisted failed provider row still has number/node ID/URL and the preexisting link remains usable. A retry must reuse #22 without POST.
- Evidence: source/SQL trace plus existing successful failure-test receipt; this reviewer did not execute a new HTTP assertion for the lost fields.

### F2 — P2: invalidate the new target cache when workspace choices change

- Location: `frontend/src/hooks/useWorkspace.ts:430–443`; consumers at `frontend/src/pages/WorkspacePage.tsx:6298–6320,6355–6366`.
- Governing commitments: `g5.governance.pr.target`, `g5.architecture.pr_flow`.
- The UI now gets targets from a separate `workspace-pull-request-targets` query, but all existing workspace changes still invalidate only `workspace`/`workspaces`. Examples include `usePatchWorkspace` (`:235–239`), delete (`:258–265`), clone (`:284–285`), and reconnect (`frontend/src/lib/ws.ts:1306–1328`). The target query is never invalidated elsewhere. It has no interval and remains mounted on WorkspacePage, so rerendering/opening the dialog does not refetch it. Refreshed workspace data can therefore coexist with an obsolete target list/default until window focus/remount. The former derived list followed workspace data automatically.
- Consequence: an eligible new default can remain unavailable, or the UI can continue selecting an old parent/Base while the omitted-target CLI resolves the current choice. Because the UI submits the stale selection explicitly, it does not benefit from server default resolution. Server access checks still prevent authorization bypass.
- Proposed repair: connect target-choice invalidation to the relevant workspace/visibility/invitation/reconnect changes, or refresh choices at the creation entrypoint while preserving explicit selection. Keep the server as the sole ordering/filter owner.
- Regression check: with the source page/target hook mounted, change the backend choices and run an actual relevant mutation or reconnect; assert the target request refetches, the dialog options/default reflect the new list, and explicit selection is preserved only while still eligible.
- Reviewer runtime evidence: a `QueryClient`/`QueryObserver` using the installed TanStack version fetched `[old-parent]` once. Applying the existing `['workspaces']` and `['workspace','old-parent']` invalidations left `[old-parent]` and fetch count 1. Invalidating `['workspace-pull-request-targets']` changed the list to `[new-base]` and fetch count 2. This demonstrates the cache-key interaction, not a complete browser mutation test.

### F3 — P2: do not apply a stale request title after a newer G5 title commits

- Location: `backend/g5-api-core/src/services/workspace_pull_request_create.rs:282–298`; persistence at `coordinated_repo_publication.rs:326–331`; provider context at `workspace_pull_request_provider_lifecycle.rs:1446–1467`.
- Governing commitments: `g5.governance.pr.title`, `g5.design.authority`, `g5.architecture.pr_flow`.
- The local-title transaction commits and releases its source lock before a separate provider transaction starts. The provider phase receives the original request's title; it does not read the current title from the now-durable PR. Two overlapping title updates permit this schedule: A commits local title A and pauses; B commits local title B and completes provider title B; A resumes, PATCHes provider title A, and returns a summary whose local title is B. Both operations succeed, leaving G5 and GitHub with different titles even though explicit title is defined to set both. Re-admission serializes each phase but does not prevent this inter-phase ordering.
- Proposed repair: make provider title application honor the currently accepted/persisted title intent, or fence the provider phase against superseded title intent. Preserve omission/per-repository derivation semantics; blindly synchronizing every automatically derived G5 title would violate those semantics.
- Regression check: deterministically pause A after its local commit, let B fully complete, resume A, and assert the final G5/provider titles agree and stale intent cannot overwrite the newer accepted title. Verify a title-omitted operation still preserves existing provider titles.
- Evidence: source-level admissible schedule; concurrent public requests were not executed by this reviewer. This is distinct from ordinary partial provider failure: both calls can return success.

## Commitment coverage and checks

| Commitment | Actual path / counterexample or check | Result or gap |
|---|---|---|
| `pr.target` | Shared `targets` SQL scopes organization, excludes source/deleted rows, orders parent/name/ID, checks each reader; omitted resolution pins one ID before save/push. Public route calls same helper. | Correct source implementation; F2 affects UI freshness. Existing 32-case API receipt includes ordered/visibility-scoped targets, empty defaults before runtime, parent preference, and later access refusal. |
| `pr.title` | Blank input rejected before preparation by CLI and server; new/reused G5 writes preserve omitted title; explicit provider create/PATCH and 422 legacy retry carry title. | Ordinary path supported; F1 and F3 remain. Existing provider tests cover create, reuse, omission, and rejection but omit failed identity preservation and concurrency. |
| `pr.json` / `pr.read` | REST `node_id` maps to `id`; `id` is in common selectable fields; G5 UUID projection unchanged; explicit read retains attached-repository/credential boundary. | Supported. Reviewer independently ran compiled `pr_commands-1e5f74be6ee08fbf --exact github_node_id_is_selected_for_numbers_and_urls --nocapture`: 1 passed. API receipt/source checks distinguish numeric REST ID from node ID. |
| `pr.write` / `architecture.pr_flow` | Public/VM thin adapters enter `create_or_update`; title/default logic is in that owner, CLI only forms request/projection, UI consumes server order. | Single sequence preserved. New target read projection is proportionate and no alternate mutation flow introduced. |
| `security.owner`, `security.admission`, `security.login.service-identities`, `runtime.lease` | Typed callers, VM source binding, owner handle/epoch/lease fencing, shared mutation restriction and target re-admission across preparation/provider boundaries. | Inspected preserved paths; existing public/VM suite covers stale/replaced/transferred/denied boundaries. Actual deployed guest callbacks remain unexecuted. |
| `security.read`, `security.tenant`, `security.tenant.write` | Targets use scoped organization query plus existing visibility helper; explicit target uses existing authority; no target re-resolution after admission. | No new access bypass found. Explicit targets retain existing policy; deleted-target compatibility in existing tests is not introduced as a new policy here. |
| `publish.readiness`, `publish.github-author`, `credentials.publication` | Target/title rejection precedes save; shared prepare/capture/publish ordering retained; new provider creation uses acting user grant; existing title PATCH uses existing write-PR credential owner; tokens remain server-side. | Preserved under inspected paths and supplied tests. No live GitHub verification. |
| `design.minimal-mechanism`, `design.change-locality`, `architecture.recovery` | Reuses shared operation, existing provider API owner, parser/projection; no added schema/credential store. | Scope is compact; F1/F3 show reused failure/transaction mechanisms need stronger title-specific guarantees. |
| Documentation / worker consumers | `docs/g5d-pr.md`, CLI help, both bundled `g5-pull-requests` skills. | Accurate advertised defaults/title/node-ID scope; skill copies are byte-identical. |

Inspected supplied receipts: `/tmp/g5-pr-followup-api-recheck.log` reports 32/32 API tests;
`/tmp/g5-pr-followup-cli.log` reports 12/12 compiled CLI tests; check log reports successful
workspace compilation. UI receipt reports its selected suites passing with React `act`
warnings. `/tmp/g5-pr-followup-units.log` reports 75 passed and 2 unrelated host mock-server
tests blocked by sandbox port binding; all selected PR unit tests passed. These are
reused receipts, not independently rerun suites.

No broad builds or shared database tests were run. Main-agent consolidation should
retain the source-only/runtime distinction above, add the missing regression checks,
and update pending `pr.target`, `pr.title`, `pr.json`, and `architecture.pr_flow` footers
after repairs. No ontology meaning change is needed for F1/F2; any durable title-intent
mechanism chosen for F3 should be recorded in the existing design before dependent work.
