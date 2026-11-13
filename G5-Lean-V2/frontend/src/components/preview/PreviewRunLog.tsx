import { useQueryClient } from "@tanstack/react-query";
import { Check, Loader2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTaskTimeline } from "../../hooks/useTasks";
import { classNames } from "../ui/UI";
import { WorkspaceTerminalPanel } from "../execution/WorkspaceTerminalPanel";
import type { TerminalViewerState } from "../execution/terminalViewer";
import type { PreviewRunView } from "./previewIndicator";
import {
  orderedUniqueEntries,
  previewFailure,
  previewRunStarted,
  previewStages,
  previewTerminalId,
  RUN_LOG_STAGE_POLL_MS,
  type PreviewStage,
} from "./previewRunLogModel";

const TERMINAL = new Set(["succeeded", "failed", "cancelled"]);
/** A live run's attach is retried this often until the guest hosts it. */
const ATTACH_RETRY_DELAY_MS = 2_500;
/** Bounded: a live run whose guest never hosts the session (an older dispatcher) falls back. */
const ATTACH_RETRY_LIMIT = 24;
/** The viewer header's words from Log open until the live run's output is attached. */
export const WAITING_FOR_APPLICATION_TEXT =
  "waiting for the application to start…";

/**
 * The preview run's log (`g5.preview.run-log`): the stage strip from the
 * preview view and the task's lifecycle timeline, and below it the
 * application's output as the read-only terminal view of the guest's hosted
 * session `preview:<task id>`. Nothing of the output is persisted: a run
 * that failed is viewable while its VM runs; after the VM stops only the
 * stages and the outcome's stderr tail remain.
 */
export function PreviewRunLog({
  workspaceId,
  taskId,
  view,
  vmRunning,
  id = "workspace-preview-run-log",
  pollIntervalMs = RUN_LOG_STAGE_POLL_MS,
  retryDelayMs = ATTACH_RETRY_DELAY_MS,
}: {
  workspaceId: string;
  taskId: string;
  view: PreviewRunView;
  /** Whether the owner's VM runs now: the only time output can be attached. */
  vmRunning: boolean;
  id?: string;
  pollIntervalMs?: number;
  retryDelayMs?: number;
}) {
  const queryClient = useQueryClient();
  const { timelineState, fetchNextPage, hasNextPage, isFetchingNextPage } =
    useTaskTimeline(taskId);
  const live = !TERMINAL.has(view.status);
  // The API pages from the oldest entry forward; a run's lifecycle entries
  // are few, so every page is loaded.
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage) fetchNextPage();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);
  useEffect(() => {
    void queryClient.invalidateQueries({
      queryKey: ["task-timeline", taskId],
    });
  }, [queryClient, taskId, view.status, view.ready, view.closing]);
  useEffect(() => {
    if (!live) return;
    const timer = window.setInterval(() => {
      void queryClient.invalidateQueries({
        queryKey: ["task-timeline", taskId],
      });
    }, pollIntervalMs);
    return () => window.clearInterval(timer);
  }, [live, pollIntervalMs, queryClient, taskId]);

  const entries = useMemo(
    () =>
      timelineState.status === "ok"
        ? orderedUniqueEntries(timelineState.data.items)
        : [],
    [timelineState],
  );
  const stages = useMemo(() => previewStages(view, entries), [view, entries]);
  const failure = useMemo(() => previewFailure(entries), [entries]);
  // Once the run is known to have started it stays started for this attach:
  // a failed timeline refresh empties `entries` for a moment and must not
  // close an attachment or hold the next one (Codex review, 2026-10-27).
  const startedNow = useMemo(
    () => previewRunStarted(view, entries),
    [view, entries],
  );

  // The guest hosts the session only once the module starts, so a live run's
  // first attach is held until the timeline says it did (an attach before
  // that is always refused); a refusal after that is the small race with the
  // guest opening the PTY and is retried on a short cadence, bounded; for an
  // ended run (or once the retries are spent, as under an older dispatcher)
  // the dialog shows what outlives the VM, nothing more. Holding and
  // retrying happen on one mounted panel (`attach` toggles the socket, never
  // the surface), so the dialog's height and the terminal's DOM stay put
  // (`g5.preview.run-log.attach`). The progress is remembered for this task
  // and VM state only; a fresh VM attaches again. An attach that connected
  // is remembered too: the session the guest hosted outlives the run, so its
  // retained output stays viewable after the run ends.
  const attachKey = `${taskId}:${vmRunning}`;
  const [startedFor, setStartedFor] = useState<string | null>(null);
  if (startedNow && startedFor !== attachKey) setStartedFor(attachKey);
  const started = startedNow || startedFor === attachKey;
  const [progress, setProgress] = useState<{
    key: string;
    attempts: number;
    waiting: boolean;
    attached: boolean;
  } | null>(null);
  const here = progress?.key === attachKey ? progress : null;
  const onViewerState = useCallback(
    (state: TerminalViewerState) => {
      if (!state.rejected) {
        if (state.status === "Connected")
          setProgress((previous) =>
            previous?.key === attachKey
              ? previous.attached
                ? previous
                : { ...previous, waiting: false, attached: true }
              : { key: attachKey, attempts: 0, waiting: false, attached: true },
          );
        return;
      }
      setProgress((previous) => {
        const attempts =
          previous?.key === attachKey ? previous.attempts + 1 : 1;
        return {
          key: attachKey,
          attempts,
          waiting: live && attempts <= ATTACH_RETRY_LIMIT,
          attached: false,
        };
      });
    },
    [attachKey, live],
  );
  useEffect(() => {
    if (!here?.waiting) return;
    const timer = window.setTimeout(
      () =>
        setProgress((previous) =>
          previous?.key === attachKey
            ? { ...previous, waiting: false }
            : previous,
        ),
      retryDelayMs,
    );
    return () => window.clearTimeout(timer);
  }, [here?.waiting, here?.attempts, attachKey, retryDelayMs]);
  const retryPending = here?.waiting === true;
  const outputAvailable =
    vmRunning &&
    (here === null ||
      here.attached ||
      (live && here.attempts <= ATTACH_RETRY_LIMIT));
  // An ended run's session is attached at once; a live run's only once it
  // started, and not while a retry waits. An attachment this log holds is
  // never closed by what the timeline says: its socket decides its own end.
  const attach = !live || here?.attached === true || (started && !retryPending);
  const waitingForApplication = live && !here?.attached;

  return (
    <div
      id={id}
      data-testid="workspace-preview-run-log"
      className="overflow-hidden rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] text-xs"
    >
      <ol
        aria-label="Preview stages"
        className="flex flex-wrap items-center gap-y-1 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-2 text-[11px]"
      >
        {stages.map((stage, index) => (
          <li
            key={stage.key}
            data-testid={`workspace-preview-stage-${stage.key}`}
            data-state={stage.state}
            aria-current={stage.state === "current" ? "step" : undefined}
            className={classNames(
              "inline-flex items-center gap-1.5 whitespace-nowrap",
              index > 0 &&
                "before:mx-1.5 before:inline-block before:h-px before:w-4 before:bg-[var(--g5-stroke)] before:content-['']",
              stage.state === "done" && "text-[var(--g5-text1)]",
              stage.state === "current" &&
                "font-semibold text-[var(--g5-text0)]",
              stage.state === "pending" && "text-[var(--g5-text2)]",
              stage.state === "failed" && "text-[var(--g5-blocked)]",
            )}
          >
            <StageMark stage={stage} />
            {stage.label}
          </li>
        ))}
      </ol>
      {outputAvailable ? (
        <div className="h-72 min-h-0">
          <WorkspaceTerminalPanel
            workspaceId={workspaceId}
            enabled
            readOnly
            attach={attach}
            pendingStateText={
              waitingForApplication ? WAITING_FOR_APPLICATION_TEXT : null
            }
            terminalId={previewTerminalId(taskId)}
            sessionLabel={`preview · ${view.profile ?? "workspace default"}`}
            onViewerState={onViewerState}
          />
        </div>
      ) : (
        <p
          className="px-2.5 py-2 text-[11px] text-[var(--g5-text2)]"
          data-testid="workspace-preview-output-unavailable"
        >
          {vmRunning
            ? "Output is not available: the running VM no longer hosts this run's session, and output is not retained after the VM stops."
            : "Output is not retained after the VM stops."}
        </p>
      )}
      {timelineState.status === "error" ? (
        <p role="alert" className="px-2.5 py-2 text-[var(--g5-blocked)]">
          {timelineState.error.message}
        </p>
      ) : null}
      {!live && failure ? (
        <div
          role="status"
          data-testid="workspace-preview-run-failure"
          className="border-t border-[var(--g5-stroke)] px-2.5 py-2"
        >
          <p className="text-[var(--g5-blocked)]">{failure.headline}</p>
          {failure.stderrTail ? (
            <pre
              data-testid="workspace-preview-stderr-tail"
              className="mt-1 max-h-40 overflow-auto whitespace-pre rounded bg-[var(--g5-bg2)] px-1.5 py-1 font-mono text-[11px] text-[var(--g5-text1)]"
            >
              {failure.stderrTailTruncated
                ? `stderr (last 4 KiB):\n${failure.stderrTail}`
                : `stderr:\n${failure.stderrTail}`}
            </pre>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function StageMark({ stage }: { stage: PreviewStage }) {
  const base =
    "inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full border";
  switch (stage.state) {
    case "done":
      return (
        <span
          className={classNames(
            base,
            "border-[var(--g5-ok)] bg-[var(--g5-ok)]",
          )}
        >
          <Check size={9} strokeWidth={3} className="text-[var(--g5-bg0)]" />
        </span>
      );
    case "failed":
      return (
        <span
          className={classNames(
            base,
            "border-[var(--g5-blocked)] bg-[var(--g5-blocked)]",
          )}
        >
          <X size={9} strokeWidth={3} className="text-[var(--g5-bg0)]" />
        </span>
      );
    case "current":
      return stage.key === "serving" ? (
        <span
          className={classNames(
            base,
            "border-[var(--g5-ok)] bg-[var(--g5-ok)]",
          )}
        >
          <Check size={9} strokeWidth={3} className="text-[var(--g5-bg0)]" />
        </span>
      ) : (
        <span className={classNames(base, "border-[var(--g5-warn)]")}>
          <Loader2
            size={9}
            className="animate-spin text-[var(--g5-warn)] motion-reduce:animate-none"
          />
        </span>
      );
    default:
      return <span className={classNames(base, "border-[var(--g5-text2)]")} />;
  }
}
