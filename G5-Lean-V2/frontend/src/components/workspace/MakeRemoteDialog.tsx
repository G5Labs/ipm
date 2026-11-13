// =============================================================================
// G5 Client — Make-Remote dialog (Adopt-Remote git-URL re-point)
// =============================================================================
//
// Make-Remote UX (workspace_service D47). A
// project born on G5 with purely local (`g5-local://`) repos can adopt GitHub
// (or another remote) as its authoritative code store by re-pointing the shared
// org-level `repos.git_url` and binding the branch to `remote_tracking`.
//
// This is the SAME promote endpoint / hook as ordinary [D28] promotion; the
// only additive input is the `gitUrl`. Because the re-point mutates the SHARED
// `repos` row for the WHOLE workspace family and is ONE-WAY (there is no
// supported inverse back to `g5-local://`), the dialog forces an explicit
// org-admin blast-radius acknowledgement before it will submit (D46 §1 —
// "org-admin only, repo_id-stable, one-way", siblings inherit the new git_url).
//
// Outcome routing (D46, backend PromoteRemoteTrackingResponseDTO):
//   - "bound" / "published" → success; the promote hook's invalidations refresh
//     the workspace so the repo chip flips to its remote identity.
//   - "merge_required"      → the backend synchronously opened a remote-tracking
//     merge run and MUST return its `mergeRunId` (D46 §3). The dialog offers
//     the choice of reconciliation (g5.workspace.remote.adopt-provider-history):
//     adopting the reviewed provider history, recommended when only the
//     initial commit would be discarded, or, off Base, the shared agent launch
//     controls, which hand the resulting workspace session to the host. A
//     malformed response that omits `mergeRunId` is treated as an error: the
//     dialog stays OPEN and reports it rather than closing with no route into
//     the workflow.
//
// Base refuses a provider branch that does not exist yet with
// `PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION` and changes nothing. The
// dialog then asks the user to confirm creating it, and on confirm resends the
// same request with `createProviderBranch: true`
// (g5.workspace.remote.base-parity). Back returns to the form with the draft.
//
// Make Remote is available without a deployment toggle.
//
// SOT:
// - .semlift/specs/software_architecture/g5-api/services/workspace_service D47
// - Chat: org-admin / siblings / flag decisions (fe00d54a, 8e636bfd)
// =============================================================================

import { useCallback, useEffect, useRef, useState } from "react";
import { Cloud } from "lucide-react";
import { classNames, Modal } from "../ui/UI";
import {
  usePromoteBranchRepoRemoteTracking,
  useStartBranchMergeRun,
} from "../../hooks/useRemoteTracking";
import { useWorkspace } from "../../hooks/useWorkspace";
import { coordinatedModalFromError } from "./CoordinatedRepoSyncModalState";
import { ApiError } from "../../lib/api";
import { RemoteConflictAgentLauncher } from "./RemoteConflictAgentLauncher";
import { ProviderHistoryAdoption } from "./ProviderHistoryAdoption";
import type { AgentLaunchSelectionV1 } from "../../lib/types";
import {
  makeRemoteOffered,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";

/** Coordinated refusal code: Base's provider branch must be created first. */
export const PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION =
  "PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION";

function isProviderBranchCreationRefusal(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    (error.details.code === PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION ||
      error.code === PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION)
  );
}

/** The exact request a creation confirmation resends. */
interface PromotionRequest {
  remoteBranch: string;
  gitUrl: string;
  createProviderBranch?: boolean;
}

export interface MakeRemoteDialogProps {
  /** Controls mount. */
  open: boolean;
  workspaceId: string;

  /** Repo scope for the per-child promotion. */
  repoId: string;
  /** Display label for the repo being adopted (e.g. `org/repo`). */
  repoLabel?: string;
  onClose: () => void;
  /**
   * Optional success notifier (toast). When omitted the dialog surfaces inline
   * feedback only. `bound`/`published` refresh happens through the promote
   * hook's query invalidations regardless.
   */
  onNotify?: (
    severity: "info" | "warning" | "error",
    title: string,
    message: string,
  ) => void;
  /** Opens the workspace session created for conflict resolution. */
  onMergeRequired?: (merge: {
    mergeRunId: string;
    resolutionSessionId?: string | null;
  }) => void;
}

/**
 * Modal that collects the remote clone URL + remote branch, forces the
 * org-admin blast-radius acknowledgement, and submits the Make-Remote promotion.
 *
 * Thin wrapper: the form body mounts only while `open`, so its input and
 * acknowledgement state resets naturally on each open (no reset effect).
 */
export function MakeRemoteDialog({ open, ...rest }: MakeRemoteDialogProps) {
  if (!open) return null;
  return <MakeRemoteDialogBody {...rest} />;
}

function MakeRemoteDialogBody({
  workspaceId,

  repoId,
  repoLabel,
  onClose,
  onNotify,
  onMergeRequired,
}: Omit<MakeRemoteDialogProps, "open">) {
  const promote = usePromoteBranchRepoRemoteTracking(workspaceId);
  const startMerge = useStartBranchMergeRun(workspaceId, repoId);
  const submitInFlightRef = useRef(false);

  const [gitUrl, setGitUrl] = useState("");
  const [remoteBranch, setRemoteBranch] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  // Set when a `merge_required` response arrives malformed (no `mergeRunId`);
  // keeps the dialog open with an inline error instead of closing.
  const [outcomeError, setOutcomeError] = useState<string | null>(null);
  const [submitInFlight, setSubmitInFlight] = useState(false);
  const [pendingMergeRunId, setPendingMergeRunId] = useState<string | null>(
    null,
  );
  // Set when Base refused an absent provider branch: the request awaiting the
  // user's confirmation to create the branch.
  const [pendingCreate, setPendingCreate] = useState<PromotionRequest | null>(
    null,
  );
  // Move focus with the confirmation step: onto its confirm button when it
  // appears, back to the submit button when Back returns to the form.
  const confirmCreateRef = useRef<HTMLButtonElement | null>(null);
  const submitRef = useRef<HTMLButtonElement | null>(null);
  const createStepShownRef = useRef(false);
  useEffect(() => {
    if (pendingCreate) {
      createStepShownRef.current = true;
      confirmCreateRef.current?.focus();
    } else if (createStepShownRef.current) {
      createStepShownRef.current = false;
      submitRef.current?.focus();
    }
  }, [pendingCreate]);

  const trimmedUrl = gitUrl.trim();
  const trimmedBranch = remoteBranch.trim();
  const urlValid = trimmedUrl.length > 0;
  const branchValid = trimmedBranch.length > 0;
  const submitting =
    submitInFlight || promote.isLoading || startMerge.isLoading;
  const formLocked = submitting || !!pendingMergeRunId || !!pendingCreate;
  // The dialog stays open through a gate change so the draft is kept, but
  // it submits only while Make Remote is still offered to the viewer
  // (`g5.workspace.ui.reader-publication`).
  const gate = useWorkspaceLiveGate();
  const publicationBlocked = gate ? !makeRemoteOffered(gate) : false;
  // Base has no VM, so it reconciles a paused binding only by adopting the
  // provider history (g5.workspace.remote.base-provider-rewrite).
  const workspace = useWorkspace(workspaceId);
  const isBase =
    gate?.kind === "base" ||
    (workspace.status === "ok" && workspace.data.isBase === true);
  const canSubmit =
    urlValid &&
    branchValid &&
    acknowledged &&
    !submitting &&
    !publicationBlocked;

  const errorState = outcomeError
    ? {
        title: "Could not open the merge workflow",
        message: outcomeError,
      }
    : promote.error
      ? coordinatedModalFromError(
          promote.error,
          "Could not make the repository remote",
        )
      : startMerge.error
        ? coordinatedModalFromError(
            startMerge.error,
            "Could not open the merge workflow",
          )
        : null;

  const submitPromotion = useCallback(
    async (request: PromotionRequest) => {
      if (pendingMergeRunId || publicationBlocked) return;
      if (submitInFlightRef.current) return;
      submitInFlightRef.current = true;
      setSubmitInFlight(true);
      setOutcomeError(null);
      try {
        const result = await promote.mutateAsync({ repoId, ...request });
        setPendingCreate(null);

        if (result.outcome === "merge_required") {
          // D46 §3 requires the backend to return the id of the merge run it
          // opened. The wire payload is an untyped cast, so a malformed
          // response can still omit it — keep the dialog OPEN and report the
          // error rather than closing with no route into the merge workflow.
          const mergeRunId = result.mergeRunId;
          if (!mergeRunId) {
            setOutcomeError(
              "The server reported a merge is required but did not return a merge run to open. Nothing was routed — retry, or resolve the divergence from the branch's remote-tracking controls.",
            );
            return;
          }
          setPendingMergeRunId(mergeRunId);
          return;
        } else {
          onNotify?.(
            "info",
            "Repository is now remote",
            result.outcome === "published"
              ? "Published the local branch to the new remote and bound remote tracking."
              : "Bound the repository to remote tracking.",
          );
        }
        onClose();
      } catch (error) {
        // Base refused an absent provider branch and changed nothing: ask to
        // create it instead of reporting an error. Any other failure (and a
        // refusal of a confirmed request) is surfaced inline via
        // promote.error / startMerge.error.
        if (
          !request.createProviderBranch &&
          isProviderBranchCreationRefusal(error)
        ) {
          promote.reset();
          setPendingCreate(request);
        } else {
          setPendingCreate(null);
        }
      } finally {
        submitInFlightRef.current = false;
        setSubmitInFlight(false);
      }
    },
    [promote, pendingMergeRunId, publicationBlocked, repoId, onNotify, onClose],
  );

  const handleSubmit = () => {
    if (!urlValid || !branchValid || !acknowledged) return;
    void submitPromotion({ remoteBranch: trimmedBranch, gitUrl: trimmedUrl });
  };

  const handleConfirmCreate = () => {
    if (!pendingCreate) return;
    void submitPromotion({ ...pendingCreate, createProviderBranch: true });
  };

  const handleStartResolution = async (agentLaunch: AgentLaunchSelectionV1) => {
    if (!pendingMergeRunId || submitInFlightRef.current) return;
    submitInFlightRef.current = true;
    setSubmitInFlight(true);
    try {
      const started = await startMerge.mutateAsync({
        mergeRunId: pendingMergeRunId,
        agentLaunch,
      });
      onNotify?.(
        "warning",
        started.run.resolutionSessionId
          ? "Conflict resolution started"
          : "Remote tracking updated",
        started.run.resolutionSessionId
          ? "A conflict resolution agent was started in this workspace."
          : started.run.phase === "reset_below_baseline"
            ? "The provider history changed. Review the new publication approval checkpoint in remote-tracking controls."
            : "The conflict resolution state was refreshed.",
      );
      onMergeRequired?.({
        mergeRunId: pendingMergeRunId,
        resolutionSessionId: started.run.resolutionSessionId,
      });
      onClose();
    } catch {
      // Keep the bound repository and launch choices available for a retry.
    } finally {
      submitInFlightRef.current = false;
      setSubmitInFlight(false);
    }
  };

  const handleAdopted = () => {
    onNotify?.(
      "info",
      "Provider history adopted",
      isBase
        ? "Base now matches the provider branch; its local commits for this repository were discarded."
        : "The workspace branch was reset to the provider tip. Tracking resumes once G5 confirms the reset.",
    );
    onClose();
  };

  return (
    <Modal
      onClose={onClose}
      closeDisabled={submitting}
      labelledBy="make-remote-dialog-title"
      size="md"
      testId="make-remote-dialog"
      className="min-w-0 [overflow-wrap:anywhere] p-4 text-sm"
    >
      <>
        <div className="flex items-center gap-2">
          <Cloud size={16} className="shrink-0 text-[var(--g5-info)]" />
          <h2
            id="make-remote-dialog-title"
            className="min-w-0 text-base font-semibold text-[var(--g5-text0)]"
          >
            Make repository remote
          </h2>
        </div>

        {repoLabel && (
          <p className="mt-1 text-xs text-[var(--g5-text2)]">
            Adopting a remote for{" "}
            <span className="font-medium text-[var(--g5-text1)]">
              {repoLabel}
            </span>
            .
          </p>
        )}

        <div className="mt-3 grid gap-3 text-xs">
          <label className="grid gap-1">
            <span className="text-[var(--g5-text2)]">Git URL</span>
            <input
              type="text"
              data-testid="make-remote-git-url"
              value={gitUrl}
              onChange={(event) => setGitUrl(event.target.value)}
              disabled={formLocked}
              placeholder="https://github.com/org/repo.git"
              aria-invalid={!urlValid}
              className={classNames(
                "min-w-0 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
                "bg-[var(--g5-bg1)] px-2 py-1 text-sm text-[var(--g5-text0)]",
                "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]",
                "disabled:opacity-60",
              )}
            />
          </label>
          <label className="grid gap-1">
            <span className="text-[var(--g5-text2)]">Remote branch</span>
            <input
              type="text"
              data-testid="make-remote-branch"
              value={remoteBranch}
              onChange={(event) => setRemoteBranch(event.target.value)}
              disabled={formLocked}
              placeholder="main"
              aria-invalid={!branchValid}
              className={classNames(
                "min-w-0 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
                "bg-[var(--g5-bg1)] px-2 py-1 text-sm text-[var(--g5-text0)]",
                "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]",
                "disabled:opacity-60",
              )}
            />
          </label>
        </div>

        {/* Org-admin blast-radius confirmation (D46 §1): the re-point mutates the
            SHARED org-level repo identity for the ENTIRE workspace family and is
            ONE-WAY. Submit stays disabled until it is explicitly acknowledged. */}
        <div
          data-testid="make-remote-blast-radius"
          className="mt-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-3 py-2 text-xs text-[var(--g5-text1)]"
        >
          <span className="font-medium text-[var(--g5-text0)]">
            Org-admin action — this affects every workspace sharing this
            repository
          </span>
          <ul className="mt-1 list-disc space-y-0.5 pl-4">
            <li>
              This re-points the <strong>shared repository identity</strong> for
              the whole workspace family onto the remote you enter above.
            </li>
            <li>
              Sibling workspaces keep their own branches but inherit this remote
              URL; each can publish its branch later.
            </li>
            <li>
              This is <strong>one-way</strong> — the repository cannot be
              returned to a local-only repository afterward.
            </li>
          </ul>
          <label className="mt-2 flex items-start gap-2">
            <input
              type="checkbox"
              data-testid="make-remote-ack"
              checked={acknowledged}
              onChange={(event) => setAcknowledged(event.target.checked)}
              disabled={formLocked}
              className="mt-0.5"
            />
            <span>
              I understand this re-points a shared repository for the whole
              workspace family and cannot be undone.
            </span>
          </label>
        </div>

        {pendingCreate && (
          <div
            data-testid="make-remote-create-branch"
            role="alertdialog"
            aria-labelledby="make-remote-create-branch-title"
            className="mt-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-info)]/40 bg-[var(--g5-info)]/10 px-3 py-2 text-xs text-[var(--g5-text1)]"
          >
            <p
              id="make-remote-create-branch-title"
              className="font-medium text-[var(--g5-text0)]"
            >
              Branch{" "}
              <code className="font-mono">{pendingCreate.remoteBranch}</code>{" "}
              doesn't exist on the provider.
            </p>
            <p className="mt-1">
              Create it from Base's current history? This pushes Base's history
              to the provider once to create the branch.
            </p>
          </div>
        )}

        {pendingMergeRunId && (
          <div data-testid="make-remote-reconcile" className="mt-3 space-y-3">
            <p className="text-xs text-[var(--g5-text1)]">
              {isBase
                ? "Remote tracking is bound, but Base's history differs from the provider branch. Base has no workspace VM to reconcile it; adopt the provider history instead."
                : "Remote tracking is bound, but the histories diverge. Adopt the provider history, or choose an agent to rebase and resolve conflicts in this running workspace."}
            </p>
            <section
              data-testid="make-remote-adopt"
              className="space-y-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-2"
            >
              <h3 className="text-xs font-medium text-[var(--g5-text0)]">
                Adopt provider history
              </h3>
              <ProviderHistoryAdoption
                workspaceId={workspaceId}
                repoId={repoId}
                isBase={isBase}
                autoReview
                onAdopted={handleAdopted}
              />
            </section>
            {!isBase && (
              <section
                data-testid="make-remote-resolve"
                className="space-y-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-2"
              >
                <h3 className="text-xs font-medium text-[var(--g5-text0)]">
                  Resolve with agent
                </h3>
                <RemoteConflictAgentLauncher
                  workspaceId={workspaceId}
                  launchId={pendingMergeRunId}
                  starting={submitting}
                  onStart={handleStartResolution}
                />
              </section>
            )}
          </div>
        )}

        {publicationBlocked && (
          <p
            data-testid="make-remote-blocked"
            role="status"
            className="mt-3 text-xs text-[var(--g5-warn)]"
          >
            Publishing is no longer available here: the workspace is read-only,
            a review copy, or no longer yours. Your entries are kept.
          </p>
        )}
        {errorState && (
          <div
            data-testid="make-remote-error"
            role="alert"
            className="mt-3 min-w-0 rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-2 text-xs text-[var(--g5-blocked)]"
          >
            <p className="font-medium">{errorState.title}</p>
            <p className="mt-1">{errorState.message}</p>
            {errorState.repos?.map((repo) => (
              <div
                key={`${repo.repoId}-${repo.workspaceId ?? repo.branchName}`}
                data-testid={`make-remote-error-repo-${repo.repoName}`}
                className="mt-2 min-w-0 border-t border-[var(--g5-blocked)]/25 pt-2"
              >
                <p className="font-medium">{repo.repoName}</p>
                {repo.error ? <p className="mt-0.5">{repo.error}</p> : null}
              </div>
            ))}
          </div>
        )}

        <div className="mt-4 flex justify-end gap-2">
          {pendingCreate ? (
            <>
              <button
                type="button"
                data-testid="make-remote-create-branch-back"
                onClick={() => setPendingCreate(null)}
                disabled={submitting}
                className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-1.5 text-sm text-[var(--g5-text1)] hover:bg-[var(--g5-bg1)] disabled:opacity-50"
              >
                Back
              </button>
              <button
                type="button"
                ref={confirmCreateRef}
                data-testid="make-remote-create-branch-confirm"
                onClick={handleConfirmCreate}
                disabled={submitting || publicationBlocked}
                className="rounded-[var(--g5-radius-sm)] bg-[var(--g5-info)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-95 disabled:opacity-50"
              >
                {submitting
                  ? "Creating branch..."
                  : "Create branch and Make Remote"}
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                data-testid="make-remote-cancel"
                onClick={onClose}
                disabled={submitting}
                className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-1.5 text-sm text-[var(--g5-text1)] hover:bg-[var(--g5-bg1)] disabled:opacity-50"
              >
                {pendingMergeRunId ? "Close" : "Cancel"}
              </button>
              {!pendingMergeRunId && (
                <button
                  type="button"
                  ref={submitRef}
                  data-testid="make-remote-submit"
                  onClick={handleSubmit}
                  disabled={!canSubmit}
                  className="rounded-[var(--g5-radius-sm)] bg-[var(--g5-info)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-95 disabled:opacity-50"
                >
                  {submitting ? "Making remote..." : "Make remote"}
                </button>
              )}
            </>
          )}
        </div>
      </>
    </Modal>
  );
}

export interface MakeRemoteMenuProps {
  workspaceId: string;
  /** Hide the trigger without unmounting an open conflict-resolution dialog. */
  available?: boolean;

  /** Repo scope for the per-child promotion. */
  repoId: string;
  /** Display label for the repo being adopted (e.g. `org/repo`). */
  repoLabel?: string;
  onNotify?: MakeRemoteDialogProps["onNotify"];
  /**
   * Host routing for the `merge_required` outcome, forwarded to the dialog. The
   * host navigates the user into the workspace-merge-workflow for the started run's
   * merge task (D46 §3).
   */
  onMergeRequired?: MakeRemoteDialogProps["onMergeRequired"];
}

/**
 * The repository card's labelled Make Remote button
 * (g5.workspace.remote-ui.actions) and its dialog. An open dialog stays
 * mounted after promotion turns the card remote-tracked.
 */
export function MakeRemoteMenu({
  workspaceId,
  available = true,

  repoId,
  repoLabel,
  onNotify,
  onMergeRequired,
}: MakeRemoteMenuProps) {
  const [open, setOpen] = useState(false);
  // Binding a remote is the owner's, or on Base also an organization
  // administrator's, and never a review copy's
  // (`g5.workspace.ui.reader-publication`,
  // `g5.governance.review-workspace.exclusions`).
  // An open dialog stays mounted through a gate change, like `available`.
  const gate = useWorkspaceLiveGate();
  const offered = gate ? makeRemoteOffered(gate) : true;
  const showTrigger = available && offered;
  if (!showTrigger && !open) return null;

  return (
    <span className="inline-flex items-center">
      {showTrigger && (
        <button
          type="button"
          data-testid="make-remote-menu-button"
          title="Make this local repository remote"
          onClick={(event) => {
            event.stopPropagation();
            setOpen(true);
          }}
          className="rounded bg-[var(--g5-text0)] px-2 py-1 text-xs font-medium text-[var(--g5-bg0)] hover:opacity-90 disabled:opacity-50"
        >
          Make Remote
        </button>
      )}
      <MakeRemoteDialog
        open={open}
        workspaceId={workspaceId}
        repoId={repoId}
        repoLabel={repoLabel}
        onNotify={onNotify}
        onMergeRequired={onMergeRequired}
        onClose={() => setOpen(false)}
      />
    </span>
  );
}
