// =============================================================================
// Merge with override — the one confirmation for G5's override of its own
// requirements and the GitHub override of GitHub's rules
// (g5.workspace.merge.admin-override.formal, g5.git.provider-override).
// Lists the current blockers grouped by the authority that waives them,
// mechanical blockers as not overridable, and requires a reason and an
// explicit confirmation. Who may override is the server's `authority` only.
// =============================================================================

import { useId, useState } from "react";
import { AlertTriangle, GitMerge, Loader2, X } from "lucide-react";
import {
  useMergeOverridePreview,
  useMergeWithOverride,
} from "../../hooks/useWorkspace";
import { ApiError } from "../../lib/api";
import {
  MERGE_OVERRIDE_CONFIRMATION,
  mergeOverrideApprovalStateLabel,
  mergeOverrideCheckBucketLabel,
  mergeOverrideGroups,
  mergeOverrideOffer,
} from "../../lib/mergeOverride";
import type {
  MergeOverridePreview,
  MergeWorkspacePullRequestResponse,
} from "../../lib/types";
import { classNames, Modal } from "../ui/UI";

export interface MergeOverrideDialogProps {
  targetWorkspaceId: string;
  targetWorkspaceName?: string;
  pullRequestId: string;
  pullRequestTitle?: string;
  onClose: () => void;
  onMerged: (result: MergeWorkspacePullRequestResponse) => void;
}

type Notice =
  | { kind: "changed" }
  | { kind: "not_needed" }
  | { kind: "error"; message: string };

function overrideErrorCode(error: unknown): string | null {
  if (!(error instanceof ApiError)) return null;
  const detail = error.details?.code;
  return typeof detail === "string" ? detail : null;
}

const SECTION =
  "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2";
const CHIP =
  "inline-flex shrink-0 items-center rounded-full border px-1.5 py-px text-[10.5px] font-medium";
const WARN_CHIP = `${CHIP} border-[var(--g5-warn)]/30 bg-[var(--g5-warn)]/10 text-[var(--g5-warn)]`;
const BLOCKED_CHIP = `${CHIP} border-[var(--g5-blocked)]/30 bg-[var(--g5-blocked)]/10 text-[var(--g5-blocked)]`;
const MUTED_CHIP = `${CHIP} border-[var(--g5-stroke)] bg-[var(--g5-bg2)] text-[var(--g5-text2)]`;

function AuthorityNote({
  allowed,
  reason,
  fallback,
  testId,
}: {
  allowed: boolean;
  reason?: string | null;
  fallback: string;
  testId: string;
}) {
  if (allowed) return null;
  return (
    <p
      data-testid={testId}
      className="mt-1.5 text-[11px] text-[var(--g5-blocked)]"
    >
      {reason?.trim() || fallback}
    </p>
  );
}

function BlockerList({ preview }: { preview: MergeOverridePreview }) {
  const { approvals, checks, github } = preview.blockers;
  const groups = mergeOverrideGroups(preview);
  return (
    <div className="space-y-2">
      {groups.g5 ? (
        <section
          className={SECTION}
          data-testid="merge-override-g5-group"
          aria-labelledby="merge-override-g5-heading"
        >
          <h3
            id="merge-override-g5-heading"
            className="text-xs font-semibold text-[var(--g5-text0)]"
          >
            G5 requirements
          </h3>
          {approvals.length > 0 ? (
            <ul className="mt-1.5 space-y-1" aria-label="Approval requirements">
              {approvals.map((approval) => (
                <li
                  key={approval.definitionId}
                  data-testid={`merge-override-approval-${approval.definitionId}`}
                  className="flex items-center gap-2 text-xs text-[var(--g5-text1)]"
                >
                  <span className="min-w-0 flex-1 truncate">
                    {approval.definitionName}{" "}
                    <span className="text-[var(--g5-text2)]">
                      v{approval.definitionVersion}
                    </span>
                  </span>
                  <span
                    className={
                      approval.state === "rejected" ? BLOCKED_CHIP : WARN_CHIP
                    }
                  >
                    {mergeOverrideApprovalStateLabel(approval.state)}
                  </span>
                </li>
              ))}
            </ul>
          ) : null}
          {checks.length > 0 ? (
            <div className="mt-2 space-y-1.5">
              <p className="text-[11px] text-[var(--g5-text2)]">
                External checks your organization requires to pass:
              </p>
              {checks.map((repo) => (
                <div
                  key={repo.repository}
                  data-testid={`merge-override-checks-${repo.repository}`}
                  className="text-xs"
                >
                  <div className="flex items-center gap-2">
                    <span className="min-w-0 flex-1 truncate font-medium text-[var(--g5-text0)]">
                      {repo.repository}
                    </span>
                    {repo.availability === "unavailable" ? (
                      <span className={BLOCKED_CHIP}>Checks unreadable</span>
                    ) : repo.availability === "pending" ? (
                      <span className={WARN_CHIP}>Not reported yet</span>
                    ) : null}
                  </div>
                  {repo.checks.length > 0 ? (
                    <ul className="mt-0.5 space-y-0.5 pl-3">
                      {repo.checks.map((check) => (
                        <li
                          key={check.name}
                          className="flex items-center gap-2 text-[var(--g5-text1)]"
                        >
                          <span className="min-w-0 flex-1 truncate">
                            {check.name}
                            {check.required ? (
                              <span className="text-[var(--g5-text2)]">
                                {" "}
                                · required
                              </span>
                            ) : null}
                          </span>
                          <span
                            className={
                              check.bucket === "pending"
                                ? WARN_CHIP
                                : BLOCKED_CHIP
                            }
                          >
                            {mergeOverrideCheckBucketLabel(check.bucket)}
                          </span>
                        </li>
                      ))}
                    </ul>
                  ) : null}
                </div>
              ))}
            </div>
          ) : null}
          <AuthorityNote
            allowed={preview.authority.g5.allowed}
            reason={preview.authority.g5.reason}
            fallback="Waiving G5 requirements needs an organization administrator."
            testId="merge-override-g5-denied"
          />
        </section>
      ) : null}

      {groups.github ? (
        <section
          className={SECTION}
          data-testid="merge-override-github-group"
          aria-labelledby="merge-override-github-heading"
        >
          <h3
            id="merge-override-github-heading"
            className="text-xs font-semibold text-[var(--g5-text0)]"
          >
            GitHub rules
          </h3>
          <ul className="mt-1.5 space-y-1" aria-label="Blocked repositories">
            {github.map((repo) => (
              <li
                key={repo.repoId}
                data-testid={`merge-override-github-${repo.repoId}`}
                className="flex items-center gap-2 text-xs text-[var(--g5-text1)]"
              >
                <span className="min-w-0 flex-1 truncate">
                  {repo.repository}
                </span>
                <span className={BLOCKED_CHIP}>
                  {repo.mergeState.replace(/_/g, " ")}
                </span>
              </li>
            ))}
          </ul>
          <p className="mt-1.5 text-[11px] text-[var(--g5-text2)]">
            G5 merges these repositories with your own GitHub account. GitHub
            decides whether you may bypass its rules; if it refuses, the merge
            stops.
          </p>
          <AuthorityNote
            allowed={preview.authority.github.allowed}
            reason={preview.authority.github.reason}
            fallback="You may not override GitHub's rules for this pull request."
            testId="merge-override-github-denied"
          />
        </section>
      ) : null}

      {preview.notOverridable.length > 0 ? (
        <section
          className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-2"
          data-testid="merge-override-not-overridable"
        >
          <h3 className="text-xs font-semibold text-[var(--g5-blocked)]">
            Not overridable
          </h3>
          <ul className="mt-1 space-y-0.5 text-xs text-[var(--g5-blocked)]">
            {preview.notOverridable.map((reason) => (
              <li key={reason} className="flex items-start gap-2">
                <span className="min-w-0 flex-1">{reason}</span>
                <span className={MUTED_CHIP}>Not overridable</span>
              </li>
            ))}
          </ul>
          <p className="mt-1 text-[11px] text-[var(--g5-text1)]">
            Resolve these first; no override waives them.
          </p>
        </section>
      ) : null}
    </div>
  );
}

export function MergeOverrideDialog({
  targetWorkspaceId,
  targetWorkspaceName,
  pullRequestId,
  pullRequestTitle,
  onClose,
  onMerged,
}: MergeOverrideDialogProps) {
  const titleId = useId();
  const reasonId = useId();
  const preview = useMergeOverridePreview(targetWorkspaceId, pullRequestId);
  const merge = useMergeWithOverride(targetWorkspaceId);
  const [reason, setReason] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const data = preview.status === "ok" ? preview.data : null;
  const groups = data ? mergeOverrideGroups(data) : null;
  const hasBlockers = !!groups && (groups.g5 || groups.github);
  const authorized =
    !!data &&
    !!groups &&
    (!groups.g5 || data.authority.g5.allowed) &&
    (!groups.github || data.authority.github.allowed);
  const busy = submitting || merge.isLoading;
  const canConfirm =
    !!data &&
    hasBlockers &&
    authorized &&
    data.notOverridable.length === 0 &&
    notice?.kind !== "not_needed" &&
    reason.trim() !== "" &&
    confirmed &&
    !busy;

  const handleConfirm = async () => {
    if (!data || !canConfirm) return;
    setSubmitting(true);
    setNotice(null);
    try {
      const result = await merge.mutateAsync({
        pullRequestId,
        reason: reason.trim(),
        digest: data.digest,
      });
      onMerged(result);
    } catch (error) {
      const code = overrideErrorCode(error);
      if (code === "merge_override_changed") {
        // What would be waived changed: show the new set, keep the reason,
        // and ask for the confirmation again.
        setConfirmed(false);
        setNotice({ kind: "changed" });
        await preview.refetch();
      } else if (code === "merge_override_not_needed") {
        setConfirmed(false);
        setNotice({ kind: "not_needed" });
        void preview.refetch();
      } else {
        setNotice({
          kind: "error",
          message:
            error instanceof Error && error.message
              ? error.message
              : "The merge with override failed.",
        });
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      onClose={onClose}
      labelledBy={titleId}
      size="md"
      tier="top"
      closeDisabled={busy}
      testId="merge-override-dialog"
      className="p-4 text-sm"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <AlertTriangle
            size={16}
            aria-hidden="true"
            className="shrink-0 text-[var(--g5-warn)]"
          />
          <h2
            id={titleId}
            className="truncate text-base font-semibold text-[var(--g5-text0)]"
          >
            Merge with override
          </h2>
        </div>
        <button
          type="button"
          data-testid="merge-override-close"
          onClick={onClose}
          disabled={busy}
          aria-label="Close"
          className="flex h-7 w-7 items-center justify-center rounded text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] disabled:cursor-not-allowed disabled:opacity-50"
        >
          <X size={15} />
        </button>
      </div>
      <p className="mt-2 text-xs text-[var(--g5-text1)]">
        {pullRequestTitle ? (
          <>
            <span className="font-medium text-[var(--g5-text0)]">
              {pullRequestTitle}
            </span>
            {targetWorkspaceName ? <> into {targetWorkspaceName}</> : null}
            .{" "}
          </>
        ) : null}
        Merges now although the requirements and rules below are not met.
      </p>

      {data?.selfOverride ? (
        <p
          data-testid="merge-override-self"
          className="mt-2 flex items-center gap-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-3 py-1.5 text-xs text-[var(--g5-warn)]"
        >
          <span className={WARN_CHIP}>Self-override</span>
          You own the source workspace; the override is flagged as a
          self-override.
        </p>
      ) : null}

      {notice?.kind === "changed" ? (
        <p
          role="alert"
          data-testid="merge-override-changed"
          className="mt-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-3 py-1.5 text-xs text-[var(--g5-warn)]"
        >
          What this override would waive changed. Review the updated list and
          confirm again.
        </p>
      ) : null}
      {notice?.kind === "not_needed" ? (
        <p
          role="alert"
          data-testid="merge-override-not-needed"
          className="mt-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-info)]/40 bg-[var(--g5-info)]/10 px-3 py-1.5 text-xs text-[var(--g5-info)]"
        >
          Nothing blocks this merge any more. Close this dialog and use Ship to
          parent or Merge instead.
        </p>
      ) : null}

      <div className="mt-3">
        {preview.status === "loading" ? (
          <p className="flex items-center gap-2 text-xs text-[var(--g5-text2)]">
            <Loader2 size={13} className="animate-spin" aria-hidden="true" />
            Reading what would be waived…
          </p>
        ) : preview.status === "error" ? (
          <p
            role="alert"
            data-testid="merge-override-preview-error"
            className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-2 text-xs text-[var(--g5-blocked)]"
          >
            Merge with override is not available: {preview.error.message}
          </p>
        ) : data && hasBlockers ? (
          <BlockerList preview={data} />
        ) : data ? (
          <>
            <p
              data-testid="merge-override-nothing"
              className="text-xs text-[var(--g5-text2)]"
            >
              Nothing to override: no G5 requirement or GitHub rule blocks this
              merge now.
            </p>
            {data.notOverridable.length > 0 ? (
              <div className="mt-2">
                <BlockerList preview={data} />
              </div>
            ) : null}
          </>
        ) : null}
      </div>

      <label
        htmlFor={reasonId}
        className="mt-3 block text-xs font-medium text-[var(--g5-text1)]"
      >
        Reason (required, recorded with the merge)
      </label>
      <textarea
        id={reasonId}
        data-testid="merge-override-reason"
        value={reason}
        onChange={(event) => setReason(event.target.value)}
        disabled={busy}
        required
        rows={3}
        className="mt-1 block w-full resize-y rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-1.5 text-xs text-[var(--g5-text0)] focus:border-[var(--g5-warn)]/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-warn)]/40"
      />

      <label className="mt-3 flex items-start gap-2 text-xs text-[var(--g5-text1)]">
        <input
          type="checkbox"
          data-testid="merge-override-confirm-checkbox"
          checked={confirmed}
          onChange={(event) => setConfirmed(event.target.checked)}
          disabled={busy}
          className="mt-0.5 shrink-0"
        />
        <span>{MERGE_OVERRIDE_CONFIRMATION}</span>
      </label>

      {notice?.kind === "error" ? (
        <p
          role="alert"
          data-testid="merge-override-error"
          className="mt-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-2 text-xs text-[var(--g5-blocked)]"
        >
          {notice.message}
        </p>
      ) : null}

      <div className="mt-4 flex justify-end gap-2">
        <button
          type="button"
          data-testid="merge-override-cancel"
          onClick={onClose}
          disabled={busy}
          className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-1.5 text-sm text-[var(--g5-text1)] hover:bg-[var(--g5-bg1)] disabled:opacity-50"
        >
          Cancel
        </button>
        <button
          type="button"
          data-testid="merge-override-confirm"
          onClick={() => void handleConfirm()}
          disabled={!canConfirm}
          className={classNames(
            "inline-flex items-center gap-1.5 rounded-[var(--g5-radius-sm)] px-3 py-1.5 text-sm font-medium text-[rgb(8,12,24)]",
            canConfirm
              ? "bg-[var(--g5-warn)] hover:bg-[var(--g5-warn)]/90"
              : "cursor-not-allowed bg-[var(--g5-warn)]/50",
          )}
        >
          {busy ? (
            <Loader2 size={13} className="animate-spin" aria-hidden="true" />
          ) : (
            <GitMerge size={13} aria-hidden="true" />
          )}
          {busy ? "Merging…" : "Merge with override"}
        </button>
      </div>
    </Modal>
  );
}

export interface MergeOverrideActionProps {
  targetWorkspaceId: string;
  targetWorkspaceName?: string;
  pullRequestId: string;
  pullRequestTitle?: string;
  /**
   * Read the preview at all: only while the ordinary merge is blocked for an
   * open pull request.
   */
  enabled: boolean;
  /** Hold the button while another action on the pull request runs. */
  disabled?: boolean;
  testIdPrefix: string;
  /** Classes for the button's size, beside the caller's Merge control. */
  buttonClassName?: string;
  /** Classes for the denied hint. */
  hintClassName?: string;
  onMerged: (result: MergeWorkspacePullRequestResponse) => void;
}

/**
 * The warning-styled secondary "Merge with override…" control placed beside
 * Ship to parent / Merge (never replacing it): offered only when the server's
 * preview reports overridable blockers and grants the caller every group
 * present; when it denies one, a short muted hint carries the server's reason.
 */
export function MergeOverrideAction({
  targetWorkspaceId,
  targetWorkspaceName,
  pullRequestId,
  pullRequestTitle,
  enabled,
  disabled = false,
  testIdPrefix,
  buttonClassName = "rounded-md px-3 py-1.5 text-xs",
  hintClassName = "text-[10.5px]",
  onMerged,
}: MergeOverrideActionProps) {
  const preview = useMergeOverridePreview(
    targetWorkspaceId,
    pullRequestId,
    enabled,
  );
  const [open, setOpen] = useState(false);
  if (!enabled && !open) return null;
  const offer = mergeOverrideOffer(
    preview.status === "ok" ? preview.data : null,
  );
  // A session from before sessions recorded how they were issued, or one
  // from CLI login, cannot override (g5.security.browser-session-actions);
  // say so rather than offering nothing.
  const needsBrowserSession =
    preview.status === "error" &&
    preview.error instanceof ApiError &&
    Array.isArray(preview.error.details?.missing_scopes) &&
    (preview.error.details.missing_scopes as unknown[]).includes(
      "browser_session_required",
    );
  return (
    <>
      {needsBrowserSession ? (
        <span
          data-testid={`${testIdPrefix}-merge-override-sign-in`}
          className={classNames("text-[var(--g5-text2)]", hintClassName)}
        >
          Sign in again to use Merge with override.
        </span>
      ) : offer.kind === "offered" ? (
        <button
          type="button"
          data-testid={`${testIdPrefix}-merge-override`}
          onClick={() => setOpen(true)}
          disabled={disabled}
          title="Merge although the listed requirements and rules are not met; recorded with your name and reason"
          className={classNames(
            "inline-flex items-center gap-1.5 border font-medium",
            "border-[var(--g5-warn)]/50 bg-[var(--g5-warn)]/10 text-[var(--g5-warn)] hover:bg-[var(--g5-warn)]/15",
            "disabled:cursor-not-allowed disabled:opacity-50",
            buttonClassName,
          )}
        >
          <AlertTriangle size={12} aria-hidden="true" />
          Merge with override…
        </button>
      ) : offer.kind === "denied" && offer.reason ? (
        <span
          data-testid={`${testIdPrefix}-merge-override-denied`}
          className={classNames("text-[var(--g5-text2)]", hintClassName)}
        >
          {offer.reason}
        </span>
      ) : null}
      {open ? (
        <MergeOverrideDialog
          targetWorkspaceId={targetWorkspaceId}
          targetWorkspaceName={targetWorkspaceName}
          pullRequestId={pullRequestId}
          pullRequestTitle={pullRequestTitle}
          onClose={() => setOpen(false)}
          onMerged={(result) => {
            setOpen(false);
            onMerged(result);
          }}
        />
      ) : null}
    </>
  );
}
