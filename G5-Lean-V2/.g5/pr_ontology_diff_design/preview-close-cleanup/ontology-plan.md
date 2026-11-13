# Preview close and cleanup — one preview at a time

## Ontology changes and governance

**Comparison base:** `main` at `b3a8d26a`, branch `preview-test`. The [chat intent](../../chat_intent/preview-close-cleanup/request.md) holds the user's request and the accepted assessment.

**Ontology changes** (`features/preview_terminal.g5.md`):
- `g5.preview.service.replace` is amended in place (stable ID, same anchor): "Stop the previous preview before replacing it" becomes "Close the running preview before starting another". Reason: the preview now shares the owner's VM with chat and terminal work, so a silent replacement left the old preview's side effects running beside the new one. Affected commitment: `start_workspace_preview` refuses instead of cancelling and queueing. Observable consequence: a second start returns a conflict while a preview is queued, running or closing; the loss paths that cancel queued previews are unchanged.
- `g5.preview.service.close` is added: close is a bounded graceful stop owned by the preview task, running the profile's optional `cleanup_command` after the application stops, also after a self-exit; the HTTP gate closes at once, closing is visible, and a failed or overrun cleanup is reported.
- Metadata for both blocks started at not implemented; after implementation the [review](ontology-review.md) recorded locations, evidence and gaps in their footers. The amended block's lineage records this plan and intent.
- During review `g5.preview.service.close` was tightened (not widened): it now states what each side guarantees when Close is recorded, the platform relay refusing new streams and the guest withdrawing the declared port, because the guest alone also admits scanned listeners.
- Independent review preserved the accepted workspace-stop exclusion and made its teardown explicit: only durable `owner_cancel` invokes profile cleanup; workspace lifecycle stop kills the separately grouped preview app without cleanup. The dispatcher/guest SIGINT handshake is the minimal mechanism needed to avoid orphaning that app group while honoring the exclusion.

Diff of the block text, metadata included: `git diff b3a8d26a -- .g5/ontology/features/preview_terminal.g5.md`.

### Relevant and governing ontology

| Block | Why relevant | Implementation must enforce or consider |
|---|---|---|
| [Close the running preview before starting another](../../ontology/features/preview_terminal.g5.md#close-the-running-preview-before-starting-another) | The owning decision | A start beside an active preview is a 409; the same idempotency key still replays its own task. Loss paths keep cancelling queued previews. |
| [Close runs the profile's cleanup](../../ontology/features/preview_terminal.g5.md#close-runs-the-profiles-cleanup) | The new behaviour | Cleanup runs inside the preview task's graceful stop and after a self-exit, with the launch command's shell, cwd and env rules; close is bounded; the port leaves the gate when close begins; a bad cleanup is visible. |
| [Preview is a visible service with explicit lifetime](../../ontology/features/preview_terminal.g5.md#preview-is-a-visible-service-with-explicit-lifetime) | Close is still task cancellation | The task stays `running` with its durable cancel intent until the guest reports the outcome; the reconciler's idempotent redelivery already covers a cleanup that takes tens of seconds. |
| [Distinguish preview process from serving readiness](../../ontology/features/preview_terminal.g5.md#distinguish-preview-process-from-serving-readiness) | Readiness during close | Once close begins the port is no longer admitted, so streams get 503 while the task is still running. |
| [Preview transport preserves owner authority](../../ontology/features/preview_terminal.g5.md#preview-transport-preserves-owner-authority) | Established streams | Unchanged: streams stay bound to task and incarnation. |
| [Owner Stop reliably ends any agent attempt](../../ontology/features/agents.g5.md#owner-stop-reliably-ends-any-agent-attempt) | The shape of bounded interruption | SIGTERM, bounded grace, then SIGKILL of the whole group; the stopped state is visible. |
| [Make cancellation terminal for the logical task](../../ontology/features/agents.g5.md#make-cancellation-terminal-for-the-logical-task) | Close ends the task | A closed preview ends `cancelled`; a new preview is a new task. |
| [Keep preview write expectations honest](../../ontology/features/preview_terminal.g5.md#keep-preview-write-expectations-honest) | Cleanup writes | `cleanup_command` is the customer's own command in the customer's VM; it is not a security boundary, like `command`. |
| [Justify each new mechanism](../../ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism) | Minimal design | No new task kind, catalog module or command path: cleanup is the preview task's stop phase. No migration or flag. |
| [Test real boundaries](../../ontology/concepts/system_design_principles.g5.md#test-real-boundaries-and-report-exact-evidence) | Evidence | Guest, dispatcher, API, component and browser tests; report what did not run. |

## Requested outcome

The preview dialog offers Close preview for an active preview. Close stops the application, runs the profile's cleanup command (for G5 itself, `just dev-down`), and shows "Closing preview" until the guest reports the outcome. While a preview is queued, running or closing, Start preview is refused with a conflict. A profile may declare `cleanup_command`; the create-profile route, the listing DTOs and the guest all carry it.

## Design

```text
owner ─ Close ─▶ POST …/preview/cancel ─▶ request_workspace_task_cancel (unchanged: durable cancel intent)
                                        └▶ deliver cancel ─▶ guest dispatcher modules.cancel
dispatcher (g5d-dispatch runner)          forget the service port (HTTP gate 503 at once)
  owner Close = SIGTERM ─ grace 90 s ─ SIGKILL group
  workspace lifecycle stop = SIGINT ─ grace 5 s ─ SIGKILL group (other modules: unchanged SIGKILL)
  outcome "cancelled" detail.close = "clean" | "failed" | "killed"   (exit 0 / non-zero / overran grace)
preview-serve (guest)                     app child in its own process group
  SIGTERM (owner Close), or app exited on its own ─▶ SIGTERM app group, wait ≤10 s, SIGKILL app group
                                                   ─▶ run cleanup_command (sh -c, same cwd/env/PATH scrub, PORT), ≤60 s
                                                   ─▶ exit 0 when cleanup succeeded or absent, 1 otherwise
  SIGINT (workspace lifecycle stop) ─▶ stop the app group immediately, skip cleanup, exit
API (task_lifecycle)                      WorkspacePreviewView += closing (running + cancel_requested)
                                                                += close (from the cancelled outcome detail)
  start_workspace_preview: active preview (queued|running|waiting) and a new key ─▶ 409 PREVIEW_ACTIVE
profile (.g5/preview_profiles/*.yaml, workspace.yaml preview:)  += cleanup_command (optional)
UI (PreviewControl)   active & !closing ─ Open preview · Close preview
                      closing           ─ "Closing preview… running cleanup" (buttons off)
                      cancelled & close≠clean ─ notice: cleanup failed / did not finish in time
```

**Owner and boundary.** The preview task owns the whole close: the API records the cancel intent as today, the dispatcher gives only an owner Close the graceful cleanup policy, and `preview-serve` runs the stop sequence. The application runs in its own process group under `preview-serve` so the guest can stop it on a bound of its own before cleanup; the dispatcher's final group SIGKILL after the grace still bounds `preview-serve` itself. A workspace lifecycle stop uses a distinct bounded SIGINT path so `preview-serve` immediately kills that separate app group without running profile cleanup; killing only the wrapper would orphan the app. Partial failure is explicit for owner Close: the cancelled outcome carries whether `preview-serve` exited cleanly, with a cleanup error, or was killed, and the preview view surfaces it; the task's output keeps the cleanup command's own logs. A refused start carries a conflict message the dialog shows.

**Acceptance outcomes.**
- A second start while a preview is queued, running or closing returns 409; the same key replays.
- Close with a cleanup command runs it after the app stops; the task ends `cancelled` with `close = clean`; the UI showed closing meanwhile.
- A cleanup that exits non-zero ends the task `cancelled` with `close = failed`, and the dialog says so.
- An app that exits on its own still runs cleanup; the task then ends as before (`succeeded` or `failed`).
- During close the preview port returns 503.
- Workspace lifecycle stop kills the preview app group without running `cleanup_command` and carries no owner-close classification.
- Profiles with `cleanup_command` list, create, render and launch; profiles without it behave as today.

**Risks and assumptions.**
- The dispatcher reads the already durable cancellation reason before choosing the preview signal. `owner_cancel` is the owner Close contract; workspace stop and other lifecycle cancellations use bounded SIGINT so the wrapper can kill its separate app group without cleanup. If the reason cannot be read, fail closed to the lifecycle-stop path rather than run customer cleanup unexpectedly.
- A `cleanup_command` that needs more than 60 s is reported as failed rather than extended; the bound is a constant, not a profile field, to avoid new configuration.
- The G5 `dev-down` cleanup runs `worker-host-stop` first, which only warns inside the VM, then `docker compose down -v`, which drops volumes.
- `deny_unknown_fields` on the API parser means a profile with `cleanup_command` is invisible to an API that predates this change; there are no external users, so no compatibility shim.

**Implementation notes.** Decisions made while building:
- The app runs in its own process group under `preview-serve`, so the guest stops it on its own bound (SIGTERM, 10 s, SIGKILL) before cleanup; the dispatcher's SIGTERM reaches `preview-serve` only. A `preview-serve` crash could orphan that group until the VM ends; docker state is per boot anyway.
- Independent review corrected an implementation/plan mismatch: treating every preview cancellation as Close made `workspace_stop` run cleanup, contrary to the accepted scope. Simply restoring dispatcher SIGKILL also orphaned the app's separate process group. The corrected contract reserves SIGTERM for durable `owner_cancel` and uses bounded SIGINT for lifecycle cancellation; `preview-serve` handles SIGINT by immediately stopping the app group and skipping cleanup.
- The dispatcher classifies the close from `preview-serve`'s exit: 0 is `clean`, non-zero `failed`, a signal (grace overrun) `killed`. The API copies that string into the view from the stored cancelled outcome, so no new table or column exists.
- `StartWorkspacePreviewResult` and the route's replacement-cancel delivery were removed; `start_workspace_preview` returns the view.
- The relay (`workspace_preview_target`) refuses a closing preview with the existing `PREVIEW_NOT_READY` 503, found during review.
- A daemon the launch command needs during cleanup must leave the app's process group: the G5 `dev-up` profile starts dockerd under `setsid`, because Close stops the whole app group before cleanup runs (found while diagnosing CI).

## Review summary

Self-review and an independent review on 2026-10-05 are recorded in [ontology-review.md](ontology-review.md). The independent pass checked both changed ontology blocks, the start refusal, cancellation reason and signal selection, guest app/cleanup process groups, the API-to-dialog close result, profile propagation and browser-flow intent. It fixed residual app and cleanup descendants, restored transfer coverage, removed a flaky signal-test sleep, and corrected the owner-Close/workspace-stop distinction. Real dispatcher tests now prove owner Close runs cleanup and reports `clean`, while workspace stop kills the app without cleanup or close classification. Guest, dispatcher, API (against local Postgres), component, type, lint, format and focused clippy checks pass. The published head's broad Playwright shards passed, but its provider-agent lane—which reruns the live-preview transfer case—failed without selector-level diagnostics; the repaired local flow and G5 `dev-up`/`dev-down` profile still need a workspace-VM rerun before merge.

Remaining uncertainty:
- The amendment overturns an agent-authored decision on the user's explicit instruction; the loss-path commitments under the same block are untouched.
- The lifecycle-stop SIGINT path is intentionally separate from owner Close so a workspace stop cannot run customer cleanup; its dispatcher-level regression proves the app process exits and the outcome has no `detail.close`.
- The 60 s cleanup bound is a constant; a profile needing longer reports `failed` and keeps its logs in the task output.
