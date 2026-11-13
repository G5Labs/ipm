# Question store for G5 Lean

## Ontology changes and governance

**Comparison base:** `origin/main` at `d5b94253`, branch `g5/question-store-plan`. The request, the G5-versus-Lean findings it rests on, and the parallel design integrated on the same day are in the [chat intent](chat_intent/question-store/request.md). The source design is [G5 PR #2532](https://github.com/G5Labs/G5/pull/2532); its question, option, answer and provenance shapes are carried over, its platform pieces are re-decided here.

**Ontology changes:**

- **Added** in [documents](ontology/features/documents.g5.md): the story `g5.story.answer-a-question-anywhere` and the blocks `g5.documents.question.referenced`, `.writer`, `.answer-checkpoint`, `.decision`, `.readers`, `.agent-access`, `.html`, `.metrics`.
- **Added** in [Git/governance/documents module](ontology/software_architecture/git_governance.g5.md): `g5.architecture.question_owner` and its public-operations row.
- **Links only:** the [story index](ontology/concepts/core_user_stories.g5.md).
- **Meaning changed:** none. `g5.documents.editor.question` keeps the embedded form; the new blocks reference it.
- **Removed:** none.
- **Revised after the parallel design** (chat intent, "Parallel design"): `.referenced` gained explicit set paths and the FILES scope; `.writer` gained intent-bound replay; `.answer-checkpoint` gained applied-versus-published outcomes and uncommitted agent writes; `.html` was added; `.metrics` dropped the `question.updated` event.
- **Human decisions** (chat intent, "Human decisions", 2026-10-09): a human answer checkpoints (`.answer-checkpoint` is human-verified); integrity checks are dropped entirely, so `.writer` keeps schema validation, the one-writer rule and receipt binding and has no signature, classifier, repair or verify (human-verified). Implementation is authorized.

- **Amended during implementation** (2026-10-09): `.answer-checkpoint` now says a person's create or append through the public route is the same explicit single-file Save (the public create route existed in the implementation map but its durability was unstated; treating it as live-only would leave a person's authored set invisible to readers). Review fixes removed a duplicated heading and anchor the handoff edit introduced in `documents.g5.md`; the new blocks' metadata now records implementation and verification limits ([review](ontology-review-question-store.md)).

- **Amended for votes** (2026-10-09, branch `feat/question-vote-page`): readers may vote on referenced questions; see [Amendment: votes](#amendment-votes-2026-10-09) below. `.readers`, `.html` and the story gained one clause each; `g5.documents.question.votes` was added.

The full diff at the handoff is at the end of this section; it is the original handoff and is not rewritten.

**Relevant and governing ontology:**

| Commitment | Obligation → owner and operation | Allowed case · counterexample | Conclusion |
| --- | --- | --- | --- |
| [g5.documents.question.referenced](ontology/features/documents.g5.md#reference-a-question-from-its-set-file) | Renderers in SpecEditor and explainers show a reference as the card; references carry the set path; the set file is the one payload home; both shapes stay readable. | A forked workspace's inherited doc resolves `docs/<parent>/questions/x.g5q.json` · a path derived from the child's id, or a card in FILES Preview. | Added. |
| [g5.documents.question.writer](ontology/features/documents.g5.md#write-set-files-only-through-the-question-service) | `QuestionSetService` is the only typed writer; schema validation on read and on the owner file routes; no signature; replay bound to actor, operation and payload. | An unparseable set renders a notice and is not actionable · a broken file accepted by the file PUT, a replay with the same key and a different payload, or a claim that a recorded answer proves approval. | Added; human-verified; honest about no prevention. |
| [g5.documents.question.answer-checkpoint](ontology/features/documents.g5.md#answering-is-an-explicit-save-of-one-file) | Human answer = whole-file live write with hash and VM preconditions + selected-file checkpoint; agent writes stay live; saved only after publication; VM required. | A pushed answer a reader sees; an agent proposal visible after the agent's push · an answer that starts the VM, a proposal that auto-commits, or "saved" shown after a failed push. | Added; human-verified. |
| [g5.documents.question.decision](ontology/features/documents.g5.md#a-decision-is-a-persons-answer) | User answers decide; agent answers propose; wording edits keep the answer with a visible earlier revision; option changes need a new question. | Owner confirms an agent proposal · an agent that clears an answer, or a decision that settles an approval. | Added; `onBehalfOf` deferred. |
| [g5.documents.question.readers](ontology/features/documents.g5.md#readers-see-committed-answers-read-only) | Readers get committed, read-only cards; no share links; reading never starts compute. | A reader replies in the thread · a non-member answering through a token. | Added; applies H072. |
| [g5.documents.question.agent-access](ontology/features/documents.g5.md#agents-read-and-propose-through-the-workspace-cli) | `g5d question list\|create\|answer` over incarnation-authenticated internal routes; proposals only; live writes; idempotent retry; harness guidance. | A retried answer replays · an agent editing the file, or a route that accepts body provenance. | Added; replaces MCP tools, task-control token and outbox. |
| [g5.documents.question.html](ontology/features/documents.g5.md#confirm-explainer-answers-in-the-host) | Host supplies card data, validates frame messages, confirms saves in trusted UI; frame holds no credentials. | A human confirms in the host bar · a frame script that saves by posting a message. | Added. |
| [g5.documents.question.metrics](ontology/features/documents.g5.md#count-referenced-questions-once) | Third source family, counted once, current plus inherited sets, incomplete flag on invalid sets; refresh through existing notifications. | Two docs reference one open entry, count 1 · a new question event, or a sibling workspace's sets in the count. | Added. |
| [g5.architecture.question_owner](ontology/software_architecture/git_governance.g5.md#question-set-operations) | One owner composes reader, runtime file write and the checkpoint moved under the Workspace owner; adapters translate only. | Internal and public routes call the same `answer` · a route that stamps or writes bytes itself, or a service importing route code. | Added. |
| [g5.documents.editor.question](ontology/features/documents.g5.md#retain-structured-question-documents) | Embedded questions keep whole-document Save; the HTML bridge keeps host confirmation. | Existing docs behave as today · a converter rewriting fences. | Preserve. |
| [g5.documents.files-view.preview](ontology/features/documents.g5.md#render-files-markdown-as-github-does) | G5 blocks render only in SpecEditor; FILES Preview shows the reference fence as code. | FILES shows GFM · a card in FILES. | Preserve; scopes `.referenced`. |
| [g5.git.durability.no-implicit](ontology/software_architecture/durability.g5.md#keep-implicit-file-writes-separate-from-git-durability), [g5.git.checkpoint.selected](ontology/software_architecture/durability.g5.md#commit-and-push-only-selected-file-changes), [.isolation](ontology/software_architecture/durability.g5.md#preserve-unrelated-git-state-during-a-checkpoint), [.retry](ontology/software_architecture/durability.g5.md#retry-a-failed-selected-file-publication) | The Answer press is an explicit owner save of one file; agent writes are not; the checkpoint commits only that file and preserves unrelated state; a failed push is retryable with the same bytes. | Unrelated staged work survives · a periodic commit, or an agent create that commits. | Preserve; reuse the existing operation. |
| [g5.documents.save](ontology/features/documents.g5.md#save-binds-bytes-and-the-attachment-they-came-from), [g5.documents.files.version](ontology/features/documents.g5.md#require-a-file-version-before-mutation), [g5.documents.files.replay](ontology/features/documents.g5.md#preserve-file-mutation-results-across-retries), [g5.data.identity.incarnation](ontology/concepts/system_invariants.g5.md#old-vm-work-cannot-change-its-replacement) | The service writes with the observed blob hash and VM id; a stale card gets the current entry; file-operation replay stays bounded. | Two tabs: the second answer fails with `revision-moved` · a late write landing on a replacement VM, or an agent JSON edit without a revision bump slipping past the hash. | Preserve. |
| [g5.security.read.durable](ontology/concepts/security.g5.md#serve-nonowners-from-durable-state), [g5.security.owner](ontology/concepts/system_invariants.g5.md#one-eligible-owner-uses-live-compute) | Readers see committed sets; only the owner (and the owner's agent) writes. | Reader card is read-only · a reader PUT accepted. | Preserve; drives `.readers` and `.answer-checkpoint`. |
| [g5.lifecycle.start.consent](ontology/concepts/system_invariants.g5.md#compute-requires-explicit-start-consent) | A card on a stopped workspace offers Start; answering never starts compute. | Owner presses Start, then answers · an answer that boots the VM. | Preserve. |
| [g5.security.admission](ontology/concepts/security.g5.md#admit-actions-against-current-authority), [g5.security.credentials.platform-token](ontology/concepts/security.g5.md#expose-the-incarnation-token-under-the-mvp-boundary) | Internal question routes join the incarnation allowlist and run the live owner/VM/epoch/lease check and the direct-agent-write policy. | Agent answer on a running owned VM · an answer from a stale epoch. | Preserve. |
| [g5.scope.retired-mechanisms](ontology/scope.g5.md#retire-competing-execution-and-editing-models), [g5.design.minimal-mechanism](ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism), [g5.design.durability](ontology/concepts/system_design_principles.g5.md#declare-durability-before-choosing-storage), [g5.architecture.commands](ontology/software_architecture/boundaries.g5.md#keep-complete-operations-with-their-state-owner) | No draft or overlay, no table, no key config, no maintenance loop, no new event; durability is the existing checkpoint. | The service composes existing operations · an outbox table, a share-link table or a keyring. | Preserve; justifies the drops. |
| [g5.data.git-authority](ontology/concepts/data_model.g5.md#git-owns-repository-content), [g5.design.authority](ontology/concepts/system_invariants.g5.md#each-fact-has-one-source-of-truth) | The committed set file is the answer's one truth; cards and metrics are projections. | Metrics recomputed from the file · a cached answer that outlives the file. | Preserve. |
| [g5.conversation.native-private.attempt](ontology/features/conversations.g5.md#bind-native-questions-to-one-live-attempt), [g5.workbench.meeting](ontology/features/workbench.g5.md#review-a-meeting-through-a-question-set) | Live provider questions keep their own lifecycle. Meeting decisions were to keep their own document format; amended 2026-10-12 (UX v2 plan, Phase 4): a meeting's decisions are a referenced question set and a card action is the owner's Answer, so this store is their home. | A set answer is consumed on the agent's next read · a set answer replying to a native request. | Preserve; the meeting row is now a consumer. |
| [g5.harness.provided](ontology/features/ontology_and_harness.g5.md#provide-the-harness-in-every-workspace), [g5.runtime.workspace-cli](ontology/software_architecture/runtime.g5.md#make-the-workspace-cli-available-to-people-and-agents) | Guidance ships through `.agents/AGENTS.md` and the synced skills; the CLI is the agent's entry path. | Both providers learn the reference form from the same bytes · a provider-specific MCP tool. | Preserve. |
| [g5.documents.explainer](ontology/features/documents.g5.md#generate-ordinary-explainer-documents), [.location](ontology/features/documents.g5.md#keep-generated-explainers-discoverable) | An explainer creates its set through the CLI and emits placeholders; the host injects cards. | Explainer with three placeholders renders three cards · an explainer that embeds payloads in HTML. | Preserve. |
| [g5.quality.evidence](ontology/concepts/system_design_principles.g5.md#test-real-boundaries-and-report-exact-evidence) | Real-stack API tests and browser E2E over the native stack; the CLI path is exercised with a fake provider turn. | Answer, replay, 409, unparseable set, fork inheritance, reader read-only, agent proposal · unit tests only. | Preserve. |

## Requested outcome

The question store of G5 PR #2532, in G5 Lean: a question lives once in a set file in the docs repository, is referenced by path from documents and explainers, is answered in place by the owner from any surface, is readable by agents and proposable by them, is counted once in open-question metrics, and survives a fork. Embedded questions keep working untouched.

## Design

```text
  owner card (doc / explainer, host-confirmed)       agent in VM
        │ POST .../questions/:set/:q/answer                │ g5d question answer|create
        ▼                                                  ▼
  public route (user provenance)         internal route (agent provenance, incarnation token)
        └──────────────────┬───────────────────────────────┘
                           ▼
                 QuestionSetService.answer / create
   read live file over committed cut ─► parse + schema check
   ─► transition + shape/revision gates + receipt ─► render canonical bytes
   ─► WorkspaceRuntime fs.write (expectedHash, expectedVmId)
   ─► human: selected-file checkpoint (commit + push that file) ─► "saved"
      agent: stop here ("applied"); the agent's own push publishes
                           │
   readers, metrics, agents read the committed set file; owner views refresh
   on workspace.file_changed, readers on the durable-cut invalidation
```

One owner, `QuestionSetService`, composes four existing operations: the content reader with the live overlay, the carried contract module (parse, canonical bytes, gates), the `WorkspaceRuntime` file write with hash and VM preconditions, and the selected-file checkpoint, which moves out of route code under the Workspace owner so the editor Save and the service share one operation. It holds no state of its own: no table, no key, no cache, no event. Two thin adapters feed it. The public route under `/workspaces/:id/questions` takes the signed-in owner and forces `user` provenance; the answer is a human Save, so it checkpoints. The internal route under `/internal/workspaces/:id/questions`, added to the incarnation allowlist, takes the agent's token, runs the live-incarnation and direct-agent-write checks, and forces `agent` provenance; the write is a proposal or an authored set and stays in the live file until the agent pushes with the rest of its work. Success for a human answer means the file is committed and pushed; a landed write whose push failed is reported as applied and not published, retryable only while that exact version is current and without a second transition; a refusal (revision moved, hash moved, unparseable set, shape gate, receipt mismatch) changes nothing. The owner file routes run the service's schema check before landing any set file, so the HTTP file API cannot land a broken set; nothing gates native edits or pushes from the VM, and the ontology says a recorded answer is attribution, not proof of approval.

References carry the set's repository-relative path, so a fork, which copies the docs repository id and rewrites no paths, keeps every inherited reference resolvable; the resolver confines paths to the current docs member and the view's selected source. The client is the PR's client, re-pointed: `ReferencedQuestionCard` and `questionSet.ts` without the signature, the write adapter calling the public route, the explainer host injection mounted in `ExplainerOutputView` behind the existing host confirmation bar, the metrics DTO gaining the set family, and the card refreshing from the file-change notification it already receives. Readers get the same card from the committed read path with writes disabled. Agents get a `g5d question` command plus one paragraph in `.agents/AGENTS.md` and the two frontend chat prompts.

**Carried over** from G5 PR #2532: the question, option, answer and provenance shapes, canonical serialization, intrinsic ids, revision token, shape and revision gates, decision predicate and fixtures (`backend/g5-contracts/src/types/question_set.rs`), re-versioned as `schema: 3` without `integrity`; the client library and cards; the open-question metrics family (D26/D52); the harness wording for the reference form.

**Re-decided for Lean:** persistence (explicit single-file save for humans, live-only for agents, instead of draft writes), reference identity (explicit set path instead of a path derived from the current workspace), integrity (none, by human decision, instead of a workspace-derived HMAC, classifier, repair, bulk re-sign and keyed CI script), agent access (CLI instead of MCP tools and task-control token), provenance (proposal only, no `onBehalfOf`), readers (no share links), prevention (detection-only, no file-tool or commit gate), refresh (existing notifications instead of a question event), replay (receipt bound to actor, operation and payload instead of key only).

**Dropped:** task-draft seeding and `crdt/task_commit.rs`, the outbox and its maintenance loop, `g5d-tooling` tools, `tool_surface_contract.rs`, `mcp_preflight.rs`, tera templates, provided-agent asset copies, share-link migration 302 and its routes and page, the keyring, classifier, repair, verify and bulk re-sign, the `question.updated` event, the `.semlift` spec tree and the `intent_grounding`/`intent_record` JSONL.

## Integration of the parallel design

A second plan for the same request (`design/lean-question-store`, see the chat intent) reached the same shape: Git files, explicit owner save, owner-only answers, proposals from agents, no draft, no outbox, no on-behalf-of. Taken from it, with the reason each changes this design:

- **Explicit set paths and fork inheritance.** A path derived from the current workspace id breaks every reference a fork inherits, because the docs repository keeps `docs/<parent-id>/`. Now in `.referenced`.
- **No signature and no integrity machinery.** A key derived from one workspace fails for inherited sets and proves nothing against a checkout holder. The parallel plan recommended dropping integrity altogether; the human confirmed that. Now in `.writer`.
- **Agent writes stay uncommitted.** An agent call is not an owner Save under the no-implicit rule, and agents push their own work. Now in `.answer-checkpoint` and `.agent-access`.
- **Applied versus published, and intent-bound replay.** A landed write with a failed push must not show as saved, and a replay must compare actor, operation and payload, not only the key. Now in `.answer-checkpoint` and `.writer`.
- **Host confirmation for explainer answers.** A frame message is not a human click; Lean's bridge already confirms in the host. Now `.html`.
- **Reuse existing notifications.** The owner's file watch and the readers' durable-cut invalidation already cover the set file. Now in `.metrics`; the event is dropped.
- **FILES stays GitHub Markdown; inherited-set discovery; incomplete summaries.** Now in `.referenced` and `.metrics`.

Not taken:

- **Tightened revision rule** (an outdated answer stops being a decision until reconfirmed). G5's human decision Q5 keeps the answer across wording edits and shows the earlier revision; reconfirmation adds a state and a prompt for a case the visible revision already discloses. Kept as in G5.
- **MCP operations beside the CLI.** Lean's MCP server is Codex-only; the CLI serves both providers with one implementation.
- **A separate `features/question_sets.g5.md` page.** The blocks live beside the embedded-question rule they extend; a page split can follow if the family grows.

## Acceptance outcomes

- The owner answers a referenced question from a Project Doc card and from an explainer card, confirming in the host bar; the set file is committed and pushed, both cards and the workspace question panel show the answer without reload, and a reader's view shows it read-only after the push.
- Two stale tabs: the second answer is refused with the current entry; the same request repeated with its receipt replays without a second revision; the same key with a different payload conflicts.
- Push denied after the file write: the card shows applied, not published, keeps the answer, and retry publishes without a new transition; a set changed before the retry requires reload.
- A set file hand-edited to an unparseable form renders a notice in every referencing card, is excluded from actionable metrics with an incomplete flag, and becomes answerable again once repaired as ordinary source; a hand-edited but valid file is simply the new state.
- A forked workspace renders and answers an inherited reference; the parent is unchanged; a sibling workspace's sets are not counted.
- A stopped workspace shows referenced cards read-only with Start; answering never starts the VM.
- An agent turn runs `g5d question list` and sees `decision` flags; `g5d question answer` lands a proposal in the live file that the owner confirms after the agent's push; a repeated command replays.
- A hostile frame message (wrong frame, stale generation, unlisted reference, oversized payload) saves nothing.
- Open-question metrics count an entry once across two referencing documents, embedded-fence counts are unchanged, an invalid set marks the summary incomplete; the home attention list links the referencing document.
- Existing embedded-question documents and the HTML answer block behave exactly as before; FILES Preview shows the reference fence as code; the legacy browser suite still passes.

## Risks and unresolved assumptions

1. **A human answer checkpoints** and **no integrity checks** are confirmed human decisions (2026-10-09); they flip G5's no-auto-commit rule for human answers and drop its signing. No longer open.
2. **Reuse of the checkpoint operation.** `write_workspace_file_and_maybe_push` and `commit_and_push_files` in `routes/workspaces.rs` are route-local and return generic errors; the plan moves the composition under the Workspace owner with applied/published outcomes. Verify at the implementation base that the checked version is the version staged under concurrent writes and Git filters.
3. **No prevention of native set-file edits.** Stated in `.writer`.
4. **Explainer set creation.** The explainer skill creates its set through `g5d question create` and the explainer job writes the HTML; two writes, no transaction. A placeholder whose set is missing renders a notice, which is the designed failure mode; the agent's push publishes both.
5. **`onBehalfOf` deferral** loses one G5 decision (an agent relaying a person's chat decision counts). Recorded with reason in `.decision`; reversible once a trusted consent binding exists.
6. **No migration.** No table is added.

## Implementation map

Paths are at the comparison base, where services are grouped under `services/{workspace,git,identity,agents}/` and the CLI lives in `backend/g5d/src/cli/`.

| Area | Lean location | Work |
| --- | --- | --- |
| Contract | `backend/g5-contracts/src/types/question_set.rs`, `fixtures/question_set/` | Port the shapes, canonical bytes, ids, gates and predicate; `schema: 3`; no `integrity` field, `Keyring`, `sign`, `verify` or classifier; receipt with actor, operation and payload digest; drop the keyring, previous-key, unknown-key, tampered and non-canonical fixtures. |
| Checkpoint owner | `backend/g5-api-core/src/services/workspace/` (new operation), from `backend/g5-api-routes/src/routes/workspaces.rs::write_workspace_file_and_maybe_push` and `commit_and_push_files` | Move the write-then-checkpoint composition under the Workspace owner with typed applied/published outcomes; the editor Save route calls it. |
| Service | `backend/g5-api-core/src/services/workspace/question_set_service.rs` | New: resolve a set path in the docs member at the selected source; effective read via `services/git/workspace_reader.rs` live overlay; answer/clear/create_or_append and a schema check for the file routes; write via `workspace_runtime.rs` file write; human answer calls the checkpoint owner; audit at `g5.audit.question_set`. |
| Public routes | `backend/g5-api-routes/src/routes/questions.rs` | New: list, get, create/append, answer, clear. Register in `routes/mod.rs`; extend the static route-scope test. |
| Internal routes | `routes/internal.rs`, `backend/g5-api-core/src/middleware/auth_middleware.rs` allowlist | `GET/POST /api/internal/workspaces/:id/questions…` with `scoped_db_for_workspace_incarnation` and `DirectAgentWrite`; no checkpoint. |
| File-route gate | `routes/workspaces.rs::put_workspace_file`, `routes/docs.rs::update_project_doc`, `routes/files.rs::put_file_content` | Run the service's schema check when the path is a set file; refuse an unparseable body with a typed error. |
| Metrics | `services/workspace/doc_service.rs`, `services/workspace/workspace_service.rs`, `services/workspace/home_service.rs`, `routes/workspaces.rs` DTO | Port D26/D52 over `open_project_doc_question_sources`; scan current plus inherited referenced sets; incomplete flag. |
| CLI | `backend/g5d/src/cli/question.rs`, `backend/g5-internal-api/src/client.rs`, `dtos.rs` | `g5d question list|create|answer` over the internal routes with the incarnation client; explicit failure outside a VM. |
| Client | `frontend/src/lib/questionSet.ts`, `hooks/useQuestionSets.ts`, `components/editor/ReferencedQuestionCard.tsx`, `components/shared/questionCardRenderer.ts`, `SpecEditor.tsx`, `ExplainerOutputView.tsx`, `SandboxedHtmlFrame.tsx`, `htmlDocAnswerBridge.tsx`, `WorkspaceStatusBand.tsx`, `lib/types.ts`, `lib/ws.ts` | Port; no signature or verification state; write adapter to the public route with applied/published states; read-only for non-owners and stopped workspaces; host confirmation kept; refresh from `workspace.file_changed` and doc invalidation; no fence rendering in FILES. |
| Guidance | `.agents/AGENTS.md`, `frontend/src/components/generate/generateChatPrompt.ts`, `explainer/explainerChatPrompt.ts`, `scripts/sync-ontology-skills.py --write` | Reference form, `g5d question`, never edit set files, act only on decisions. |
| Tests | `backend/g5-api/tests/api_question_sets.rs`, `api_question_metrics.rs`; `frontend/e2e/flows/question-store.spec.ts`; `tests/e2e/src/scenarios/question_store.rs` with the fake provider CLI for the agent path | Real stack; fork inheritance, push-denied partial failure, hostile frame, authority matrix (owner, reader, org admin, revoked, stale incarnation, Base, after transfer). |

## Implementation notes

- Routes: `GET /api/v1/workspaces/:id/questions` (own sets), `GET …/questions/set?path=`, `POST …/questions` (create/append), `POST …/questions/answer` and `…/clear` with the set path in the body; internal `GET|POST /api/internal/workspaces/:id/questions` and `POST …/questions/answer`. Refusals carry the typed `QUESTION_CONFLICT` error (`revision-moved`, `idempotency-key-reused`, `reload-required`) with the current entry.
- A write that loses the file-hash race under an unchanged question revision re-reads and re-checks up to three times instead of surfacing a hash conflict; a moved revision is still refused.
- The checkpoint moved to `services/workspace/workspace_file_save.rs`; it was inspected before reuse: it writes under `expectedHash` and the observed VM, retries only identical bytes after a failed publication, and commits only the observed blob, which matches `.answer-checkpoint`.
- The fake provider gained a `G5_QUESTION_CLI_E2E` workflow so the browser flow can drive `g5d question` from a real agent turn; a separate Rust E2E scenario was not added.

## Review summary

- Self-review of the whole implementation against this plan and the governing blocks ([review](ontology-review-question-store.md)): contract, service, checkpoint move, routes, CLI, client, guidance, metrics and tests.
- Fixed in review: clippy findings, duplicated ontology heading/anchor, the layering inventory, and the schema check on review-accept and rename writes.
- Verified by unit, API (real Postgres, Git storage and the scripted workspace runtime) and component tests. Not executed: the browser flow `frontend/e2e/flows/question-store.spec.ts` — the review host has no `/dev/kvm` for workspace VMs.
- Open for the human: whether a new owner may supersede the previous owner's decision after a transfer (today refused).

<details>
<summary>Actual ontology diff at the handoff (<code>git diff -U0</code> against the comparison base)</summary>

```diff
diff --git a/.g5/ontology/concepts/core_user_stories.g5.md b/.g5/ontology/concepts/core_user_stories.g5.md
index 728008d4..32d94e58 100644
--- a/.g5/ontology/concepts/core_user_stories.g5.md
+++ b/.g5/ontology/concepts/core_user_stories.g5.md
@@ -77,0 +78 @@ These links add no separate commitments or implementation percentages.
+- [Answer a question wherever it is shown](../features/documents.g5.md#answer-a-question-wherever-it-is-shown).
diff --git a/.g5/ontology/features/documents.g5.md b/.g5/ontology/features/documents.g5.md
index 1df7c7a4..ee0198b7 100644
--- a/.g5/ontology/features/documents.g5.md
+++ b/.g5/ontology/features/documents.g5.md
@@ -81,0 +82,10 @@ Governing contracts: [open FILES files for reading](documents.g5.md#open-files-f
+### Answer a question wherever it is shown
+
+[I · g5.story.answer-a-question-anywhere]
+
+As a workspace owner, I can answer a design question from any surface that shows it — a Project Doc card, an explainer card or an agent's reading of the same question — and every surface shows the same answer.
+
+Governing concepts: [Git content authority](../concepts/data_model.g5.md#git-owns-repository-content) and [one eligible owner uses live compute](../concepts/security.g5.md#one-eligible-owner-uses-live-compute) govern where an answer lives and who writes it.
+
+Governing contracts: [reference a question from its set file](documents.g5.md#reference-a-question-from-its-set-file), [write set files only through the question service](documents.g5.md#write-set-files-only-through-the-question-service), [answering is an explicit save of one file](documents.g5.md#answering-is-an-explicit-save-of-one-file).
+
@@ -366,0 +377,62 @@ Live provider questions follow [native conversation requests](conversations.g5.m
+### Reference a question from its set file
+
+[D · g5.documents.question.referenced]
+
+A Project Doc or explainer may hold a question by reference: a `g5-question-ref` fence in Markdown, or a `data-g5-question` placeholder in HTML, names the exact repository-relative path of a question set file and an intrinsic question id. New sets are written at `docs/<authoring-workspace-id>/questions/<set-id>.g5q.json` in the docs repository, one file per set, any number of documents per set; a reference is resolved in the current workspace's docs member at the containing view's selected source, never derived from the current workspace id, so inherited documents keep resolving after a fork and a fork's answers evolve independently through ordinary Git. Traversal, symlinks and non-question paths are refused. SpecEditor and managed explainer views render the reference as the same interactive card as an embedded question; [FILES Markdown keeps its ordinary GitHub rendering](#render-files-markdown-as-github-does). An unresolvable or invalid reference renders a plain notice and never blocks the document. The embedded `g5-question` fence of [structured question documents](#retain-structured-question-documents) stays valid permanently and is never migrated; every reader handles both shapes. The set file is a distinct Lean schema (`schema: 3`, unknown fields preserved, 1 MiB cap) carrying the question, option, answer and provenance shapes, intrinsic ids and revision token of [G5 PR #2532](https://github.com/G5Labs/G5/pull/2532)'s contract, without its signature ([one writer](#write-set-files-only-through-the-question-service)).
+
+Why: one answer shown identically on every surface needs [one home](../concepts/system_invariants.g5.md#each-fact-has-one-source-of-truth); the document and the explainer become views of that home instead of copies, and [Git stays the truth](../concepts/data_model.g5.md#git-owns-repository-content). A path derived from the current workspace breaks every reference a fork inherits. A database-owned answer store that leaves the checkout is ruled out.
+
+### Write set files only through the question service
+
+[D · g5.documents.question.writer]
+
+One service writes every set file: answer, clear, create or append. It validates the file's schema and the answer transition and revision rule, takes provenance from the authenticated caller and never from the body, and renders canonical bytes. The owner's file routes validate a set file's schema on write and refuse an unparseable one, so the HTTP file API cannot land a broken set. A replayed command is honoured only when its idempotency key, actor, operation and payload digest all match the recorded receipt; the same key with a different request conflicts. Agents never edit a set file.
+
+Set files carry no signature, digest or tamper classification (human decision, 2026-10-09). Nothing in Lean stops a native edit, commit or push of a set file from the VM: there are no worker file tools, no draft-commit choke point, and no applied docs write policy. A recorded answer is therefore a well-formed entry with recorded attribution, not proof that a person approved it; an unparseable set renders a plain notice and its questions are not actionable until a person repairs the file as ordinary source. Rejected: the workspace-derived HMAC, classifier, repair endpoint and bulk re-sign of G5 PR #2532 (a publicly derivable key proves nothing a checkout holder cannot recompute, and it fails for every inherited set after a fork); an unkeyed digest (the same accidents are caught by schema validation at read time, and the classifier and repair flow it needs are mechanism without a guarantee); filesystem hardening of `questions/**` (an owner-run agent can change permissions); a commit gate (agents commit natively).
+
+### Answering is an explicit save of one file
+### Answering is an explicit save of one file
+
+[D · g5.documents.question.answer-checkpoint]
+
+Answering or clearing a referenced question is the owner's explicit Save of the set file. The service reads the live file over the committed cut, applies the transition, writes the whole file to the attached VM under the [file-version precondition](#require-a-file-version-before-mutation) and the [VM instance invariant](../concepts/system_invariants.g5.md#old-vm-work-cannot-change-its-replacement) — a Save answer therefore also saves any live edits already in that file, while other document buffers stay unsaved — then invokes the [selected-file checkpoint](../software_architecture/durability.g5.md#commit-and-push-only-selected-file-changes) for that one file. The card reports **saved** only after publication; a file write that landed but did not publish is shown as applied and not published, and publication is retried only while that exact applied version is current, without a second transition. Agent commands that create, append or propose change the live file and do not checkpoint: [no implicit write commits](../software_architecture/durability.g5.md#keep-implicit-file-writes-separate-from-git-durability), and the agent's own commit and push, or the lifecycle flush, owns their durability. There is no draft layer: the committed set file is what readers, agents and the next session see. Answering needs the owner's attached VM like every other Save; a stopped workspace shows the cards read-only with the ordinary Start affordance and [does not start compute](../concepts/system_invariants.g5.md#compute-requires-explicit-start-consent).
+
+Why: G5 PR #2532 kept answers out of commits because one commit per answer would fragment a document's history. In Lean the set file is its own file, checkpoints are reserved for explicit owner saves, and [non-owners read durable state](../concepts/security.g5.md#serve-nonowners-from-durable-state): an uncommitted human answer would be invisible to every reader and every agent reading the committed branch. Rejected: a live-only human answer that waits for the next document Save (invisible to readers, lost on unplanned VM loss); a database answer store (breaks Git authority); auto-committing agent proposals (an agent call is not an owner Save).
+
+### A decision is a person's answer
+
+[D · g5.documents.question.decision]
+
+A referenced question is decided only when its status is `answered` with `user` provenance, which the card sets because the Answer press is the authenticated owner's own action. An agent's answer lands as `proposed`: the slot is filled, the card shows a proposal, agents treat it as unapproved, and the owner confirms it by answering — replacing the provenance — or clears it. Agents never clear or reopen an answered question; a superseded question gets a new id and the answered one stays as the record. Authoring edits follow the revision rule: wording edits keep the answer as a new revision, and every surface shows that it was answered against an earlier revision; adding, removing or relabelling an option, or changing multi-select, needs a new question. A decision grants no permission, settles no [approval](governance.g5.md) and answers no [native provider request](conversations.g5.md#bind-native-questions-to-one-live-attempt); an agent consumes it on its next ordinary read.
+
+Relay-for-a-person (an agent answer carrying a verified `onBehalfOf` chat intent) is not carried over: Lean agents act under the owner's token, so a relayed answer would carry no identity beyond the owner's, and the person's decision stays the owner pressing Answer. It can be added later as its own block once a trusted human-consent binding exists; a session id or an agent-written chat-intent file is not one.
+
+### Readers see committed answers, read-only
+
+[D · g5.documents.question.readers]
+
+Non-owners see referenced questions from the committed docs branch as read-only cards, like every other document; reading never starts compute. There are no answerable share links for people outside the workspace: [one owner writes](../concepts/security.g5.md#one-eligible-owner-uses-live-compute), and a non-member answer would need a second write path into the owner's VM. A person asked to decide replies in the [thread](#let-readers-reply-in-threads), forks, or receives ownership.
+
+Why: G5 PR #2532's share-link table, token page and migration serve a multi-writer model that Lean replaced with ownership transfer (H072); carrying them would add a table and an anonymous-recipient path for one feature.
+
+### Agents read and propose through the workspace CLI
+
+[D · g5.documents.question.agent-access]
+
+A workspace agent reads, creates and proposes with `g5d question list|create|answer` on the [workspace CLI](../software_architecture/runtime.g5.md#make-the-workspace-cli-available-to-people-and-agents). The command calls incarnation-authenticated internal routes of the same service under [live-mutation admission](../concepts/security.g5.md#admit-actions-against-current-authority) and the direct-agent-write mutation policy; the route forces `agent` provenance whatever the body says, so an answer lands as a [proposal](#a-decision-is-a-persons-answer) and the write [stays uncommitted](#answering-is-an-explicit-save-of-one-file) until the agent pushes. A retried command replays through an idempotency key derived from the set, question and expected revision, and a moved revision returns the current entry so the agent re-reads. The command fails explicitly outside a workspace VM. The packaged harness instructions teach the referenced form beside the embedded one, name the command, and say never to edit a set file. An agent acts only on `decision: true` entries.
+
+Why: Lean has no provided-agent MCP tool surface and no task-control token ([the harness ships two review tools only](ontology_and_harness.g5.md#provide-the-harness-in-every-workspace)); the CLI on PATH with the [incarnation token](../concepts/security.g5.md#expose-the-incarnation-token-under-the-mvp-boundary) is the existing agent entry path, and it serves Claude Code and Codex alike. Rejected: a file outbox with an ingestion loop (new state and a maintenance loop where a direct call exists); extending the Codex-only review MCP server.
+
+### Confirm explainer answers in the host
+
+[D · g5.documents.question.html]
+
+An HTML explainer's placeholders display question data the host supplies; the sandboxed frame never holds credentials or calls the API. The host validates each frame message against that frame, its document generation and the references the document is known to hold, and a human confirms a save in trusted host controls, as the existing [HTML answer bridge](#retain-structured-question-documents) already requires: a frame message alone never mints a human answer. An answer update refreshes the cards in place without regenerating the frame and keeps unsent input.
+
+### Count referenced questions once
+
+[D · g5.documents.question.metrics]
+
+Open-question metrics and the home attention list add set entries that are `open` or `proposed` as a third source family beside embedded fences and escalation documents, counted once per entry however many documents reference it, with a referencing document as the navigation target. The scan covers the current workspace's questions directory plus the inherited sets its documents reference, not every sibling workspace directory; an invalid set is listed with a flag and marks the summary incomplete. Open cards refresh through the existing [live file-change](#expose-concurrent-disk-changes) notification for the attached owner and the durable-cut invalidation for readers, filtered by repository and path; no separate question event exists.
+
+<a id="g5.story.trace-decisions-to-implementation"></a>
@@ -646,0 +719,6 @@ Partial incorporation does not terminalize a future contribution session.
+### g5.story.answer-a-question-anywhere
+
+- agent
+- Agent story for the [question store design](../../chat_intent/question-store/request.md), carrying the intent of G5 PR #2532 D10 into Lean.
+- Gap · not implemented. Embedded questions are answered through the whole-document Save (`SpecEditor.tsx`, `questionAnswerWrite.ts`); HTML answers apply live-only; agents have no question operation.
+
@@ -886,0 +965,49 @@ Partial incorporation does not terminalize a future contribution session.
+### g5.documents.question.referenced
+
+- agent
+- Agent decision in the [question store design](../../chat_intent/question-store/request.md); the shapes are G5 PR #2532's shared module (`g5-contracts/src/types/question_set.rs`) restated for Lean with human decisions Q4 and Q8. Explicit set paths and fork inheritance, and the FILES scope, were integrated from the [parallel Lean design](../../chat_intent/question-store/request.md#parallel-design).
+- Gap · not implemented. `frontend/src/lib/questionBlock.ts` parses only the embedded schema-1 fence; `SpecEditor.tsx` renders no reference fence; `ExplainerOutputView.tsx` mounts no card injection; `workspace_fork_service.rs` copies `docs_repo_id` and rewrites no docs paths, which the path rule relies on.
+
+### g5.documents.question.writer
+
+- agent
+- human-verified
+- The human dropped integrity checks entirely in the [question store design](../../chat_intent/question-store/request.md#human-decisions) (2026-10-09), after the [parallel design](../../chat_intent/question-store/request.md#parallel-design) showed G5 PR #2532's workspace-derived key cannot authenticate and breaks fork inheritance; the no-prevention posture is a Lean finding (no file tools; `/work` mounted read-write in `backend/g5d-host/src/workspace_vm.rs`; agents push natively). The one-writer rule, receipt binding and schema gate are agent decisions.
+- Gap · not implemented. No `QuestionSetService`; `routes/workspaces.rs::put_workspace_file` and `routes/docs.rs::update_project_doc` land any bytes under `questions/**`.
+
+### g5.documents.question.answer-checkpoint
+
+- human-verified
+- The human confirmed that a human answer checkpoints in the [question store design](../../chat_intent/question-store/request.md#human-decisions) (2026-10-09), replacing G5 PR #2532's draft-write rule with Lean's [explicit checkpoint](#publish-an-explicit-editor-save); the agent-writes-stay-uncommitted and applied-versus-published rules are agent decisions integrated from the [parallel design](../../chat_intent/question-store/request.md#parallel-design).
+- Gap · not implemented. The checkpoint composition exists only as route code (`routes/workspaces.rs::write_workspace_file_and_maybe_push`, `commit_and_push_files` over `services/workspace/workspace_runtime.rs` Changes commit and push) and must move under the Workspace owner before the service can call it.
+
+### g5.documents.question.decision
+
+- agent
+- Agent decision in the [question store design](../../chat_intent/question-store/request.md); restates G5 PR #2532 decisions Q3 and Q5 without the relay-for-a-person half, whose omission is a Lean-specific agent choice shared with the [parallel design](../../chat_intent/question-store/request.md#parallel-design).
+- Gap · not implemented. The predicate, shape gate and revision gate are pure functions of the carried contract module.
+
+### g5.documents.question.readers
+
+- agent
+- Agent decision in the [question store design](../../chat_intent/question-store/request.md), applying [H072](../../uplift/g5-lean/candidates/human-decisions.md#h072--one-owner-writes-transfer-replaces-multi-writer-sharing); drops G5 PR #2532 D4 share links. Awaiting human confirmation.
+- Gap · not implemented; committed read-only rendering follows the existing reader document path once the reference fence renders.
+
+### g5.documents.question.agent-access
+
+- agent
+- Agent decision in the [question store design](../../chat_intent/question-store/request.md), replacing G5 PR #2532's `read_questions`/`answer_question` MCP tools, task-control token and outbox ingester with the Lean CLI entry path (PR #248 removed the tool surface).
+- Gap · not implemented. `backend/g5d/src/cli/` has no `question` command; `auth_middleware.rs` lists no question route in the incarnation allowlist; `.agents/AGENTS.md` and the frontend chat prompts (`generateChatPrompt.ts`, `explainerChatPrompt.ts`) teach only the embedded fence.
+
+### g5.documents.question.html
+
+- agent
+- Agent decision in the [question store design](../../chat_intent/question-store/request.md), integrated from the [parallel design](../../chat_intent/question-store/request.md#parallel-design); keeps the host confirmation G5 PR #2532's direct apply removed.
+- Gap · not implemented. `frontend/src/components/shared/htmlDocAnswerBridge.tsx` already validates frame messages and confirms through `HtmlDocAnswerBar`; placeholders and in-place data updates are absent.
+
+### g5.documents.question.metrics
+
+- agent
+- Agent decision in the [question store design](../../chat_intent/question-store/request.md), restating G5 PR #2532 D26/D52 over Lean's retained metrics; the reuse of existing notifications and the inherited-set scan were integrated from the [parallel design](../../chat_intent/question-store/request.md#parallel-design).
+- Gap · not implemented. `services/workspace/workspace_service.rs::capture_open_question_metrics` and `services/workspace/doc_service.rs::open_project_doc_question_sources` count embedded fences only.
+
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index af44e61d..cdc8065b 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -24,0 +25 @@ and [governance](../features/governance.g5.md); post-MVP governance retains its
+| QuestionSetService | `list_sets`, `get_set`, `answer`, `clear`, `create_or_append`; public `/workspaces/:id/questions` and incarnation `/internal/workspaces/:id/questions` adapters | [Question sets](#question-set-operations) |
@@ -116,0 +118,14 @@ parsing and authorized repository views.
+### Question set operations
+
+[D · g5.architecture.question_owner]
+
+`QuestionSetService` owns every write of a question set file — answer, clear, create or
+append, and the schema check the owner file routes call before landing a set file —
+composed from the shared content reader, the
+`WorkspaceRuntime` file write and the selected-file checkpoint, which moves from route
+code under the Workspace owner so the editor Save and this service share one operation.
+It holds no table; public and incarnation adapters translate
+requests and supply provenance only.
+
+The [question rules](../features/documents.g5.md#write-set-files-only-through-the-question-service) own the behavior; [complete operations](boundaries.g5.md#keep-complete-operations-with-their-state-owner) forbid a second writer.
+
@@ -398,0 +414,6 @@ of a check disappears, and accept harmless internal refactors. Guards follow
+### g5.architecture.question_owner
+
+- agent
+- Agent decision in the [question store design](../../chat_intent/question-store/request.md), the Lean owner of G5 PR #2532's `question_set_service` without its draft and keyring wiring.
+- Gap · not implemented; the checkpoint composition it reuses is still route-local (`backend/g5-api-routes/src/routes/workspaces.rs::write_workspace_file_and_maybe_push`).
+
```

</details>

## Amendment: votes (2026-10-09)

**Request:** the week's design explainers (workbench header styles, one mounted page versus many, the UX v2 plan-versus-prompts contradictions) become one HTML page in the platform where *everyone* votes on the options ([chat intent](chat_intent/question-votes/request.md)). The store above lets only the attached owner write, so the platform gains a vote beside the answer.

**Ontology changes:** added `g5.documents.question.votes` in [documents](ontology/features/documents.g5.md#let-readers-vote-on-referenced-questions); `.readers` names voting as a reader's path, `.html` validates a vote like an answer and applies it with the one press (amended 2026-10-11, below), the story says readers vote and the owner sees the tally. Meaning changed: none; a decision is still the owner's answer.

| Commitment | Obligation → owner and operation | Allowed case · counterexample | Conclusion |
| --- | --- | --- | --- |
| [g5.documents.question.votes](ontology/features/documents.g5.md#let-readers-vote-on-referenced-questions) | `QuestionVoteService` validates a vote against the resolved set entry, upserts one row per voter, withdraws, tallies, and publishes `question.vote`; routes under the read scope; cards show tallies and a Vote control for every reader. | A reader votes on a stopped workspace with no VM call · a vote with an unlisted option, a vote on an invalid set, or a tally that flips the status to answered. | Added. |
| [g5.documents.comments.replies](ontology/features/documents.g5.md#let-readers-reply-in-threads), [g5.security.read](ontology/concepts/security.g5.md#read-permission-is-explicit) | The same grant: anyone who can read the workspace writes a platform record that changes no repository content. | An invited member votes · an org member without visibility. | Preserve; precedent for the grant. |
| [g5.documents.question.decision](ontology/features/documents.g5.md#a-decision-is-a-persons-answer), [.readers](ontology/features/documents.g5.md#readers-see-committed-answers-read-only), [.answer-checkpoint](ontology/features/documents.g5.md#answering-is-an-explicit-save-of-one-file) | Votes never touch the set file; the owner's Answer is unchanged. | Owner answers against a 3–1 tally · a vote that writes the set file or auto-answers. | Preserve. |
| [g5.documents.question.html](ontology/features/documents.g5.md#confirm-explainer-answers-in-the-host) | The frame proposes `propose-question-vote`; the host validates it like an answer and applies it at once; the bar shows only a refused vote, with its reason and a retry. | One press records the vote · an unvalidated frame message (other window, stale generation, unlisted reference, moved revision, unoffered option) that votes. | Preserve, one clause added. |
| [g5.design.durability](ontology/concepts/system_design_principles.g5.md#declare-durability-before-choosing-storage), [g5.data.git-authority](ontology/concepts/data_model.g5.md#git-owns-repository-content) | Votes are platform state, declared as such; Git keeps the question and the answer. | Tally recomputed from rows · a vote cached in a card past its row. | Preserve; justifies the table. |

**Design.** One new owner, `QuestionVoteService` (`services/workspace/question_vote_service.rs`), over one table `question_votes (org_id, workspace_id, set_path, question_id, user_id, selected_option_ids, text, question_revision, created_at, updated_at)` with the tenant RLS shape of the other workspace tables. A vote command resolves the set through `QuestionSetService::get_set` as the voter (live for the attached owner, committed for everyone else), refuses an invalid set, a missing question, an unlisted option, an empty vote or a moved revision (the cards send the revision they show; the refusal carries the current entry like an answer's), then upserts the voter's row and publishes `question.vote {workspaceId, path, questionId}` on the workspace channel after commit. A tally read returns the set's vote rows with voter display names; the client aggregates per option. Routes under `/workspaces/:id/questions`: `GET /votes?path=`, `POST /vote`, `POST /unvote`, all admitted by `scoped_db_for_workspace` with the read scope (the same contract exception as `comments.rs::add_reply`). The client adds the tally and a Vote control to `ReferencedQuestionCard` (every reader; the owner also keeps Answer) and to the in-frame explainer renderer, which proposes `propose-question-vote`; `questionCardBridge` validates both message kinds with one validator; the host bar confirms "Answer", while a validated vote is applied at once and the bar shows only a refused one. Reader cards therefore become interactive on a stopped workspace too, for the vote only.

Rejected: votes in the set file (readers have no write path to the owner's VM, and the owner's checkpoint rule would make every reader vote an owner Save); votes as structured thread replies (no tally, and threads anchor on documents, not questions); an agent-readable tally in `g5d question list` (deferred: agents act on decisions, not votes).

**Acceptance outcomes.** A reader of a running or stopped workspace votes from the explainer card with one press and from the Project Doc card, with no VM call; the tally and voter names show on every card for every reader and for the owner; a changed vote replaces the row and a withdrawal removes it; an unlisted option, an invalid set, a missing question, an empty vote and a moved revision are refused; a non-member and an uninvited member of an invite-only workspace are refused; a fork starts with no votes; a tally that cannot be read shows as unavailable, never as zero; the owner's Answer and `decision` flag are unchanged by any tally.

**The vote page.** `docs/design-votes/2026-10-09/` holds the explainer (`design-votes-2026-10-09.html`, placeholders over `__WORKSPACE_ID__`; 15 questions from the three explainers and the 2026-10-09 standup, each with a mock, a wireframe or a real PR #406 screenshot), the question set body (`questions.json`, ids fixed so the page can reference them) and `publish.sh`, which creates the set through `POST /questions` (an owner Save that publishes the set file) and commits the page as `explainers/design-votes-2026-10-09.html` through the workspace files route, then prints the document URL. The page step sends the current page's hash as `expectedHash` (null for a new page), so a re-run replaces the page.

**Amendment (2026-10-11): one-press votes.** The human asked to "avoid confirmations in the host bar so you need to click only once, even if there are platform changes" ([follow-up](chat_intent/question-votes/request.md)). The host still validates every `propose-question-vote` message with the same validator, then applies it with no bar; a key with a vote in flight drops a repeated press. The card status reads "Saving your vote…" / "Vote recorded." (or the withdrawal forms); a refused vote's card names the reason ("Vote not saved: …" or "The question changed; vote again.") and its Vote press retries, and the bar shows the reason with the existing Vote retry and Dismiss unless it holds an unfinished answer, which stays, or the question changed (the card shows the current entry; a retry of the old revision would be refused again). An empty vote stays in the frame with a prompt to pick an option. A vote that settles after its frame reloaded reports on the reloaded card only; it never opens or closes that load's bar. Answers keep the bar confirmation. Accepted risk: a script in the page can vote for its viewer; the vote is attributed, shown with the viewer's name on every card, withdrawable, and never a decision, while an answer can be one. Deferred: one decision per page with previous and next buttons on the vote page (part a of the same request).

