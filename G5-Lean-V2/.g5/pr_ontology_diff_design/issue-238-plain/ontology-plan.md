# Issue #238 plain PR — focused merge readiness

## Ontology changes and governance

**Comparison base:** The plan began on `origin/main` at `a1c3e29e`, with plain PR `c6cbc83e` applied. The publication branch was rebased onto `origin/main` at `e386351a`, then at `967bf34c`. The later main changes update metadata in the affected ontology files without changing their governing commitments. The scoped diff below records the PR's commitment changes; compare the branch with current `origin/main` for the complete current diff.

**Ontology changes:** modified `g5.git.durability.no-implicit` and metadata for existing Save/read commitments; added `g5.git.checkpoint.selected`, `.isolation` and `.retry` in `software_architecture/durability.g5.md`; added `g5.documents.save.checkpoint` in `features/documents.g5.md`; added `g5.governance.accept.checkpoint` and `.applied-pending` in `features/governance.g5.md`. Their metadata changes are part of the same edits. The scoped design diff is:

```diff
diff --git a/.g5/ontology/features/documents.g5.md b/.g5/ontology/features/documents.g5.md
index 2b4c663c..84e911df 100644
--- a/.g5/ontology/features/documents.g5.md
+++ b/.g5/ontology/features/documents.g5.md
@@ -126,0 +127,8 @@ Save obeys [durability boundaries](../software_architecture/durability.g5.md) an
+### Publish an explicit editor Save
+
+[I · g5.documents.save.checkpoint]
+
+When the workspace owner explicitly saves a supported editor file, save the submitted
+edits and invoke the [selected-file checkpoint](../software_architecture/durability.g5.md#commit-and-push-only-selected-file-changes), whether or not the file has review suggestions.
+Other document writes do not acquire this effect merely by writing.
+
@@ -496 +504 @@ Partial incorporation does not terminalize a future contribution session.
-- ~70% · `workspace_runtime.rs`, owner file routes and frontend file hooks; `useFiles.ts`, `useDocs.ts` and `useOntology.tsx` apply Save results without checking the current attachment. Complete stream revocation and former-owner buffer invalidation remain unassessed.
+- ~75% · `workspace_runtime.rs`, owner file routes and frontend file hooks; `useFiles.ts`, `useDocs.ts` and `useOntology.tsx` now fence late Save cache results against the current attachment, with focused hook tests. Complete stream revocation and former-owner buffer invalidation remain unassessed.
@@ -502 +510 @@ Partial incorporation does not terminalize a future contribution session.
-- ~65% · `routes/workspaces.rs` and `useFiles.ts` carry a content hash but omit the client-observed VM ID; matching bytes can therefore allow a stale Save into the replacement.
+- ~80% · owner file/document/ontology routes and editor hooks carry the content hash and client-observed VM ID; API tests reject a stale VM, and browser Save flows pass. A browser VM-replacement race and broader non-editor Save paths remain unassessed.
@@ -508 +516 @@ Partial incorporation does not terminalize a future contribution session.
-- Unassessed across editors · `CodeEditor.tsx` retains submitted content, but `SpecEditor.tsx::persist` records the post-request buffer and clears dirty state, so later edits can appear saved. Runtime cases have not been rerun in this checkout.
+- `CodeEditor.tsx` and `SpecEditor.tsx::persist` retain the submitted content as the Save baseline, leaving later edits dirty; targeted editor tests pass. Other editors remain unassessed.
@@ -514 +522,7 @@ Partial incorporation does not terminalize a future contribution session.
-- ~90% · `CodeEditor.tsx`, `SpecEditor.tsx`, `StructuredQuestionEditor.tsx` and file hooks; every blur/editor integration was not exercised.
+- ~90% · `CodeEditor.tsx`, `SpecEditor.tsx`, `StructuredQuestionEditor.tsx` and file hooks; explicit Code, Project Doc, ontology and JSONL Save plus HTML answer Apply were exercised against the live stack. Every blur/editor integration was not exercised.
+
+### g5.documents.save.checkpoint
+
+- human-provided
+- [Issue #238](https://github.com/G5Labs/G5-Lean/issues/238) specifies the explicit editor Save trigger; the scope clarification is informed by [plain PR follow-up](../../chat_intent/issue-238-plain/request.md).
+- Code, document, ontology and JSONL source Save request the selected checkpoint; HTML answer Apply remains save-only. The live browser suite passed in Firefox and WebKit, with Chromium's repaired JSONL case rerun after its first failure.
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 1ab30406..84e05612 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -82,0 +83,17 @@ Governing contracts: [review in a workspace you own](governance.g5.md#review-in-
+### Publish accepted review files
+
+[I · g5.governance.accept.checkpoint]
+
+When the source workspace owner accepts a reviewer's proposed file change, include
+that file, including a reviewer-created new file, in the
+[selected-file checkpoint](../software_architecture/durability.g5.md#commit-and-push-only-selected-file-changes).
+
+### Keep an applied acceptance settled when publication fails
+
+[D · g5.governance.accept.applied-pending]
+
+If an accepted file change was applied but its checkpoint fails, keep the decision
+accepted rather than rejectable and disclose pending publication in the result.
+The [selected-file operation](../software_architecture/durability.g5.md#retry-a-failed-selected-file-publication)
+can be retried while that live file version remains available.
+
@@ -214,0 +232,12 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
+### g5.governance.accept.checkpoint
+
+- human-provided
+- [Issue #238](https://github.com/G5Labs/G5-Lean/issues/238) specifies acceptance and reviewer-created new files.
+- Acceptance writes and checkpoints selected files, including a reviewer-created new file; live Chromium, Firefox and WebKit acceptance cases passed.
+
+### g5.governance.accept.applied-pending
+
+- agent
+- Protects truthful review decisions when commit or push fails after a file write, as examined in the [plain PR follow-up](../../chat_intent/issue-238-plain/request.md).
+- Applied review changes become accepted even after a checkpoint failure, with a pending response/notice. The failure path passed an injected API test; live acceptance failure injection and crash recovery remain unexecuted.
+
diff --git a/.g5/ontology/software_architecture/durability.g5.md b/.g5/ontology/software_architecture/durability.g5.md
index e24d3cb2..f9dcb12b 100644
--- a/.g5/ontology/software_architecture/durability.g5.md
+++ b/.g5/ontology/software_architecture/durability.g5.md
@@ -11 +11 @@ Native push and a complete verified shutdown/fork flush are repository durabilit
-## Keep file saving separate from Git durability
+## Keep implicit file writes separate from Git durability
@@ -15 +15,26 @@ Native push and a complete verified shutdown/fork flush are repository durabilit
-Do not create periodic platform commits, a manual Flush-now control or a commit on Save.
+Do not create periodic platform commits or a manual Flush-now control. Only the
+owner's explicit editor Save and acceptance of a review file change invoke the
+[selected-file checkpoint](#commit-and-push-only-selected-file-changes); other
+writes do not implicitly commit.
+
+## Commit and push only selected file changes
+
+[D · g5.git.checkpoint.selected]
+
+One reusable operation accepts selected repository files and their expected versions,
+commits only changed selected content to each workspace member branch, then pushes
+that commit. It includes accepted new files and omits empty commits.
+
+## Preserve unrelated Git state during a checkpoint
+
+[D · g5.git.checkpoint.isolation]
+
+A selected-file checkpoint preserves unrelated working and staged changes, including
+native staging concurrent with the checkpoint.
+
+## Retry a failed selected-file publication
+
+[D · g5.git.checkpoint.retry]
+
+A reported selected-file publication failure is retryable while the same live
+file version is available, without silently adding a different version.
@@ -192,2 +217,20 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- From [C009/C051/C054](../../uplift/g5-lean/candidates/contracts.md), [H066/H077](../../uplift/g5-lean/candidates/human-decisions.md).
-- ~90% · file Save and lifecycle flush routes; complete legacy hidden-commit audit outstanding.
+- From [C009/C051/C054](../../uplift/g5-lean/candidates/contracts.md), [H066/H077](../../uplift/g5-lean/candidates/human-decisions.md); the explicit Save/acceptance exception comes from [issue #238](https://github.com/G5Labs/G5-Lean/issues/238).
+- Scoped exception implemented in file Save/review routes; ordinary writes and HTML Apply stayed save-only in API/browser checks. Complete hidden-commit audit unexecuted.
+
+### g5.git.checkpoint.selected
+
+- human-verified
+- From [issue #238](https://github.com/G5Labs/G5-Lean/issues/238); expected versions are an agent decision to keep the selected-file operation safe under races.
+- Implemented in shared selected-file helper and guest Git operation; live browser checks covered edited/new review files, four editor surfaces, no-op Save and durable forks.
+
+### g5.git.checkpoint.isolation
+
+- agent
+- Preserves existing [Git working-state intent](#preserve-the-users-git-working-state-during-flush) when a selected-file checkpoint runs.
+- Private-index selected commits and a native index lock preserve unrelated state; unit and live browser checks pass, but simultaneous native-index stress remains unexecuted.
+
+### g5.git.checkpoint.retry
+
+- agent
+- From the [issue #238](https://github.com/G5Labs/G5-Lean/issues/238) selected-file scope and the [plain PR follow-up](../../chat_intent/issue-238-plain/request.md) production-safety request.
+- Same-content Save retry, selected hash check and pinned-head push pass API/browser failure checks; crash between Git phases remains unassessed.
```

**Relevant and governing ontology:**

- `g5.git.durability.no-implicit` allows only explicit Save and acceptance to use the new checkpoint; ordinary writes and HTML Apply generation retain their prior behavior.
- `g5.git.checkpoint.selected` owns affected-file selection and no-empty-commit; `.isolation` preserves unrelated work, and `.retry` governs failed-publication versions. The route callers request it; the Git owner validates actual selected content.
- `g5.documents.save.explicit`, `g5.documents.save.checkpoint`, `g5.documents.save` and `g5.documents.save.acknowledgment` govern the click/shortcut trigger, submitted-buffer acknowledgment and observed attachment. A generated HTML update is a counterexample to explicit editor Save.
- `g5.governance.accept.checkpoint`, `.applied-pending` and `g5.story.accept-review-changes` govern owner acceptance, new files and nonrejectable applied changes. A file written before failed push must not be shown as a fresh, unapplied proposal.
- `g5.documents.files`, `g5.documents.files.version`, `g5.documents.association` and `g5.git.durability.working-state` govern path, version, branch and unrelated staged/working state. An unrelated staged file must never enter the checkpoint.
- `g5.git.durability` and `g5.governance.publish` distinguish internal workspace checkpoint from provider PR publication.

## Outcome and scope

Keep the existing shared file-list Git helper and its per-path hash checks. Add the missing JSONL source Save callback and ensure HTML Apply does not request a checkpoint. Make a review file application terminal as an acceptance even when the selected commit/push fails, expose pending publication to the caller, and make retry act only on the chosen file version. Repair directly implicated attachment/acknowledgment defects when tests expose them. Avoid a new API intent database, guest receipt journal, new queue or cross-repository transaction.

## High-level design

```text
Explicit editor Save / review acceptance
  -> authorized file mutation at selected version
  -> shared selected-file commit and push
  -> truthful success or pending publication
```

Save and review choose the target file and retain their distinct product policy. The shared helper validates selected Git content and publishes only the returned commit. Review status records an applied decision before a publication failure can make it rejectable again. The existing Changes/Save mechanisms can retry pending Git work; no new durable operation protocol is introduced.

## Observable acceptance and risks

- Explicit Save of ordinary file, document, ontology and JSONL source saves exact submitted bytes, creates at most one changed-file commit, and pushes the mapped branch; no-review-suggestion path is included.
- HTML Apply retains its existing save behavior and does not gain an implicit checkpoint. No-op Save creates no commit. Stale hash, wrong branch and wrong VM refuse unsafe writes.
- Review hunk and new-file acceptance publish only their chosen file; a failed commit or push does not leave an applied change rejectable, and retry neither duplicates the commit nor stages unrelated content.
- Live API/browser verification must run against this PR’s own build and dedicated data. Crash and authority lifecycle cases are tested where feasible; any unexecuted case is reported explicitly.

The minimal partial-failure policy may need amendment if source inspection shows existing review statuses or UI cannot represent accepted but unpublished work. No claim of cross-repository atomicity is made. The shared helper’s branch and index policy must be inspected during coding, not assumed from its name.
