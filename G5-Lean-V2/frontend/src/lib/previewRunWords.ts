// =============================================================================
// G5 Client — words for a preview run's stage and ending
// =============================================================================
// One vocabulary for every surface that describes a preview run: the Activity
// row and its Current badge, the Workspace rail's Current row and the preview
// dialog's run log (ui g5.ui.activity.preview-row, g5.preview.run-log). The
// server decides the stage and the structured ending; this module only words
// them, so the two surfaces cannot drift.
// =============================================================================

import type {
  PreviewEndingDTO,
  PreviewRunActivityDTO,
  PreviewRunStage,
} from "./types";

/** The stop reasons the platform retains (g5.preview.service.stop-reason). */
export const PREVIEW_STOP_SENTENCES: Record<string, string> = {
  idle_stop: "Stopped because the workspace went idle",
  owner_stop: "Stopped with the workspace",
  workspace_stop: "Stopped with the workspace",
  transfer: "Stopped for an ownership transfer",
  start_required: "Stopped because the workspace had to restart",
  host_drain: "Stopped while its host was drained",
};

function words(value: string) {
  return value.replace(/_/g, " ");
}

/** The badge word of a live run: Queued, Starting, Building, Serving on port N, Closing. */
export function previewStageWord(
  stage: PreviewRunStage,
  port?: number | null,
): string {
  switch (stage) {
    case "queued":
      return "Queued";
    case "starting":
      return "Starting";
    case "building":
      return "Building";
    case "serving":
      return port ? `Serving on port ${port}` : "Serving";
    case "closing":
      return "Closing";
  }
}

/** The verb an ended row's title carries after "Preview". */
export function previewEndingVerb(kind: PreviewEndingDTO["kind"]): string {
  return kind === "did_not_start" ? "did not start" : kind;
}

/** "Preview closed · full-stack"; "Preview · full-stack" while the run is live. */
export function previewRunTitle(
  preview: Pick<PreviewRunActivityDTO, "profile" | "ending">,
): string {
  const verb = preview.ending
    ? ` ${previewEndingVerb(preview.ending.kind)}`
    : "";
  return `Preview${verb} · ${preview.profile ?? "default profile"}`;
}

function stopSentence(reason: string | undefined): string | null {
  if (!reason) return null;
  return PREVIEW_STOP_SENTENCES[reason] ?? `Stopped: ${words(reason)}`;
}

/** The one sentence an ended row shows under its title. */
export function previewEndingSentence(ending: PreviewEndingDTO): string {
  switch (ending.kind) {
    case "ended":
      return "The application exited on its own";
    case "closed":
      if (ending.close === "failed") {
        return "Closed by you, but the cleanup command failed";
      }
      if (ending.close === "killed") {
        return "Closed by you, but cleanup did not finish in time and was killed";
      }
      if (ending.close === "lost") {
        return "Closed by you, but the workspace VM was lost before cleanup finished";
      }
      return "Closed by you";
    case "stopped": {
      // A close the platform recorded without an actor (the run log reads
      // the module's outcome, which carries no stop reason) still names a
      // cleanup that failed, was killed, or was cut short by a lost VM; with
      // a retained reason the stop sentence leads.
      const stop = stopSentence(ending.reason);
      const cleanup =
        ending.close === "failed"
          ? "the cleanup command failed"
          : ending.close === "killed"
            ? "cleanup did not finish in time and was killed"
            : ending.close === "lost"
              ? "the workspace VM was lost before cleanup finished"
              : null;
      if (cleanup) return `${stop ?? "Closed"}, but ${cleanup}`;
      return (
        stop ??
        (ending.cause === "unstarted" ? "Stopped before it started" : "Stopped")
      );
    }
    case "did_not_start":
      return ending.message
        ? `Could not be started: ${ending.message}`
        : "Could not be started";
    case "failed":
      return failureSentence(ending);
  }
}

function failureSentence(ending: PreviewEndingDTO): string {
  switch (ending.cause) {
    case "app_exited": {
      const how =
        typeof ending.exitCode === "number"
          ? `status ${ending.exitCode}`
          : typeof ending.signal === "number"
            ? `signal ${ending.signal}`
            : "an error";
      return `Application exited with ${how}`;
    }
    case "timed_out":
      return "Exceeded its time limit";
    case "limit_exceeded":
      return "Exceeded its output limit";
    case "interrupted":
      return "Interrupted: supervision of the run was lost";
    case "vm_lost": {
      const stop =
        ending.reason && ending.reason !== "vm_lost"
          ? stopSentence(ending.reason)
          : null;
      return stop
        ? `Lost with the workspace VM while it was being stopped ${stop.replace(/^Stopped[:]? ?/, "")}`
        : "Lost with the workspace VM";
    }
    default:
      return `Preview failed: ${ending.message ?? "unknown reason"}`;
  }
}

/**
 * Whether the run has a log worth opening (g5.preview.run-log.ended): a live
 * run, a failed one whose VM may still hold its output, or a close whose
 * cleanup failed or was killed. A run that never started, one lost with its
 * VM, and a cleanly closed, stopped or ended run keep no log link.
 */
export function previewRunHasLog(
  preview: Pick<PreviewRunActivityDTO, "stage" | "ending">,
): boolean {
  if (preview.stage) return true;
  const ending = preview.ending;
  if (!ending) return false;
  if (ending.kind === "failed") return ending.cause !== "vm_lost";
  return (
    (ending.kind === "closed" || ending.kind === "stopped") &&
    (ending.close === "failed" || ending.close === "killed")
  );
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object"
    ? (value as Record<string, unknown>)
    : {};
}

/**
 * The ending a module's `invocation.outcome` payload describes, as the run
 * log reads it from the task's timeline. The platform's retained stop reason
 * lives only on the task's outcome, so a stopped run reads as `stopped`
 * without one here.
 */
export function previewEndingFromOutcome(
  payload: unknown,
): PreviewEndingDTO | null {
  const outcome = asRecord(payload);
  const status = typeof outcome.status === "string" ? outcome.status : "";
  const result = asRecord(outcome.result);
  const detail = asRecord(outcome.detail);
  const text = (value: unknown) =>
    typeof value === "string" && value ? value : undefined;
  switch (status) {
    case "succeeded":
      return { kind: "ended" };
    case "cancelled": {
      const reason = text(outcome.reason);
      return {
        kind: reason === "owner_cancel" ? "closed" : "stopped",
        reason,
        close: text(detail.close),
      };
    }
    case "timed_out":
      return { kind: "failed", cause: "timed_out" };
    case "limit_exceeded":
      return { kind: "failed", cause: "limit_exceeded" };
    case "interrupted":
      return { kind: "failed", cause: "interrupted" };
    case "failed":
      if (result.status === "app_exited") {
        return {
          kind: "failed",
          cause: "app_exited",
          exitCode:
            typeof result.app_exit_code === "number"
              ? result.app_exit_code
              : null,
          signal:
            typeof result.app_signal === "number" ? result.app_signal : null,
        };
      }
      return {
        kind: "failed",
        cause: "module",
        message:
          text(result.message) ?? text(detail.reason) ?? text(outcome.detail),
      };
    default:
      return null;
  }
}
