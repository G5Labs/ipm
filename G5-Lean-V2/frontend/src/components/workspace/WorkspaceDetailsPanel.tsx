// =============================================================================
// G5 Workspace Page — WorkspaceDetailsPanel
// =============================================================================
// Read-mostly center-pane overview of the current workspace: identity, owner,
// activity, estimated spend, read-only status, and repos/branches. Hosts the
// owner-gated read-only transition toggle (the only client control that
// transitions an existing workspace between read_write and read_only).
//
// Ontology:
//   @ref software_architecture/client/workspace-page/center-panel#D53
//   @ref software_architecture/g5-api/services/workspace_mutation_policy#I9
//
// The page is read-only with respect to workspace content and safe on a
// read-only workspace: it never ensures a draft or opens editable live workspace file state.
// The only write paths are the owner description edit (offered only when
// editable) and the owner read-only transition toggle for working children.
// =============================================================================

import React, { useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Check,
  Copy,
  Database,
  Download,
  GitBranch,
  Lock,
  LockOpen,
  Pencil,
  X,
} from "lucide-react";

import { ReadOnlyBadge } from "./ReadOnlyBadge";
import { BranchTrackingInline } from "./BranchTrackingInline";
import { RemoteStatusPill } from "./RemoteStatusPill";
import { workspacePagePath } from "./reviewWorkspace";
import { chatLandingQuery } from "../../lib/workspaceLanding";
import { PublishBranchMenu } from "./PublishBranchMenu";
import { MakeRemoteMenu } from "./MakeRemoteDialog";
import { SpendBreakdown } from "../settings/SpendBreakdown";
import { Button, Chip, Modal, classNames } from "../ui/UI";
import { useAuth } from "../../hooks/useAuth";
import {
  usePatchWorkspace,
  useRenameWorkspace,
  useSetWorkspaceMutationPolicy,
  useWorkspace,
  useWorkspaceStats,
} from "../../hooks/useWorkspace";
import { RenameWorkspaceDialog } from "./RenameWorkspaceDialog";
import { useOrgMembers } from "../../hooks/useOrg";
import { ApiError } from "../../lib/api";
import { formatCost } from "../../lib/utils";
import { IdentityAvatar } from "../shared/IdentityAvatar";
import { WorkspaceIssuesPanel } from "../external-issues/WorkspaceIssuesPanel";
import { useWorkspaceLiveGate } from "../../lib/workspaceLiveGate";
import { isPersonalWorkspace } from "../../lib/workspaceKind";
import type {
  MutationPolicyResult,
  TransitionBlocker,
  WorkspaceDetail,
  WorkspaceIssueLinkDTO,
} from "../../lib/types";

/** Why a review copy's policy, description and issue links are off. */
/** Why the same controls wait while the page has not learned if this is a copy. */
export const REVIEW_IDENTITY_PENDING_REASON =
  "Checking whether this is a review copy; try again in a moment.";

export const REVIEW_COPY_DETAILS_REASON =
  "A review copy follows its source: change the read-only policy, description and issue links on the source workspace. A read-only copy could not push its review.";

/** Why issue links are off on Base (issues g5.issue.base-links). */
export const BASE_ISSUE_LINK_REASON =
  "Base is read-only and takes no issues. Fork it to work on an issue.";

export interface WorkspaceDetailsPanelProps {
  workspaceId: string;
  /**
   * Owning org id. Optional — when omitted it is derived from the loaded
   * `WorkspaceDetail.orgId`, so the panel can be rendered with only a
   * workspace id from the route.
   */
  orgId?: string;
  /**
   * Open a linked issue's read-only view. When provided, the panel shows the
   * workspace's Issues section (external issue links).
   */
  onOpenIssue?: (link: WorkspaceIssueLinkDTO) => void;
  /**
   * Download the workspace source as a zip. The page owns the request; the
   * panel is the one place the shell offers it (ui g5.ui.workbench-shell).
   */
  onDownloadSource?: () => void;
  downloadSourceInProgress?: boolean;
}

function formatTimestamp(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}

// ---------------------------------------------------------------------------
// Card — local tokenized container (there is no shared Card primitive).
// ---------------------------------------------------------------------------
const Card: React.FC<{ title: string; children: React.ReactNode }> = ({
  title,
  children,
}) => (
  <section
    className={classNames(
      "rounded-[var(--g5-radius-md)]",
      "border border-[var(--g5-stroke)]",
      "bg-[var(--g5-bg1)]",
      "p-4",
    )}
  >
    <h3 className="mb-3 text-sm font-semibold text-[var(--g5-text0)]">
      {title}
    </h3>
    {children}
  </section>
);

const Field: React.FC<{
  label: string;
  children: React.ReactNode;
  /** Optional native-tooltip text describing the field's exact criteria. */
  hint?: string;
}> = ({ label, children, hint }) => (
  <div className="flex flex-col gap-0.5 py-1" title={hint}>
    <span className="text-xs uppercase tracking-wide text-[var(--g5-text2)]">
      {label}
    </span>
    <span className="text-sm text-[var(--g5-text0)]">{children}</span>
  </div>
);

// POSIX single-quote escaping for copy-paste-safe checkout commands
// (center-panel D75). Single quotes suppress ALL shell evaluation —
// `$(...)`, backticks, and variable expansion — which double quotes do not,
// so a user-typed external repo/branch name cannot inject a command when the
// copied line is pasted into a shell. The closing/reopening `'\''` sequence
// escapes an embedded single quote.
const shellQuote = (value: string): string =>
  `'${value.replaceAll("'", "'\\''")}'`;

export const WorkspaceDetailsPanel: React.FC<WorkspaceDetailsPanelProps> = ({
  workspaceId,
  orgId,
  onOpenIssue,
  onDownloadSource,
  downloadSourceInProgress = false,
}) => {
  const { user, hasScope } = useAuth();
  const workspaceState = useWorkspace(workspaceId);
  const statsState = useWorkspaceStats(workspaceId);
  // Route slugs for the Make-Remote merge-required deep link and the
  // `org/repo` chip labels (center-panel D75).
  const { org: orgSlug, workspace: workspaceSlug } = useParams<{
    org: string;
    workspace: string;
  }>();
  const navigate = useNavigate();
  const [copiedRepoId, setCopiedRepoId] = useState<string | null>(null);

  const detail: WorkspaceDetail | undefined =
    workspaceState.status === "ok" ? workspaceState.data : undefined;

  // Derive the owning org from the loaded detail when not supplied by the
  // caller. useOrgMembers is disabled until a non-empty id is available.
  const effectiveOrgId = orgId ?? detail?.orgId ?? "";
  const membersState = useOrgMembers(effectiveOrgId);

  const patchWorkspace = usePatchWorkspace(workspaceId);
  const renameWorkspace = useRenameWorkspace(workspaceId, orgId);
  const setPolicy = useSetWorkspaceMutationPolicy(workspaceId);

  const stats = statsState.status === "ok" ? statsState.data : undefined;
  const members = membersState.status === "ok" ? membersState.data : undefined;

  // Client scope check is a UX gate only; the server rechecks current
  // ownership. Organization administrators transfer ownership before editing.
  const isOwner =
    hasScope("workspaces:write") &&
    !!user?.id &&
    detail?.ownerUserId === user.id;

  const owner = useMemo(() => {
    if (!detail?.ownerUserId || !members) return null;
    return members.find((m) => m.userId === detail.ownerUserId) ?? null;
  }, [detail, members]);

  const repoRows = useMemo(
    () =>
      (detail?.branches ?? []).map((branch) => ({
        repoId: branch.repoId,
        repoName: branch.repoName,
        branch,
      })),
    [detail],
  );

  const readOnly = detail?.isBase === true || detail?.readOnly === true;
  // A personal workspace has no lineage, publication or Make Remote, and no
  // Project Docs folder (g5.personal.refusals, g5.personal.project-docs).
  const personal = isPersonalWorkspace(detail);
  // A review copy follows its source (`g5.governance.review-workspace.exclusions`):
  // its policy, description and issue links are offered disabled, with the
  // reason, rather than omitted.
  const gate = useWorkspaceLiveGate();
  // While the page has not learned whether this is a copy, the same
  // controls wait rather than act on an ordinary-workspace assumption.
  const reviewCopy = gate
    ? gate.reviewCopy || !gate.reviewIdentityKnown
    : false;
  const reviewCopyReason =
    gate && !gate.reviewIdentityKnown && !gate.reviewCopy
      ? REVIEW_IDENTITY_PENDING_REASON
      : REVIEW_COPY_DETAILS_REASON;
  const canEditDescription = isOwner && !readOnly && !reviewCopy;
  // Rename is owner housekeeping (g5.workspace.rename): offered for a
  // non-Base workspace, read-only workspaces and ordinary review copies
  // included; the server refuses what the client cannot see. A personal
  // workspace keeps the name the system gave it (g5.personal.names).
  const canRename = isOwner && detail?.isBase === false && !personal;
  const [renaming, setRenaming] = useState(false);
  // A read-write → read-only drain in progress (server-truth via detail polling).
  const serverFreezing = detail?.mutationPolicyTransition === "freezing";

  // ----- Transition modal state -----
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [transitionError, setTransitionError] = useState<string | null>(null);
  const [transitionMessage, setTransitionMessage] = useState<string | null>(
    null,
  );
  const [blockers, setBlockers] = useState<TransitionBlocker[]>([]);
  const [freezing, setFreezing] = useState(false);

  // Draining state from either the immediate mutation result (`freezing`) or the
  // polled server truth (`serverFreezing`).
  const isFreezing = freezing || serverFreezing;
  // "Make editable" is offered for any *not effectively read-write* state —
  // committed read-only OR an in-flight/stuck `freezing` drain — and issues a
  // `read_write` request, which also reverts an in-progress freeze (the operator
  // escape hatch, @ref software_architecture/g5-api/services/workspace_service#I6).
  // "Make read-only" shows only when the workspace is fully read-write.
  const notReadWrite = readOnly || isFreezing;

  const openToggle = () => {
    if (detail?.isBase) return;
    setTransitionError(null);
    setTransitionMessage(null);
    setBlockers([]);
    setFreezing(false);
    setConfirmOpen(true);
  };

  const closeModal = () => {
    if (setPolicy.isLoading) return;
    setConfirmOpen(false);
  };

  const runTransition = async () => {
    // Rechecked at submit: the identity may have arrived while the dialog
    // was open, and a read-only copy could not push its review.
    if (detail?.isBase || reviewCopy) {
      setConfirmOpen(false);
      return;
    }
    setTransitionError(null);
    setTransitionMessage(null);
    setBlockers([]);
    try {
      const result: MutationPolicyResult = await setPolicy.mutateAsync(
        notReadWrite
          ? { policy: "read_write" }
          : { policy: "read_only", mode: "drain" },
      );
      setTransitionMessage(result.message ?? null);
      if (result.blockers && result.blockers.length > 0) {
        setBlockers(result.blockers);
        setFreezing(false);
        return;
      }
      if (result.transition === "freezing") {
        // Drain accepted and running in the background (HTTP 202). Keep the modal
        // open in a draining state; detail polling flips the badge when it
        // commits or reverts.
        setFreezing(true);
        return;
      }
      // Committed — cache invalidation flips badge/banner/switcher.
      setConfirmOpen(false);
    } catch (err) {
      if (err instanceof ApiError) {
        // Dirty-workspace precondition and other structured refusals surface
        // as an actionable message rather than a raw error.
        setTransitionError(err.message);
      } else {
        setTransitionError(
          err instanceof Error ? err.message : "Transition failed.",
        );
      }
    }
  };

  // ----- Inline description edit (current owner only) -----
  const [editingDescription, setEditingDescription] = useState(false);
  const [descriptionDraft, setDescriptionDraft] = useState("");
  const [descriptionError, setDescriptionError] = useState<string | null>(null);

  const beginEditDescription = () => {
    if (!canEditDescription) return;
    setDescriptionDraft(detail?.description ?? "");
    setDescriptionError(null);
    setEditingDescription(true);
  };

  const saveDescription = async () => {
    if (!canEditDescription) return;
    setDescriptionError(null);
    try {
      await patchWorkspace.mutateAsync({ description: descriptionDraft });
      setEditingDescription(false);
    } catch (err) {
      setDescriptionError(
        err instanceof Error ? err.message : "Failed to save description.",
      );
    }
  };

  if (workspaceState.status === "loading") {
    return (
      <div
        data-testid="workspace-details-panel"
        className="flex h-full items-center justify-center bg-[var(--g5-bg0)] text-[var(--g5-text2)]"
      >
        Loading workspace details…
      </div>
    );
  }

  if (workspaceState.status === "error" || !detail) {
    return (
      <div
        data-testid="workspace-details-panel"
        className="flex h-full items-center justify-center bg-[var(--g5-bg0)] text-[var(--g5-blocked)]"
      >
        Failed to load workspace details.
      </div>
    );
  }

  return (
    <div
      data-testid="workspace-details-panel"
      className="flex h-full flex-col overflow-y-auto bg-[var(--g5-bg0)]"
    >
      {/* Header */}
      <header className="flex items-center gap-3 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-6 py-4">
        <h2 className="text-lg font-semibold text-[var(--g5-text0)]">
          {detail.name}
        </h2>
        {readOnly && <ReadOnlyBadge />}
        {isFreezing && !readOnly && (
          <Chip
            state="info"
            text="Freezing…"
            data-testid="workspace-freezing-badge"
          />
        )}
        {detail.isBase && <Chip state="info" text="Base" />}
        {personal && (
          <Chip
            state="info"
            text="Personal workspace"
            data-testid="workspace-personal-badge"
          />
        )}
        <div className="ml-auto flex items-center gap-2">
          {onDownloadSource && (
            <Button
              kind="secondary"
              data-testid="workspace-download-source"
              disabled={downloadSourceInProgress}
              title="Download the workspace source as a zip"
              onClick={onDownloadSource}
            >
              <span className="inline-flex items-center gap-1.5">
                <Download size={14} />
                {downloadSourceInProgress ? "Downloading…" : "Download code"}
              </span>
            </Button>
          )}
          {!detail.isBase && (
            <Button
              kind={notReadWrite ? "secondary" : "primary"}
              data-testid="workspace-readonly-toggle"
              disabled={!isOwner || reviewCopy}
              title={
                reviewCopy
                  ? reviewCopyReason
                  : isOwner
                    ? notReadWrite
                      ? "Make this workspace editable again"
                      : "Make this workspace read-only"
                    : "Only the current workspace owner can change read-only status"
              }
              onClick={openToggle}
            >
              {notReadWrite ? (
                <span className="inline-flex items-center gap-1.5">
                  <LockOpen size={14} /> Make editable
                </span>
              ) : (
                <span className="inline-flex items-center gap-1.5">
                  <Lock size={14} /> Make read-only
                </span>
              )}
            </Button>
          )}
        </div>
      </header>

      <div className="grid grid-cols-1 gap-4 p-6 lg:grid-cols-2">
        {/* Overview */}
        <Card title="Overview">
          <div className="flex flex-col divide-y divide-[var(--g5-stroke)]">
            {/* Name, with Rename for the owner (g5.workspace.rename). */}
            <div className="py-1" data-testid="workspace-name-row">
              <div className="flex items-center justify-between">
                <span className="text-xs uppercase tracking-wide text-[var(--g5-text2)]">
                  Name
                </span>
                {canRename && (
                  <button
                    type="button"
                    data-testid="workspace-name-rename"
                    className="inline-flex items-center gap-1 text-xs text-[var(--g5-text1)] hover:text-[var(--g5-text0)]"
                    onClick={() => {
                      renameWorkspace.reset();
                      setRenaming(true);
                    }}
                  >
                    <Pencil size={12} /> Rename
                  </button>
                )}
              </div>
              <p
                data-testid="workspace-name"
                className="mt-0.5 text-sm text-[var(--g5-text0)]"
              >
                {detail.name}
              </p>
              {detail.formerNames && detail.formerNames.length > 0 && (
                <p
                  data-testid="workspace-former-names"
                  className="mt-0.5 text-xs text-[var(--g5-text2)]"
                  title={`Formerly ${detail.formerNames.join(", ")}`}
                >
                  formerly {detail.formerNames.join(", ")}
                </p>
              )}
            </div>

            <div className="py-1">
              <div className="flex items-center justify-between">
                <span className="text-xs uppercase tracking-wide text-[var(--g5-text2)]">
                  Description
                </span>
                {canEditDescription && !editingDescription && (
                  <button
                    type="button"
                    data-testid="workspace-description-edit"
                    className="inline-flex items-center gap-1 text-xs text-[var(--g5-text1)] hover:text-[var(--g5-text0)]"
                    onClick={beginEditDescription}
                  >
                    <Pencil size={12} /> Edit
                  </button>
                )}
                {isOwner && reviewCopy && (
                  <button
                    type="button"
                    data-testid="workspace-description-edit"
                    className="inline-flex items-center gap-1 text-xs text-[var(--g5-text2)] opacity-60"
                    disabled
                    title={reviewCopyReason}
                  >
                    <Pencil size={12} /> Edit
                  </button>
                )}
              </div>
              {editingDescription && canEditDescription ? (
                <div className="mt-1 flex flex-col gap-2">
                  <textarea
                    data-testid="workspace-description-input"
                    className="min-h-[64px] w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2 text-sm text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
                    value={descriptionDraft}
                    onChange={(e) => setDescriptionDraft(e.target.value)}
                  />
                  {descriptionError && (
                    <span className="text-xs text-[var(--g5-blocked)]">
                      {descriptionError}
                    </span>
                  )}
                  <div className="flex gap-2">
                    <Button
                      kind="primary"
                      data-testid="workspace-description-save"
                      disabled={patchWorkspace.isLoading}
                      onClick={saveDescription}
                    >
                      {patchWorkspace.isLoading ? "Saving…" : "Save"}
                    </Button>
                    <Button
                      kind="ghost"
                      onClick={() => setEditingDescription(false)}
                      disabled={patchWorkspace.isLoading}
                    >
                      Cancel
                    </Button>
                  </div>
                </div>
              ) : (
                <p className="mt-0.5 text-sm text-[var(--g5-text0)]">
                  {detail.description?.trim()
                    ? detail.description
                    : "No description."}
                </p>
              )}
            </div>

            {!detail.isBase && (
              <Field label="Owner">
                {owner || detail.ownerUserId ? (
                  <span className="inline-flex items-center gap-2">
                    <IdentityAvatar
                      identity={owner?.email ?? detail.ownerUserId ?? ""}
                      avatarUrl={owner?.avatarUrl}
                      size={20}
                    />
                    {owner ? (
                      owner.displayName
                    ) : (
                      <span className="text-[var(--g5-text2)]">
                        {detail.ownerUserId}
                      </span>
                    )}
                  </span>
                ) : (
                  "—"
                )}
              </Field>
            )}

            <Field label="Created">{formatTimestamp(detail.createdAt)}</Field>
            <Field label="Last updated">
              {formatTimestamp(detail.updatedAt)}
            </Field>
            {!personal && (
              <Field label="Forked from">
                {detail.clonedFromId ??
                  (detail.isBase ? "Base workspace" : "—")}
              </Field>
            )}
          </div>
        </Card>

        {/* Activity */}
        <Card title="Activity">
          {stats ? (
            <div className="grid grid-cols-2 gap-3">
              <Field
                label="Total tasks"
                hint="Every task in this workspace, in any status (queued, running, waiting, succeeded, failed, or cancelled)."
              >
                {stats.totalTasks}
              </Field>
              <Field
                label="Active now"
                hint="Tasks currently running (status = running). This is a live count of executing work, not a time-based window."
              >
                {stats.concurrentTasks}
              </Field>
              <Field
                label="Completed"
                hint="Tasks that finished successfully (status = succeeded). Stopped chats are not counted here — see Stopped."
              >
                {stats.tasksCompleted}
              </Field>
              <Field
                label="Stopped"
                hint="Tasks you stopped or cancelled (status = cancelled) — e.g. stopping a chat mid-response."
              >
                {stats.tasksStopped}
              </Field>
            </div>
          ) : (
            <p className="text-sm text-[var(--g5-text2)]">
              {statsState.status === "loading"
                ? "Loading activity…"
                : "Activity unavailable."}
            </p>
          )}
        </Card>

        {/* External issue links (Issues). */}
        {onOpenIssue && (
          <div className="lg:col-span-2">
            <WorkspaceIssuesPanel
              workspaceId={workspaceId}
              viewerOwnsWorkspace={!!user?.id && detail.ownerUserId === user.id}
              writeBlockedReason={reviewCopy ? reviewCopyReason : null}
              linkBlockedReason={detail.isBase ? BASE_ISSUE_LINK_REASON : null}
              onOpenIssue={onOpenIssue}
            />
          </div>
        )}

        {/* Spend — framed as estimated token cost, not an amount charged. */}
        <Card title="Estimated token cost">
          <Field label="Estimated total token cost">
            <span className="text-base font-semibold">
              {stats ? formatCost(stats.totalCostUsd) : "—"}
            </span>
          </Field>
          <p className="mt-1 mb-3 text-xs text-[var(--g5-text2)]">
            Estimated from model token usage. These figures are approximate and
            do not represent an amount you were charged. The total includes the
            conversation and task costs shown below.
          </p>
          {stats && (
            <SpendBreakdown
              byTask={stats.byTask}
              totalCostUsd={stats.totalCostUsd}
            />
          )}
        </Card>

        {/* Repos & branches — the workspace's full repository context and
            management surface, relocated from the ReposBar (center-panel
            D75; top-bar D5/D13). Chips keep the ReposBar's remote-identity
            display and hold no controls; tracking state
            renders as a text pill with in-flow detail below each chip
            (g5.workspace.remote-ui.*), and each repo lists the git commands that check it out locally via
            the API's smart-HTTP surface (`/git/:repoId`). */}
        <Card title="Repos & branches">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs uppercase tracking-wide text-[var(--g5-text2)]">
              Repos
            </span>
          </div>
          {repoRows.length > 0 ? (
            <div className="flex flex-col gap-3">
              {repoRows.map(({ repoId, repoName, branch }) => {
                const isRemoteTracking =
                  branch.trackingMode === "remote_tracking";
                const localRepoLabel = `${orgSlug ?? ""}/${repoName}`;
                // For remote-tracking branches, show the remote identity the
                // user sees on their provider in place of the G5-internal
                // one — exactly as the ReposBar chips did (top-bar D22).
                const displayRepoLabel =
                  (isRemoteTracking && branch.remoteRepoName) || localRepoLabel;
                const displayBranch =
                  (isRemoteTracking && branch.remoteBranch) ||
                  branch.branchName;
                // Every user-influenced argument is POSIX single-quoted so
                // the copied commands stay paste-safe and cannot evaluate
                // shell metacharacters in a user-typed external repo/branch
                // name (center-panel D75).
                const cloneCommands = `git clone ${shellQuote(`${window.location.origin}/git/${repoId}`)} ${shellQuote(repoName)}\ncd ${shellQuote(repoName)}\ngit checkout ${shellQuote(branch.branchName)}`;
                return (
                  <div key={repoId} className="flex flex-col gap-1.5">
                    <span
                      data-testid="repo-chip"
                      className={classNames(
                        "inline-flex w-fit items-center gap-1.5",
                        "px-2.5 py-1",
                        "rounded-[var(--g5-radius-sm)]",
                        "border border-[var(--g5-stroke)] border-opacity-60",
                        "bg-[var(--g5-bg2)] bg-opacity-40",
                        "whitespace-nowrap",
                        "cursor-default select-text",
                        "transition-none",
                        "border-[var(--g5-info)]",
                        "text-xs",
                      )}
                      title={`${displayRepoLabel} on ${displayBranch}`}
                      aria-label={`Repository ${displayRepoLabel}, branch ${displayBranch}`}
                    >
                      <Database
                        size={12}
                        className="text-[var(--g5-text2)] shrink-0"
                      />
                      <span className="text-[var(--g5-text1)] font-medium">
                        {displayRepoLabel}
                      </span>
                      <span className="text-[var(--g5-text2)] opacity-40">
                        :
                      </span>
                      <GitBranch
                        size={12}
                        className="text-[var(--g5-info)] shrink-0"
                      />
                      <span className="text-[var(--g5-info)]">
                        {displayBranch}
                      </span>
                    </span>
                    {/* Remote-tracking state (g5.workspace.remote-ui.*): a
                        full-text status pill plus collapsed-by-default detail
                        in page flow, so nothing clips off the full page.
                        Branches without a remote binding read "Local only",
                        with a labelled Publish button (publish-eligible
                        branches) or Make Remote button beside the pill;
                        repository actions are card buttons, never chip
                        glyphs (g5.workspace.remote-ui.actions). Make
                        Remote's host stays mounted after promotion so an
                        open dialog keeps its reconciliation choice. */}
                    {isRemoteTracking && (
                      <BranchTrackingInline
                        workspaceId={workspaceId}
                        repoId={repoId}
                        repoLabel={displayRepoLabel}
                      />
                    )}
                    {personal ? (
                      <div className="flex flex-wrap items-center gap-2">
                        <RemoteStatusPill
                          state="local"
                          label="Private repository — never published"
                        />
                      </div>
                    ) : (
                      <div className="flex flex-wrap items-center gap-2 empty:hidden">
                        {!isRemoteTracking && (
                          <RemoteStatusPill
                            state="local"
                            label="Local only — not published"
                          />
                        )}
                        {!isRemoteTracking &&
                          branch.remoteEligible === true && (
                            <PublishBranchMenu
                              workspaceId={workspaceId}
                              repoId={repoId}
                              repoLabel={displayRepoLabel}
                            />
                          )}
                        <MakeRemoteMenu
                          available={
                            !isRemoteTracking && branch.remoteEligible !== true
                          }
                          workspaceId={workspaceId}
                          repoId={repoId}
                          repoLabel={displayRepoLabel}
                          onMergeRequired={({ resolutionSessionId }) => {
                            // Open the resolution chat at the workspace root;
                            // a relative `?session=` would keep an open file,
                            // doc or task path and open that instead.
                            if (
                              resolutionSessionId &&
                              orgSlug &&
                              workspaceSlug
                            ) {
                              navigate(
                                `${workspacePagePath(orgSlug, workspaceSlug)}${chatLandingQuery(resolutionSessionId)}`,
                              );
                              return;
                            }
                          }}
                        />
                      </div>
                    )}
                    {/* Checkout commands (center-panel D75): display-only
                        strings from origin + repo id + active branch; the
                        smart-HTTP auth (HTTP Basic + JWT) is never embedded. */}
                    <div
                      data-testid="workspace-repo-checkout-commands"
                      className="flex items-start justify-between gap-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2.5 py-1.5"
                    >
                      <pre className="min-w-0 flex-1 overflow-x-auto whitespace-pre font-mono text-[11px] leading-5 text-[var(--g5-text1)]">
                        {cloneCommands}
                      </pre>
                      <button
                        type="button"
                        data-testid="workspace-repo-checkout-copy"
                        onClick={() => {
                          void navigator.clipboard
                            ?.writeText(cloneCommands)
                            .then(() => {
                              setCopiedRepoId(repoId);
                              window.setTimeout(
                                () =>
                                  setCopiedRepoId((current) =>
                                    current === repoId ? null : current,
                                  ),
                                2000,
                              );
                            })
                            .catch(() => {});
                        }}
                        title="Copy checkout commands"
                        aria-label={`Copy checkout commands for ${repoName}`}
                        className="inline-flex shrink-0 items-center justify-center rounded-[var(--g5-radius-sm)] p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]"
                      >
                        {copiedRepoId === repoId ? (
                          <Check size={13} className="text-[var(--g5-ok)]" />
                        ) : (
                          <Copy size={13} />
                        )}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          ) : (
            <p className="text-sm text-[var(--g5-text2)]">No repos.</p>
          )}
          {!personal && (
            <div className="mt-3">
              <Field label="Docs folder">{detail.docsFolderPath}</Field>
            </div>
          )}
        </Card>
      </div>

      {/* Read-only transition confirmation modal */}
      {confirmOpen && !detail.isBase && (
        <Modal
          onClose={closeModal}
          ariaLabel="Confirm read-only transition"
          size="sm"
          closeDisabled={setPolicy.isLoading}
          className="p-5"
          testId="workspace-readonly-modal"
        >
          <>
            <div className="mb-3 flex items-center justify-between">
              <h3 className="text-base font-semibold text-[var(--g5-text0)]">
                {notReadWrite
                  ? "Make workspace editable"
                  : "Make workspace read-only"}
              </h3>
              <button
                type="button"
                data-testid="workspace-readonly-modal-close"
                className="text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"
                onClick={closeModal}
                aria-label="Close"
              >
                <X size={16} />
              </button>
            </div>

            {notReadWrite ? (
              <p className="mb-4 text-sm text-[var(--g5-text1)]">
                This makes the workspace editable again. Document editing and
                chat become available, and any in-progress read-only transition
                is cancelled. The change is instantaneous.
              </p>
            ) : (
              <p className="mb-4 text-sm text-[var(--g5-text1)]">
                Entering read-only freezes new work and drains in-flight tasks
                in the background. Git operations, admin, and preview stay
                available. A live chat, connected editor, or uncommitted drafts
                must be ended or committed/discarded first.
              </p>
            )}

            {isFreezing && (
              <p
                className="mb-3 text-sm text-[var(--g5-warn)]"
                data-testid="workspace-readonly-draining"
              >
                Draining in-flight work in the background… you can close this
                dialog; the badge updates when the transition commits or
                reverts.
              </p>
            )}

            {transitionMessage && (
              <p
                className="mb-3 text-sm text-[var(--g5-text1)]"
                data-testid="workspace-readonly-message"
              >
                {transitionMessage}
              </p>
            )}

            {blockers.length > 0 && (
              <div
                className="mb-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2"
                data-testid="workspace-readonly-blockers"
              >
                <p className="mb-1 text-xs font-semibold text-[var(--g5-warn)]">
                  Outstanding blockers — stop these and retry:
                </p>
                <ul className="list-disc pl-4 text-xs text-[var(--g5-text1)]">
                  {blockers.map((b) => (
                    <li key={`${b.kind}:${b.id}`}>
                      <span className="font-medium">{b.kind}</span>: {b.detail}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {transitionError && (
              <p
                className="mb-3 text-sm text-[var(--g5-blocked)]"
                data-testid="workspace-readonly-error"
              >
                {transitionError}
              </p>
            )}

            <div className="flex justify-end gap-2">
              <Button
                kind="ghost"
                onClick={closeModal}
                disabled={setPolicy.isLoading}
              >
                Close
              </Button>
              <Button
                kind="primary"
                data-testid="workspace-readonly-confirm"
                disabled={setPolicy.isLoading}
                onClick={runTransition}
              >
                {setPolicy.isLoading
                  ? "Working…"
                  : notReadWrite
                    ? "Make editable"
                    : "Make read-only"}
              </Button>
            </div>
          </>
        </Modal>
      )}
      {renaming && canRename && (
        <RenameWorkspaceDialog
          workspace={{ id: detail.id, name: detail.name }}
          isRenaming={renameWorkspace.isLoading}
          errorMessage={renameWorkspace.error?.message ?? null}
          onClose={() => {
            if (renameWorkspace.isLoading) return;
            renameWorkspace.reset();
            setRenaming(false);
          }}
          onConfirm={async (name) => {
            await renameWorkspace.mutateAsync({ name });
            setRenaming(false);
          }}
        />
      )}
    </div>
  );
};
