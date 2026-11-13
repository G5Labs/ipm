# Dead-code retirement — batches 1–4

The authorized scope includes backend and frontend unreachable code, Native/NaiveReact execution, and MicrovmAgent/WSL2 execution. Batch 5 (public API retirement) is excluded. The approved G5-Lean layout remains; G5-original-reference is untouched. Existing staged relocation changes are preserved separately from these unstaged edits.

## Follow-up schema and fixture cleanup

The retired workspace GCS cache enum and typed GCS settings are removed. Existing `.g5/workspace.yaml` files with `cache.mode: gcs` continue to load; that string does not select a runtime cache tier. The obsolete `cache.gcs` block is ignored and omitted when the manifest is rewritten. No YAML migration is required, although the unused block can be deleted. This does not change the separately owned host cache door or fleet artifact cache.

Unused tree-search UI and stale `cloud_workers` fixture markers are removed. The live document-sync hook is named `useTriggerSync`; its request and invalidation behavior are unchanged. The Codex seed script provisions a dedicated docs member and prints an explainer request with `topic`, `prompt`, and `format`, while the public view-creation route retains its 410 response. Optional instruction files are seeded into the docs repository before the fixture parent cut is captured; reruns preserve existing docs and explainers. Without the optional instruction file, the printed request asks for a general architecture explanation. Agent provider credentials and permission settings remain prerequisites for job admission. Serialized WSL2 compatibility markers remain; this cleanup does not add a Windows backend or change serialized isolation contracts.

## Execution boundaries

- Removed the unconstructed daemon HTTP server, direct connection stack, GitManager facade, scheduler family, automatic HIL supervisor, manifest service, unregistered view tools, unused service methods, duplicate routes, and remote L2/prewarm cache implementation. Local build caching remains.
- Removed disconnected frontend components, 21 uncalled hooks, unused layout/filter helpers, and the alternate MSW bootstrap. Mounted pages, editor, notifications, takeover services, mock fetch interceptor, and public APIs remain.
- Removed the Native inference engine, model HTTP providers, Native task agent, subagent executor, and exclusive fixtures. Claude Code, Codex, and their shared tools, prompts, skills, lifecycle, transport, and usage accounting remain. Legacy delegation schemas remain because the provided-agent prompt builder still consumes them; their Native dispatcher was removed.
- Native assignments now fail explicitly with `agent_provider_retired` before execution; prepared workspaces are reclaimed. Historical provider serialization and database defaults remain readable, without silently selecting a different engine.
- Removed MicrovmAgent task boot/drive and its unused bootstrap executor ports/adapters, standalone `g5d agent-runner`, host Codex carrier execution, and the WSL2 backend. Historical MicrovmAgent mode values are readable but rejected by the isolation manager, even with the former live-carrier flag. The CLI does not offer the retired mode.
- Ordinary MicroVm/libkrun execution and Claude preview Attach remain. Shared guest relays, authentication, CRDT, transport, cancellation, teardown, and recovery machinery are retained where used by those paths. Some shared internal names still contain “MicrovmAgent”; the name alone is not a deletion criterion.
- Azure self-hosted canary and installer configuration now use ordinary MicroVm. Their local contract tests cover packaging and launch commands; this change has not been deployed to Azure.

The audit initially classified `WsUpgradeParams` with the unused WebSocket handler. Its actual mounted adapter caller was found during implementation, so the type and its regression test were retained.

## Test coverage preservation

Tests exclusive to retired implementations were retired with them. Tests covering retained behavior were migrated to their actual production boundaries:

| Previous fixture | Retained coverage |
| --- | --- |
| Native mock model/AgentLoop | Scripted tool requests invoking the real ToolRegistry: filesystem and CRDT changes, repair after tool errors, Git operations, task completion, prompt and skill definitions |
| GitManager/TaskWorktree | Real `g5_workspace_ops::fork_task_workspace`, branches, checkout, conflicts, and cleanup |
| Removed view-emission tool | Real WriteFileTool with materialized multi-repository CRDT routing |
| Uncalled job/spend method | Mounted job pause endpoint and workspace isolation |
| Removed WorkerHosts wrapper | Mounted WorkerHostsPageContent |
| Draft editor harness | Mounted CodeEditor and Monaco behavior |
| MicrovmAgent-only configuration | Retired-mode rejection and retained MicroVm/preview packaging, port, mount, cancellation, and teardown boundaries |

The Git status regression now checks actual tool output, published Git head, and the separate CRDT draft state. Its old text assertion could match “clean” in a prompt without proving a clean result. Test-only publication and teardown adapters remain for existing workspace reconciliation, conflict, cancellation, and recovery tests; their helper families are excluded from production builds. Private wrappers with no remaining callers were removed.

Reduced test counts reflect removal of retired features, not a measured coverage percentage. No before/after line-coverage equivalence is claimed.

## Docker CI audit

`just docker-ci` aliases `ci-docker`. The recipe now includes omitted backend packages and integration targets, discovers repository script regression suites, runs retained Claude/Codex provider tests, and propagates failures from coverage-enabled tests, isolation tests, API smoke, unfiltered view-sync E2E, and coverage report generation. Eleven executable CI contract tests verify package coverage, failure propagation, and scoped Docker access for E2E worker isolation.

`just test-live-llm` is retired with the Native HTTP provider harness. `just test-live-e2e` remains for live Claude/Codex execution.

The default command runs deterministic suites. Live-provider tests require credentials; hardware/VM and selected toolchain tests retain their explicit environment requirements/ignore markers. The release-installation smoke script additionally requires `G5_SELFHOST_SMOKE_API_IMAGE` and `G5_SELFHOST_SMOKE_EDGE_IMAGE` and is run by the script runner when both are supplied (otherwise it prints an explicit conditional skip). Thus the default command does not claim to execute every credentialed, hardware-dependent, or release-installation scenario.

Script audit fixes include relocated repository roots and fail-closed SQL scan directories in the approval-rollout verifier. Its mocked test now explicitly models upstream ancestry and separately verifies rejection when required ancestry is missing.

The dependency audit found `libssh2-sys 0.3.2` yanked in the registry. The original reference checkout has the same lockfile entry. Lean updates only that package to the compatible, non-yanked 0.3.3 patch; the audit remains enforced without an exception.

The runtime CI pass exposed two additional guard/setup defects. The image fleet guard was explicitly passed the old `docker/` path and could succeed without scanning any files. It now scans `infra/docker`, rejects missing/empty directories, and recognizes the existing pinned Playwright exclusion; three executable tests cover those boundaries. The three CI worker replicas also shared one exclusive task-workspace root, causing two workers to restart with a root-lease error. Each replica now keeps control state, provider configuration, and worktrees in its private writable home, while fixtures and repository storage remain shared. A test of the rendered Compose configuration checks these ownership boundaries. A real two-container check also verified both non-root replicas could generate provider configuration and hold their root locks simultaneously.

Fixture seeding also stopped inside a temporary Alpine container whose package-install output was suppressed; the install succeeded on a later isolated retry, so the transient cause could not be established. The seed now uses the already-built G5 base image with Git installed instead of downloading Git repeatedly, and reports the failing line on errors. The complete `just docker-ci 4c` stack startup and fixture-seeding phase passed with this change.

The integration audit also repaired fixtures without weakening their assertions:

- Platform issue-reporting tests now use the configured Redis service and CA certificate, with quota data isolated in database 15. The application and test fixture share the existing production TLS-aware Redis client builder. Seven real Redis tests passed, including quota enforcement and outbox delivery.
- Seed-contract and stale-writer tests now exercise the Lean paths. The stale-writer fixture includes both workspace and backend manifests, so it continues to prove that an outdated copied binary refuses regeneration. Sixteen seed-contract and eleven writer/rebase tests passed.
- The multi-VM port demonstration lacked the libkrun availability gate used by neighboring hardware tests. It now probes before allocating resources, and the Docker integration runner forwards the detected KVM device. This host has no KVM, so its early return does not validate VM boot.
- The real ontology migration document is preserved as historical test data in `tests/fixtures/ontology-v4`, copied into an isolated workspace. Ontology audits now use non-empty fixtures and verify detection and repair of ownership overlap, dangling child references, and dependency cycles, rather than passing against an absent old directory.

The default Rust E2E lane previously omitted the Docker socket needed by its worker-isolation helpers. It now supplies Docker access plus an explicit allowlist selected by this stack's Compose project and service labels; discovery cannot accidentally pause another local stack. Nested guards also preserve pause ownership: workers already paused by the outer runner remain paused when the inner guard exits. Nine helper tests cover allowlist isolation, existing/concurrent pauses, transient retries, and hard failures. The same E2E helper now resolves the checkout's `.dev` directory through the Lean `tests/e2e` layout.

The browser fixture must be reseeded after Rust E2E: those tests cancel and mutate tasks in the shared test organization. Reusing the earlier poison fixture caused browser draft isolation to receive a completed-task conflict instead of entering the editor. Phase 7 now forces a fresh seed; all three affected browser scenarios passed against a fresh fixture. The CI contract executes the seed guard and verifies normal reuse versus forced reseeding.

The organization-home browser test issued its second document-answer save before the first completed. It now waits for each real PUT to succeed and the answer button to disappear. The unchanged production focus path also cleared an approval URL hint when all session queries were ready but route task selection was still null. Runtime instrumentation confirmed this ordering. Focus-hint cleanup now waits for matching task/session selection, with an added regression that fails without the guard. All 104 workspace-page tests and both organization-home browser journeys passed with the rebuilt production bundle; both existing browser focus assertions remain.

The browser lane runs Chromium; the configured Firefox and WebKit projects are not part of the default Docker command. The inherited Playwright configuration excludes two compliance navigation journeys in CI and gates review-metadata and plan-overview journeys on prepared-workspace variables. Those variables are not forwarded by this Docker recipe; use their prepared-workspace commands. The value-loop matrix and live MicroVm browser scenarios also require explicit opt-in. These retained tests are not counted as executed by a default CI pass.

## Validation status

The complete `G5_CI_NO_INTERPHASE_PRUNE=1 just docker-ci` invocation finished. All script, formatting, lint, dependency, Rust unit/doc/isolation/integration, provider replay, frontend unit, API smoke, and service E2E stages passed. Chromium finished with 460 passed, 68 skipped, 3 failed, and 1 serial case not run. The three failures were the two stale draft-scope fixture cases and the organization-home save race documented above. The save fix exposed the focus hydration race, which is also fixed. All five cases in these two files passed targeted checks. The final `just docker-ci 7` invocation of both files passed all five browser cases without retries against the rebuilt image and forced-reseed recipe. It completed successfully in 2m43s, including stack setup and cleanup.

This is not a claim that one complete invocation was green. `G5_CI_NO_INTERPHASE_PRUNE=1` only preserves build caches between phases; it does not omit tests.

Completed checks:

- Frontend Docker lane: 228 files, 4,184 tests passed. After the focus regression fix, the complete native frontend suite passed all 228 files and 4,185 tests; the production build and affected-file ESLint also passed (existing warnings remain).
- Host library: 308 tests passed, including park-rejection ownership and transient teardown recovery.
- Docker backend unit/bin groups: 1,521, 1,995, 3,066, and 1,261 tests passed. Isolation groups: 17 and 195 passed. Some existing process tests report nextest leak warnings; they are not reported as warning-free.
- Real WriteFileTool multi-repository CRDT ownership boundary: passed.
- Retained tool/prompt/workspace suites: agent pipeline 11, build cache 123 (before two L2-only error cases were retired), architecture 44, Git worktree 8, prompt parity 9, pipeline tools 15, skills 50, synthesis lifecycle 11 passed. Existing ignored cases remain explicit.
- Native assignment retirement/cleanup boundary: passed.
- CLI retirement boundary (valid modes accepted, retired command/mode rejected): passed.
- Isolation retirement boundary, including the former live opt-in flag: passed.
- Repository script runner: all 36 suites passed, including Azure/self-hosted packaging, image scanning, and CI worker-storage contracts.
- Docker CI contract: eleven tests passed, with runners injecting failures only at the test/report operation (successful setup cannot mask the result).
- Real ontology migration/read: 10 tests passed; non-empty ontology audit fixtures and optional local audit cases: 6 passed.
- Live API smoke: 38 tests passed against the updated Docker stack.
- Worker-owned service E2E: 8 sync-job, 3 internal view-sync, and 3 Git-diff wire scenarios passed after the Docker/pause-ownership fixes.
- Broad service E2E: 497 passed, 5 explicitly ignored; separate view-sync wiring: 10 passed. The relocated catalog retains all 38 selectors; seven catalog receipt tests also passed. Real deterministic Claude/Codex citation turns passed.
- API/common integration: 2,424 passed, 11 skipped; worker integration: 643 passed, 12 skipped. Retained provider replay: Claude 230 and Codex 165 passed.
- The opt-in persistent-task restart fixture path now points to the seeded `frontend/e2e` directory; its contract test verifies that directory exists and passed. This does not claim execution of the ignored restart scenario.

Native strict Clippy passed for the entire workspace and all targets after the ontology fixture changes; Rust formatting and `git diff --check` also passed. The final browser-recipe rerun passed. All deterministic suites have passing evidence across the complete run and the corrected reruns; the original complete invocation remains recorded as failed rather than being relabeled green. No deployment or live provider/hardware claim is made.


### Reproduction and result provenance

- Complete run: `G5_CI_NO_INTERPHASE_PRUNE=1 just docker-ci` — all pre-browser stages passed; Chromium 460 passed / 68 skipped / 3 failed / 1 serial case blocked. Log: `/tmp/g5-retirement-docker-ci-full-8.log`.
- Corrected browser recipe: `G5_CI_NO_INTERPHASE_PRUNE=1 just docker-ci 7 false "" "e2e/flows/draft-session-scope-isolation.spec.ts e2e/flows/organization-home.spec.ts"` — five passed, no retries; setup authentication also passed. Log: `/tmp/g5-retirement-docker-ci-browser-final.log`.
- Final complete frontend unit suite: `cd frontend && npm test` — 228 files / 4,185 tests passed. Log: `/tmp/g5-retirement-frontend-final.log`.
- The new route-selection focus regression fails without the guard and passes with it. This was checked explicitly, in addition to the retained real-browser reload assertion.
- All 11 Docker CI contract tests and the final whitespace/diff check passed. The normal dev stack was not restarted by this validation; the Docker test stack was cleaned up by the recipe.

The logs are local temporary evidence, not committed artifacts. No fresh full invocation was run after the final browser fixes: affected browser files, the production client build, and the entire frontend unit suite were rerun. Hardware, credentialed/live-provider, and explicitly prepared-workspace limitations above remain. No coverage percentage equivalence or cloud deployment is claimed.
