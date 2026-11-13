# Address issue #387: g5d pr gaps an agent hit against gh

## Ontology changes and governance

Comparison base: this branch at `4e94eb7a` (PR #382), which already carries the #379 review reads and the gh-alignment fixes. Intent: [request](../../chat_intent/issue-387-g5d-pr-gaps/request.md).

All changes are in `features/governance.g5.md`:

- **New:**
  - [`g5.governance.pr.read.freshness`](../../ontology/features/governance.g5.md#say-how-current-merge-state-is)
  - [`g5.governance.pr.conversation`](../../ontology/features/governance.g5.md#take-part-in-the-pr-conversation-from-the-cli)
  - [`g5.governance.pr.body`](../../ontology/features/governance.g5.md#publish-an-explicit-pr-description)
  - the question `g5.governance.pr.checks.logs`, since answered (see [amendments](#implementation-amendments)); it is now [a decision](../../ontology/features/governance.g5.md#return-build-log-tails-only-as-github-published-output)
- **Amended during implementation:** see [Implementation amendments](#implementation-amendments). The diff below is the original handoff.
- **Amended:** [`g5.governance.pr.write`](../../ontology/features/governance.g5.md#create-or-update-a-pr-from-a-workspace-agent). Its alias and result-presentation choices were recorded as agent interface decisions, so this amendment stays within its provenance.

```diff
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 99e33602..921cccc7 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -224,6 +224,14 @@ Governing: [workspace read authority](../concepts/security.g5.md#read-permission
 [current token authority](../concepts/security.g5.md#admit-actions-against-current-authority)
 and [server-side provider credentials](../concepts/security.g5.md#keep-provider-publication-credentials-server-side).
 
+### Say how current merge state is
+
+[D · g5.governance.pr.read.freshness]
+
+Stored G5 merge evidence names the target revision it was computed against, is reported stale once that target has moved, and shows its conflicting paths. Reads never recompute it. Live mergeability that GitHub is still computing is re-read briefly before it is reported unknown, and readable output tells a merged or closed PR apart from one still being computed.
+
+Reason: agents trusted a stored "ready" while GitHub reported conflicts, and could not tell a pending answer from a permanent one.
+
 ### Inspect provider checks from a workspace agent
 
 [D · g5.governance.pr.checks]
@@ -278,18 +286,49 @@ Governing: [PR retrieval](#retrieve-pull-requests-through-the-cli),
 [server-side credentials](../concepts/security.g5.md#keep-provider-publication-credentials-server-side)
 and [public-output redaction](../concepts/security.g5.md#redact-secrets-from-public-output).
 
+### Should check retrieval return build logs?
+
+[Q · g5.governance.pr.checks.logs]
+
+[Check details](#read-the-failure-evidence-published-to-github) exclude Cloud Build and build-log retrieval, so agents read CI logs through a log-reader key placed in the VM and several manual gcloud steps. Options:
+
+- (a) Keep the exclusion.
+- (b) For a failed check whose link names the deployment's own Cloud Build project, return a bounded, redacted tail of the failing step's log, read by the server under its own reader identity, so the key leaves the VM.
+
+Recommended: (b). It reverses the human-verified exclusion, so the human owner of that decision settles it.
+
+### Take part in the PR conversation from the CLI
+
+[D · g5.governance.pr.conversation]
+
+`g5d pr comment` posts to a provider PR's conversation, `--reply-to THREAD` replies in a review thread instead, and `g5d pr resolve THREAD` (`--unresolve`) changes a thread's resolution. They take the [retrieval selectors](#retrieve-pull-requests-through-the-cli), so they reach only PRs in repositories attached to the workspace, and they act under the acting human's connected GitHub identity as [PR creation does](#attribute-github-pr-creation-to-the-initiating-user): the workspace owner for a VM token, never the installation bot or another person.
+
+Comment and reply bodies pass the public-output secret screen and are recorded before dispatch with an idempotency key and a hidden marker. An uncertain result is reconciled by finding the marker, never by posting again. Resolution is idempotent, and an uncertain result is re-read. These writes confer no approval, review decision or merge authority; [formal approval](#approval-is-a-human-authority) keeps its owner.
+
+Reason: bot reviewers are driven through the PR conversation, so an agent that cannot reply, resolve or post `@coderabbitai review` cannot finish a review loop.
+
 ### Create or update a PR from a workspace agent
 
 [D · g5.governance.pr.write]
 
 `g5d pr create` and `g5d pr update` invoke the same create-or-update operation as the UI
-buttons, with optional target, title and description inputs. Both names preserve
-existing PR reuse. The current workspace is the source; a VM token may act only for its
+buttons, with optional target, title and description inputs. `create` reuses an open G5 PR
+for the same source and target, as the UI does. `update` requires one and never creates a
+G5 PR or a provider PR, so a mistaken update fails and names the command to run instead.
+Neither replaces a recorded provider PR that GitHub reports merged or closed: `update`
+reports it as action required, and `create` opens a replacement only with `--new`.
+The current workspace is the source; a VM token may act only for its
 current owner and incarnation, with target read access and the same source restrictions
 as the UI. Server-side credentials retain [acting-user authorship](#attribute-github-pr-creation-to-the-initiating-user).
 
-The command includes the UI's save/push preparation and reports the resulting G5 PR and
-provider outcomes without presenting partial provider failure as complete publication.
+The command includes the UI's save/push preparation and reports the resulting G5 PR and,
+for each provider repository, the provider PR number, whether it was created, reused or
+left for action, and whether its head and any explicit title or description reached the
+provider. A save/push failure, a saved G5 PR with incomplete or pending provider
+publication, and complete publication exit differently; partial provider failure is never
+presented as complete publication. The operation reconciles current state, so a lost
+response is recovered by running it again; the CLI retries gateway failures a bounded
+number of times.
 It supplies no merge, approval or implicit compute-start authority. [Current admission](../concepts/security.g5.md#admit-actions-against-current-authority),
 [typed service identity](../concepts/security.g5.md#keep-human-and-machine-identities-distinct)
 and [one shared flow](../software_architecture/git_governance.g5.md#one-create-or-update-pr-flow) govern every entry point.
@@ -350,6 +389,14 @@ older title over a newer accepted title. Existing provider PRs remain
 eligible for title synchronization when their member changes are already included in the
 target; this never authorizes a new empty provider PR.
 
+### Publish an explicit PR description
+
+[D · g5.governance.pr.body]
+
+An explicit description (`-b/--body`, `-F/--body-file`, the UI's description override) becomes the body of the G5 PR's provider PRs, existing ones included, as an [explicit title](#accept-an-explicit-pr-title) does. Omission never reasserts a stored description onto an existing provider PR. A description the provider cannot accept is refused before any mutation, never silently replaced by a derived body. A failed body update is reported, and resubmitting the description retries it; no pending body is stored.
+
+Reason: the stored description silently diverged from GitHub's, and the agent reported a body as published that reviewers never saw.
+
 ### Select PR JSON fields using familiar syntax
 
 [D · g5.governance.pr.json]
@@ -693,6 +740,12 @@ Automatic divergence pauses synchronization without launching resolution. An exp
 - Implemented in `g5d/src/cli/pr.rs`, `routes/{workspaces,internal}.rs`, incarnation auth middleware and `workspace_pull_request_read.rs`. Compiled CLI tests cover environment, config, stable metadata and explicit selection; restricted-role API tests cover user/VM admission, repository selection, narrow App and personal credentials, redirects and failures. Live GitHub and deployed guest execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#cli-retrieval-consolidated-discovery-before-repair). Current SCM migration tests additionally cover configured-instance attached identity and exact App/personal credentials with foreign-origin refusal. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
 - [Issue #379](https://github.com/G5Labs/G5-Lean/issues/379): comment reads also return inline review threads with resolved/outdated state and reviews, read through GitHub GraphQL under the same PR read grant in bounded pages; GraphQL errors fail the read as refusals. A follow-up `gh pr` alignment audit (agent decision) added gh-shaped `reviews`, a separate `reviewDecision`/`latestReviews` read, live-command guidance for GitHub-only fields on stored PRs and for gh's `--jq`/`-q`/`--template`/`--web`, and documented exit codes that differ from gh. `pulls.rs::{review_connection,graphql}`, `g5d/src/cli/{mod,pr,pr_json}.rs`; SCM, route and compiled CLI tests. Live GitHub GraphQL remains unverified.
 
+### g5.governance.pr.read.freshness
+
+- agent
+- From [issue #387](../../chat_intent/issue-387-g5d-pr-gaps/request.md): stored ready versus GitHub conflicting, always-UNKNOWN mergeability and unnamed conflicting paths. Design: [plan](../../pr_ontology_diff_design/issue-387-g5d-pr-gaps/ontology-plan.md).
+- Gap · not implemented: stored preflight carries per-repo `targetHead` and `conflictPaths`, but no read compares the target, the text views (`g5d/src/cli/pr.rs::render_workspace_pr`) omit conflicting paths, and `g5-scm` `read_pull_request` reads `mergeable` once.
+
 ### g5.governance.pr.checks
 
 - human-verified
@@ -711,11 +764,23 @@ Automatic divergence pauses synchronization without launching resolution. An exp
 - [Failure reading and Cloud Build exclusion](../../chat_intent/codex-pr-user-authorship/request.md#inspect-pr-checks-from-the-vm). Bounded GitHub output and link handling are agent design decisions.
 - Implemented · the checks owner retains and redacts GitHub output, bounds annotations/text, preserves partial output on retrieval failure, and stops requests on rate limits. API/CLI regressions verify partial diagnostics, 403/429, multi-repository fanout, secret/enum separation and external-link refusal. Actual Cloud Build payloads and direct logs remain unverified/outside scope. Current route regressions place configured secrets across provider diagnostic/header caps, including failed annotations; redaction precedes truncation and preserves retry evidence. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
 
+### g5.governance.pr.checks.logs
+
+- agent, uncertain
+- From [issue #387](../../chat_intent/issue-387-g5d-pr-gaps/request.md): every CI failure needed gcloud, the reader key and the right region. The exclusion it would reverse is human-verified in `g5.governance.pr.checks.details`.
+- Unassessed · open question; nothing to implement until answered.
+
+### g5.governance.pr.conversation
+
+- agent
+- From [issue #387](../../chat_intent/issue-387-g5d-pr-gaps/request.md), ranked first by both agent reports. Reusing the issue-comment publication ledger and the acting-user grant are agent design decisions in the [plan](../../pr_ontology_diff_design/issue-387-g5d-pr-gaps/ontology-plan.md).
+- Gap · not implemented: no PR comment, reply or resolve path. Review-thread reads (`g5-scm/src/providers/pulls.rs`) do not yet return the thread node ID the writes need; the GraphQL helper is read-only by transport classification.
+
 ### g5.governance.pr.write
 
 - human-verified
-- [Shared CLI/UI PR flow request](../../chat_intent/codex-pr-user-authorship/request.md). Target/title follow-up supersedes the initial explicit-target requirement; CLI aliases and result presentation are agent interface decisions.
-- Implemented by `workspace_pull_request_create.rs`, thin public/internal routes, runtime fencing and `g5d/src/cli/pr.rs`. Public/VM HTTP tests verify reuse, current source/target authority, save/push ordering and explicit partial provider errors; compiled CLI and UI tests verify projection, recovery and refreshed PR lists. Actual active-agent/native Git callbacks, live GitHub and deployed workers remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment). Queued UI/VM writes and provider admission retain current target access across PR waits; refused metadata and authorized retries are checked in the [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
+- [Shared CLI/UI PR flow request](../../chat_intent/codex-pr-user-authorship/request.md). Target/title follow-up supersedes the initial explicit-target requirement; CLI aliases and result presentation are agent interface decisions. The create/update split, `--new`, per-repository outcome reporting, distinct exits and bounded retry amend those agent decisions after [issue #387](../../chat_intent/issue-387-g5d-pr-gaps/request.md) ([plan](../../pr_ontology_diff_design/issue-387-g5d-pr-gaps/ontology-plan.md)).
+- Implemented by `workspace_pull_request_create.rs`, thin public/internal routes, runtime fencing and `g5d/src/cli/pr.rs`. Public/VM HTTP tests verify reuse, current source/target authority, save/push ordering and explicit partial provider errors; compiled CLI and UI tests verify projection, recovery and refreshed PR lists. Actual active-agent/native Git callbacks, live GitHub and deployed workers remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment). Queued UI/VM writes and provider admission retain current target access across PR waits; refused metadata and authorized retries are checked in the [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment). Gap · the issue #387 amendment is not implemented: `update` is a clap alias of `create`, a 2xx always exits 0, the CLI prints only the G5 row's created/updated state, an externally merged provider PR is replaced silently (`workspace_pull_request_provider_lifecycle.rs`), and the VM client does not retry create.
 
 ### g5.governance.pr.follow-push
 
@@ -736,6 +801,12 @@ Automatic divergence pauses synchronization without launching resolution. An exp
 - [Title support and manual retry requests](../../chat_intent/codex-pr-user-authorship/request.md#manual-retry-without-new-migrations). Failed explicit titles may require resubmission, superseding durable pending title intent; omission and concurrency protections remain.
 - Implemented with request-scoped title input and PR-row serialization/comparison. API tests with mocked GitHub cover explicit resubmission, omission, external edits, overlapping/stale requests, included-member updates and terminal refusal. UI/VM retries preserve completed provider pins and observed proof; arbitrary post-advance merge import is not claimed. Compiled CLI checks pass; live GitHub remains unexecuted. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment). Current provider migration tests preserve explicit resubmission and identity, including long create-error guidance through stored status/events and public output. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
 
+### g5.governance.pr.body
+
+- agent
+- From [issue #387](../../chat_intent/issue-387-g5d-pr-gaps/request.md): a 9255-character stored description against a 4148-character GitHub body, reported as published. Mirrors the human-verified title rule `g5.governance.pr.title`. Design: [plan](../../pr_ontology_diff_design/issue-387-g5d-pr-gaps/ontology-plan.md).
+- Gap · not implemented: an explicit description is stored as `description_override` and used only when a provider PR is created; `g5-scm` has no body update, and an over-limit project-doc or derived body falls back silently.
+
 ### g5.governance.pr.json
 
 - human-verified
```

### Relevant and governing ontology

| Block | Why relevant | Implementation must |
|---|---|---|
| [`g5.governance.publish.github-author`](../../ontology/features/governance.g5.md#attribute-github-pr-creation-to-the-initiating-user) | Identity for user-authored provider writes | Post comments, replies and resolutions with the acting user's App grant (owner for a VM); no bot or other-person fallback; missing grant → reconnect error |
| [`g5.governance.pr.read`](../../ontology/features/governance.g5.md#retrieve-pull-requests-through-the-cli) | Selector and attached-repository rules | Conversation writes reuse the read selectors; reads stay side-effect free (freshness never recomputes) |
| [`g5.governance.pr.title`](../../ontology/features/governance.g5.md#accept-an-explicit-pr-title) | Pattern the description rule mirrors | Same explicit-only, no-pending-state, ordered-overlap behavior for the body |
| [`g5.governance.pr.follow-push`](../../ontology/features/governance.g5.md#follow-successful-source-pushes-automatically) | Head publication after later pushes is asynchronous and policy-bound | Report a lagging provider head as pending, never as published |
| [`g5.governance.pr.checks.details`](../../ontology/features/governance.g5.md#read-the-failure-evidence-published-to-github) | Human-verified build-log exclusion | Nothing changes until `g5.governance.pr.checks.logs` is answered |
| [`g5.external-issues.broker`](../../ontology/features/external_issues.g5.md) / [`.discussion`](../../ontology/features/external_issues.g5.md) | The existing comment-publication broker; "do not create another authoritative comment store" | Reuse the publication ledger (a dispatch record, not an authoritative store); GitHub stays the store of PR conversation |
| [`g5.security.repository-credentials.github`](../../ontology/concepts/security.g5.md) | No fallback on insufficient App permission | A write needing `pull_requests: write` fails closed with a permission message |

## Outcome and design

Findings, against this branch:

| Finding (#387) | Status here | Plan |
|---|---|---|
| Inline review read | Done (#379): threads, resolved/outdated, reviews, `reviewDecision`, `latestReviews` | Phase 1 adds thread `id` and `diffHunk`, which writes need |
| Comment, reply, resolve | Missing | Phase 1 |
| `create` opened a new PR after a merge | Missing | Phase 2 |
| `update` claims success; body never published; head lag | Missing | Phase 2 |
| 502s; which stage landed | Missing | Phase 2 |
| `view -c` truncation | Mostly the deployed build lacking #379 (13 of 14 were inline). Text already says when more exist | Phase 4: header shows fetched of total |
| `mergeable` always UNKNOWN; stale `localMergeStatus`; no conflicting paths | Missing | Phase 3 |
| Stored/live schema differ silently | Done (#382 hint) | — |
| `reviewDecision` | Done (#382) | — |
| `files`, `commits`, `labels`, `statusCheckRollup` | Missing | Phase 4, lowest priority |
| `pr diff` | Missing | Not planned: `git diff origin/<base>...HEAD` is equivalent in the VM; document it |
| Build logs | Excluded by a human decision | Question `g5.governance.pr.checks.logs`; no work until answered |
| Issue creation | Out of `g5d pr` | Not planned here: platform reporting is a deployment setting, and general issue creation belongs to external-issues publication policy |
| Discoverability | Flags exist; help is thin | Phase 4 |
| Keep `provider/g5/<branch>` | Undocumented | Phase 4 docs |

```
g5d pr comment / --reply-to / resolve ─► internal + v1 PR conversation routes (owner or user)
                                          │ select attached repo + PR (read selectors)
                                          │ ledger row (idempotency key, marker) ─► GitHub
                                          │   as acting user's App grant (RepositoryCredentials)
                                          ▼   uncertain → reconcile by marker / re-read state
g5d pr create | update [--new] ─► create_or_update(mode) ─► save/push ─► G5 PR ─► provider sync
                                     per-repo outcome {created|reused|replaced|action_required,
                                     head published|pending, title/body updated|failed} ─► exit 0/5/4/1
```

**Phase 1: PR conversation (P0).**
- **Reads:** threads gain their node `id` and the first comment's `diffHunk`.
- **Writes:** one server operation behind internal (VM) and public routes. It:
  - resolves the repository and PR with the read selectors;
  - for a reply or resolve, re-reads the thread node to confirm it belongs to that PR;
  - authorizes a new user-authored `RepositoryOperation` through the existing acting-user branch that `CreatePullRequest` uses, with `PullRequestsWrite`. Without an App binding it uses the personal token, as today.
- **How each write is sent:**
  - Comments reuse g5-scm's issue-comment send, since a PR conversation is an issue thread.
  - Replies use GraphQL `addPullRequestReviewThreadReply`.
  - Resolve uses `resolveReviewThread` / `unresolveReviewThread`, through a new mutation request that the transport classifies as a write. A failure then reports as unknown, not refused.
- **Recording:** comment and reply bodies extend the existing `issue_comment_publications` ledger with a destination kind (PR conversation or review thread). The ledger already provides the idempotency key, the hidden marker, the frozen payload and outcome reconciliation. Resolution needs no ledger, because it is idempotent and re-readable.
- **CLI:** `g5d pr comment [PR] -b|-F [--reply-to THREAD] [--idempotency-key]` and `g5d pr resolve THREAD [--unresolve]` follow gh's `pr comment` shape and print the comment URL.

**Phase 2: truthful create/update (P0/P1).**
- **Split the commands:** `update` becomes its own subcommand sending `mode=update`, which fails with "no open G5 PR; run g5d pr create" instead of creating one.
- **No silent replacement:** provider sync checks a recorded provider PR's state before creating another. A merged or closed one becomes `action_required` naming the PR, unless `create --new` asked for a replacement.
- **Description:** an explicit description goes to existing provider PRs through a g5-scm PR update carrying title and/or body. An over-limit derived body becomes a reported refusal, not a silent fallback.
- **Response:** the create response gains a per-repository publication result covering action, PR number, provider head vs saved head, and title/body outcome. It is computed from this request, not stored as new state.
- **Exit codes:**

  | Exit | Meaning |
  |---|---|
  | 0 | Complete publication |
  | 5 | G5 PR saved, provider publication incomplete or pending |
  | 4 | Save/push failure (the server tags it with a typed error code, which also fixes the inaccurate exit-4 row this branch documented) |
  | 1 | Other errors |

- **Retries:** the CLI retries 502/503/504 and transport loss up to three times with backoff, since the operation reconciles. An HTML gateway body is summarized as "gateway error; the PR may be saved — run `g5d pr status`".

**Phase 3: merge-state freshness (P1).**
- **Stored PRs:** reads compare stored per-repository `targetHead` with the target branch's current head and flag stale evidence (`targetMoved`), without recomputing. Text views show conflicting paths per repository.
- **Live PRs:** the provider read re-requests an open PR's null `mergeable` a few times within about 3 s. Readable output shows `mergeable`/`mergeStateStatus`, and says "not applicable" for a merged or closed PR. JSON keeps gh's `UNKNOWN`.

**Phase 4: polish (P2).**
- **Help:**
  - `g5d pr checks --help` documents the PR argument, `-R`, `-i` and `--fail-fast`, and says details cover only failed or cancelled checks.
  - `g5d pr --help` gains a push → `checks --watch --head` example.
- **Discussion header:** "Discussion comments (fetched of total)".
- **Docs:** `docs/g5d-pr.md` and `AGENTS.md` document `refs/remotes/provider/g5/<branch>`, the conversation commands, create/update semantics and the corrected exit codes.
- **Optional:** `files`, `commits`, `labels` and `statusCheckRollup` (projected from the existing checks reader).

### Acceptance

- **Conversation:** an agent reads a CodeRabbit thread, replies in it, resolves it and posts `@coderabbitai review`, each attributed to the workspace owner. A retry with the same key posts nothing new. An uncertain dispatch reconciles to the posted comment. A thread ID from another PR or repository, or a user without a grant, is refused before any provider call.
- **Create/update:**
  - After a provider PR merged on GitHub, `g5d pr update` exits non-zero naming the merged PR and opens nothing.
  - `g5d pr create --new` opens the replacement and prints its number.
  - `--body` changes an existing GitHub PR's body.
  - A failed body or title update, or a lagging head, exits 5 with the reason.
- **Freshness:** after the target moves, stored views mark the PR's evidence stale and list conflicting paths. An open PR whose mergeability GitHub computes within about 3 s reports it instead of `UNKNOWN`.
- **Lost response:** a lost create response retried by the CLI leaves one G5 PR and one provider PR per repository.

### Risks

- **Concurrent retry:** a gateway 502 doesn't stop the server. A CLI retry can overlap the original request, and two concurrent provider syncs could each create a GitHub PR. The open-G5-PR unique index covers only the G5 row, so provider sync needs serialization per G5 PR before retries are enabled.
- **App permissions:** the QA App's documented permissions list only Issues. Review replies and resolution need `pull_requests: write`, so installations without it fail closed until reconfigured.
- **Behaviour change:** `update` no longer creates PRs, and partial publication exits 5 instead of 0. Scripts and agent instructions (AGENTS.md) that relied on the alias or on exit 0 need the doc update in the same release.
- **Merged on GitHub:** a G5 PR whose provider PRs all merged on GitHub stays open in G5. This plan stops the silent replacement but doesn't reconcile that state; that remains a follow-up.

## Implementation amendments

After the human's [answers](../../chat_intent/issue-387-g5d-pr-gaps/request.md#implementation-answers):

- **Build logs.** `g5.governance.pr.checks.logs` became a decision: G5 never reads Cloud Build; a log tail reaches check details only when a CI-side publisher posts it to GitHub, under its own identity and a checks-only GitHub App that build steps never hold. Option (b), a server-side reader, is not approved. The publisher is not built: it needs a human to create and install the App.
- **UI = CLI.** The UI's Update PR sends `mode: update` like `g5d pr update`, and the Prepare to Ship description is editable once a GitHub PR exists, so an edit reaches it as `--body` does.
- **`update` and provider PRs.** `update` never creates a G5 PR, but may open a provider PR for a repository that has none. Recovery after a failed first publication depends on that, so `g5.governance.pr.write` now says so instead of "never creates a provider PR".
- **Save failures.** They keep their own status, code and details and gain `details.kind` (`pull_request_source_save_failed`), instead of the generic 409 first built, so existing codes such as `WORKSPACE_NOT_RUNNING` stay stable. g5d maps the kind to exit 4.
- **Stored freshness** compares recorded target heads with the mirrors and reports only whether each target moved, not its current head, so a reader without target access learns nothing more than staleness.

## Review summary

- **Challenged:**
  - Reusing the external-issues ledger against `g5.external-issues.discussion`: it's a dispatch record, and GitHub stays the authoritative store.
  - The comment identity against `g5.governance.publish.github-author`: it's the same user-grant path, never the bot.
  - The `pr.write` amendment against its provenance: the aliases were agent decisions.
- **Open for you:**
  - `g5.governance.pr.checks.logs`, the build-log question.
  - Whether the UI's Update PR button should get the same no-replacement behavior. Recommended yes, but it's outside the CLI findings.
  - Exit code 5's meaning.
- **Verify during coding:**
  - the ledger's destination trigger and unique key extend cleanly to PR destinations;
  - GitHub lets the user-to-server token resolve threads with the App's current permissions;
  - provider sync is serialized per G5 PR (needed before CLI retries);
  - the VM save step is safe to rerun while an earlier request is still running.

### Implementation review

Self-review of phases 1-4 against this plan; findings, fixes and the commitment accounting are in the [review](review.md). Results of the design-time checks:

- The ledger extends to PR destinations through migration 074 (API tests cover replay and marker reconciliation).
- Provider sync is serialized by the PR-row lock held to commit, so CLI retries reuse rather than duplicate (inspected; no concurrent-retry test).
- Thread resolution with the user-to-server token and the App's actual permissions is unverified against live GitHub.
- The open items above were answered by the human (build logs, UI alignment); exit 5 is implemented as planned.
