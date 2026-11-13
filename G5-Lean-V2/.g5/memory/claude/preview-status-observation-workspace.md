---
name: preview-status-observation-workspace
description: Workspace preview-status-observation (no Project Docs or issue; PRs #655/#662 merged) now carries the 2026-10-20 redesign of preview output over the terminal path and bounded journal reconciliation, after a noisy build blocked the shared dispatcher on QA.
metadata:
  type: project
---

The workspace `preview-status-observation` (id 39c1eaf4-2a18-4044-ac71-25587c1623d4) delivered the lit preview trigger and the preview run log through PRs #655 and #662, both merged to main on 2026-10-20. Its requirements came only from owner chat on 2026-10-20 and are recorded in the metadata of `g5.preview.trigger-state` and `g5.preview.run-log` in `.g5/ontology/features/ui.g5.md`; there is no `docs/<workspace-id>/` folder in the docs repo and no linked issue; before the redesign below there was also no `.g5/chat_intent/` record and no saved ontology plan for it.

On 2026-10-20 the owner reported from live QA that the run log's per-line journal events (8,600+ from one build) held the single API reconciliation loop for minutes and delayed queued chats in every organization, and asked for a redesign that reuses the terminal mechanisms. The proposal is in `.g5/pr_ontology_diff_design/preview-status-observation/ontology-plan.md` with the request in `.g5/chat_intent/preview-status-observation/request.md`: preview output goes to a guest-held read-only terminal-style session (`preview:<task id>`, offset reattach) and the reconciler gets a per-attempt batch bound, one ack per batch and an independent dispatch loop. Status at writing: proposed, awaiting owner acceptance; ontology amendments land with the implementation. On the owner's request the per-line output events were reverted first (both streams; stdout events had existed since d832235b on 2026-10-14, stderr since #655), from this same branch, with the run-log and exit-diagnostics ontology blocks amended to say live output is withdrawn pending the redesign.

**Why:** A later session asked for "this workspace's change" plan found nothing to plan; knowing the delivered scope avoids re-deriving it.

**How to apply:** Before planning more work here, ask the owner for the new requirement. Known residuals on 2026-10-20: the ontology assumption that the red last-failed state persists until the next start (owner undecided on clearing it when the notice is seen), and one trivial unresolved CodeRabbit thread on #662 about selecting stage labels by visible text. Main renamed `previewRunLog.ts` to `previewRunLogModel.ts` after #662. See [[bot-review-scope-discipline]].
