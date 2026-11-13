// =============================================================================
// G5 Client — Per-repository-child auto-pull toggle + Publish control
// =============================================================================
//
// Option 1/2 (G5 #1198): the per-child "Auto-pull" toggle and the explicit
// "Publish to Remote" control for a single workspace repository member.
//
//   * A live remote-tracking child renders the "Auto-pull" checkbox. The
//     mutation is optimistic with rollback (useSetBranchRepoAutoPull), and the
//     authoritative server value wins after the settle-time invalidation so a
//     failed toggle never masquerades as the confirmed state (spec §5.5).
//   * A local-only/pinned child renders "Publish to Remote", which creates a
//     new confirmed remote ref and transitions the child to remote-tracking
//     (usePublishBranchRepo).
//   * Base tracks each repository's existing default branch, so a local-only
//     Base child gets no Publish control (it uses Make Remote); its remote-
//     tracking children use the same Auto-pull toggle as any workspace.
//     Unknown children get no control.
//
// The control reads its own state from the workspace detail (`useWorkspace`) so
// it does not couple to the popover's sync-runs data model.
//
// SOT:
// - docs/.../auto_pull_publication_options_spec.md §5.5, §6.2
// - .semlift/specs/software_architecture/g5-api/services/workspace_service D42/D43
// - .semlift/specs/software_architecture/g5-api/routes/workspaces I30
// =============================================================================

import { useWorkspace } from "../../hooks/useWorkspace";
import {
  ownerOrBaseAdmin,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import {
  usePublishBranchRepo,
  useSetBranchRepoAutoPull,
} from "../../hooks/useRemoteTracking";

export interface BranchAutoPullControlProps {
  workspaceId: string;

  repoId: string;
  /** Called after a successful Publish (e.g. to close the enclosing dialog). */
  onActionCompleted?: () => void;
  /**
   * Container className override. Defaults to a bordered top-margin section for
   * standalone use; the tracking popover passes a compact class so the Auto-pull
   * toggle sits with minimal spacing directly under the Pull button.
   */
  className?: string;
}

export function BranchAutoPullControl({
  workspaceId,

  repoId,
  onActionCompleted,
  className,
}: BranchAutoPullControlProps) {
  const ws = useWorkspace(workspaceId);
  const setAutoPull = useSetBranchRepoAutoPull(workspaceId);
  const publish = usePublishBranchRepo(workspaceId);

  const isBase = ws.status === "ok" && ws.data.isBase;
  // Auto-pull checks ownership alone on the server, so a read-only owner
  // keeps it, and on Base an organization administrator sets it too
  // (`g5.workspace.remote.base-admin-pull`); publishing a new branch needs a
  // writable source (`g5.workspace.ui.reader-publication`).
  const gate = useWorkspaceLiveGate();
  const branch =
    ws.status === "ok"
      ? ws.data.branches.find((b) => b.repoId === repoId)
      : undefined;

  // An unresolved branch renders nothing.
  if (!branch) {
    return null;
  }

  const isRemoteTracking = branch.trackingMode === "remote_tracking";
  // Publish creates a new provider branch; Base binds its default branch instead.
  const isLocalOnly = branch.trackingMode === "pinned" && !isBase;
  const autoPullEnabled = branch.autoPullEnabled === true;

  if (!isRemoteTracking && !isLocalOnly) {
    return null;
  }
  const offered = gate
    ? isLocalOnly
      ? gate.canWrite && !gate.reviewCopy && gate.reviewIdentityKnown
      : ownerOrBaseAdmin(gate)
    : true;
  if (!offered) return null;

  return (
    <div
      data-testid="branch-auto-pull-control"
      className={
        className ?? "mt-2 border-t border-[var(--g5-stroke)] pt-2 text-xs"
      }
    >
      {isRemoteTracking ? (
        <>
          <label
            className="flex items-center gap-2"
            data-testid="branch-auto-pull-label"
          >
            <input
              type="checkbox"
              data-testid="branch-auto-pull-toggle"
              checked={autoPullEnabled}
              disabled={setAutoPull.isLoading}
              onChange={(e) =>
                setAutoPull.mutate({
                  repoId,
                  enabled: e.target.checked,
                })
              }
            />
            <span>Auto-pull</span>
          </label>
          <p
            data-testid="branch-auto-pull-hint"
            className="mt-1 text-[var(--g5-text2)]"
          >
            {autoPullEnabled
              ? "Auto: G5 checks the remote in the background and pulls new commits."
              : isBase
                ? "Manual: Base only syncs when you pull explicitly."
                : "Manual: this branch only syncs when you pull or push explicitly."}
          </p>
          {setAutoPull.error && (
            <p
              data-testid="branch-auto-pull-error"
              role="alert"
              className="mt-1 text-[var(--g5-blocked)]"
            >
              {setAutoPull.error.message}
            </p>
          )}
        </>
      ) : (
        <>
          <button
            type="button"
            data-testid="publish-branch-button"
            disabled={publish.isLoading}
            onClick={() => {
              // MutationHook.mutate takes only the input; success feedback is
              // surfaced via the async result so the enclosing dialog can close
              // only after the publish actually confirms. Errors stay visible
              // through `publish.error`, so swallow the rejection here.
              publish
                .mutateAsync({ repoId })
                .then(() => onActionCompleted?.())
                .catch(() => {});
            }}
            className="w-full rounded bg-[var(--g5-text0)] px-2 py-1 font-medium text-[var(--g5-bg0)] hover:opacity-90 disabled:opacity-50"
          >
            {publish.isLoading ? "Publishing..." : "Publish to Remote"}
          </button>
          <p
            data-testid="publish-branch-hint"
            className="mt-1 text-[var(--g5-text2)]"
          >
            This branch is local-only. Publish creates a new remote branch and
            starts tracking it.
          </p>
          {publish.error && (
            <p
              data-testid="publish-branch-error"
              role="alert"
              className="mt-1 text-[var(--g5-blocked)]"
            >
              {publish.error.message}
            </p>
          )}
        </>
      )}
    </div>
  );
}
