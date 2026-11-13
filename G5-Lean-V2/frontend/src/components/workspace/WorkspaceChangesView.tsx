import {
  AlertCircle,
  FileCode2,
  GitPullRequestArrow,
  RefreshCw,
  Undo2,
  Upload,
} from "lucide-react";
import { useState } from "react";
import {
  useWorkspaceChanges,
  useWorkspaceChangesAction,
} from "../../hooks/useFiles";
import { useWorkspaceReviewContext } from "../../hooks/useWorkspaceReview";
import { CommitChangesButton } from "./CommitChangesButton";
import {
  liveGateReason,
  GatedControl,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";

const LIVE_CHANGES_POLL_INTERVAL_MS = 5_000;

export function WorkspaceChangesView({
  workspaceId,
  selectedFileKey,
}: {
  workspaceId: string;
  selectedFileKey?: string;
}) {
  // The open Changes view tracks the live checkout: a terminal checkout or
  // an agent turn can change the association at any time.
  const changes = useWorkspaceChanges(workspaceId, {
    refetchIntervalMs: LIVE_CHANGES_POLL_INTERVAL_MS,
  });
  const action = useWorkspaceChangesAction(workspaceId);
  // A review copy is saved by Push review; the commit action refuses it,
  // so its Changes view offers no Commit. Nothing is offered until the
  // context is known; if it cannot be loaded the button stays, and the
  // action itself still refuses a copy.
  const reviewContext = useWorkspaceReviewContext(workspaceId);
  const gate = useWorkspaceLiveGate();
  // The page's combined identity wins when provided: it also reads the
  // workspace list, so a failed context refresh cannot turn a copy back into
  // an ordinary workspace; nothing is offered until the identity is known.
  const reviewCopy = gate
    ? gate.reviewCopy || !gate.reviewIdentityKnown
    : reviewContext.status === "loading" ||
      (reviewContext.status === "ok" && reviewContext.data.asReview != null);
  const [actionError, setActionError] = useState<string | null>(null);
  // The page's gate, when present, outranks a Changes read that may lag a
  // stop or a freeze; attachment still counts, since the server runs these
  // in the attached checkout.
  const attached = changes.status === "ok" && changes.data.attached;
  const offered = gate ? gate.canWrite : attached;
  const liveBlockedReason = gate
    ? liveGateReason(gate.kind, "commit or push", {
        workspaceNoun: gate.workspaceNoun,
      })
    : null;
  const liveBlocked = liveBlockedReason !== null || !attached;

  if (changes.status === "loading") {
    return (
      <div
        data-testid="workspace-changes-view"
        className="flex h-full items-center justify-center gap-2 text-sm text-[var(--g5-text2)]"
      >
        <RefreshCw className="h-4 w-4 animate-spin" />
        Loading changes…
      </div>
    );
  }
  if (changes.status === "error") {
    return (
      <div
        data-testid="workspace-changes-view"
        className="flex h-full items-center justify-center gap-2 text-sm text-[var(--g5-blocked)]"
      >
        <AlertCircle className="h-4 w-4" />
        Could not load workspace changes.
      </div>
    );
  }

  const rows = changes.data.members.flatMap((member) =>
    (member.dirty ?? []).map((change) => ({
      key: `${member.repo_id}:${change.path}`,
      repo: member.root_path || member.repo_id,
      ...change,
    })),
  );
  const mismatched = changes.data.members.filter((member) =>
    member.errors?.some((error) =>
      [
        "BRANCH_ASSOCIATION_MISMATCH",
        "DETACHED_HEAD",
        "INTERRUPTED_SEQUENCER",
      ].includes(error),
    ),
  );
  const runAction = async (requested: "push" | "update" | "switch-back") => {
    // Every action runs in the attached checkout; the page may have learned
    // of a stop since the control was rendered.
    if (liveBlocked || !offered) return;
    setActionError(null);
    try {
      const result = await action.mutateAsync({ action: requested });
      if (result.status === "failed") {
        setActionError(
          `Changes ${requested} did not complete for every repository.`,
        );
      }
    } catch (error) {
      setActionError(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <div
      data-testid="workspace-changes-view"
      className="h-full overflow-auto bg-[var(--g5-bg0)] p-4"
    >
      <div className="mb-3 flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold text-[var(--g5-text0)]">
          Changes
        </h2>
        <div className="flex items-center gap-2">
          <span
            data-testid="workspace-changes-source"
            className="text-xs text-[var(--g5-text2)]"
          >
            {changes.data.source === "live"
              ? "Live workspace"
              : "Durable snapshot"}
          </span>
          {changes.data.attached && reviewCopy && (
            <span
              className="text-xs text-[var(--g5-text2)]"
              data-testid="workspace-changes-review-copy"
            >
              A review copy is saved and published by Push review.
            </span>
          )}
          {!reviewCopy && offered && (
            // Update, push and commit run in the attached workspace. With a
            // page gate, durable authority shows them and live attachment
            // enables them; while the owner's VM is not running they stay,
            // disabled with the reason and a Start
            // (`g5.workspace.ui.unavailable`). Outside a page gate only an
            // attached workspace offers them, as before.
            <>
              <button
                type="button"
                data-testid="workspace-changes-update"
                disabled={
                  liveBlocked || action.isLoading || mismatched.length > 0
                }
                title={liveBlockedReason ?? undefined}
                onClick={() => void runAction("update")}
                className="flex items-center gap-1 rounded px-2 py-1 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
              >
                <GitPullRequestArrow className="h-3.5 w-3.5" /> Update from
                origin
              </button>
              <GatedControl
                verb="push"
                reason={liveBlocked ? (liveBlockedReason ?? null) : null}
                testId="workspace-changes-start-hint"
              >
                <button
                  type="button"
                  data-testid="workspace-changes-push"
                  disabled={
                    liveBlocked || action.isLoading || mismatched.length > 0
                  }
                  title={liveBlockedReason ?? undefined}
                  onClick={() => void runAction("push")}
                  className="flex items-center gap-1 rounded px-2 py-1 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
                >
                  <Upload className="h-3.5 w-3.5" /> Push existing commits
                </button>
              </GatedControl>
              <GatedControl
                verb="commit"
                reason={liveBlocked ? (liveBlockedReason ?? null) : null}
                testId="workspace-changes-commit-hint"
              >
                <CommitChangesButton
                  workspaceId={workspaceId}
                  disabledReason={liveBlockedReason ?? undefined}
                />
              </GatedControl>
            </>
          )}
        </div>
      </div>
      {mismatched.length > 0 && (
        <div
          data-testid="workspace-association-mismatch"
          className="mb-3 flex items-center gap-2 rounded border border-[var(--g5-warn)]/50 bg-[var(--g5-warn)]/10 p-2 text-xs text-[var(--g5-text0)]"
        >
          <AlertCircle className="h-4 w-4 shrink-0 text-[var(--g5-warn)]" />
          <span className="flex-1">
            {mismatched.length} repository checkout
            {mismatched.length === 1 ? " is" : "s are"} no longer on the
            workspace branch.
          </span>
          <button
            type="button"
            data-testid="workspace-changes-switch-back"
            disabled={liveBlocked || !offered || action.isLoading}
            title={liveBlockedReason ?? undefined}
            onClick={() => void runAction("switch-back")}
            className="flex items-center gap-1 rounded px-2 py-1 text-[var(--g5-active)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
          >
            <Undo2 className="h-3.5 w-3.5" /> Switch back
          </button>
        </div>
      )}
      {(actionError || action.error) && (
        <p role="alert" className="mb-3 text-xs text-[var(--g5-blocked)]">
          {actionError || action.error?.message}
        </p>
      )}
      {changes.data.source === "live" && (
        <ul className="mb-3 space-y-1" aria-label="Repository branch status">
          {changes.data.members.map((member) => (
            <li
              key={member.repo_id}
              data-testid="workspace-change-member-status"
              className="flex gap-2 text-xs text-[var(--g5-text2)]"
            >
              <span className="font-medium text-[var(--g5-text1)]">
                {member.root_path || member.repo_id}
              </span>
              <span>{member.head || "detached"}</span>
              {member.against_upstream && (
                <span>
                  {member.against_upstream.ahead ?? "?"} ahead ·{" "}
                  {member.against_upstream.behind ?? "?"} behind parent
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
      {rows.length === 0 ? (
        <p className="text-sm text-[var(--g5-text2)]">
          No uncommitted changes.
        </p>
      ) : (
        <ul className="space-y-1" aria-label="Workspace changes">
          {rows.map((row) => (
            <li
              key={row.key}
              data-testid="workspace-change-row"
              className={`flex items-center gap-2 rounded px-2 py-1.5 text-sm ${
                selectedFileKey === row.key
                  ? "bg-[var(--g5-bg2)] text-[var(--g5-text0)]"
                  : "text-[var(--g5-text1)]"
              }`}
            >
              <FileCode2 className="h-4 w-4 shrink-0 text-[var(--g5-text2)]" />
              <span className="min-w-0 flex-1 truncate">{row.path}</span>
              <span className="shrink-0 text-xs text-[var(--g5-text2)]">
                {row.status || "changed"} · {row.repo}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
