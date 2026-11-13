# Issue 282: verified native provider resume

## Ontology changes and governance

**Ontology changes:** Compared with `origin/main` at `8a3ec325`, this design modifies `g5.conversation.bundle.publication` and `g5.conversation.recovery`, and adds `g5.conversation.recovery.notice` in [conversations](ontology/features/conversations.g5.md). Implementation review also updates the scoped metadata for `g5.conversation.bundle.harvest`, `g5.architecture.provider_adapter`, `g5.architecture.session_store`, `g5.data.platform-authority`, `g5.data.identity.generations` and `g5.design.durability` in their owning files. The actual ontology diff is:

```diff
diff --git a/.g5/ontology/concepts/data_model.g5.md b/.g5/ontology/concepts/data_model.g5.md
index aaabb95f..ca93f2ca 100644
--- a/.g5/ontology/concepts/data_model.g5.md
+++ b/.g5/ontology/concepts/data_model.g5.md
@@ -104,7 +104,7 @@ After scope resolution, apply [provider authority](../features/provider_accounts

 - human-verified
 - From [H028/H056/H098](../../uplift/g5-lean/candidates/human-decisions.md).
-- ~75% · API services and PG tables; incomplete session-bundle publication and assistant-outcome projection leave inconsistent durable state.
+- Historical ~75% whole-platform baseline retained · API services and PG tables; session-bundle publication now has a committed complete-set marker in Postgres with focused tenant-RLS and restricted-migrator checks. Assistant-outcome projection remains incomplete; whole-platform authority was not remeasured.

 ### g5.data.platform-authority.vm-access

@@ -128,7 +128,7 @@ After scope resolution, apply [provider authority](../features/provider_accounts

 - agent
 - From [H103](../../uplift/g5-lean/candidates/human-decisions.md#h103--execution-existence-input-delivery-and-durable-state-have-different-owners).
-- ~60% · session_inputs and session-file generations; closed-generation acceptance is rejected and failed harvest does not fully prevent stale restore.
+- Historical ~60% whole-generation baseline retained · `session_inputs` and session-file bundles keep delivery and harvest facts separate. The scoped bundle path now fences later uploads and refuses stale restore after failed harvest; focused worker and PostgreSQL checks pass. Other generation consumers remain unassessed.

 ### g5.data.projections

diff --git a/.g5/ontology/concepts/system_design_principles.g5.md b/.g5/ontology/concepts/system_design_principles.g5.md
index f1731d47..1690dc43 100644
--- a/.g5/ontology/concepts/system_design_principles.g5.md
+++ b/.g5/ontology/concepts/system_design_principles.g5.md
@@ -211,7 +211,7 @@ Provider fakes may replay recorded protocol but must not replace the production

 - agent
 - Applies the supplied [design guidance](../../uplift/g5-lean/evidence/design-guidance.md) to [G5 module contracts](../software_architecture/boundaries.g5.md); [scope clarification](../../chat_intent/g5-lean/ontology-and-harness.md#minimal-design-and-bounded-review-work) separates asynchronous work from a required durability guarantee.
-- Historical ~75% estimate retained, not reassessed · durable Git/platform records and transient VM/files/streams; failed session harvest does not fully prevent stale bundle restore.
+- Historical ~75% whole-system estimate retained, not reassessed · durable Git/platform records and transient VM/files/streams; the scoped session-bundle path now refuses stale restore after failed harvest. Other durability paths remain unassessed.

 ### g5.architecture.recovery

diff --git a/.g5/ontology/features/conversations.g5.md b/.g5/ontology/features/conversations.g5.md
index cb5dc204..d572fc4b 100644
--- a/.g5/ontology/features/conversations.g5.md
+++ b/.g5/ontology/features/conversations.g5.md
@@ -343,15 +343,37 @@ Harvest changed session bytes at turn or Stop under incarnation/generation fence

 A failed or partial latest harvest makes older provider bytes unrestorable as current, and only complete generations may publish.

+The session store records the expected file set and its integrity in one completion
+record after all bytes arrive. Beginning a provider turn invalidates the prior
+generation for later turns until the new harvest publishes. A missing, incomplete or
+failed generation cannot be made current by files left on a VM.
+Uploads from the previous worker contract remain available to its legacy manifest,
+but cannot publish a generation or overwrite the current published native session.
+If a fresh session cannot establish a completion fence, its turn may proceed with
+a persistence warning and no published files. A resumed session must not advance
+without first fencing its earlier state.
+
 <a id="g5.story.conversation-recovery"></a>
 ### Recover a conversation from known delivered history

 [D · g5.conversation.recovery]

-When native restore is absent or refused, start a fresh native session with bounded deterministic positively delivered history and current input, disclosing truncation or loss.
+Resume the same provider session only when the current complete generation is verified
+and its full file set is restored for that session. Otherwise start a fresh native
+session with bounded deterministic positively delivered history and current input,
+disclosing truncation or loss. If neither path can establish usable context, fail
+visibly before delivering the input.

 Recovery applies to both native providers, including [conversation carry](conversations.g5.md#carry-selected-conversation-history-into-a-fresh-session).

+### Report the actual context path
+
+[D · g5.conversation.recovery.notice]
+
+The conversation reports whether a turn resumed natively, rebuilt from delivered
+history, or failed without context. A complete reconstruction with no lost content is
+informational; omissions, truncation or unavailable attachment bytes remain visible.
+
 ### Exclude undelivered inputs from recovery

 [D · g5.conversation.recovery.exclude]
@@ -663,19 +685,25 @@ Automatic first-request or best-effort summary naming uses the existing turn’s

 - agent
 - Preserves H028’s human decision for private Postgres storage; generation/publication design comes from [C129–C130](../../uplift/g5-lean/candidates/contracts.md), [H028](../../uplift/g5-lean/candidates/human-decisions.md); [A5](../../uplift/g5-lean/investigations/agents.md#a5--bundle-generation-and-failed-harvest-state-do-not-fully-govern-future-turns).
-- ~75% · session harvest/upload; complete generation and stop propagation remain incomplete.
+- ~85% of inspected turn harvest · `g5d-worker::session_files::harvest_complete` scans and uploads changed files under a generation fence; the API commits the complete set. Stop-path harvest and live-provider file layouts remain unverified. Focused worker and PostgreSQL publication checks pass.

 ### g5.conversation.bundle.publication

 - agent
-- Preserves H028’s human decision for private Postgres storage; generation/publication design comes from [C129–C130](../../uplift/g5-lean/candidates/contracts.md), [H028](../../uplift/g5-lean/candidates/human-decisions.md); [A5](../../uplift/g5-lean/investigations/agents.md#a5--bundle-generation-and-failed-harvest-state-do-not-fully-govern-future-turns).
-- ~35% · session-file generation/restore selection; complete publication and failed-harvest propagation are missing.
+- Preserves H028’s human decision for private Postgres storage; generation/publication design comes from [C129–C130](../../uplift/g5-lean/candidates/contracts.md), [H028](../../uplift/g5-lean/candidates/human-decisions.md), [A5](../../uplift/g5-lean/investigations/agents.md#a5--bundle-generation-and-failed-harvest-state-do-not-fully-govern-future-turns) and [issue 282](https://github.com/G5Labs/G5-Lean/issues/282).
+- ~85% of inspected turn publication · migration 057, `ChatSessionFileService` and `g5d-worker::session_files` fence uploads, validate every named file and restore only a committed set, replacing stale local bytes. An N-1 worker may still upload without publication and cannot replace the committed native session; a fresh turn may degrade without publication when its fence fails. Focused worker, tenant-RLS, restricted-migrator and dispatcher selectors pass; live-provider and crash-injection qualification remain unexecuted.

 ### g5.conversation.recovery

 - agent
-- [C130–C131](../../uplift/g5-lean/candidates/contracts.md), human [H028](../../uplift/g5-lean/candidates/human-decisions.md). Legacy Claude-only carry was copied into the new path; provider-neutral v9 governs ([QA03](../../uplift/g5-lean/refinement/question-audit.md#qa03--codex-carry-is-a-stale-implementation-restriction-not-a-new-scope-choice)); outcome lineage: [v9](../../../docs/lean-backend-design-v9.md) §§8.5–8.6.
-- ~70% of inspected paths · agent_dispatch.rs freezes bounded delivered transcript history and preserves a complete continuation objective or fails visibly; g5d-agent-turn starts fresh for both providers and reports reconstruction only after input acceptance. Native restore is disabled because complete bundle publication is unproven. Deterministic real-provider recall and UI rendering remain unexecuted.
+- [C130–C131](../../uplift/g5-lean/candidates/contracts.md), human [H028](../../uplift/g5-lean/candidates/human-decisions.md), [issue 282](https://github.com/G5Labs/G5-Lean/issues/282). Legacy Claude-only carry was copied into the new path; provider-neutral v9 governs ([QA03](../../uplift/g5-lean/refinement/question-audit.md#qa03--codex-carry-is-a-stale-implementation-restriction-not-a-new-scope-choice)); outcome lineage: [v9](../../../docs/lean-backend-design-v9.md) §§8.5–8.6.
+- ~85% of inspected paths · `agent_dispatch.rs` freezes bounded delivered history and the separate current input; `g5d-agent-turn` restores a complete generation for native resume or rebuilds from that history. Worker, launch and API-core regressions pass. Deterministic live-provider recall and CLI refusal after input handoff remain unverified.
+
+### g5.conversation.recovery.notice
+
+- human-verified
+- [Issue 282](https://github.com/G5Labs/G5-Lean/issues/282) requests truthful context-path notices and suggests informational severity for a complete rebuild.
+- Implemented on the inspected runtime path · `g5d-agent-turn::runtime` emits an informational native-resume notice, an informational complete rebuild or a warning when reconstruction loses content, and a visible context-unavailable error. Live UI projection remains unexecuted.

 ### g5.conversation.recovery.exclude

diff --git a/.g5/ontology/software_architecture/agents.g5.md b/.g5/ontology/software_architecture/agents.g5.md
index 221fadee..289027f3 100644
--- a/.g5/ontology/software_architecture/agents.g5.md
+++ b/.g5/ontology/software_architecture/agents.g5.md
@@ -174,7 +174,7 @@ workspace owner using ProviderAccountService's eligibility decision.

 - agent
 - [Agent API §5 and §9–§11](../../../docs/agent-api-surface-v9.md).
-- ~75% · `backend/g5d-agent-turn/src/{types, policy, protocol, runtime, harness}.rs`; fresh-session recovery notice and failure paths are implemented, with prompt transport unit coverage for both providers. Interactive Codex frame rejection, native-response replay and private/display payload separation remain incomplete; live-provider recall is unexecuted.
+- Historical ~75% whole-adapter baseline retained · `backend/g5d-agent-turn/src/{types, policy, protocol, runtime, harness}.rs`; verified native resume selection and current-input-only transport now have focused checks for both providers. Interactive Codex frame rejection, native-response replay and private/display payload separation remain incomplete; live-provider recall is unexecuted.

 ### g5.architecture.provider_adapter.claude-background

@@ -198,7 +198,7 @@ workspace owner using ProviderAccountService's eligibility decision.

 - agent
 - [Design v9 §8.5](../../../docs/lean-backend-design-v9.md#85-provider-session-files-in-postgres).
-- ~65% · `backend/g5-api-core/src/services/chat_session_file_service.rs`, `g5d-agent-turn/src/runtime.rs`; file hash/byte and incarnation checks exist. Agent turns do not restore native files until complete bundle-generation publication is proven; failed-harvest propagation and publication remain incomplete.
+- ~85% of inspected session-store boundary · `chat_session_file_service.rs`, `g5d-worker::session_files` and `g5d-agent-turn::runtime` publish a task-fenced complete file set and restore it for native resume. Auth middleware admits the three new workspace-incarnation bundle endpoints. Focused worker, auth, tenant-RLS and restricted-migrator checks pass. Live-provider recall and crash-injection qualification remain unexecuted.

 ### g5.architecture.account_owner

```

The follow-up to the `b53d90ff` review changes the same ontology file. Its
incremental governance diff is recorded here; the historical diff above
remains the original implementation handoff:

```diff
@@ g5.conversation.bundle.harvest
-Harvest changed session bytes at turn or Stop under incarnation/generation fences, with a 128 MiB per-file cap.
+Harvest changed session bytes at turn end or Stop under incarnation/generation fences, with a 128 MiB per-file cap. If the bounded Stop-time save cannot finish, the same VM incarnation may complete that pending generation before its next turn or during its stop flush, after the advancing attempt has ended. A bounded flush retry of the native save does not block the workspace Git receipt when the session store remains unavailable.
@@ g5.conversation.bundle.publication
+A generation left pending by a stopped or limited turn may be completed later
+only by its fencing VM incarnation, for the provider session reported by its
+ended attempt, before any later turn begins. Other incarnations leave it
+pending.
@@ g5.conversation.recovery
-If published-state lookup, file download or safe local replacement fails,
-fail before delivering input and preserve the published generation for retry;
-reconstruction applies to a known absent or unusable native set.
+An unreachable store or failed local file replacement fails the turn before
+input delivery and preserves the published generation for retry. A definitive
+missing or corrupt stored set may instead reconstruct from delivered history.
+A resumed turn whose provider never reports its session publishes nothing.
@@ g5.conversation.recovery.notice
-The conversation reports whether a turn resumed natively, rebuilt from delivered
-history, or failed without context.
+The conversation shows reconstruction from delivered history or failure without
+context; a verified native resume adds no conversation notice, while its
+provider-session event records the path. A failed session save shows one
+reason-coded warning, with internal errors confined to worker logs.
```

The follow-up to the `24c85189` review ([PR comment](https://github.com/G5Labs/G5-Lean/pull/286#issuecomment-5967082245), [follow-up intent](chat_intent/issue-282/request.md#follow-up-after-the-pr-286-review-at-24c85189)) amends the same four blocks in [conversations](ontology/features/conversations.g5.md). Their metadata now names the gaps the code must close. The incremental body diff is:

```diff
@@ g5.conversation.bundle.harvest
+The provider adapter defines each resumable set from its pinned CLI: Claude's
+transcript and every regular file in that session's directory, and Codex's session
+rollout. Only a provider session reported by the attempt is harvested.
+Provider session files are append-only in normal use, so a harvest transfers only the
+bytes appended to a stored file whose stored bytes match the local file's prefix;
+other changed files upload whole. This keeps save time proportional to new output,
+which the fixed Stop budgets require.
@@ g5.conversation.bundle.publication
-A missing, incomplete or failed generation cannot be made current by files left on a VM.
-A generation left pending by a stopped or limited turn may be completed later
-only by its fencing VM incarnation, for the provider session reported by its
-ended attempt, before any later turn begins. Other incarnations leave it pending.
+Files left on a VM cannot make a missing, incomplete or failed generation current,
+with one exception: after its attempt has ended, the fencing VM incarnation may
+complete a pending generation from its own files, for the provider session that
+attempt reported, before any later turn begins. Other incarnations leave it pending.
+If the fenced attempt ends with explicit evidence that the provider process never
+started and its input is still reserved, the store restores the publication that
+`begin` fenced. Accepted, ambiguous, provider-started or recovered attempts keep
+the fence: a provider that refused a restored session must not be offered the same
+set again.
@@ g5.conversation.recovery
+Local session bytes are reused without download only when every file matches the
+committed set exactly; other files in that session's paths are removed. Claude
+resumes only in the working directory its generation was harvested from, because
+its session paths and transcript references are keyed by that directory; a changed
+workspace target rebuilds from delivered history.
@@ g5.conversation.recovery.notice
+Because the same VM may still complete that save, the
+warning says the next turn rebuilds only if the save is still incomplete then.
```

The `80d399da` review follow-up changes the same governing blocks and the
`g5.architecture.session_store` metadata. The incremental governing body diff is:

```diff
@@ g5.conversation.bundle.harvest
-before its next turn or during its stop flush
+at the start of the next turn that resumes the same provider session, or during the shutdown flush
-This keeps save time proportional to new output, which the fixed Stop budgets require.
+This keeps transfer proportional to new output; VM hashing and the stored rewrite still scale with file size, so a Stop-time save of a long session may miss its budget and rely on later completion.
@@ g5.conversation.bundle.publication
+This exception trusts the VM's copy of that session's files between the attempt's end and its completion; every writer in the workspace VM shares that trust.
@@ g5.conversation.recovery
-Local session bytes are reused without download only when every file matches the committed set exactly; other files in that session's paths are removed.
+Each local file is reused without download only when it matches its committed entry exactly; other files are downloaded and verified, and files in that session's paths outside the committed set are removed.
+A VM whose module catalog predates verified resume receives that catalog's input shape without a native-session binding and rebuilds from delivered history.
@@ g5.conversation.recovery.notice
-Because the same VM may still complete that save, the warning says the next turn rebuilds only if the save is still incomplete then.
+The warning says whether the next turn will rebuild, or may still complete the save first.
```

The later S2 inclusion further refines the harvest block:

```diff
@@ g5.conversation.bundle.harvest
-VM hashing and the stored rewrite still scale with file size
+Postgres keeps 8 MiB segments and a resumable digest so an append rewrites at most its last segment; the VM still hashes its local file
```

The `001660ba` review adds this recovery exception before its implementation:

```diff
@@ g5.conversation.recovery
+A pending generation with a local scan failure or definitive store refusal may rebuild on the next turn; an uncertain store failure still prevents input handoff for retry.
```

The footer edits distinguish in-turn Stop saves from pending completion,
record the focused retry and flush regressions, and correct the endpoint count.
The follow-up review also governs tenant-role segmented writes under
`g5.security.tenant` and one-snapshot load under
`g5.conversation.bundle.publication`.

**Relevant and governing ontology:**

| Commitment | Specific obligation, owner and counterexample |
|---|---|
| [g5.conversation.bundle.publication](ontology/features/conversations.g5.md#publish-only-complete-session-generations) | Session store publishes one complete verified current file set; a partial upload or residual VM file cannot authorize resume. |
| [g5.conversation.bundle.harvest](ontology/features/conversations.g5.md#fence-and-bound-session-harvesting), [g5.data.identity.generations](ontology/concepts/data_model.g5.md#keep-harvest-and-delivery-generations-distinct) | Harvest remains bounded and incarnation fenced; file publication cannot be inferred from input delivery. |
| [g5.conversation.recovery](ontology/features/conversations.g5.md#recover-a-conversation-from-known-delivered-history), [g5.conversation.recovery.notice](ontology/features/conversations.g5.md#report-the-actual-context-path) | Agent turn selects verified native resume, truthful bounded rebuild, or visible failure; a clean rebuild is informational. |
| [g5.conversation.recovery.exclude](ontology/features/conversations.g5.md#exclude-undelivered-inputs-from-recovery), [g5.conversation.recovery.snapshot](ontology/features/conversations.g5.md#freeze-recovery-evidence-with-the-reserved-prompt) | Reconstruction uses frozen positive delivery evidence; pending or uncertain input cannot become prior context. |
| [g5.conversation.recovery.carry](ontology/features/conversations.g5.md#carry-selected-conversation-history-into-a-fresh-session), [g5.conversation.continue](ontology/features/conversations.g5.md#continuation-says-what-is-being-continued) | Fork and carry start fresh with copied history; Continue retains task identity and does not repeat the original request. |
| [g5.architecture.session_store](ontology/software_architecture/agents.g5.md#opaque-provider-session-storage), [g5.architecture.provider_adapter](ontology/software_architecture/agents.g5.md#provider-protocol-adapter) | API store owns opaque bytes and publication; adapter owns provider-specific restore and launch. The dispatcher must not interpret native files. |
| [g5.data.platform-authority](ontology/concepts/data_model.g5.md#the-api-owns-durable-platform-records), [g5.security.tenant](ontology/concepts/system_invariants.g5.md#tenant-scope-survives-privileged-operations) | Private state remains in tenant-scoped Postgres, not Git or a VM-only authority. |
| [g5.design.durability](ontology/concepts/system_design_principles.g5.md#declare-durability-before-choosing-storage) | Postgres owns the completion marker across VM loss; local files cannot establish durable resume eligibility. |
| [g5.design.minimal-mechanism](ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism), [g5.architecture.recovery](ontology/concepts/system_design_principles.g5.md#make-failures-and-retries-explicit) | A small durable completion barrier is justified; retry and crash cases need explicit current-generation meaning. |

## Requested outcome and design

A turn uses the preceding native session only after confirming that the immediately preceding provider state was fully harvested and that the complete file set, identities and hashes match the committed manifest. The store makes a generation unavailable for future turns when the provider can advance it; the begin operation checks the active task, attempt and reserved input before fencing the previous set. It publishes only after enumeration and every upload succeeds, as one durable completion. A failed upload, interrupted harvest or old files left on the VM cannot make an older generation current. If verification or restore fails, the turn uses the frozen delivered-history reconstruction. If that cannot provide sufficient context, the turn fails visibly before input handoff. This selection applies to Claude and Codex.

Review amendment at `adce6bc`: an indeterminate published-state lookup or file download error must fail before input handoff and before `begin`, preserving the previously complete bundle for a retry of the same reservation. Reconstruction remains for a known absent or unusable native set. If a bound native session cannot resume and the turn reconstructs, its notice warns that earlier tool-only context may be lost even when delivered message text was complete. The prior broad "restore fails" fallback wording did not distinguish a transient store error from verified unusability; the former would otherwise fence a valid bundle.

The subsequent read-only Claude review exposed the same loss on safe local replacement failures. Such a failure also preserves the complete published generation and fails before input handoff; an invalid published file set still reconstructs. This extends the amendment's error boundary without changing the native-resume proof.

The dispatcher keeps its frozen history and input reservation for fallback. The session store owns complete-set publication and retrieval. The worker restores all expected bytes into provider-specific paths and launches with the prior provider session ID only after verifying the bundle; otherwise it launches a fresh session. The user-facing event records the path actually taken and distinguishes complete reconstruction from lost or truncated context. A fork or workspace conversation carry has no provider identity and therefore reconstructs in a new native session. Continue supplies its own continuation frame, with prior delivered content only as context.

## Acceptance and limits

- A deterministic provider follow-up retains an earlier tool-only fact and the same provider session ID.
- After VM replacement, only the committed complete set is restored; a partial/failed set and stale local files force reconstruction.
- A known absent or unusable native set falls back without claiming resume; lookup, download and local replacement errors preserve the bundle and fail before input handoff for retry. Missing usable history yields `context_unavailable`.
- Fork, carry and Continue retain their established semantics. Notices accurately identify resume, complete/incomplete reconstruction or failure.
- Focused boundary and affected tests precede one workspace check, strict Clippy, formatting and diff check. Claude CLI reviews only that verified candidate; required Cloud CI follows the PR.

## Risks and assumptions for coding

- Inspect the actual file service, RPCs, worker harvest and provider launch before reusing them. Existing per-file generations are not complete-set proof.
- Native provider file layouts may include multiple files and change across turns. The committed set must identify exactly what was enumerated for the session; verify provider/session/incarnation and integrity before launch.
- A crash after a provider advances but before harvest completion must not retain old resume eligibility. A fallback cannot silently redeliver queued, withdrawn or uncertain input.
- The completion record is the publication point, not any individual file upload. Forward migration preserves existing file data while treating unmarked historical generations as unverified.
- One-version-back workers retain the upload contract during rollout. Their unfenced files remain unverified and cannot replace a published native session; a later worker reconstructs from delivered history if no complete generation matches its provider session ID.
- If the bundle service is unavailable, a fresh session can still deliver the turn with a persistence warning and no published generation; a native resume cannot advance previously published state without the fence. This keeps delivery possible without making stale bytes eligible.

## Design review

The allowed case is a completed set from the last provider turn, resumed after a VM restart. The forbidden cases are residual local files, a partial newer upload and an older published generation after an advancing turn. The failure case is reconstruction from frozen positive delivery facts, or a visible refusal when that evidence cannot suffice. These outcomes follow the publication, recovery, generation and authority commitments above. The source inspection must resolve the exact store interface and provider path mapping; the design does not assume current operations are compatible.

## Follow-up design: review at `24c85189`

The review found that the Claude set omits `<sid>/tool-results/*`, which Claude Code
writes for large tool outputs and references from the transcript by absolute path.
It also found that every turn re-uploads whole transcripts, that pending completion
at turn start uses the wrong Claude directory, that the fence has no undo when the
provider never received the turn, that the failed-save notice can be untrue, and
that e2e and parity coverage are thin. The user chose append-in-place transfer,
same-directory Claude resume, amending migration 057 and an API-side undo.

```text
turn start ── pending for this session? ──▶ complete it (layout from the fenced cwd)
   │
   ├─ published(metadata) ─▶ Claude key ≠ current key ─▶ Unusable ─▶ rebuild (warning)
   │                     └─▶ hash local files: match ─▶ reuse │ mismatch ─▶ download+verify │ extra ─▶ remove
   ▼
begin (row keeps prior publication) ─▶ provider ─▶ harvest reported session only
   per file: stored bytes = local prefix ─▶ append suffix │ otherwise ─▶ whole file
   ─▶ publish set
settlement: explicit pre-spawn outcome + fenced input still `reserved` + no session reported
  ─▶ restore prior publication
```

- **Claude set and directory (adapter).** The scan and restore treat Claude's set as
  `projects/<key>/<sid>.jsonl` plus every regular file under `projects/<key>/<sid>/`,
  restored at its harvested path. Re-keying is removed. A published set whose project
  key differs from the current working directory is unusable, so the turn rebuilds
  with the bound-native warning. v9 §8.5 already fixes the key for a given target.
  Codex keeps its rollout-only set. One shared layout constructor serves the runtime,
  turn-start pending completion (using `pending.cwd_relative_path`) and the workspace
  flush.
- **Append transfer (store and worker).** The fenced upload gains an optional base
  (size, sha256). The worker sends only the suffix when its local prefix hashes to the
  stored copy. Under the same fence and row lock, the API checks the base against the
  row, advances its versioned SHA-256 state over the suffix, compares the claimed
  full digest, fills at most the last 8 MiB segment, and enforces the 128 MiB cap.
  A base mismatch returns a conflict, and the worker retries once with the whole file.
  Legacy unfenced uploads stay whole-file. An API that ignores the base fails closed:
  the claimed full hash does not match the suffix body.
- **Undoing the fence (store and lifecycle).** `begin` copies the current complete
  publication into a `prior` column in amended migration 057. When the API applies the
  fenced attempt's normal terminal outcome, it restores `prior` in the same
  transaction only when that outcome explicitly reports that the provider never
  started, its reservation is still `reserved` (not delivered or
  `delivery_unknown`), and the row is still pending with no reported provider
  session. A rejected `--resume` has started the provider and keeps the fence
  even if it reported no session. Older outcomes without spawn evidence and
  recovered outcomes from a failed journal never undo.
  The Claude seed-ID harvest fallback is removed, so both providers harvest only a
  reported session.
- **Notice.** The failed-save warning says the next turn rebuilds only if the save is
  still incomplete then.

**Acceptance and tests.** Each item ships with its tests:
- **Worker.** Session-file tests become table-driven over the Claude and Codex layouts:
  fence, partial and late uploads, legacy upload, idempotent publish, warm reuse,
  re-download after a local change, extra-file removal, local-failure retry, the
  Claude session directory including `tool-results/`, and a changed-directory refusal.
  An in-memory store counts transferred bytes to prove an append sends only new bytes,
  and covers a prefix mismatch falling back to a whole upload.
- **Real PostgreSQL.** A Claude publication case; append with a matching base, a
  stale base, a wrong full hash and an exceeded cap; the fence undo for a `reserved`
  input; and no undo after acceptance, `delivery_unknown` or a reported session.
- **Runtime.** Context-path selection, `begin` error mapping, notice and
  `session_files_state` selection become small functions with unit tests. Pending
  completion uses the fenced directory.
- **Process.** A Codex `begin` refusal joins the Claude one, and a process test takes
  the resume path against a mocked store. A spawn failure after `begin` leaves the
  input reserved, so the next attempt resumes natively.
- **E2E (M14).** One helper stays parameterized by provider, and every mode runs for
  both:
  - Resume: record, then a same-VM follow-up that must resume silently with the same
    ID, then VM replacement, then recall. This covers the warm path and a chain
    through a resumed turn's save.
  - Absent bundle and failed save, as today, with Codex added for failed save.
  - Stop during a running turn: the fake persists native files mid-turn and blocks;
    Stop, replace the VM, then recall natively.
  - Stop with an unsaveable file, which rebuilds with the failed-save notice.
- **Fake Claude.** Writes the fact only to a `tool-results/` file that the transcript
  references, as the real CLI does.
- **Live-provider qualification.** In an opt-in G5 browser run with a provisioned
  Claude or Codex account, create a unique fact in tool output, verify that the
  assistant message does not reveal it, and then check same-VM and replacement-VM
  recall with the same reported provider session ID. Keep this separate from the
  deterministic fake-provider CI matrix; record the pinned CLI version, account
  mode and actual result. The normal Cloud Stop cases exercise in-turn native-file
  save and failed-save reconstruction with fake providers on the real G5 VM path.
  A separate deterministic browser case should delay the native write beyond the
  in-turn Stop save, allow it before the shutdown flush, then replace the VM and
  require tool-only recall from the same provider ID. The process test below
  proves the flush route now; that flush-required G5 browser path remains open.

**Risks and assumptions to verify during coding.**
- The original whole-`bytea` append rewrite is addressed by migration 058's
  segments and resumable digest. Measure WAL and latency before claiming a
  Stop-time budget; a whole replacement can still rewrite up to 128 MiB.
- Verify the hash state against full SHA-256 at padding and partition boundaries,
  and keep the existing upsert's incarnation tags on append and retry.
- Verify the `tool-results/` layout on the pinned Claude Code 2.1.285, recording a
  fixture where credentials allow. The directory rule also admits future
  session-directory files within the per-file cap.
- After Stop, the held task requires Continue. Continue sends its own recovery
  prompt on the same logical task. Once that attempt settles, a subsequent
  human recall runs as a new task. Check the native context path and provider
  ID on Continue, then check the following task resumes that ID and recalls
  the tool-only fact (or reports it unavailable after an unsaveable Stop).
- Amending 057 assumes it never ran on a persistent database; its checksum and
  dedicated test change together.

## Review summary

The review at `b53d90ff` found that the new `resume_prompt` field was sent to
`1.0.2` VMs under the unchanged `2026.09.30-agent-turn-background` catalog.
The rollout contract is amended: the new catalog and `agent.turn` version must
identify this input shape, while older catalogs receive their own prior shape
and never receive a native-session binding they cannot verify. The existing
`2026.09.29` transform remains available while such VMs can run. This closes a
control-plane-first rollout failure that the original same-version CI could
not exercise.

The same review asked to suppress the routine clean-resume notice. The
`provider.session` event remains the durable record of native resume; a
conversation notice appears for reconstruction, context failure or failed
session persistence. This deliberately refines issue 282's wording about a
resume notice to avoid a rendered status line on every follow-up. Provider
restore errors are classified by whether the answer definitively makes the
bundle unusable: a transient store failure preserves the published set for
retry, while a definitively missing or corrupt set may reconstruct. A
provider that rejects resume before reporting its session must not republish
the unchanged restored files.

The review's Stop-path finding is valid: the one-second forced harvest and
two-second process reap can leave the current generation pending after Stop,
causing a later Continue to lose tool-only state. The recovery contract is
amended to permit completion of that pending generation only on the same VM
incarnation, after the advancing attempt has ended, for the provider session
reported by that attempt, and before a later `begin`. The bounded turn-end
save remains the fast path. The implementation must derive these facts from
the API's task/attempt lifecycle and authenticated VM incarnation, and must
not infer completion from residual files alone. A different incarnation
continues to reconstruct. The stop flush is a second completion opportunity.
That opportunity has a short overall budget and must not block the workspace
Git receipt if the session-file service remains unavailable; reconstruction
is the explicit fallback for the private provider context.
The review record retains this finding and its eventual executed evidence.

Self-review traced the task-bound publication, upload, restore, prompt and notice paths against the governing commitments; it repaired scanner omissions, launch preconditions, the route authorization inventory and a restore memory spike. Focused worker and provider-launch regressions, affected crates, a real tenant-RLS publication test and restricted-migrator replay passed. One Claude CLI review round found an N-1 upload compatibility gap; the legacy endpoint path and PostgreSQL regression now cover it. First Cloud Rust and E2E runs exposed a fresh-turn fixture assumption and a missing internal auth allowlist; focused selectors and checks pass after their repair. The second E2E build passed; the second Rust build passed all dispatcher process tests and exposed two stale lifecycle assertions, now corrected and verified. Two in-scope CodeRabbit code comments were fixed. A third comment asked for live-provider recall evidence: a focused Codex CLI follow-up recalled a tool-only fact with the same session ID after copying only the rollout into a fresh Codex home. This does not establish the full G5 path with a live provider. The third Cloud Rust and E2E builds passed. Findings and checks are in [the review record](ontology-review-issue-282.md).

After the `edc04fea` Cloud E2E M14 selector passed, follow-up review found that it still did not assert recall of a fact present only in prior tool output. The existing value loop now asks for a random tool-only fact after Claude VM replacement; companion cases exercise Codex native recall and Claude reconstruction when no complete bundle can exist. The fake-provider CLI regressions and the later `c295ffba` G5 Cloud E2E browser matrix pass; live-provider G5 recall remains unverified. No recovery contract or production mechanism changed.

The independent Claude CLI review of this follow-up found three scoped defects:
normal harvest timeout notice reason, session-save failure blocking the Git
flush, and a transient lease/token refusal treated as permanent supersession.
All three were repaired in one round. A real-dispatcher selector now proves
that the Git receipt survives an unavailable pending-session lookup, and
another proves an expired-lease `begin` refusal stays retryable before input
acceptance. The notice regression distinguishes a real forced timeout, a
normal timeout and an HTTP upload failure.

The `c295ffba` Cloud E2E run passed the expanded Claude/Codex replacement-VM resume and reconstruction matrix, including Claude reconstruction after a failed save. Its Cloud Rust run exposed one Linux process-test fixture that cloned `agent.turn` with `1.0.3`, now the production version; changing only the fixture's alternate version and request to `1.0.4` makes the focused Linux selector pass. The in-memory session store also now matches the real service's identical publish retry and completed-bundle legacy-upload fence. A second read-only Claude CLI review found no significant issue after focused tests and workspace checks.

Rebase reconciliation against `fd2ad7ca` on main: the new Base remote-tracking and fork-cut commitments govern repository refs and the source workspace's captured member vector. Session carry still copies delivered chat history into a fresh provider binding, while native resume remains scoped to turns in the same chat. Neither commitment changes the other. Main's expand-only migration 056 updates Base workspace and remote-integration rows; this design's session-bundle table migration follows it as 057, with no shared table or policy mutation. The migration file, checksum and dedicated test name move together before the rebased candidate runs CI.

Rebase reconciliation against `2bfb3cc6` on main: the only intervening change removes obsolete ephemeral-G5-workspace branch guidance from `AGENTS.md`. It changes no governing ontology or task/session implementation. The PR's recovery, Stop and publication commitments remain compatible, so no ontology block or dependent-code resolution is needed for this rebase.

The `24c85189` review follow-up was checked against the recovery exclusion and
snapshot contracts: the undo keys on positive non-delivery (`reserved`), never on
`delivery_unknown` or a recovered outcome. A rejected resume keeps its fence, so a set
the CLI refused cannot loop. Same-directory Claude resume follows v9's deterministic
working directory and adds no state. The append path reuses the existing fence and row
lock, and an older API fails closed. The remaining uncertainties are the pinned CLI's
session-directory contents, the database write cost of append, and how a held task
continues after Stop. These are listed above for coding.

The `80d399da` review found two retry boundaries and an overbroad flush trigger.
`begin` must be idempotent for an identical generation and fence, preserving the
prior publication when a committed response is lost; it must serialize its active
task check with settlement. A retried append whose full hash and size already
match the stored file is likewise a no-op. Pending completion remains limited to
the next same-session turn and the shutdown flush, so a fork capture does not wait
for native-session upload. The failed-save notice distinguishes a potentially
completable pending generation from a definitive failed save. A resumed CLI that
exits without reporting its session still fences the old set; its delivery timing
requires a separate provider-protocol decision and live CLI evidence. These
clarifications preserve the publication and positive-delivery commitments above.

The [S2 proposal](https://github.com/G5Labs/G5-Lean/pull/286#issuecomment-5972036356)
addresses server write amplification with bounded segments and a resumable
SHA-256 state. Its author initially sequenced it after PR 286; the user's
subsequent instruction explicitly includes it in PR 286. Keep the existing
wire protocol and worker fallback. Migration 058 adds nullable versioned hash
state to the parent file row and a child segment table keyed by that file's
same-org composite key and an index starting at 1, with forced RLS, tenant
grants, a same-org cascading FK, and an 8 MiB segment cap. The existing
`bytes` column is segment 0. Every new whole-file upload splits at the same
cap, including replacement of a legacy large row, and writes hash state and
segments atomically. An old row without state remains readable; its first
append returns 409 so the worker re-uploads it whole once. No historical data
rewrite is required. During an API rolling deployment, an older API can
rewrite a segmented parent row as one bytea. A database trigger clears that
row's child segments on every parent-byte rewrite; the new API recognizes a
large unsplit parent and converts it through the same whole-upload fallback.
An older API reading only segment 0 will fail the worker's digest check and
rebuild safely until it is replaced; rolling availability is not claimed.

A fenced append locks the parent file row, checks the prior size and digest,
advances a versioned SHA-256 chaining state over only the suffix, and compares
the resulting full digest to the client's claim. It fills only the last
segment and creates further capped segments. The parent metadata, state and
segments commit together. A lost-response retry with the already stored full
size and digest remains a no-op. Publication checks that segment lengths sum
to the declared size and that indexes are contiguous; load concatenates in
index order and verifies size and full digest. The relaxed parent row size
check alone would permit an incomplete set, so these checks are part of the
publication barrier. The state encoding carries an explicit version, eight
chaining words, byte count and the uncompressed tail; malformed or obsolete
state fails closed to the worker's whole-upload fallback.

Verification includes SHA-256 equivalence at 63/64-byte padding boundaries and
random append partitions; real PostgreSQL checks below-cap append, crossing
the cap, multiple segments and appends, exact readback, whole rewrite cleanup,
legacy no-state fallback, idempotent and stale retries, concurrent appends,
cascade, older-API whole rewrite, Claude old-key cleanup, forced RLS and
cross-org FK. Measure append latency and WAL at
10/50/100 MiB before claiming an empirical budget; the structural bound is
one rewritten segment of at most 8 MiB per append. The VM's local verification
still hashes the full file.

The implementation audit tightened undo to require an explicit `provider_started=false`
outcome in addition to the reserved-input row check. The pinned Claude Code 2.1.285
OAuth CLI wrote a transcript referencing a 30,001-byte file in its session's
`tool-results/` directory, confirming that directory's shape. The deterministic
G5 browser E2E now covers same-VM resume, replacement recall and Stop while
each fake provider is running, with a failed-save companion for both providers.
Cloud E2E on `5827b154` passed all four stopped-provider cases on the first
attempt, including tool-only recall after late native save and truthful rebuild
after a stopped failed save.
Live provider recall through G5 remains a separate opt-in `test-live-e2e` run
requiring the dev stack and a configured provider credential; host CLI login
alone does not provision a guest credential. That run must assert recall of a
random tool-only fact after VM replacement, not just a resumed session ID.

After the local gates passed, the fourth Claude CLI review of the production
patch found no significant defect. Its question about the spawn flag was
resolved at the actual process boundary: the runtime stores it immediately
after successful spawn, and a dispatcher process regression now distinguishes
a missing binary from an immediately failing provider for both adapters.

## Review of `001660ba`: completion and tenant evidence

The [next PR review](https://github.com/G5Labs/G5-Lean/pull/286#issuecomment-5974883370)
found no stale-resume code path but identified gaps in real-VM pending
completion, tenant-role segment writes, and several non-discriminating tests.
The recovery and publication commitments above still govern. A fake-provider
only prompt directive will make the runtime skip its in-turn forced harvest,
leaving the actual native file and fenced generation for completion by the
shutdown flush or the next same-VM turn. Browser cases must first observe the
failed-save notice, then prove the appropriate completion path by same-ID
tool-only recall. No production provider may enter this test hook.

The pending-generation case needs a narrower error decision than published
restore. An unreachable store or uncertain upload remains retryable before
input handoff. A local scan failure or definitive conflict, missing file or
invalid request leaves no valid publication to preserve, so the next turn may
rebuild now with a warning. Published restore still preserves its complete
generation on uncertain local replacement errors. This distinction amends
`g5.conversation.recovery` before dependent code changes. Reads of a stored
segmented file should obtain parent and child bytes from one database snapshot,
so a concurrent older writer cannot create a false integrity failure between
two statements. Whole upload validation may reuse the resumable digest state
instead of hashing the body twice.

Verification includes the >8 MiB upload/append/publish/load path under a minted
tenant `ScopedDb` capability; the existing migration regression separately
checks older-API rewrite and cross-org refusal. It includes a real-VM
flush-required case, a same-VM chat Stop/Continue case, a per-turn fake
refusal case, and discriminating retry, request-order, fence,
provider-argument and restored-byte assertions. The reviewer's populated
restricted-migrator replay is recorded as independent runtime evidence, not
as CI coverage. Optional full-set caps, VM-side S2 measurement, metrics and
legacy code removal remain separate decisions with tracking issues. Live
provider G5 recall still requires a guest-provisioned account.

The `001660ba` review follow-up closes the missing evidence at the scoped
boundaries: a fake-only forced Stop hook makes pending completion observable
through the real VM browser flow, a per-turn refusal directive makes rejected
resume observable, and a tenant-capability PostgreSQL regression exercises
segmented writes. The read path uses one SQL statement snapshot; uncertain
store failures remain retryable, while definitive pending-completion failures
rebuild. The sixth independent read-only CLI review found no significant
defect. The new browser cases and required Cloud checks still need to run.

Rebase reconciliation against `f19cc664` on main: the intervening ownership
work makes ChatService the transcript and notice row writer inside the
deciding owner's transaction, routes task and inbox allocation through
TaskService, and adds source guards for core table writers. PR 286's frozen
history and pending bundle fence use those same owners; its session-file
service writes a separate tenant-scoped table family. The intentions are
compatible. Preserve main's owner operations and writer guards while
carrying this PR's recovery and storage decisions through code resolution.
Main's recorded host-reported VM release RLS gap is separate from this PR's
explicit Stop and shutdown-flush paths; no ontology block requires a new
behavior decision for the rebase.

Rebase reconciliation against `9583bf93` on main: the round-2 ownership work
routes VM-token checks through the workspace lifecycle's single reader, moves
shared attempt-shape reads to TaskService, and keeps transcript and notice
projections in ChatService. PR 286's session-file endpoints use the existing
workspace-incarnation scoping helper, so they inherit its stronger lease and
owner check; the bundle store remains the separate fenced publication writer.
Main also settles host-reported VM loss in the host organization's scope,
which cannot publish a pending bundle because publication still requires the
same live incarnation. Its new `g5.lifecycle.start.held-work` question leaves
stopped turns held until explicit Continue at that base, including PR 286's
then-current chat Stop test. Preserve these main ownership, lock-order and lease decisions
while retaining the PR's task-fenced recovery, native input and segmented
storage commitments. The two lines of intent are compatible; no governing
behavior decision is needed before resolving code conflicts.

Rebase reconciliation against `8a026f8a` on main: the owner has since
confirmed `g5.agents.lifecycle.chat-stop`, so Stop on an ordinary chat turn
cancels that logical turn and its next owner message starts a new turn without
Continue. This supersedes the earlier test assumption above that a chat Stop
leaves its task waiting for Continue. It does not weaken the session-file
commit rule: after the cancelled attempt's terminal journal outcome, the same
live VM may finish its pending generation at the next turn's start, before
fencing the new task, then use the prior native session for that new input.
Change the deterministic browser case to assert a cancelled first task, no
Continue call, a distinct next task, ordered pending completion, same provider
session identity, silent native resume and recall of the earlier tool-only
fact. Workspace Stop still leaves a held task that can use explicit Continue.
Main's protected job and platform turns keep their hold; this test exercises
an ordinary owner chat. The main decision and PR 286 recovery commitments are
compatible with that test correction. No new storage state is warranted.

Rebase reconciliation against `07434cd6` on main: the `g5.security.read`
current-owner rule now has one reader for ordinary workspace checks and
PermissionService scope checks. In an invite-only workspace, its former
creator has no access after transfer without an invitation. PR 286 does not
grant creator access: its user-facing chat paths inherit those workspace
checks, while internal session-file operations require the current VM
incarnation and the task- and generation-fenced bundle authority. Preserve
main's shared reader and transfer tests; do not add a `created_by` shortcut
to the bundle path. This security refinement and the native recovery design
are compatible, with no new storage or recovery decision.

Rebase reconciliation against `7218a71a` on main: `g5.documents.files-view`
adds a read-first presentation and explicit text editing in FILES, while
`g5.governance.file-suggestions` lets a non-owner reader push review records
without changing source content or starting compute. These contracts govern
workspace source files shown to people. PR 286's provider session files stay
private runtime state in tenant-scoped storage; its Project Docs and chat
recovery flows do not acquire a FILES editor or a reader suggestion route.
Preserve main's FILES page behavior, draft handling and suggestion authority.
The shared route authorization inventory must retain both the new file-
suggestion entry and PR 286's session-file entries. Main's new FILES browser
flows and PR 286's M14 flow can then be validated on the combined head. No
session-recovery commitment or storage design changes.

Rebase reconciliation against `b3a8d26a` on main: the QA deployment gate now
pins a READY worker image only when its commit covers the promoted commit's
worker inputs across the whole intervening range, waiting for an in-range
bake or failing closed. Main changes deployment scripts, Terraform and their
tests; it changes no ontology block or session-file API, worker runtime or
PR CI lane. PR 286's worker changes remain subject to that gate when later
promoted, using the trigger's input paths rather than a duplicate list here.
Preserve the new main gate and its server-side build filter unchanged. This
does not alter the provider-session recovery commitments or their owner.
