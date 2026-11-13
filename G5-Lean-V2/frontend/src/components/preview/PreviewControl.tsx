import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ChevronDown,
  ExternalLink,
  Loader2,
  ScrollText,
  Square,
  X,
} from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState } from "react";

import {
  previewEndingSentence,
  previewRunHasLog,
} from "../../lib/previewRunWords";
import type {
  PreviewEndingDTO,
  PreviewOpenRequest,
  StartPreviewOutcome,
} from "../../lib/types";
import { useAuth } from "../../hooks/useAuth";
import {
  type PreviewApplicationTarget,
  usePreviewApplications,
} from "../../hooks/useTasks";
import { useWorkspace, useWorkspaceVm } from "../../hooks/useWorkspace";
import { api, type ApiError } from "../../lib/api";
import { randomUuid } from "../../lib/randomUuid";
import { classNames, Modal } from "../ui/UI";
import {
  liveGateCanStart,
  liveGateKind,
  liveGateReason,
  policyFreeGateKind,
  GatedControl,
  useWorkspaceLiveGate,
  type LiveGateKind,
} from "../../lib/workspaceLiveGate";
import { PreviewMonitorIcon } from "./PreviewMonitorIcon";
import { PreviewRunLog } from "./PreviewRunLog";
import {
  previewIndicatorState,
  previewIndicatorText,
} from "./previewIndicator";

export interface PreviewControlProps {
  workspaceId: string;
  isDirty: boolean;
  mode?: "unmerged" | "merged";
  mergeResult?: {
    preflightFingerprint: string;
    targetWorkspaceId?: string;
  };
  label?: string;
  triggerClassName?: string;
  triggerTestId?: string;
  /** Glyph size of the trigger (ribbon icon styles pass 22 or 18). */
  iconSize?: number;
  /**
   * Hide the trigger's text (icon-only ribbon style); the label stays the
   * accessible name and the tooltip.
   */
  labelHidden?: boolean;
  title?: string;
  disabled?: boolean;
  readOnly?: boolean;
  onOpenFileTab?: (filePath: string, branchId: string) => void;
  /**
   * An Activity or rail row asked for the dialog (ui
   * g5.ui.activity.preview-row): each new `seq` opens it, with the run log
   * when `log` is set. The dialog shows the workspace's current preview; the
   * row's `taskId` is the run the owner read about.
   */
  openRequest?: PreviewOpenRequest & { seq: number };
}

interface WorkspacePreview {
  workspaceId: string;
  taskId: string;
  status: string;
  repoId: string;
  profile: string | null;
  port: number;
  generation: number;
  ready: boolean;
  /** Close was requested; the guest is stopping the app and running cleanup. */
  closing: boolean;
  /** How a cancelled preview closed: "clean", "failed", "killed" or "lost". */
  close: string | null;
  /** How the run ended, in the Activity row's structure (g5.ui.activity.preview-row). */
  ending?: PreviewEndingDTO;
  /**
   * The queued task waits for the owner's Start to bring the VM up
   * (g5.preview.start-and-preview); it is dispatched once the VM runs.
   */
  pendingStart?: boolean;
}

const TERMINAL = new Set(["succeeded", "failed", "cancelled"]);

function applicationLabel(application: PreviewApplicationTarget): string {
  const config =
    application.source === "workspaceDefault"
      ? "Workspace default"
      : (application.profile ?? "Named profile");
  const detail = application.description?.trim() || application.command;
  return `${application.repoName} — ${config} — ${detail} (:${application.port})`;
}

/**
 * What the running preview is, for the state block: the profile name (or
 * "Workspace default") and, when the owner's application list knows it, the
 * profile's description. The list is loaded only while the owner could start
 * a preview, so a fixed-target preview or a failed discovery shows the name
 * alone; a viewer gets no control at all.
 */
function runningProfileLabel(
  current: Pick<WorkspacePreview, "repoId" | "profile">,
  applications: PreviewApplicationTarget[] | null,
): { name: string; description: string | null } {
  const name = current.profile ?? "Workspace default";
  const match = applications?.find(
    (item) =>
      item.repoId === current.repoId &&
      (item.profile ?? null) === (current.profile ?? null),
  );
  return { name, description: match?.description?.trim() || null };
}

export function PreviewControl({
  workspaceId,
  mode = "unmerged",
  mergeResult,
  label = "Preview",
  triggerClassName,
  iconSize = 12,
  labelHidden = false,
  triggerTestId = "preview-control-trigger",
  title,
  disabled = false,
  onOpenFileTab,
  openRequest,
}: PreviewControlProps) {
  const { user } = useAuth();
  const gate = useWorkspaceLiveGate();
  const workspace = useWorkspace(workspaceId);
  const vm = useWorkspaceVm(workspaceId);
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [selectedKey, setSelectedKey] = useState("");
  const [openError, setOpenError] = useState<string | null>(null);
  const [isOpening, setIsOpening] = useState(false);
  // The run log (g5.preview.run-log) opens in place; it closes with the dialog.
  const [logOpen, setLogOpen] = useState(false);
  const indicatorDescriptionId = useId();
  const titleId = useId();

  const detail = workspace.status === "ok" ? workspace.data : null;
  const isOwner = !!user?.id && detail?.ownerUserId === user.id;
  const running = vm.status === "ok" && vm.data.status === "running";
  // The VM-state kind the launcher answers to: the page's gate when there
  // is one, else the VM read with the real ownership.
  const vmKind: LiveGateKind = gate
    ? policyFreeGateKind(gate)
    : liveGateKind({
        isBase: false,
        isOwner,
        readOnly: false,
        freezing: false,
        vmStatus: vm.status === "ok" ? vm.data.status : null,
      });
  // Start workspace and preview (g5.preview.start-and-preview): while the
  // owner's VM is stopped or failed, one request records the Start and the
  // preview bound to it. While a Start is already under way the preview
  // waits for it under that consent. A merged preview keeps the running-VM
  // rule: it boots the base branch, not the workspace.
  const canCompoundStart = isOwner && !mergeResult && liveGateCanStart(vmKind);
  const waitingForStart =
    isOwner && !mergeResult && (vmKind === "starting" || vmKind === "queued");
  const launchAllowed = running || canCompoundStart || waitingForStart;
  const branches = useMemo(
    () =>
      (detail?.branches ?? [])
        .filter((branch) => branch.repoId !== detail?.docsRepoId)
        .map((branch) => ({
          workspaceId: branch.workspaceId,
          repoId: branch.repoId,
          repoName: branch.repoName,
          branchName: branch.branchName,
        })),
    [detail?.branches, detail?.docsRepoId],
  );
  // Without the running VM the list comes from the configuration committed
  // on the branch; the live checkout is the source again once it runs.
  const applications = usePreviewApplications(
    branches,
    // Discovery waits for the VM read: the source follows it.
    open && isOwner && launchAllowed && !mergeResult && vm.status === "ok",
    mode,
    running ? "live" : "committed",
  );
  const preview = useQuery<WorkspacePreview | null, Error>({
    queryKey: ["workspace-preview", workspaceId],
    queryFn: ({ signal }) =>
      api.get<WorkspacePreview | null>(
        `/api/v1/workspaces/${workspaceId}/preview`,
        { signal },
      ),
    // The trigger shows the preview's state while the dialog is closed too,
    // so the owner need not open it to learn whether the preview serves: a
    // light read at the VM poll's cadence while the VM runs, shared by every
    // mounted trigger; the open dialog follows a live preview more closely.
    // A stopped or starting VM is read once, so a preview waiting for the
    // owner's Start (`pendingStart`) shows after a reload too, including
    // before placement while the VM still reads as stopped.
    enabled: isOwner && !mergeResult,
    refetchInterval: (query) => {
      const status = query.state.data?.status;
      const live = !!status && !TERMINAL.has(status);
      if (!running) {
        // A stopped VM is not polled, dialog open or not; the stop itself
        // re-reads once (below) so a preview the loss ended shows as ended.
        // A preview waiting for a Start is followed until the VM is up or
        // the Start ends.
        const pending = live && !!query.state.data?.pendingStart;
        if (pending) return open ? 2_000 : 5_000;
        return false;
      }
      if (open) return live ? 1_000 : false;
      return 5_000;
    },
  });
  useEffect(() => {
    if (!running && open && isOwner && !mergeResult) {
      void queryClient.invalidateQueries({
        queryKey: ["workspace-preview", workspaceId],
      });
    }
  }, [running, open, isOwner, mergeResult, queryClient, workspaceId]);
  const selected =
    applications.status === "ok"
      ? (applications.data.find((item) => item.key === selectedKey) ??
        (applications.data.length === 1 ? applications.data[0] : null))
      : null;
  const target: { repoId: string; profile: string | null } | null = selected;

  const start = useMutation<
    WorkspacePreview & StartPreviewOutcome,
    Error,
    { repoId: string; profile: string | null; startCompute: boolean }
  >({
    mutationFn: (application) =>
      api.post<WorkspacePreview & StartPreviewOutcome>(
        `/api/v1/workspaces/${workspaceId}/preview`,
        {
          repoId: application.repoId,
          profile: application.profile,
          idempotencyKey: randomUuid(),
          // The clearly labelled compound action records Start consent
          // with the preview (g5.lifecycle.start).
          ...(application.startCompute ? { startCompute: true } : {}),
        },
      ),
    onSuccess: ({ startCompute, ...value }) => {
      queryClient.setQueryData(["workspace-preview", workspaceId], value);
      queryClient.invalidateQueries({ queryKey: ["tasks", workspaceId] });
      // A Start workspace and preview recorded a Start: re-read the VM
      // state so the header, banner and gate follow.
      if (startCompute) {
        void queryClient.invalidateQueries({ queryKey: ["workspace-vm"] });
      }
    },
  });
  const cancel = useMutation({
    mutationFn: () =>
      api.post(`/api/v1/workspaces/${workspaceId}/preview/cancel`, {}),
    onSettled: () => {
      queryClient.invalidateQueries({
        queryKey: ["workspace-preview", workspaceId],
      });
      queryClient.invalidateQueries({ queryKey: ["tasks", workspaceId] });
    },
  });

  // Preview starts a process in the VM. A stopped, failed or starting VM no
  // longer closes the launcher: the dialog starts the workspace with the
  // preview (g5.preview.start-and-preview). Every other non-live kind
  // (reader, Base, stopping, unknown) names why it cannot now
  // (`g5.ui.unavailable-actions`); outside a gate the VM read decides. The
  // server checks live ownership, not the mutation policy, so a read-only
  // or freezing owner keeps Preview (`g5.workspace.ui.gates`).
  const gateReason = launchAllowed
    ? null
    : liveGateReason(vmKind, "preview", {
        workspaceNoun: gate?.workspaceNoun,
      });
  const triggerDisabled = disabled || !launchAllowed || gateReason !== null;
  const current = preview.data;
  // A stopped VM has no preview whatever the cached read says, except one
  // still waiting for the owner's Start.
  const indicator =
    running || current?.pendingStart ? previewIndicatorState(current) : "none";
  const indicatorText = previewIndicatorText(indicator, current);
  const runningProfile = current
    ? runningProfileLabel(
        current,
        applications.status === "ok" ? applications.data : null,
      )
    : null;
  const active = current && !TERMINAL.has(current.status);
  const previewSettled = preview.isSuccess || preview.isError;
  // A terminal preview task stays the workspace's current preview until the
  // next start succeeds it. Its failure is otherwise invisible here: the
  // dialog silently falls back to the launcher while the task's output only
  // lives in the task drawer (QA, 2026-09-23: every `dev-up` attempt exited 1
  // and the dialog kept offering "Start preview" with no explanation).
  // A queued run cancelled by a VM or host loss ends `cancelled` with a
  // failed ending (`g5.ui.activity.preview-row`); it is a failed last run too.
  // A run that never started because its Start ended (`did_not_start`) is
  // a failed last run too: the launcher says why and offers Start again.
  const lastFailed =
    current &&
    (current.status === "failed" ||
      (current.status === "cancelled" &&
        (current.ending?.kind === "failed" ||
          current.ending?.kind === "did_not_start")))
      ? current
      : null;
  // A run lost with its VM, or refused admission, left no output to show
  // (`g5.preview.run-log.ended`); the notice then carries no Log toggle.
  const lastFailedHasLog =
    !!lastFailed &&
    (!lastFailed.ending || previewRunHasLog({ ending: lastFailed.ending }));
  // A close whose cleanup failed or overran its bound is reported, not
  // hidden: the app may have left state behind in the shared VM.
  const lastClosed =
    current && current.status === "cancelled" && current.close ? current : null;
  // A close cut short by a lost VM is told apart (`g5.ui.activity.preview-row`):
  // nothing of it can still run, and no output survived to show.
  const lastCloseLost =
    lastClosed && lastClosed.close === "lost" ? lastClosed : null;
  const lastCloseFailed =
    lastClosed && lastClosed.close !== "clean" && !lastCloseLost
      ? lastClosed
      : null;
  // The action row's three readings of the live preview: Open is offered
  // only while the application serves and no stop is in flight.
  const starting = !!active && !current.ready && !current.closing;
  // The task waits for the workspace itself to come up; its application
  // has not been started yet.
  const pendingStart = !!active && !!current.pendingStart && !current.closing;
  const openReady = !!active && current.ready && !current.closing;
  const stopping = !!active && (cancel.isPending || current.closing);
  // A log is rendered only in the active branch and under a failure notice
  // in the launcher; the shell's width follows the rendered log, not the
  // toggle, so a clean stop returns the dialog to its narrow width
  // (`g5.preview.dialog-controls`).
  // Whether the notice shown in the launcher has a log behind it: a failed
  // run only when it kept one, a failed close always.
  const noticeHasLog = lastFailed ? lastFailedHasLog : !!lastCloseFailed;
  const logShown = logOpen && (!!active || noticeHasLog);
  useEffect(() => {
    if (!open) {
      setLogOpen(false);
      return;
    }
    // The toggle belongs to the run it was opened for: once that run ends
    // with nothing to report, the next Start does not auto-expand the log.
    // A failed run keeps it, so its notice's log stays open.
    if (!active && !noticeHasLog) setLogOpen(false);
  }, [open, active, noticeHasLog]);
  // A row's request (ui g5.ui.activity.preview-row) opens the dialog at
  // once; its log waits for a run or a notice to show it under (`logShown`),
  // because the opening effect above clears the toggle while neither is
  // loaded. Declared after that effect so the same commit cannot undo it;
  // closing the dialog drops a request its log never applied.
  const requestedLogRef = useRef(false);
  // The run the row described; the dialog shows the workspace's current
  // preview, so the log opens only when that is the same run.
  const requestedTaskIdRef = useRef<string | null>(null);
  // When the request arrived: the cached preview can lag behind Activity,
  // so the request is answered only by a read fetched after it.
  const requestedAtRef = useRef(0);
  const currentTaskId = current?.taskId ?? null;
  const previewUpdatedAt = preview.dataUpdatedAt;
  // Bumped by each request so this effect also runs when the dialog was
  // already open and `setOpen(true)` changed nothing.
  const [requestTick, setRequestTick] = useState(0);
  useEffect(() => {
    if (!open) {
      requestedLogRef.current = false;
      return;
    }
    if (!requestedLogRef.current) return;
    const fresh = previewSettled && previewUpdatedAt >= requestedAtRef.current;
    if (requestedTaskIdRef.current === currentTaskId) {
      if (active || noticeHasLog) {
        requestedLogRef.current = false;
        setLogOpen(true);
      } else if (fresh) {
        // The launcher answered the request; a run the owner starts next
        // keeps the usual closed toggle.
        requestedLogRef.current = false;
      }
    } else if (fresh) {
      // A read fetched after the request names another run: the row's run
      // is not the current preview, so its log cannot be shown.
      requestedLogRef.current = false;
    }
  }, [
    open,
    active,
    noticeHasLog,
    requestTick,
    previewSettled,
    previewUpdatedAt,
    currentTaskId,
  ]);
  // A request already present when this control mounts was answered by an
  // earlier instance (the header remounts when the workspace reloads); only
  // a sequence that arrives while mounted opens the dialog.
  const openRequestSeq = openRequest?.seq;
  const openRequestLog = openRequest?.log ?? false;
  const openRequestTaskId = openRequest?.taskId ?? null;
  const answeredSeqRef = useRef(openRequestSeq);
  useEffect(() => {
    if (
      openRequestSeq === undefined ||
      openRequestSeq === answeredSeqRef.current
    )
      return;
    answeredSeqRef.current = openRequestSeq;
    requestedTaskIdRef.current = openRequestTaskId;
    requestedLogRef.current = openRequestLog;
    requestedAtRef.current = Date.now();
    // Re-read the preview now rather than at the next poll, so the request
    // is resolved against the run Activity just described.
    void queryClient.invalidateQueries({
      queryKey: ["workspace-preview", workspaceId],
    });
    setOpen(true);
    setRequestTick((tick) => tick + 1);
  }, [
    openRequestSeq,
    openRequestLog,
    openRequestTaskId,
    queryClient,
    workspaceId,
  ]);

  if (!isOwner || mergeResult) return null;

  const handleOpenPreview = async () => {
    if (!current || isOpening) return;
    setIsOpening(true);
    setOpenError(null);
    const tab = window.open("about:blank", "_blank");
    if (tab) tab.opener = null;
    try {
      const { previewUrl } = await api.post<{ previewUrl: string }>(
        `/api/v1/preview/session/${current.taskId}`,
      );
      if (tab) tab.location.replace(previewUrl);
      else window.open(previewUrl, "_blank", "noopener,noreferrer");
    } catch (error) {
      tab?.close();
      setOpenError(
        (error as ApiError)?.message ?? "Failed to authorize the preview tab",
      );
    } finally {
      setIsOpening(false);
    }
  };

  return (
    <>
      <GatedControl
        verb="preview"
        policyFree
        reason={gateReason}
        testId={`${triggerTestId}-hint`}
        placement="fixed"
      >
        <button
          type="button"
          data-testid={triggerTestId}
          disabled={triggerDisabled}
          aria-label={labelHidden ? label : undefined}
          // The state the glyph shows is also said: colour is never the
          // only cue, and the name stays the plain label.
          aria-describedby={indicatorText ? indicatorDescriptionId : undefined}
          data-preview-state={indicator}
          title={
            labelHidden
              ? // Icon-only ribbon style: the tooltip names the control and
                // appends the gate reason or the preview's state.
                gateReason
                ? `${label} — ${gateReason}`
                : indicatorText
                  ? `${label} — ${indicatorText}`
                  : (title ?? label)
              : gateReason
                ? (title ?? gateReason)
                : indicatorText
                  ? `${title ?? label} — ${indicatorText}`
                  : (title ??
                    (running
                      ? "Start or view the workspace preview"
                      : canCompoundStart
                        ? "Start the workspace and launch a preview"
                        : waitingForStart
                          ? "Launch a preview once the workspace is running"
                          : "Start the workspace before launching a preview"))
          }
          onClick={() => setOpen(true)}
          className={
            triggerClassName ??
            "inline-flex items-center gap-1.5 rounded px-2 py-1 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] disabled:cursor-not-allowed disabled:opacity-50"
          }
        >
          <PreviewMonitorIcon size={iconSize} state={indicator} />
          {labelHidden ? null : <span>{label}</span>}
        </button>
        {indicatorText ? (
          <span id={indicatorDescriptionId} className="sr-only">
            {indicatorText}
          </span>
        ) : null}
      </GatedControl>
      {open && (
        // One dialog shell: the shared Modal owns the border, padding, width
        // and scrolling, so nothing inside is wider than the panel and the ×
        // is the one dismiss (issue #786; `g5.preview.dialog-controls`).
        <Modal
          onClose={() => setOpen(false)}
          labelledBy={titleId}
          // A rendered log widens the one shell; the terminal inside is the
          // scroll region (g5.preview.dialog-controls).
          size={logShown ? "xl" : "sm"}
          testId="workspace-preview-dialog"
        >
          <div className="mb-4 flex items-center justify-between gap-2">
            <h2
              id={titleId}
              className="text-sm font-semibold text-[var(--g5-text0)]"
            >
              Workspace preview
            </h2>
            <button
              type="button"
              data-testid="workspace-preview-close"
              aria-label="Close dialog"
              title="Close this dialog; the preview keeps running"
              onClick={() => setOpen(false)}
              className="rounded p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]"
            >
              <X size={14} />
            </button>
          </div>

          {active ? (
            <div className="flex flex-col gap-3">
              {runningProfile ? (
                <p
                  className="text-xs text-[var(--g5-text1)]"
                  data-testid="workspace-preview-profile"
                >
                  <span className="font-medium">{runningProfile.name}</span>
                  {runningProfile.description ? (
                    <span className="text-[var(--g5-text2)]">
                      {" "}
                      · {runningProfile.description}
                    </span>
                  ) : null}
                </p>
              ) : null}
              {/* The one place the lifecycle is spelled out: a live region
                  so the change from starting to serving is announced, with
                  a glyph beside it so colour is never the only cue
                  (`g5.preview.dialog-controls`). */}
              <p
                role="status"
                data-testid="workspace-preview-state"
                className="flex items-start gap-2 text-xs text-[var(--g5-text1)]"
              >
                <span
                  aria-hidden="true"
                  className="mt-px inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center"
                >
                  {openReady ? (
                    <span className="h-2 w-2 rounded-full bg-[var(--g5-ok)]" />
                  ) : (
                    <Loader2
                      size={14}
                      className="animate-spin text-[var(--g5-warn)]"
                    />
                  )}
                </span>
                <span>
                  {current.closing
                    ? "Stopping the application and running its cleanup command…"
                    : current.ready
                      ? `Serving on port ${current.port} · ready to open`
                      : pendingStart
                        ? vmKind === "queued"
                          ? "Waiting for capacity to start the workspace… the preview launches once it is running"
                          : "Starting the workspace… the preview launches once it is running"
                        : "Starting the configured application…"}
                </span>
              </p>
              {/* Open is green only while it can open. Not ready, it sits
                  on the neutral surface with its own label while starting
                  ("Starting…"), so readiness never rests on opacity or
                  colour alone; Stop stays a full-contrast action beside it
                  and carries the pending label while the stop runs. The
                  row wraps rather than overflow a narrow panel. */}
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  data-testid="workspace-preview-open"
                  disabled={!openReady || isOpening}
                  aria-busy={starting || isOpening || undefined}
                  title={
                    current.closing
                      ? "The preview is stopping"
                      : starting
                        ? "The application is still starting"
                        : undefined
                  }
                  onClick={() => void handleOpenPreview()}
                  className={classNames(
                    "inline-flex min-w-0 flex-1 basis-44 items-center justify-center gap-1.5 rounded px-3 py-1.5 text-xs font-medium",
                    openReady
                      ? "bg-[var(--g5-ok)] text-[var(--g5-bg0)] disabled:cursor-progress"
                      : "cursor-not-allowed border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] text-[var(--g5-text2)]",
                  )}
                >
                  {starting || isOpening ? (
                    <Loader2 size={12} className="animate-spin" />
                  ) : (
                    <ExternalLink size={12} />
                  )}
                  {starting ? "Starting…" : "Open preview"}
                </button>
                <button
                  type="button"
                  data-testid="workspace-preview-stop"
                  disabled={stopping}
                  aria-busy={stopping || undefined}
                  title={
                    pendingStart
                      ? "Cancel the preview; the workspace keeps starting"
                      : "Stop the application and run its cleanup command"
                  }
                  onClick={() => cancel.mutate()}
                  className="inline-flex items-center gap-1.5 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-1.5 text-xs font-medium text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)] disabled:cursor-progress disabled:text-[var(--g5-text2)] disabled:hover:bg-[var(--g5-bg1)]"
                >
                  {stopping ? (
                    <Loader2 size={12} className="animate-spin" />
                  ) : (
                    <Square size={12} className="text-[var(--g5-blocked)]" />
                  )}
                  {stopping ? "Stopping…" : "Stop preview"}
                </button>
                <button
                  type="button"
                  data-testid="workspace-preview-log-toggle"
                  aria-expanded={logOpen}
                  aria-controls="workspace-preview-run-log"
                  title="Show the run's stages and the application's live output"
                  onClick={() => setLogOpen((value) => !value)}
                  className={classNames(
                    "inline-flex items-center gap-1 rounded border border-[var(--g5-stroke)] px-2 py-1.5 text-xs font-medium text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]",
                    logOpen && "bg-[var(--g5-bg2)] text-[var(--g5-text0)]",
                  )}
                >
                  <ScrollText size={12} />
                  Log
                  <ChevronDown
                    size={10}
                    className={classNames(
                      "transition-transform",
                      logOpen && "rotate-180",
                    )}
                  />
                </button>
              </div>
              {logOpen ? (
                <PreviewRunLog
                  workspaceId={workspaceId}
                  taskId={current.taskId}
                  view={current}
                  vmRunning={running}
                />
              ) : null}
              <p className="text-[11px] text-[var(--g5-text2)]">
                <span className="font-medium text-[var(--g5-text1)]">
                  Close dialog
                </span>{" "}
                (×) keeps the preview running.{" "}
                <span className="font-medium text-[var(--g5-text1)]">
                  Stop preview
                </span>{" "}
                stops the application and runs its cleanup command. One preview
                runs at a time.
              </p>
            </div>
          ) : (
            <div className="flex flex-col gap-3">
              {lastFailed && (
                <p
                  role="status"
                  data-testid="workspace-preview-last-failed"
                  className="rounded border border-[var(--g5-blocked)] px-2 py-1.5 text-xs text-[var(--g5-blocked)]"
                >
                  The last preview attempt
                  {lastFailed.profile ? ` (${lastFailed.profile})` : ""} on port{" "}
                  {lastFailed.port} failed.{" "}
                  {lastFailed.ending
                    ? `${previewEndingSentence(lastFailed.ending)}.`
                    : "Its command exited with an error."}{" "}
                  {lastFailedHasLog ? (
                    <LogToggleLink open={logOpen} onToggle={setLogOpen} />
                  ) : null}
                </p>
              )}
              {lastFailed && lastFailedHasLog && logOpen ? (
                <PreviewRunLog
                  workspaceId={workspaceId}
                  taskId={lastFailed.taskId}
                  view={lastFailed}
                  vmRunning={running}
                />
              ) : null}
              {lastCloseFailed && (
                <p
                  role="status"
                  data-testid="workspace-preview-last-close-failed"
                  className="rounded border border-[var(--g5-blocked)] px-2 py-1.5 text-xs text-[var(--g5-blocked)]"
                >
                  The last preview
                  {lastCloseFailed.profile
                    ? ` (${lastCloseFailed.profile})`
                    : ""}{" "}
                  closed, but its cleanup{" "}
                  {lastCloseFailed.close === "killed"
                    ? "did not finish in time and was killed"
                    : "command failed"}
                  . Its leftover state may still be running in the workspace.{" "}
                  <LogToggleLink open={logOpen} onToggle={setLogOpen} />
                </p>
              )}
              {lastCloseFailed && logOpen ? (
                <PreviewRunLog
                  workspaceId={workspaceId}
                  taskId={lastCloseFailed.taskId}
                  view={lastCloseFailed}
                  vmRunning={running}
                />
              ) : null}
              {lastCloseLost && (
                <p
                  role="status"
                  data-testid="workspace-preview-last-close-lost"
                  className="rounded border border-[var(--g5-stroke)] px-2 py-1.5 text-xs text-[var(--g5-text1)]"
                >
                  The last preview
                  {lastCloseLost.profile
                    ? ` (${lastCloseLost.profile})`
                    : ""}{" "}
                  was closed, but the workspace VM was lost before its cleanup
                  finished. Nothing of it is still running, and no output was
                  retained.
                </p>
              )}
              <label
                htmlFor="workspace-preview-application"
                className="text-xs font-medium text-[var(--g5-text1)]"
              >
                Preview application
              </label>
              {applications.status === "loading" ? (
                <span className="text-xs text-[var(--g5-text2)]">
                  Loading configured applications…
                </span>
              ) : applications.status === "error" ? (
                <p role="alert" className="text-xs text-[var(--g5-blocked)]">
                  {applications.error.message}
                </p>
              ) : applications.data.length === 0 ? (
                <p className="text-xs text-[var(--g5-text2)]">
                  Add a preview block to .g5/workspace.yaml or a named preview
                  profile, then save it before launching.
                </p>
              ) : (
                <select
                  id="workspace-preview-application"
                  aria-label="Preview application"
                  value={selected?.key ?? ""}
                  onChange={(event) => setSelectedKey(event.target.value)}
                  className="rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2 py-1.5 text-xs text-[var(--g5-text0)]"
                >
                  {applications.data.length > 1 && (
                    <option value="">Choose an application…</option>
                  )}
                  {applications.data.map((application) => (
                    <option key={application.key} value={application.key}>
                      {applicationLabel(application)}
                    </option>
                  ))}
                </select>
              )}
              {!running && applications.status === "ok" ? (
                <p
                  data-testid="workspace-preview-committed-note"
                  className="text-[11px] text-[var(--g5-text2)]"
                >
                  Applications as committed on the branch; changes saved in the
                  workspace apply once it runs.
                </p>
              ) : null}
              {selected && onOpenFileTab && (
                <button
                  type="button"
                  data-testid="workspace-preview-open-profile"
                  onClick={() =>
                    onOpenFileTab(
                      selected.profile
                        ? `.g5/preview_profiles/${selected.profile}.yaml`
                        : ".g5/workspace.yaml",
                      selected.repoId,
                    )
                  }
                  className="self-start text-xs text-[var(--g5-active)]"
                >
                  Open profile in editor
                </button>
              )}
              <GatedControl
                verb="preview"
                policyFree
                reason={gateReason}
                testId="workspace-preview-start-hint"
              >
                <button
                  type="button"
                  data-testid="workspace-preview-start"
                  // The dialog may outlive the VM: the gate is applied at
                  // submission too (`g5.workspace.ui.unavailable`).
                  disabled={
                    !target ||
                    start.isPending ||
                    !launchAllowed ||
                    gateReason !== null
                  }
                  title={
                    gateReason ??
                    (canCompoundStart
                      ? "Start the workspace, then launch this application"
                      : waitingForStart
                        ? "Launch this application once the workspace is running"
                        : undefined)
                  }
                  onClick={() =>
                    target &&
                    launchAllowed &&
                    gateReason === null &&
                    start.mutate({
                      ...target,
                      startCompute: canCompoundStart,
                    })
                  }
                  className={classNames(
                    "inline-flex items-center justify-center gap-1.5 rounded px-3 py-1.5 text-xs",
                    "bg-[var(--g5-active)] text-white disabled:cursor-not-allowed disabled:opacity-50",
                  )}
                >
                  {start.isPending && (
                    <Loader2 size={12} className="animate-spin" />
                  )}
                  {canCompoundStart
                    ? "Start workspace and preview"
                    : waitingForStart
                      ? "Preview once running"
                      : "Start preview"}
                </button>
              </GatedControl>
            </div>
          )}

          {(start.error || cancel.error || preview.error || openError) && (
            <p role="alert" className="mt-3 text-xs text-[var(--g5-blocked)]">
              {start.error?.message ??
                cancel.error?.message ??
                preview.error?.message ??
                openError}
            </p>
          )}
        </Modal>
      )}
    </>
  );
}

/** "View log" inside a failure notice: the failed run's log opens in place. */
function LogToggleLink({
  open,
  onToggle,
}: {
  open: boolean;
  onToggle: (open: boolean) => void;
}) {
  return (
    <button
      type="button"
      data-testid="workspace-preview-log-toggle"
      aria-expanded={open}
      aria-controls="workspace-preview-run-log"
      onClick={() => onToggle(!open)}
      className="underline underline-offset-2"
    >
      {open ? "Hide log" : "View log"}
    </button>
  );
}
