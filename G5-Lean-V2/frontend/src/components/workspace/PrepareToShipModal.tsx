// =============================================================================
// Prepare-to-Ship modal — the compact one-stop ship surface opened from the
// status band's Finalize panel. Four fixed sections: 1 approvals summary
// (required requirements counted by their server status, refreshed on the
// dialog's own schedule, + server merge admission), 2 Prepare to ship (PR
// create/update with an editable
// commit-derived description), 3 Update codebase (source-only rebase with the
// Rebase & Merge agent/worker options inline), 4 Ship to parent (target-side PR
// merge, gated on the PR being `ready` and on the server's merge admission).
// Mock provenance: variant B of
// mocks/mockups/davide/approvals/mock-prepare-to-ship.html.
//
// @ref software_architecture/client/workspace-page/prepare-to-ship-modal#I1
// @ref software_architecture/client/workspace-page/workspace-page-shell#D52
// =============================================================================

import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useQueryClient } from "@tanstack/react-query";
import { GithubPrConnection } from "./GithubPrConnection";
import { formatGithubPrError } from "../../lib/githubConnection";
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  Power,
  ChevronDown,
  ClipboardCheck,
  ExternalLink,
  GitMerge,
  RefreshCw,
  Rocket,
  X,
} from "lucide-react";
import type {
  AgentLaunchSelectionV1,
  CreateWorkspacePullRequestResponse,
  SyncAgentConfig,
  WorkspaceApprovalsStateDTO,
  WorkspacePullRequestSummary,
} from "../../lib/types";
import {
  githubPublications,
  workspacePullRequestProviderOutcome,
} from "../../lib/types";
import { currentWorkspacePullRequest } from "../../lib/workspacePullRequests";
import { ApiError } from "../../lib/api";
import {
  useCreateWorkspacePullRequest,
  useMergeWorkspacePullRequest,
  usePreviewWorkspacePullRequestText,
  useSetWorkspacePullRequestAutoMerge,
  useWorkspacePullRequestChecks,
} from "../../hooks/useWorkspace";
import { useOrgScopedConfig } from "../../hooks/useSettings";
import { isOrgExternalChecksBlockMerge } from "../../lib/externalChecksFlags";
import {
  aggregateVerdict,
  hasExternalRepos,
  type ExternalChecksVerdict,
} from "../../lib/externalPrChecks";
import { ExternalPrChecksSection } from "./ExternalPrChecksSection";
import { MergeWhenReadySwitch, StopVmAfterMergeSwitch } from "./MergeWhenReady";
import { MergeOverrideAction } from "./MergeOverrideDialog";
import { mergeOverrideSummary } from "../../lib/mergeOverride";
import {
  AgentLaunchConfigControls,
  initialAgentConfig,
} from "../agent/AgentLaunchConfigControls";
import type { ResolveAgentLaunchPreferences } from "../agent/AgentLaunchConfigControls";
import { classNames, Modal } from "../ui/UI";
import {
  usePublicationBlockedReason,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import { StartAndSendNotice } from "../chat/StartAndSendNotice";
import type {
  RepositoryComparison,
  RepositoryComparisonSummary,
} from "../../lib/workspaceBranches";
import { mergeBlockReason } from "./PullRequestList";
import {
  admissionBlockReason,
  countRequirementStatuses,
  mergeAdmission,
  requirementStatusText,
} from "../../lib/approvalPhases";
import { useShipApprovalsRefresh } from "../../hooks/useShipApprovalsRefresh";
import { MERGE_ADMISSION_UNKNOWN } from "./PullRequestList";
import type { WorkspaceMergeAdmissionDTO } from "../../lib/types";

/**
 * The freshest server verdict between the pull request's cached admission
 * and the workspace-level one (g5.governance.approval.requirements): the one
 * evaluated later wins; otherwise the pull request's own verdict stands (a
 * decision invalidates the PR summaries, so it is refetched at once).
 */
export function freshestAdmission(
  pullRequest: WorkspaceMergeAdmissionDTO | null | undefined,
  workspace: WorkspaceMergeAdmissionDTO | null | undefined,
): WorkspaceMergeAdmissionDTO | null {
  if (!pullRequest) return workspace ?? null;
  if (!workspace) return pullRequest;
  const prAt = Date.parse(pullRequest.evaluatedAt ?? "");
  const wsAt = Date.parse(workspace.evaluatedAt ?? "");
  if (Number.isFinite(wsAt) && (!Number.isFinite(prAt) || wsAt > prAt)) {
    return workspace;
  }
  return pullRequest;
}

type RequirementStatus = "approved" | "rejected" | "pending";

/** One required requirement as section ① shows it. */
interface ShipRequirement {
  key: string;
  name: string;
  state: RequirementStatus;
}

/** An unknown future word reads as not satisfied. */
function requirementStatus(state: string): RequirementStatus {
  return state === "approved" || state === "rejected" ? state : "pending";
}

const REQUIREMENT_STATUS_WORD: Record<RequirementStatus, string> = {
  approved: "Approved",
  rejected: "Rejected",
  pending: "Pending",
};

// Symbols carry the status with the words; the tint only reinforces it, and
// the glyph keeps the theme foreground so it stays readable in every theme.
const REQUIREMENT_STATUS_GLYPH: Record<RequirementStatus, string> = {
  approved: "✓",
  rejected: "×",
  pending: "○",
};

const REQUIREMENT_ICON_TINT: Record<RequirementStatus, string> = {
  approved: "bg-[color-mix(in_srgb,var(--g5-ok)_18%,var(--g5-bg1))]",
  rejected: "bg-[color-mix(in_srgb,var(--g5-blocked)_18%,var(--g5-bg1))]",
  pending: "bg-[color-mix(in_srgb,var(--g5-warn)_18%,var(--g5-bg1))]",
};

export interface PrepareToShipModalProps {
  workspaceId: string;
  /** Source workspace's organization — reads the external-check merge policy. */
  orgId: string;
  /** Dirty live workspace file count from the workspace shell. */
  dirtyCount?: number;
  sourceWorkspaceName: string;
  targetWorkspaceId: string;
  targetWorkspaceName: string;
  /** Per-repository comparison; this update-readiness surface renders behind only (D3). */
  repositoryComparison: RepositoryComparisonSummary | null;
  /** The shell's already-fetched approvals state (shell D50). */
  shipApprovals?: WorkspaceApprovalsStateDTO | null;
  /** Open PR for the target, from the shell's source-role PR list. */
  existingPullRequest: WorkspacePullRequestSummary | null;
  agentConfig: SyncAgentConfig;
  agentConfigOptions?: SyncAgentConfig[];
  resolveAgentLaunch: ResolveAgentLaunchPreferences;
  isRebaseSubmitting?: boolean;
  rebaseErrorMessage?: string | null;
  destinationStatus?: ReactNode;
  /** The shell's existing rebase confirm flow (preflight → choice → start). */
  onStartRebase: (
    agentConfig: SyncAgentConfig,
    agentLaunch: AgentLaunchSelectionV1,
  ) => void;
  /** Opens the workspace Approvals tab; closes the modal when followed. */
  onReviewApprovals?: () => void;
  /** Opens the chat Merge with agent started. */
  onAgentStarted?: (sessionId: string) => void;
  /** Why a Start under way waits for capacity, when the shell knows. */
  startWaitReason?: string | null;
  onClose: () => void;
}

interface CreateResultNotice {
  warning: boolean;
  text: string;
}

/** Honesty copy for the create result (pull-request-panel D3 posture). */
function createResultNotice(
  result: CreateWorkspacePullRequestResponse,
): CreateResultNotice {
  const outcome = workspacePullRequestProviderOutcome(result.pullRequest);
  if (outcome.hasSkippedRepos) {
    const label = outcome.allReposSkipped
      ? "GitHub PR not created"
      : "some GitHub PRs not created";
    return {
      warning: true,
      text: result.alreadyExisted
        ? `Local PR updated — ${label}.`
        : `Local PR created — ${label}.`,
    };
  }
  return {
    warning: false,
    text: result.alreadyExisted ? "PR updated." : "PR created.",
  };
}

function repositoryComparisonLabel({
  behindBase,
}: RepositoryComparison): string {
  if (behindBase == null) return "comparison unknown";
  return behindBase > 0 ? `${behindBase} behind` : "up to date";
}

function SectionHeading({
  step,
  title,
  done,
}: {
  step: number;
  title: string;
  done?: boolean;
}) {
  return (
    <div className="flex items-center gap-2">
      <span
        className={classNames(
          "flex h-5 w-5 shrink-0 items-center justify-center rounded-full border text-[11px] font-bold",
          done
            ? "border-[var(--g5-ok)] bg-[var(--g5-ok)] text-[rgb(8,12,24)]"
            : "border-[var(--g5-stroke)] bg-[var(--g5-bg2)] text-[var(--g5-text2)]",
        )}
      >
        {done ? "✓" : step}
      </span>
      <span className="text-[12.5px] font-semibold text-[var(--g5-text0)]">
        {title}
      </span>
    </div>
  );
}

export function PrepareToShipModal({
  workspaceId,
  orgId,
  sourceWorkspaceName,
  targetWorkspaceId,
  targetWorkspaceName,
  repositoryComparison,
  shipApprovals,
  existingPullRequest,
  agentConfig,
  agentConfigOptions,
  resolveAgentLaunch,
  isRebaseSubmitting = false,
  rebaseErrorMessage = null,
  destinationStatus,
  onStartRebase,
  onReviewApprovals,
  onAgentStarted,
  startWaitReason,
  onClose,
}: PrepareToShipModalProps) {
  const previewText = usePreviewWorkspacePullRequestText(workspaceId);
  const createPullRequest = useCreateWorkspacePullRequest(workspaceId);
  const [pendingAction, setPendingAction] = useState<"create" | "ship" | null>(
    null,
  );
  const pendingActionRef = useRef<"create" | "ship" | null>(null);
  // The dialog stays open through a gate change so its description and
  // selections are kept, but it publishes only while publication is still
  // the viewer's (`g5.workspace.ui.reader-publication`).
  const publicationBlocked = usePublicationBlockedReason();
  // Start and ship with agent (g5.chat.start-and-send): the owner's stopped
  // or failed VM makes the agent control the compound action that records
  // Start consent with the chat; a Start already under way just queues.
  const liveGate = useWorkspaceLiveGate();
  const startAndShip = liveGate?.canStart ?? false;
  const startUnderWay =
    liveGate != null &&
    liveGate.owner &&
    (liveGate.kind === "starting" || liveGate.kind === "queued");
  const mergePullRequest = useMergeWorkspacePullRequest(targetWorkspaceId);
  const setAutoMerge = useSetWorkspacePullRequestAutoMerge(targetWorkspaceId);
  const queryClient = useQueryClient();

  // Freshness (ontology D3, issue-1995 plan D9): Base-side head advancement
  // fires no event on this viewer's channel, so the shell's cached detail —
  // and with it the behind-base gate — can be stale. Invalidate the detail
  // query exactly once per modal open so the ship gate reflects divergence
  // as of the moment the user opens it. Ref-guarded one-shot: a background
  // refetch must not re-fire it (guarded self-heal effect pattern).
  const detailInvalidatedRef = useRef(false);
  useEffect(() => {
    if (detailInvalidatedRef.current) return;
    detailInvalidatedRef.current = true;
    void queryClient.invalidateQueries({
      queryKey: ["workspace", workspaceId],
    });
  }, [queryClient, workspaceId]);

  const [createdResult, setCreatedResult] =
    useState<CreateWorkspacePullRequestResponse | null>(null);
  const [mergedPullRequest, setMergedPullRequest] =
    useState<WorkspacePullRequestSummary | null>(null);

  // --- section 1: approvals summary — the required approval requirements,
  // counted once each by their one server status, and the server's merge
  // admission (g5.workbench.ship-approvals, g5.governance.approval.status).
  // The open dialog reads the shared response on its own schedule; the
  // shell's copy only stands in until that read answers.
  const liveApprovals = useShipApprovalsRefresh(workspaceId);
  const approvalsState =
    liveApprovals.status === "ok" ? liveApprovals.data : shipApprovals;
  const requirements = useMemo(
    () => approvalsState?.outgoing?.requirements ?? [],
    [approvalsState],
  );

  // --- section 2: PR description prefill + send-only-when-edited -----------
  const [description, setDescription] = useState<string>(
    existingPullRequest?.descriptionOverride ?? "",
  );
  const [descriptionOpen, setDescriptionOpen] = useState(false);
  // Backend spec D16: honest body-source attribution for the prefill.
  const [previewSource, setPreviewSource] = useState<
    "projectDoc" | "derived" | "legacy" | null
  >(null);
  // Baseline the edit check compares against: the stored override, else the
  // derived preview body (backend spec D15 — untouched creates omit the
  // field so per-repo derivation is preserved).
  const [descriptionBaseline, setDescriptionBaseline] = useState<string>(
    existingPullRequest?.descriptionOverride ?? "",
  );
  const previewRequestedRef = useRef(false);
  const previewMutateRef = useRef(previewText.mutateAsync);
  useEffect(() => {
    previewMutateRef.current = previewText.mutateAsync;
  });
  const hasStoredOverride = existingPullRequest?.descriptionOverride != null;

  useEffect(() => {
    if (previewRequestedRef.current || hasStoredOverride) return;
    previewRequestedRef.current = true;
    let cancelled = false;
    previewMutateRef
      .current({ targetWorkspaceId })
      .then((preview) => {
        if (cancelled) return;
        setDescription((current) => (current ? current : preview.body));
        setDescriptionBaseline((current) => (current ? current : preview.body));
        setPreviewSource(preview.source ?? null);
      })
      .catch(() => {
        // Preview is best-effort: the section stays usable with an empty
        // description and the derivation note below.
      });
    return () => {
      cancelled = true;
    };
  }, [hasStoredOverride, targetWorkspaceId]);

  const descriptionEdited = description.trim() !== descriptionBaseline.trim();

  const activePullRequest =
    mergedPullRequest ??
    currentWorkspacePullRequest(
      existingPullRequest,
      createdResult?.pullRequest,
    );
  const hasOpenPullRequest = activePullRequest?.status === "open";
  // The server verdict is the only readiness source: the freshest of the
  // PR's cached admission and the workspace projection. Without one the
  // readiness is unknown — never recomputed from the roster.
  const admission = freshestAdmission(
    activePullRequest?.mergeAdmission,
    mergeAdmission(approvalsState),
  );
  const hasRequirements =
    requirements.length > 0 || (admission?.requirements.length ?? 0) > 0;
  const admissionKnown = admission !== null || !approvalsState?.outgoing;
  const approvalsReady = hasRequirements && admission?.admitted === true;
  // One entry per required requirement in the response's (alphabetical)
  // order; an older payload without the list falls back to the admission's.
  const shownRequirements: ShipRequirement[] =
    requirements.length > 0
      ? requirements.map((entry) => ({
          key: entry.definition.id,
          name: entry.definition.name,
          state: requirementStatus(entry.state),
        }))
      : (admission?.requirements ?? []).map((entry) => ({
          key: entry.definitionId,
          name: entry.definitionName,
          state: requirementStatus(entry.state),
        }));
  const shownCounts = countRequirementStatuses(shownRequirements);
  // Requirements still rejected or pending, named in the message.
  const shownUnsatisfied = shownRequirements.filter(
    (entry) => entry.state !== "approved",
  );
  const resultNotice =
    createdResult && activePullRequest
      ? createResultNotice({ ...createdResult, pullRequest: activePullRequest })
      : null;
  const dirtyDrafts = createdResult?.dirtyDrafts ?? [];
  const providerLinks = (
    activePullRequest ? githubPublications(activePullRequest) : []
  ).filter((repo) => repo.providerPrUrl);
  const providerFailures = (
    activePullRequest ? githubPublications(activePullRequest) : []
  ).filter(
    (repo) =>
      repo.providerStatus === "failed" ||
      repo.providerStatus === "action_required",
  );
  const providerNeedsAttention = activePullRequest
    ? Boolean(activePullRequest.blockingMessage) ||
      workspacePullRequestProviderOutcome(activePullRequest).hasSkippedRepos
    : false;
  // An edited description replaces existing GitHub PR bodies too, as
  // `g5d pr update --body` does (g5.governance.pr.body); unedited, it is not
  // sent and GitHub keeps its body.
  const providerPrExists = (
    activePullRequest ? githubPublications(activePullRequest) : []
  ).some((repo) => repo.providerPrNumber != null);

  // --- external PR status (governance g5.governance.pr.checks.surface) ------
  // Read the live check/status snapshot for the open PR; display it and, when
  // the org requires passing checks, pre-gate the Ship button (the server
  // re-reads and is authoritative at merge).
  const checksQuery = useWorkspacePullRequestChecks(
    workspaceId,
    hasOpenPullRequest ? activePullRequest?.id : null,
    hasOpenPullRequest && providerPrExists,
  );
  const checksSnapshot = checksQuery.status === "ok" ? checksQuery.data : null;
  const externalVerdict: ExternalChecksVerdict | null = hasExternalRepos(
    checksSnapshot,
  )
    ? aggregateVerdict(checksSnapshot)
    : null;
  const orgConfig = useOrgScopedConfig(orgId);
  const externalChecksBlockMerge =
    orgConfig.status === "ok" && isOrgExternalChecksBlockMerge(orgConfig.data);
  // Policy blocks Ship until external checks are passing. A required check
  // still pending or not yet reported (or a status not yet known) blocks too,
  // so a just-opened PR cannot merge before its required checks report.
  const externalPolicyBlocks =
    externalChecksBlockMerge &&
    providerPrExists &&
    externalVerdict !== "passing";
  const externalPolicyBlockReason =
    externalVerdict === "failing"
      ? "External checks are failing. Your organization requires passing external checks before merge."
      : "Required external checks have not finished. Your organization requires passing external checks before merge.";

  const handleCreate = useCallback(async () => {
    if (pendingActionRef.current != null) return;
    if (publicationBlocked !== null) return;
    // An edited-to-blank textarea sends the explicit blank so a stored
    // override is CLEARED, never silently retained (backend D15 three-state).
    const sendDescription = descriptionEdited;
    pendingActionRef.current = "create";
    setPendingAction("create");
    try {
      try {
        const result = await createPullRequest.mutateAsync({
          targetWorkspaceId,
          ...(sendDescription ? { description: description.trim() } : {}),
          // Update PR sends `mode: update` and saves first; `g5d pr update` sends `save: false`.
          ...(existingPullRequest ? { mode: "update" as const } : {}),
        });
        setCreatedResult(result);
        if (sendDescription) {
          const stored = result.pullRequest.descriptionOverride ?? "";
          setDescriptionBaseline(stored);
          setDescription(stored);
        }
      } catch {
        // Mutation error state renders inline below.
      }
    } finally {
      pendingActionRef.current = null;
      setPendingAction(null);
    }
  }, [
    createPullRequest,
    description,
    descriptionEdited,
    existingPullRequest,
    publicationBlocked,
    targetWorkspaceId,
  ]);

  // --- section 3: update codebase (ported Rebase & Merge options) ----------
  const [rebaseOptionsOpen, setRebaseOptionsOpen] = useState(false);
  const [selectedAgentConfig, setSelectedAgentConfig] =
    useState<SyncAgentConfig>(() =>
      initialAgentConfig(agentConfig, agentConfigOptions),
    );
  const [confirmedAgentLaunch, setConfirmedAgentLaunch] =
    useState<AgentLaunchSelectionV1 | null>(null);
  const canStartRebase =
    !isRebaseSubmitting &&
    pendingAction == null &&
    !createPullRequest.isLoading &&
    !mergePullRequest.isLoading &&
    !!confirmedAgentLaunch &&
    publicationBlocked === null;
  // Three-state Update gate over per-repository incoming history. Ahead is
  // intentionally omitted: an ahead-only branch already contains the target
  // history, and raw counts may include rebase-preservation commits (D3).
  const comparisonState: "behind" | "unknown" | "upToDate" | null =
    repositoryComparison == null
      ? null
      : repositoryComparison.hasBehind
        ? "behind"
        : repositoryComparison.hasUnknownBehind
          ? "unknown"
          : "upToDate";
  const updateStepDone = comparisonState === "upToDate";

  // --- section 4: ship to base — gated on the PR being ready AND the
  // server's merge admission (g5.governance.approval.requirements); an
  // unknown verdict blocks until the next refresh.
  const shipped =
    mergedPullRequest != null || activePullRequest?.status === "merged";
  const showMergeOptions =
    !shipped && hasOpenPullRequest && activePullRequest != null;
  // Unsatisfied approvals read exactly `Approval(s) required before merging`,
  // never naming requirements (g5.workbench.ship-to-parent).
  const shipAdmissionReason = admission
    ? admissionBlockReason(admission)
    : admissionKnown
      ? null
      : MERGE_ADMISSION_UNKNOWN;
  const canShip =
    !shipped &&
    hasOpenPullRequest &&
    activePullRequest != null &&
    activePullRequest.localMergeStatus === "ready" &&
    shipAdmissionReason === null &&
    !externalPolicyBlocks &&
    !mergePullRequest.isLoading &&
    !setAutoMerge.isLoading &&
    pendingAction == null;
  // The admission refusal is named first: it is the reason to act on before
  // a PR exists, and the only one approvals ever add.
  const shipBlockReason = shipped
    ? null
    : (shipAdmissionReason ??
      (!activePullRequest || !hasOpenPullRequest
        ? "Create the PR first (Prepare to ship)"
        : activePullRequest.localMergeStatus !== "ready"
          ? mergeBlockReason(activePullRequest)
          : externalPolicyBlocks
            ? externalPolicyBlockReason
            : null));

  // Merge when ready as the last change returned it, until a fresher read of
  // the pull request arrives (g5.git.pr-auto-merge).
  const [autoMergeUpdate, setAutoMergeUpdate] = useState<{
    replaces: WorkspacePullRequestSummary;
    result: WorkspacePullRequestSummary;
  } | null>(null);
  const currentAutoMerge =
    activePullRequest && autoMergeUpdate?.replaces === activePullRequest
      ? autoMergeUpdate.result
      : activePullRequest;
  const autoMergeRequest = currentAutoMerge?.autoMergeRequest ?? null;
  // Ship with agent is always offered once a PR is open
  // (g5.workbench.ship-to-parent); it becomes the primary control when
  // something keeps Ship to parent from merging now: admission, branch
  // state, required checks, or a refusal from the merge itself (not while
  // the admission is still being read).
  // A permission denial (403) is not a blocker an agent can clear
  // (g5.workbench.ship-to-parent, #1104 decision 4): show it as its own
  // reason, never as "Ship with agent resolves it". For a member the cause is
  // almost always that the parent has no attached approval requirement, which
  // the approvals section already knows, so name it.
  const mergePermissionDenied =
    mergePullRequest.error instanceof ApiError &&
    mergePullRequest.error.status === 403;
  const permissionDeniedReason = mergePermissionDenied
    ? requirements.length === 0
      ? "You can ship this once the parent has an approval requirement. It has none, so only the parent's owner can merge into it."
      : (mergePullRequest.error?.message ?? "Insufficient permissions.")
    : null;
  const shipBlocked =
    !shipped &&
    hasOpenPullRequest &&
    activePullRequest != null &&
    shipAdmissionReason !== MERGE_ADMISSION_UNKNOWN &&
    (shipBlockReason !== null ||
      (mergePullRequest.error != null && !mergePermissionDenied));
  // The agent chat's request key (g5.chat.request-key) outlives a failed
  // attempt: the chat may have committed before the decision did, and a
  // retry under the same key replays that chat instead of starting a second
  // one. A refusal (409) means the key's chat cannot be continued (withdrawn
  // by an early-ended Start, the pull request no longer open), so the next
  // attempt takes a fresh key; success clears it.
  const agentRequestKeyRef = useRef<string | null>(null);
  const changeAutoMerge = useCallback(
    async (enabled: boolean, withAgent: boolean) => {
      if (!activePullRequest || (withAgent && !confirmedAgentLaunch)) return;
      const requestKey = (agentRequestKeyRef.current ??= crypto.randomUUID());
      try {
        const result = await setAutoMerge.mutateAsync({
          pullRequestId: activePullRequest.id,
          enabled,
          agent:
            withAgent && confirmedAgentLaunch
              ? {
                  agentLaunch: confirmedAgentLaunch,
                  requestKey,
                  ...(startAndShip ? { startCompute: true } : {}),
                }
              : undefined,
        });
        agentRequestKeyRef.current = null;
        setAutoMergeUpdate({
          replaces: activePullRequest,
          result: result.pullRequest,
        });
        if (result.chatSessionId) onAgentStarted?.(result.chatSessionId);
      } catch (error) {
        // Mutation error state renders inline below.
        if (error instanceof ApiError && error.status === 409) {
          agentRequestKeyRef.current = null;
        }
      }
    },
    [
      activePullRequest,
      confirmedAgentLaunch,
      onAgentStarted,
      setAutoMerge,
      startAndShip,
    ],
  );
  // Who cannot start the source workspace's agent is told why; the agent
  // choice still resolving only holds the button.
  const shipWithAgentUnavailable = publicationBlocked;

  const changeStopVmAfterMerge = async (selected: boolean) => {
    if (!activePullRequest) return;
    try {
      const result = await setAutoMerge.mutateAsync({
        pullRequestId: activePullRequest.id,
        stopVmAfterMerge: selected,
      });
      setAutoMergeUpdate({
        replaces: activePullRequest,
        result: result.pullRequest,
      });
    } catch {
      // Mutation error state renders inline below.
    }
  };

  const handleShip = useCallback(async () => {
    if (!activePullRequest || pendingActionRef.current != null) return;
    pendingActionRef.current = "ship";
    setPendingAction("ship");
    try {
      try {
        const result = await mergePullRequest.mutateAsync({
          pullRequestId: activePullRequest.id,
        });
        setMergedPullRequest(result.pullRequest);
      } catch {
        // Mutation error state renders inline below.
      }
    } finally {
      pendingActionRef.current = null;
      setPendingAction(null);
    }
  }, [activePullRequest, mergePullRequest]);

  const actionBusy =
    isRebaseSubmitting ||
    pendingAction != null ||
    createPullRequest.isLoading ||
    mergePullRequest.isLoading ||
    setAutoMerge.isLoading;
  const shipWithAgentDisabled =
    shipped ||
    !hasOpenPullRequest ||
    activePullRequest == null ||
    actionBusy ||
    shipWithAgentUnavailable !== null ||
    !confirmedAgentLaunch;

  const sectionClass =
    "rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3.5 py-3";

  return (
    <Modal
      onClose={onClose}
      labelledBy="prepare-to-ship-title"
      size="lg"
      closeDisabled={actionBusy}
      testId="prepare-to-ship-modal"
      className="p-0"
    >
      <div className="flex items-center gap-2 border-b border-[var(--g5-stroke)] px-4 py-3">
        <Rocket size={16} className="text-[var(--g5-ok)]" />
        <h2
          id="prepare-to-ship-title"
          className="min-w-0 flex-1 truncate text-sm font-semibold text-[var(--g5-text0)]"
        >
          Prepare to Ship — {sourceWorkspaceName} → {targetWorkspaceName}
        </h2>
        <button
          type="button"
          data-testid="prepare-to-ship-close-button"
          onClick={onClose}
          disabled={actionBusy}
          className="flex h-7 w-7 items-center justify-center rounded text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] disabled:cursor-not-allowed disabled:opacity-50"
          title="Close"
        >
          <X size={15} />
        </button>
      </div>

      <div className="max-h-[75vh] space-y-2.5 overflow-y-auto px-4 py-4">
        {destinationStatus}
        {/* ================= 1 · Approvals summary ================= */}
        <section
          className={sectionClass}
          data-testid="prepare-to-ship-approvals"
        >
          <div className="flex flex-wrap items-center gap-2.5">
            <SectionHeading step={1} title="Approvals" done={approvalsReady} />
            <span className="flex-1" />
            {hasRequirements ? (
              <span
                data-testid="prepare-to-ship-approvals-counts"
                className="inline-flex items-center rounded-md bg-[var(--g5-bg2)] px-2 py-0.5 text-[10.5px] font-medium tabular-nums text-[var(--g5-text0)]"
              >
                {requirementStatusText(shownCounts)}
              </span>
            ) : null}
          </div>
          {hasRequirements ? (
            <div className="mt-2 flex min-w-0 flex-wrap items-center gap-1.5">
              <span
                data-testid="prepare-to-ship-approvals-icons"
                className="flex flex-wrap items-center gap-1"
              >
                {shownRequirements.map((entry) => (
                  <span
                    key={entry.key}
                    data-testid="prepare-to-ship-approval-icon"
                    data-state={entry.state}
                    role="img"
                    aria-label={`${entry.name}: ${REQUIREMENT_STATUS_WORD[entry.state]}`}
                    title={`${entry.name} · ${REQUIREMENT_STATUS_WORD[entry.state]}`}
                    className={classNames(
                      "inline-grid h-[18px] w-[18px] shrink-0 place-items-center rounded-full border border-[var(--g5-stroke)] text-[11px] leading-none text-[var(--g5-text0)]",
                      REQUIREMENT_ICON_TINT[entry.state],
                    )}
                  >
                    <span aria-hidden="true">
                      {REQUIREMENT_STATUS_GLYPH[entry.state]}
                    </span>
                  </span>
                ))}
              </span>
              <span
                data-testid="prepare-to-ship-approvals-message"
                className="min-w-0 flex-1 text-[11px] text-[var(--g5-text2)]"
              >
                {shownUnsatisfied.length > 0
                  ? shownUnsatisfied
                      .map(
                        (entry) =>
                          `${entry.name} ${REQUIREMENT_STATUS_WORD[entry.state].toLowerCase()}`,
                      )
                      .join("; ")
                  : "All approval requirements are approved"}
              </span>
            </div>
          ) : (
            <p className="mt-2 text-[11px] text-[var(--g5-text2)]">
              No approvals are required for this target.
            </p>
          )}
          {onReviewApprovals ? (
            <div className="mt-2 flex items-center gap-2">
              <button
                type="button"
                data-testid="prepare-to-ship-review-approvals"
                disabled={actionBusy}
                onClick={() => {
                  if (actionBusy) return;
                  onReviewApprovals();
                  onClose();
                }}
                className="inline-flex items-center gap-1.5 rounded-md border border-[var(--g5-info)]/20 bg-[var(--g5-info)]/10 px-2.5 py-1 text-[11px] font-medium text-[var(--g5-info)] hover:bg-[var(--g5-info)]/[0.13] disabled:cursor-not-allowed disabled:opacity-50"
              >
                <ClipboardCheck size={12} className="shrink-0" />
                View approvals
              </button>
            </div>
          ) : null}
        </section>

        {/* ================= 2 · Prepare to ship ================= */}
        <section className={sectionClass} data-testid="prepare-to-ship-prepare">
          <div className="flex flex-wrap items-center gap-2.5">
            <SectionHeading
              step={2}
              title="Prepare to ship"
              done={shipped || (hasOpenPullRequest && !providerNeedsAttention)}
            />
            <span className="flex-1" />
            {activePullRequest ? (
              <span className="truncate rounded-full border border-[var(--g5-info)]/30 bg-[var(--g5-info)]/10 px-2 py-0.5 text-[10.5px] font-medium text-[var(--g5-info)]">
                {activePullRequest.title}
              </span>
            ) : null}
          </div>
          {publicationBlocked !== null && (
            <p
              data-testid="prepare-to-ship-blocked"
              role="status"
              className="mt-2 text-xs text-[var(--g5-text2)]"
            >
              {publicationBlocked}
            </p>
          )}
          {activePullRequest?.blockingMessage && (
            <p
              data-testid="prepare-to-ship-error"
              className="mt-2 text-xs text-[var(--g5-blocked)]"
            >
              {activePullRequest.blockingMessage}
            </p>
          )}
          <p className="mt-1.5 text-[11px] text-[var(--g5-text2)]">
            Commits pending changes, including new files, saves them to G5, and
            opens a pull request into {targetWorkspaceName}. For connected
            repositories, it also pushes the changes and opens the GitHub PR.
            Nothing merges yet.
          </p>

          <button
            type="button"
            data-testid="prepare-to-ship-description-toggle"
            aria-expanded={descriptionOpen}
            aria-controls="prepare-to-ship-description"
            onClick={() => setDescriptionOpen((open) => !open)}
            className="mt-2 flex w-full items-center gap-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-1.5 text-left text-[11px] font-semibold text-[var(--g5-text1)] hover:bg-[var(--g5-overlay)]"
          >
            <ChevronDown
              size={11}
              aria-hidden="true"
              className={classNames(
                "shrink-0 transition-transform",
                descriptionOpen ? "rotate-180" : "",
              )}
            />
            PR description
            <span className="min-w-0 flex-1 truncate text-[10.5px] font-normal text-[var(--g5-text2)]">
              {providerPrExists && !descriptionEdited
                ? "unchanged on GitHub unless edited — expand to replace it"
                : previewText.isLoading
                  ? "deriving from your commits…"
                  : hasStoredOverride && !descriptionEdited
                    ? "stored description — expand to edit"
                    : previewText.error
                      ? "derivation unavailable — write your own"
                      : previewSource === "projectDoc"
                        ? "pre-populated from pr_body in Project Docs — expand to edit"
                        : "pre-populated from your commits — expand to edit"}
            </span>
          </button>
          {descriptionOpen ? (
            <textarea
              id="prepare-to-ship-description"
              data-testid="prepare-to-ship-description"
              aria-label="PR description"
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              spellCheck={false}
              className="mt-1.5 block max-h-64 min-h-28 w-full resize-y rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-2 font-mono text-[11px] leading-relaxed text-[var(--g5-text1)] focus:border-[var(--g5-info)]/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]/40"
            />
          ) : null}

          <div className="mt-2.5 flex flex-wrap items-center gap-2.5">
            <button
              type="button"
              data-testid="prepare-to-ship-create"
              onClick={handleCreate}
              disabled={actionBusy || shipped || publicationBlocked !== null}
              title={publicationBlocked ?? undefined}
              className={classNames(
                "inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium",
                actionBusy || shipped || publicationBlocked !== null
                  ? "cursor-not-allowed bg-[var(--g5-info)]/50 text-white/80"
                  : "bg-[var(--g5-info)] text-white hover:bg-[var(--g5-info)]/90",
              )}
            >
              <Rocket size={12} />
              {pendingAction === "create" || createPullRequest.isLoading
                ? "Preparing…"
                : hasOpenPullRequest || shipped
                  ? "Update Pull Request (PR)"
                  : "Prepare to ship - open Pull Request (PR)"}
            </button>
            {resultNotice ? (
              <span
                data-testid="prepare-to-ship-result"
                className={classNames(
                  "text-[11px]",
                  resultNotice.warning
                    ? "text-[var(--g5-warn)]"
                    : "text-[var(--g5-ok)]",
                )}
              >
                {resultNotice.text}
              </span>
            ) : null}
            {providerLinks.map((repo) => (
              <a
                key={repo.id}
                href={repo.providerPrUrl ?? undefined}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 text-[11px] text-[var(--g5-info)] hover:underline"
              >
                <ExternalLink size={11} aria-hidden="true" />
                {repo.providerRepoFullName}#{repo.providerPrNumber}
              </a>
            ))}
          </div>
          {providerPrExists ? (
            <ExternalPrChecksSection
              state={checksQuery}
              verdict={externalVerdict}
              policyOn={externalChecksBlockMerge}
            />
          ) : null}
          {providerFailures.length > 0 ? (
            <div
              role="alert"
              data-testid="prepare-to-ship-provider-errors"
              className="mt-2 rounded-md border border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-2.5 py-1.5 text-[11px] text-[var(--g5-warn)]"
            >
              {providerFailures.map((repo) => (
                <p key={repo.id}>
                  {repo.providerRepoFullName}:{" "}
                  {repo.lastError
                    ? formatGithubPrError(repo.lastError)
                    : "GitHub PR needs attention."}
                </p>
              ))}
              <GithubPrConnection
                errors={providerFailures.map((repo) => repo.lastError)}
              />
              <p className="mt-1">
                Saved commits are kept. Resolve the issue, then choose Update
                Pull Request.
              </p>
            </div>
          ) : null}
          {createPullRequest.error ? (
            <p className="mt-2 rounded-md border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-2.5 py-1.5 text-[11px] text-[var(--g5-blocked)]">
              {createPullRequest.error.message}
            </p>
          ) : null}
          {dirtyDrafts.length > 0 ? (
            <p className="mt-2 flex items-start gap-1.5 text-[10.5px] text-[var(--g5-warn)]">
              <AlertTriangle size={11} className="mt-0.5 shrink-0" />
              Pending changes to include when you update this PR (
              {dirtyDrafts
                .map((draft) => `${draft.repoName}: ${draft.dirtyCount}`)
                .join(", ")}
              ).
            </p>
          ) : null}
        </section>

        {/* ================= 3 · Update codebase ================= */}
        <section className={sectionClass} data-testid="prepare-to-ship-update">
          <div className="flex flex-wrap items-center gap-2.5">
            <SectionHeading
              step={3}
              title={`Get the latest changes from ${targetWorkspaceName}`}
              done={updateStepDone}
            />
            <span className="flex-1" />
            {comparisonState && repositoryComparison ? (
              <span
                data-testid="prepare-to-ship-behind-count"
                data-state={comparisonState ?? undefined}
                className={classNames(
                  "max-w-full rounded-full border px-2 py-0.5 text-right text-[10.5px] font-medium break-words",
                  comparisonState === "behind"
                    ? "border-[var(--g5-warn)]/30 bg-[var(--g5-warn)]/10 text-[var(--g5-warn)]"
                    : comparisonState === "unknown"
                      ? "border-[var(--g5-stroke)] bg-[var(--g5-overlay)] text-[var(--g5-text2)]"
                      : "border-[var(--g5-ok)]/30 bg-[var(--g5-ok)]/10 text-[var(--g5-ok)]",
                )}
              >
                {repositoryComparison.allBehindKnownZero
                  ? `Up to date with ${targetWorkspaceName}`
                  : repositoryComparison.repositories.map(
                      (repository, index) => (
                        <Fragment key={repository.repoId}>
                          {index > 0 ? (
                            <>
                              {" "}
                              <span
                                role="separator"
                                aria-orientation="vertical"
                                className="mx-1 font-normal"
                              >
                                │
                              </span>{" "}
                            </>
                          ) : null}
                          <span className="font-semibold">
                            {repository.repoName}:
                          </span>{" "}
                          {repositoryComparisonLabel(repository)}
                        </Fragment>
                      ),
                    )}
              </span>
            ) : null}
          </div>
          <p className="mt-1.5 text-[11px] text-[var(--g5-text2)]">
            Get the latest changes from {targetWorkspaceName} into this
            workspace. Only {sourceWorkspaceName} will be changed. An agent
            resolves conflicts in its running workspace; review the result
            before shipping.
          </p>

          <button
            type="button"
            data-testid="prepare-to-ship-rebase-options-toggle"
            aria-expanded={rebaseOptionsOpen}
            onClick={() => setRebaseOptionsOpen((open) => !open)}
            className="mt-2 flex w-full items-center gap-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-1.5 text-left text-[11px] font-semibold text-[var(--g5-text1)] hover:bg-[var(--g5-overlay)]"
          >
            <ChevronDown
              size={11}
              aria-hidden="true"
              className={classNames(
                "shrink-0 transition-transform",
                rebaseOptionsOpen ? "rotate-180" : "",
              )}
            />
            Auto-resolution settings
            <span className="min-w-0 flex-1 truncate text-[10.5px] font-normal text-[var(--g5-text2)]">
              agent and model for conflicts and Merge with agent
            </span>
          </button>
          <div className={rebaseOptionsOpen ? "mt-2 space-y-3" : "hidden"}>
            <AgentLaunchConfigControls
              value={selectedAgentConfig}
              options={agentConfigOptions}
              onChange={setSelectedAgentConfig}
              resolvePreferences={resolveAgentLaunch}
              onResolutionSelectionChange={setConfirmedAgentLaunch}
              disabled={isRebaseSubmitting}
              testIdPrefix="prepare-to-ship-rebase"
              selectVariant="themed"
            />
          </div>

          <div className="mt-2.5 flex flex-wrap items-center gap-2.5">
            <button
              type="button"
              data-testid="prepare-to-ship-update-codebase"
              onClick={() => {
                if (!confirmedAgentLaunch || !canStartRebase) return;
                onStartRebase(selectedAgentConfig, confirmedAgentLaunch);
              }}
              disabled={!canStartRebase}
              title={publicationBlocked ?? undefined}
              className={classNames(
                "inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs font-medium",
                canStartRebase
                  ? "border-[var(--g5-stroke)] bg-[var(--g5-bg2)] text-[var(--g5-text0)] hover:bg-[var(--g5-overlay)]"
                  : "cursor-not-allowed border-[var(--g5-stroke)] bg-[var(--g5-bg2)] text-[var(--g5-text2)]",
              )}
            >
              <RefreshCw size={12} />
              {isRebaseSubmitting
                ? "Updating…"
                : `Get the latest changes from ${targetWorkspaceName}`}
            </button>
            <span className="text-[10.5px] text-[var(--g5-text2)]">
              Safe — rewrites only this workspace's branch.
            </span>
          </div>
          {rebaseErrorMessage ? (
            <p className="mt-2 rounded-md border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-2.5 py-1.5 text-[11px] text-[var(--g5-blocked)]">
              {rebaseErrorMessage}
            </p>
          ) : null}
        </section>

        {/* ================= 4 · Ship to parent ================= */}
        <section className={sectionClass} data-testid="prepare-to-ship-ship">
          <div className="flex flex-wrap items-center gap-2.5">
            <SectionHeading step={4} title="Ship to parent" done={shipped} />
            <span className="flex-1" />
          </div>
          <p className="mt-1.5 text-[11px] text-[var(--g5-text2)]">
            Merges the pull request into {targetWorkspaceName}. Not a
            deployment.
          </p>
          <div
            role={showMergeOptions ? "group" : undefined}
            aria-label={showMergeOptions ? "Merge options" : undefined}
            className={classNames(
              showMergeOptions
                ? "mt-4 overflow-hidden rounded-lg border border-[var(--g5-stroke)]"
                : "mt-2.5",
            )}
          >
            {showMergeOptions ? (
              <StopVmAfterMergeSwitch
                checked={currentAutoMerge?.stopVmAfterMerge ?? false}
                disabled={actionBusy || publicationBlocked !== null}
                onChange={(selected) => void changeStopVmAfterMerge(selected)}
              />
            ) : null}
            <div className={showMergeOptions ? "p-3" : undefined}>
              <div className="flex flex-wrap items-center gap-2.5">
                <button
                  type="button"
                  data-testid="prepare-to-ship-ship-button"
                  onClick={handleShip}
                  disabled={!canShip}
                  className={classNames(
                    // Warning treatment (spec D6): shipping advances the shared
                    // target, so the action must not read as a green "safe/done"
                    // state before it runs. Dark foreground for contrast on the
                    // amber warn token (same dark-on-status pairing as the
                    // section-step badge).
                    "inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium text-[rgb(8,12,24)]",
                    canShip
                      ? "bg-[var(--g5-warn)] hover:bg-[var(--g5-warn)]/90"
                      : "cursor-not-allowed bg-[var(--g5-warn)]/50",
                  )}
                >
                  <GitMerge size={12} />
                  {pendingAction === "ship" || mergePullRequest.isLoading
                    ? "Shipping…"
                    : shipped
                      ? "Shipped"
                      : "Ship to parent"}
                </button>
                {shipped ? null : (
                  <button
                    type="button"
                    data-testid="prepare-to-ship-ship-with-agent"
                    onClick={() => void changeAutoMerge(true, true)}
                    disabled={shipWithAgentDisabled}
                    title={shipWithAgentUnavailable ?? undefined}
                    data-emphasis={shipBlocked ? "primary" : "secondary"}
                    className={classNames(
                      "inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium",
                      // The resolution of a blocked ship reads as the next step;
                      // on the all-green path it stays secondary.
                      shipBlocked
                        ? "text-[rgb(8,12,24)] bg-[var(--g5-warn)] hover:bg-[var(--g5-warn)]/90"
                        : "border border-[var(--g5-stroke)] text-[var(--g5-text1)] hover:bg-[var(--g5-bg1)]",
                      shipWithAgentDisabled
                        ? "cursor-not-allowed opacity-50"
                        : "",
                    )}
                  >
                    {startAndShip ? <Power size={12} /> : <Bot size={12} />}
                    {setAutoMerge.isLoading
                      ? "Starting…"
                      : startAndShip
                        ? "Start and ship with agent"
                        : "Ship with agent"}
                  </button>
                )}
                {/* Merge with override (g5.workbench.ship-to-parent): beside
                Ship to parent, never replacing it, and only while the ship
                is blocked; the server's preview decides whether it is
                offered. */}
                {activePullRequest ? (
                  <MergeOverrideAction
                    targetWorkspaceId={targetWorkspaceId}
                    targetWorkspaceName={targetWorkspaceName}
                    pullRequestId={activePullRequest.id}
                    pullRequestTitle={activePullRequest.title}
                    enabled={shipBlocked}
                    disabled={actionBusy}
                    testIdPrefix="prepare-to-ship"
                    onMerged={(result) => {
                      mergePullRequest.reset();
                      setMergedPullRequest(result.pullRequest);
                    }}
                  />
                ) : null}
              </div>
              <p
                data-testid="prepare-to-ship-ship-note"
                className="mt-2 text-[10.5px] text-[var(--g5-text2)]"
              >
                {shipped
                  ? activePullRequest?.mergeOverride
                    ? mergeOverrideSummary(activePullRequest.mergeOverride)
                    : `Merged into ${targetWorkspaceName}.`
                  : permissionDeniedReason != null
                    ? permissionDeniedReason
                    : shipBlocked
                      ? `${shipBlockReason ?? mergePullRequest.error?.message ?? ""} ${
                          shipWithAgentUnavailable ??
                          "Ship with agent resolves it; G5 ships once it is ready."
                        }`
                      : (shipBlockReason ?? "Ready to merge.")}
              </p>
              {mergePullRequest.error &&
              !shipBlocked &&
              !mergePermissionDenied ? (
                <p className="mt-2 rounded-md border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-2.5 py-1.5 text-[11px] text-[var(--g5-blocked)]">
                  {mergePullRequest.error.message}
                </p>
              ) : null}
              {/* Start and ship with agent (g5.chat.start-and-send): what the
              compound action does while the VM is stopped; the Start's
              progress while one is under way. */}
              {!shipped && hasOpenPullRequest && startAndShip ? (
                <div
                  role="note"
                  data-testid="prepare-to-ship-start-and-ship-note"
                  className="mt-2.5 flex items-start gap-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-1.5 text-xs leading-5 text-[var(--g5-text1)]"
                >
                  <Power
                    size={13}
                    className="mt-1 shrink-0 text-[var(--g5-text2)]"
                  />
                  <span>
                    {liveGate?.kind === "failed"
                      ? "The workspace failed to start. "
                      : "The workspace is stopped. "}
                    <strong className="font-medium text-[var(--g5-text0)]">
                      Start and ship with agent
                    </strong>{" "}
                    starts it, turns on Merge when ready and queues the agent's
                    first message, sent once it is running.
                  </span>
                </div>
              ) : null}
              {!shipped && hasOpenPullRequest && startUnderWay ? (
                <div className="mt-2.5 [&>*]:mb-0">
                  <StartAndSendNotice waitReason={startWaitReason} />
                </div>
              ) : null}
            </div>
            {showMergeOptions ? (
              <div className="border-t border-[var(--g5-stroke)]">
                <MergeWhenReadySwitch
                  orgId={orgId}
                  request={autoMergeRequest}
                  disabled={actionBusy}
                  onChange={(enabled) => void changeAutoMerge(enabled, false)}
                />
              </div>
            ) : null}
          </div>
          {setAutoMerge.error ? (
            <p className="mt-2 rounded-md border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-2.5 py-1.5 text-[11px] text-[var(--g5-blocked)]">
              {setAutoMerge.error.message}
            </p>
          ) : null}
        </section>
      </div>
    </Modal>
  );
}
