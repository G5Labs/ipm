/**
 * CommitChangesButton — the owner commits the running workspace's working
 * tree with a message from Source Control, and the commit is pushed.
 * The button reads the live changes report for the
 * count of uncommitted files. When the tree is clean but the branch still
 * holds commits the internal mirror has not seen (a commit whose push
 * failed, or a native commit), the button becomes "Push N commits" and runs
 * the push alone, so recovery never depends on having something new to
 * commit. It is disabled while the VM is not running or there is nothing to
 * commit or push.
 */

import React, { useId, useRef, useState } from "react";
import { GitCommitHorizontal, Loader2 } from "lucide-react";
import { Button } from "../ui/UI";
import { Modal } from "../ui/Modal";
import {
  useWorkspaceChanges,
  useWorkspaceChangesAction,
  type WorkspaceChanges,
} from "../../hooks/useFiles";

const LIVE_CHANGES_POLL_INTERVAL_MS = 5_000;

export interface CommitChangesButtonProps {
  workspaceId: string;
  /** Prefills the message field; the owner can still change it. */
  defaultMessage?: string;
  /** Why the button is disabled beyond what the changes report says. */
  disabledReason?: string;
}

/** Uncommitted files across every repository of the report. */
function uncommittedFileCount(report: WorkspaceChanges): number {
  return report.members.reduce(
    (total, member) => total + (member.dirty?.length ?? 0),
    0,
  );
}

/** Commits not yet on the internal mirror, across every repository. */
function unpushedCommitCount(report: WorkspaceChanges): number {
  return report.members.reduce(
    (total, member) => total + (member.against_internal?.ahead ?? 0),
    0,
  );
}

/** "Committed 3 files as ab12cd3", from a commit action's report. */
function commitOutcomeText(report: WorkspaceChanges): string {
  const members = report.members as Array<{
    committed?: boolean;
    files?: number;
    head?: string | null;
  }>;
  const committed = members.filter((member) => member.committed === true);
  if (committed.length === 0) return "Nothing to commit.";
  const files = committed.reduce(
    (total, member) => total + (member.files ?? 0),
    0,
  );
  const heads = committed
    .map((member) => member.head?.slice(0, 7))
    .filter((head): head is string => !!head);
  const filesText = `${files} file${files === 1 ? "" : "s"}`;
  return heads.length > 0
    ? `Committed ${filesText} as ${heads.join(", ")}`
    : `Committed ${filesText}`;
}

export function CommitChangesButton({
  workspaceId,
  defaultMessage = "",
  disabledReason,
}: CommitChangesButtonProps) {
  // Polled while the VM is attached, like the Changes view: a commit or
  // reset in the terminal touches only .git, which the file watcher does
  // not report, so nothing else would refresh the count.
  const changes = useWorkspaceChanges(workspaceId, {
    refetchIntervalMs: LIVE_CHANGES_POLL_INTERVAL_MS,
  });
  const action = useWorkspaceChangesAction(workspaceId);
  const [open, setOpen] = useState(false);
  const [message, setMessage] = useState(defaultMessage);
  const [error, setError] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<string | null>(null);
  const titleId = useId();
  const messageId = useId();
  const messageRef = useRef<HTMLTextAreaElement>(null);

  const attached = changes.status === "ok" && changes.data.attached;
  const count =
    changes.status === "ok" ? uncommittedFileCount(changes.data) : 0;
  const unpushed =
    changes.status === "ok" ? unpushedCommitCount(changes.data) : 0;
  const pushOnly = count === 0 && unpushed > 0;
  const reason =
    disabledReason ??
    (changes.status !== "ok"
      ? "Loading the workspace's changes"
      : !attached
        ? "Start the workspace to commit"
        : count === 0 && unpushed === 0
          ? "Nothing to commit or push"
          : null);

  const pushPending = async () => {
    setError(null);
    setOutcome(null);
    try {
      const result = await action.mutateAsync({ action: "push" });
      if (result.status === "failed") {
        setError("The push did not complete for every repository.");
        return;
      }
      setOutcome(`Pushed ${unpushed} commit${unpushed === 1 ? "" : "s"}`);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  const openDialog = () => {
    setMessage(defaultMessage);
    setError(null);
    setOutcome(null);
    setOpen(true);
  };
  const close = () => {
    if (action.isLoading) return;
    setOpen(false);
  };
  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    // The workspace may have stopped since the dialog opened; the draft
    // message stays for when it runs again (`g5.workspace.ui.unavailable`).
    if (reason) {
      setError(reason);
      return;
    }
    const trimmed = message.trim();
    if (!trimmed) {
      setError("A commit needs a message.");
      return;
    }
    setError(null);
    try {
      const result = await action.mutateAsync({
        action: "commit",
        message: trimmed,
      });
      if (result.status === "failed") {
        setError("The commit did not complete for every repository.");
        return;
      }
      setOutcome(commitOutcomeText(result));
      setOpen(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  return (
    <>
      <span className="inline-flex items-center gap-2">
        <button
          type="button"
          data-testid="workspace-commit"
          disabled={reason !== null || action.isLoading}
          title={
            reason ??
            (pushOnly
              ? "Nothing new to commit; publishes the commits not yet on the branch"
              : "Commit and push workspace changes")
          }
          onClick={pushOnly ? () => void pushPending() : openDialog}
          className="flex items-center gap-1 rounded px-2 py-1 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
        >
          {pushOnly && action.isLoading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <GitCommitHorizontal className="h-3.5 w-3.5" />
          )}
          {count > 0
            ? `Commit ${count} file${count === 1 ? "" : "s"}`
            : pushOnly
              ? `Push ${unpushed} commit${unpushed === 1 ? "" : "s"}`
              : "Commit"}
        </button>
        {outcome && (
          <span
            data-testid="workspace-commit-outcome"
            className="text-xs text-[var(--g5-text2)]"
          >
            {outcome}
          </span>
        )}
        {error && !open && (
          <span
            role="alert"
            data-testid="workspace-commit-error"
            className="text-xs text-[var(--g5-blocked)]"
          >
            {error}
          </span>
        )}
      </span>
      {open && (
        <Modal
          labelledBy={titleId}
          onClose={close}
          size="sm"
          closeDisabled={action.isLoading}
          initialFocusRef={messageRef}
          testId="workspace-commit-dialog"
        >
          <form
            noValidate
            onSubmit={(event) => void submit(event)}
            className="space-y-3"
          >
            <h2
              id={titleId}
              className="text-sm font-semibold text-[var(--g5-text0)]"
            >
              Commit {count} file{count === 1 ? "" : "s"}
            </h2>
            <p className="text-xs text-[var(--g5-text2)]">
              Commits what is in the working tree and pushes the branch. Rounds
              reviewed before this commit move to their commit's group, and
              reviewers are told the workspace moved on.
            </p>
            <label
              htmlFor={messageId}
              className="block text-xs font-medium text-[var(--g5-text1)]"
            >
              Message
            </label>
            <textarea
              id={messageId}
              data-testid="workspace-commit-message"
              value={message}
              onChange={(event) => setMessage(event.target.value)}
              rows={3}
              ref={messageRef}
              className="w-full rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1 text-sm text-[var(--g5-text0)]"
            />
            {reason && !error && (
              <p
                role="status"
                data-testid="workspace-commit-blocked"
                className="text-xs text-[var(--g5-warn)]"
              >
                {reason}
              </p>
            )}
            {error && (
              <p role="alert" className="text-xs text-[var(--g5-blocked)]">
                {error}
              </p>
            )}
            <div className="flex justify-end gap-2">
              <Button
                kind="ghost"
                type="button"
                onClick={close}
                disabled={action.isLoading}
              >
                Cancel
              </Button>
              <Button
                kind="primary"
                type="submit"
                data-testid="workspace-commit-submit"
                disabled={action.isLoading || reason !== null}
                title={reason ?? undefined}
              >
                {action.isLoading ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  "Commit and push"
                )}
              </Button>
            </div>
          </form>
        </Modal>
      )}
    </>
  );
}
