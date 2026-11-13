# Independent review 3 — initial Git abstraction slice

Read-only discovery against `3f7d2874` plus the eight paths in `/tmp/g5-git-abstraction-initial-review.sha256`, including untracked `backend/g5-scm/src/resolution.rs`. All eight hashes matched during review. I did not implement this change, read peer reports, or receive the author's solution history. Inputs were the original saved request, the full pinned design starting with `Ontology changes and governance`, AGENTS.md, the review skill/design guide, governing ontology and affected metadata. The supplied initial-slice status was a scope boundary, not evidence of correctness.

No production, ontology or test files were changed. No Cargo build, database test or shared test resource was started; the main agent owns those runs. Findings below are source-established counterexamples, with concrete existing tests identified for runtime verification.

## Findings saved before sharing

### R3-1 — P1: create-only pins strand a retried provider-resolution run

**Changed location:** `backend/g5-scm/src/resolution.rs:111-129`, reached through `backend/g5-api-core/src/services/git_conflict_resolution.rs:202`.

The new operation rejects every incomplete or changed existing pair. That is correct for an immutable committed attempt, but the live provider caller also retries *uncommitted preparation* using the same run ID. `remote_integrations/resolution.rs:20-24` explicitly states that the SQL row lock distinguishes committed tasks from abandoned preparation; `:59` passes `input.run_id` as the resolution ID. `remote_integrations/merge.rs:1943-1993` admits the same waiting run again and captures a fresh provider tip; `:2117-2135` passes the same `merge_run_id` to enqueue. A failure after Git pinning but before enqueue/SQL commit leaves pins while `resolution_task_id` remains absent. A later user retry with advanced local/provider heads, or after a partial pin write, now always returns conflict. Existing resolution recovery is therefore lost, not merely left for later migration.

The existing real API/Git/SQL test `backend/g5-api/tests/remote_repo_tracking_integration.rs::resolution_retry_recovers_after_enqueue_or_commit_rollback_and_remote_advance` (starts at line 3371) already constructs both cases: failed enqueue or deferred commit, retained Git pins, no task, then changed heads; one case also removes one pin. Its expected retry success at line 3456 contradicts the new unconditional refusal. I inspected this test but did not run it.

**Governing commitments:** `g5.architecture.git_ref_updates` already specifies checked replacement with expected values; `g5.architecture.git_retry` preserves existing separately owned recovery; `g5.architecture.recovery` and the design's preservation of remote-tracking merge behavior apply. The initial-slice design assertion that “the live resolution caller only creates fresh run IDs” is false for this caller.

**Repair:** retain create-or-confirm for fresh workspace-resolution runs, and provide the existing provider policy owner a complete checked replacement/repair operation using expected optional values for both refs while it holds the existing SQL/workspace fences and establishes that no task committed. Reject stale expected state before mutation and never overwrite committed task pins. This requires neither retry storage nor automatic replay. Correct the initial-slice paragraph and keep the existing regression's assertions.

**Verification:** run the existing test above for both injected failure stages, plus successful repeated start after a committed task (must reuse that task and pins), and stale expected-ref refusal. The current new collision unit test is valid for create-only semantics but cannot establish compatibility with the retrying caller.

### R3-2 — P2: clear uncertain/refused errors disappear at the public response boundary

**Changed locations:** `backend/g5-scm/src/resolution.rs:77-80`, `:139-146`, and the conversion at `:46-50`.

The useful messages for non-conflict refusals and uncertain commit failures are placed in `G5Error::GitError`; the typed `ResolutionPinError` is discarded by the `?` at `git_conflict_resolution.rs:202`. Both caller paths return `AppError`. `backend/g5-api-core/src/errors/mod.rs:117` serializes through `G5Error::to_error_response`, whose `backend/g5-contracts/src/error.rs:570-576` branch replaces every GitError message with `An internal error occurred` and empty details. Consequently an owner receiving a ref-lock failure or a commit failure with possibly changed refs gets no operation, target, effect distinction, or inspect-before-retry instruction. The new unit tests assert `Display` strings only, which are server-log evidence, not the promised agent/human error surface.

**Governing commitments:** user's clear-errors/manual-retry requirement; `g5.architecture.git_outcomes`, `g5.architecture.git_retry`, `g5.data.outcome-evidence`, and `g5.security.credentials.redaction`.

**Repair:** translate the operation-specific typed result into a deliberately safe public outcome/error with explicit refused versus unknown state, authorized repository/ref target and corrective action. Preserve generic GitError sanitization; do not expose raw backend messages globally. Keep the secret scrub and avoid adding persistence.

**Verification:** drive a refusal and an injected uncertain commit result through the real `AppError`/HTTP serialization path, assert retained effect category and inspect/retry guidance, and assert credentials/raw private paths are absent. No such boundary assertion exists in the changed tests.

### R3-3 — P2: the new production boundary check rejects permitted test-only backend fixtures

**Changed location:** `backend/g5-api/tests/authoritative_workspace_reader_guard.rs:72-77` (and its unconditional AST traversal at `:96-97`).

The AST scan visits `#[cfg(test)]` modules and flags every `git2` identifier whenever a migrated owner has required function names. A valid production implementation plus `#[cfg(test)] mod tests { fn fixture() { git2::Repository::init_bare(path); } }` therefore fails. The same traversal flags maintenance-reader calls in tests outside the owner. This changes the architectural contract into a test-fixture restriction, despite `g5.architecture.git_guard` explicitly exempting tests/fixtures from production dependency checks. It discourages exactly the independent backend fixtures needed to test boundary behavior. The calibration tests exercise comments and helper renames but omit the stated test exception.

**Repair/verification:** exclude syntactically identified test-only items/modules from production-access checks without excluding production functions; add paired calibration fixtures showing a `cfg(test)` backend fixture succeeds while the equivalent production bypass fails. Required production-operation presence checks should remain effective.

## Commitment coverage and limits

| Commitment | Actual path and counterexample/check | Result |
|---|---|---|
| `g5.architecture.git_execution`, `git_owner`; `g5.design.abstraction`, `minimal-api`, `information-hiding`, `dependencies` | Three changed consumers now call owned `g5-scm` APIs; history is a domain enum, diff context is backend independent, repository path comes from RepoStorage. No new manifest dependency or generic command/backend escape hatch is introduced by this slice. | Preserved in the migrated production paths by inspection. Existing public backend handles and wider backend/native/provider migration remain explicitly planned work, not new defects. |
| `g5.architecture.git_ref_updates` | `pin_resolution_inputs` locks local then remote, checks both direct targets before staging, rejects symbols and mismatches, returns identical-pair no-op. Unit fixtures cover absent/identical/conflicting/incomplete/symbolic/noncommit/lock/concurrent cases. Provider caller needs checked replacement. | Create-only invariant looks correct; composition violates existing retry behavior (R3-1). Runtime tests not independently run. |
| `g5.architecture.git_outcomes`; `g5.data.outcome-evidence` | Transaction commit is distinguished from pre-write failures; both locks drop on error. Existing test injects invalid reflog and checks `Uncertain`. Downstream response sanitizes it. | Backend uncertainty is honestly represented; public outcome is lost (R3-2). No claim that libgit2 offers rollback atomicity. |
| `g5.architecture.git_retry`; `g5.architecture.recovery` | No added schema, persistence, workers, sleeps or automatic replay in diff. Same provider run ID is reused after SQL rollback. | No-new-tracking constraint preserved; existing recovery regressed (R3-1). |
| `g5.architecture.projection_owner`, `content_reader`, `git_locations` | `projection_convergence::{ref_oids,ref_oid}` still owns storage selection and blocking execution; new methods reproduce heads-only sorted enumeration and exact-ref peeling. Tests distinguish missing ref from dangling symbolic/blob target and missing mirror. | No defect found by inspection. New methods do not create files or substitute mutable checkout content. Public consumers continue existing authorized readers. |
| Full-context diff acceptance; `git_execution`, `content_reader` | `semantic_record_diff_from_text_with_budget` delegates supplied bytes to the new helper and retains its existing parse/budget path. Default wrapper uses default DiffOptions; full context counts newline bytes plus one, safely covering final unterminated line. | No regression found. Default/full/unchanged/unterminated cases have discriminating unit assertions; runtime execution unverified here. |
| `g5.architecture.git_guard`; `g5.quality.review.guards`, `review.cases`; `g5.design.enforcement` | AST scan rejects git2 aliases, qualified/method maintenance calls, absent named operations, malformed source and missing files; helper rename fixture passes. New test-only false positive is uncalibrated. | R3-3. The selected guard is a partial migration ratchet, not whole-platform enforcement. |
| `g5.security.credentials.redaction` | New backend error detail is scrubbed; targets are generated refs. All public generic GitErrors are hidden. | No observed credential disclosure; safe useful outcome must be added without weakening generic sanitization (R3-2). Credential injection migration itself is outside this slice. |
| `git_native_runner`, `git_credentials`, `scm_provider_client`, `scm_credential_scopes`, `github_transport`, broader `git_locations`/`git_guard` | No new native/provider implementation in the snapshot. | Explicitly remaining implementation work; no completion claim evaluated. |

The saved design's ontology inventory matches the added Git execution/provider commitments and navigation described in the current governing files; this implementation snapshot contains no new ontology body edits. Main should update affected metadata after repair and verification, particularly `git_execution`, `git_ref_updates`, `git_outcomes`, `git_retry`, `projection_owner` and `git_guard`, while retaining broader migration gaps. The design needs the specific caller/retry correction described in R3-1; it does not need another plan.

## Checks performed

- `shasum -a 256 -c /tmp/g5-git-abstraction-initial-review.sha256`: all eight snapshot paths matched.
- Inspected complete production/test diff, new resolution module, direct caller composition, public error serialization, existing retry integration regression, governing body text and relevant metadata.
- No runtime tests executed by this reviewer; main owns shared compilation/test resources. The source counterexamples above are not represented as runtime failures.
