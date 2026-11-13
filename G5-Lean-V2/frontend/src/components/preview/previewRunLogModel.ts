import type { TimelineEntry } from "../../lib/types";
import {
  previewEndingFromOutcome,
  previewEndingSentence,
} from "../../lib/previewRunWords";
import type { PreviewRunView } from "./previewIndicator";

const TERMINAL = new Set(["succeeded", "failed", "cancelled"]);

/** How often a live run's lifecycle entries are re-read for the stage strip. */
export const RUN_LOG_STAGE_POLL_MS = 2_000;

/** The hosted session a preview task's output lives on (`g5.preview.output.live-tail`). */
export function previewTerminalId(taskId: string): string {
  return `preview:${taskId}`;
}

export type PreviewStageState = "done" | "current" | "pending" | "failed";

export interface PreviewStage {
  key: "queued" | "starting" | "building" | "serving" | "closing" | "closed";
  label: string;
  state: PreviewStageState;
}

function eventKind(entry: TimelineEntry): string {
  const kind = entry.metadata?.kind;
  return typeof kind === "string" ? kind : "";
}

function eventPayload(entry: TimelineEntry): Record<string, unknown> {
  const payload = entry.metadata?.payload;
  return payload && typeof payload === "object"
    ? (payload as Record<string, unknown>)
    : {};
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object"
    ? (value as Record<string, unknown>)
    : {};
}

/**
 * The run's stages from the preview view and the task's lifecycle events
 * (`g5.preview.run-log`). The view decides the live stages (ready, closing:
 * a Close is recorded on the task, not as an event); the events decide what
 * already happened. Application output is not an event and plays no part.
 */
export function previewStages(
  view: PreviewRunView | null | undefined,
  entries: readonly TimelineEntry[],
): PreviewStage[] {
  const status = view?.status ?? "queued";
  const live = !TERMINAL.has(status);
  const started = moduleStarted(entries);
  const ready =
    (view?.ready ?? false) ||
    entries.some((entry) => eventKind(entry) === "module.preview.ready");
  const closing = live && (view?.closing ?? false);
  const port = view?.port;

  const starting: PreviewStageState =
    started || ready || !live ? "done" : "current";
  let buildingStage: PreviewStage;
  if (ready) {
    buildingStage = { key: "building", label: "Building", state: "done" };
  } else if (live) {
    buildingStage = {
      key: "building",
      label: "Building",
      state: started ? "current" : "pending",
    };
  } else if (status === "failed") {
    buildingStage = {
      key: "building",
      label: "Failed while building",
      state: "failed",
    };
  } else {
    buildingStage = { key: "building", label: "Building", state: "pending" };
  }
  const serving: PreviewStage = {
    key: "serving",
    label: port ? `Serving on port ${port}` : "Serving",
    state: ready ? (live && !closing ? "current" : "done") : "pending",
  };
  const closingStage: PreviewStage = {
    key: "closing",
    label: "Closing",
    state: closing ? "current" : status === "cancelled" ? "done" : "pending",
  };
  let closed: PreviewStage;
  if (live) {
    closed = { key: "closed", label: "Closed", state: "pending" };
  } else if (status === "failed") {
    closed = { key: "closed", label: "Failed", state: "failed" };
  } else if (status === "cancelled" && view?.close === "lost") {
    closed = { key: "closed", label: "Closed; VM lost", state: "failed" };
  } else if (status === "cancelled" && view?.close && view.close !== "clean") {
    closed = {
      key: "closed",
      label: "Closed; cleanup failed",
      state: "failed",
    };
  } else {
    closed = { key: "closed", label: "Closed", state: "done" };
  }
  return [
    { key: "queued", label: "Queued", state: "done" },
    { key: "starting", label: "Starting", state: starting },
    buildingStage,
    serving,
    closingStage,
    closed,
  ];
}

function moduleStarted(entries: readonly TimelineEntry[]): boolean {
  return entries.some((entry) => {
    const kind = eventKind(entry);
    return kind === "invocation.started" || kind.startsWith("module.");
  });
}

/**
 * Whether the guest can host the run's session yet
 * (`g5.preview.run-log.attach`): the module started (the timeline says so,
 * or the view already reports ready), or the run ended and its session is
 * whatever the guest retained. A queued or dispatching run has no session
 * to attach, so the log holds its first attach until then.
 */
export function previewRunStarted(
  view: PreviewRunView | null | undefined,
  entries: readonly TimelineEntry[],
): boolean {
  const status = view?.status ?? "queued";
  return (
    TERMINAL.has(status) || (view?.ready ?? false) || moduleStarted(entries)
  );
}

/**
 * The task's events once, in the server's order. Live appends land on the
 * last loaded page while earlier pages may still be loading, so the same
 * entry can appear twice and a newer one before an older one until the
 * pages reconcile; dedupe by id and order as the API does (created time,
 * then id).
 */
export function orderedUniqueEntries(
  entries: readonly TimelineEntry[],
): TimelineEntry[] {
  const seen = new Set<string>();
  const unique: TimelineEntry[] = [];
  for (const entry of entries) {
    if (seen.has(entry.id)) continue;
    seen.add(entry.id);
    unique.push(entry);
  }
  return unique.sort((a, b) => {
    const byTime = compareIsoTimes(a.createdAt, b.createdAt);
    return byTime !== 0 ? byTime : a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  });
}

/**
 * Compare two RFC 3339 timestamps at their full precision. The API writes
 * microseconds and events can land within one millisecond, which
 * `Date.parse` alone would truncate; the integer seconds are parsed (so a
 * `Z` and a `+00:00` offset compare alike) and the fractional digits are
 * compared as padded strings.
 */
function compareIsoTimes(a: string, b: string): number {
  const pa = splitIso(a);
  const pb = splitIso(b);
  if (!pa || !pb) return a < b ? -1 : a > b ? 1 : 0;
  if (pa.seconds !== pb.seconds) return pa.seconds - pb.seconds;
  return pa.fraction < pb.fraction ? -1 : pa.fraction > pb.fraction ? 1 : 0;
}

function splitIso(iso: string): { seconds: number; fraction: string } | null {
  const match = /^(.*?)(?:\.(\d+))?(Z|[+-]\d\d:?\d\d)$/.exec(iso);
  if (!match) return null;
  const seconds = Date.parse(`${match[1]}${match[3]}`);
  if (Number.isNaN(seconds)) return null;
  return { seconds, fraction: (match[2] ?? "").padEnd(9, "0") };
}

/**
 * The failure a run ended with, from its outcome event
 * (`g5.preview.service.exit-diagnostics`): how the application exited and
 * the stderr tail the guest kept, the only output that outlives the VM.
 */
export interface PreviewFailure {
  /** "Application exited with status 7", "Preview failed: reason", … */
  headline: string;
  stderrTail: string | null;
  stderrTailTruncated: boolean;
}

export function previewFailure(
  entries: readonly TimelineEntry[],
): PreviewFailure | null {
  const outcome = entries.find(
    (entry) => eventKind(entry) === "invocation.outcome",
  );
  if (!outcome) return null;
  const payload = eventPayload(outcome);
  const ending = previewEndingFromOutcome(payload);
  if (!ending) return null;
  // The notice names a failure or a close whose cleanup failed; a clean
  // close or an app that exited on its own needs none. The same words as
  // the Activity row (ui g5.ui.activity.preview-row).
  const failed = ending.kind === "failed" || ending.kind === "did_not_start";
  const cleanupFailed =
    ending.kind !== "failed" && !!ending.close && ending.close !== "clean";
  if (!failed && !cleanupFailed) return null;
  const result = asRecord(payload.result);
  const tail = result.stderr_tail;
  return {
    headline: previewEndingSentence(ending),
    stderrTail:
      failed && typeof tail === "string" && tail.trim() ? tail.trimEnd() : null,
    stderrTailTruncated: failed && result.stderr_tail_truncated === true,
  };
}
