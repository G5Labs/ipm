import { SourceControlProvider } from "../components/source-control/SourceControlBrowser";
import { isCurrentOntologyPath } from "../lib/currentOntology";
import { OntologyChangesView } from "../components/editor/OntologyChangesView";
import { useWorkspaceReviewFlow } from "../hooks/useWorkspaceReviewFlow";
import { WorkspaceReviewContent } from "../components/workspace-review/WorkspaceReviewContent";
// =============================================================================
// G5 Workspace Page — Layout Shell
// =============================================================================
// Top-level orchestrating container for the G5 unified editor. Manages the
// overall layout grid, responsive breakpoints, route parameter extraction,
// tab state management, panel visibility toggling, keyboard shortcuts, and
// error boundary wrapping.
//
// Route: /:org/:workspace (and all deep-link sub-routes)
//
// This shell does NOT implement the internals of any panel — it provides the
// layout skeleton, wires up state and callbacks, and delegates rendering to
// child components imported from sibling modules.
// =============================================================================

import {
  Bell,
  BookOpen,
  FileCode,
  FileText,
  MessageCircle,
  MessageSquare,
  PanelRightClose,
  SquareTerminal,
  AlertCircle,
  ClipboardList,
  LayoutDashboard,
  ShieldCheck,
  ChevronsRight,
} from "lucide-react";
import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  useMatch,
  useLocation,
  useNavigate,
  useParams,
  useSearchParams,
} from "react-router-dom";

// Design system
import { AppShell } from "../components/layout/Layout";
import { CornerButton, CornerStack } from "../components/ui/CornerButton";
import { classNames, Modal, useCoveringOverlayOpen } from "../components/ui/UI";

// Auth
import { useAuth, useRenderedPrincipal } from "../hooks/useAuth";
import { useResolveAgentLaunch } from "../hooks/useAgentLaunch";

// Data hooks
import {
  refreshChatMessages,
  useChatMessages,
  useChatSession,
  useSessionTask,
  useChatResultInspection,
  useChatSessions,
  useForkSession,
  useRenameChatSession,
  useSendChatMessage,
  useStartChatMessage,
  useReplyToProviderRequest,
  useSessionTimeline,
  useEndBackgroundWait,
  type TimelineCoverage,
} from "../hooks/useChat";
import {
  useFollowView,
  useStartWorkbenchAction,
  useWorkbenchConfig,
} from "../hooks/useWorkbench";
import {
  workbenchActionContext,
  workbenchActionMessage,
} from "../lib/workbenchMessage";
import {
  RIBBON_FALLBACK_CHAT_TITLE,
  ribbonActionState,
  type RibbonActionState,
  type RibbonConfigStatus,
  type RibbonButtonShape,
} from "../components/workbench/ActionRibbon";
import { BuildMockModal } from "../components/workbench/BuildMockModal";
import { recentMockDocs } from "../lib/mockDocs";
import {
  useAddComment,
  useDeleteComment,
  useFileComments,
  useReplyToComment,
  useResolveComment,
} from "../hooks/useComments";
import { type RefreshingState, toRefreshingState } from "../hooks/hookUtils";
import {
  parseRepositoryDocPath,
  useDeleteDoc,
  useDeleteViewDoc,
  useDocList,
} from "../hooks/useDocs";
import {
  CreateProjectDocModal,
  TRANSCRIPT_FOLDER,
} from "../components/views/CreateProjectDocModal";
import { DeleteProjectDocDialog } from "../components/views/DeleteProjectDocDialog";
import { CreateExplainerModal } from "../components/explainer/CreateExplainerModal";
import { isManagedExplainerPath } from "../components/explainer/explainerPaths";
import {
  EXPLAINER_CHAT_NAMING,
  explainerSaveInstructions,
} from "../components/explainer/explainerChatPrompt";
import {
  classifyAgentApprovals,
  classifyTimedOutQuestions,
} from "../lib/agentApprovals";
import { useChatStopController } from "../hooks/useChatStopController";
import { useLiveTail, type LiveTailState } from "../hooks/useLiveTail";
import type { ChatStopState } from "../hooks/useChatStopController";
import { isProviderAccountsEnabled } from "../lib/providerFlags";
import { randomUuid } from "../lib/randomUuid";
import { useProviderAccounts } from "../hooks/useProviderAccounts";
import {
  effectiveAssignment,
  useAssignmentConfigs,
} from "../components/settings/ProviderAccountAssignment";
import { authenticatableAgentProviders } from "../lib/providerAccountSelection";
import {
  WORKBENCH_ACTIONS,
  WORKBENCH_PHASES,
  isWorkspacePhase,
} from "../lib/workbenchDefaults";
import { workspaceRibbonActions } from "../lib/workbenchNavigation";
import { PhaseChevrons } from "../components/workbench/PhaseChevrons";
import {
  ActionRibbon,
  RibbonShapedButton,
  ribbonButtonShape,
} from "../components/workbench/ActionRibbon";
import { WorkbenchWorkspaceControls } from "../components/workbench/WorkbenchWorkspaceControls";
import { terminalControlState } from "../lib/workspaceVmControls";
import { useWorkspaceUserActivity } from "../hooks/useWorkspaceUserActivity";
import { ShellControls } from "../components/layout/ShellControls";
import { WorkspaceLanding } from "../components/workspace-landing/WorkspaceLanding";
import { approvalSummary } from "../lib/landingStatus";
import {
  useDeleteFileEntry,
  useFileTree,
  fetchWorkspaceFile,
  useSaveWorkspaceFile,
  useWorkspaceChanges,
  useWorkspaceFileContent,
  useWriteFileEntry,
} from "../hooks/useFiles";
import type { FileTreeEntry as SidebarFileTreeEntry } from "../components/tree/FileTree";
import {
  useTask,
  useWorkspaceTasks,
  useWorkspaceActiveSessionIds,
  useWorkspaceHasActiveTasks,
} from "../hooks/useTasks";
import { useClientToasts } from "../hooks/useClientToasts";
import { describeStopOutcome } from "../lib/workspaceStopToast";
import { useNotifications } from "../hooks/useNotifications";
import {
  useGuidedJourneyEnabled,
  useInstanceInfo,
  useOrgScopedConfig,
  useWorkspaceScopedConfig,
} from "../hooks/useSettings";
import {
  journeyKey,
  useJourneyMarks,
  useSetJourneyMark,
} from "../hooks/useJourney";
import {
  GuidedJourneySection,
  type JourneyMark,
  type JourneyMarkFailure,
} from "../components/guided/GuidedJourneySection";
import { findWorkbenchAction, type JourneyFacts } from "../lib/journeySteps";
import { ownerName } from "../lib/workspaceOwner";
import { useUserSettings } from "../hooks/useUserSettings";
import { useWorkspaceTabs } from "../hooks/useWorkspaceTabs";
import {
  sameWorkspaceTab,
  type WorkspaceTab,
  workspaceTabPath,
} from "../lib/workspaceTabs";
import { useWSChannel, useWSEventListener } from "../hooks/useWebSocket";
import {
  resolveWorkspaceKey,
  useResolveWorkspaceRoute,
  workspaceRouteResolutionKey,
  type ResolvedIds,
  type WorkspaceRouteTarget,
  useDeleteWorkspace,
  useWorkspace,
  useUpdateWorkspacePhase,
  useWorkspaceOpenQuestionMetrics,
  useWorkspaces,
  useWorkspacePullRequests,
  workspacePullRequestsQueryOptions,
  useWorkspacePullRequestTargets,
  usePreflightWorkspaceMerge,
  useResumeWorkspaceVm,
  useStartWorkspaceMerge,
  useStopWorkspaceVm,
  useTransferWorkspace,
  useWorkspaceVm,
} from "../hooks/useWorkspace";
import { useOrgMembers } from "../hooks/useOrg";
import { WorkspaceNamesProvider } from "../hooks/useWorkspaceNames";
import { useWorkspaceApprovals } from "../hooks/useWorkspaceApprovals";

/** Base's ribbon: no groups in any phase, workspace controls only. */
const EMPTY_RIBBON_ACTIONS: WorkbenchActions = { be: [], se: [], ie: [] };

// Shared components
import { ErrorBoundary } from "../components/shared/ErrorBoundary";
import { MarkdownRenderer } from "../components/shared/MarkdownRenderer";
import { api, ApiError, WORKSPACE_READ_ONLY_MESSAGE } from "../lib/api";
import { subscribeReadOnlyNotice } from "../lib/readOnlyNotice";
import { ReviewWorkspaceBanner } from "../components/workspace/ReviewWorkspaceBanner";
import { ReviewCopyPage } from "../components/workspace/ReviewCopyPage";
import {
  type ComposerSeedHeader,
  reviewWorkspaceSeed,
  verificationSeed,
} from "../lib/reviewPrompts";
import {
  VERIFICATION_ACTION_OFF_TITLE,
  isOrgVerificationActionEnabled,
} from "../lib/verificationActionFlags";
import {
  changedLineRanges,
  requestWorkspaceStart,
  sourceReviewsPath,
  WORKSPACE_START_EVENT,
  finishReviewStep,
  finishReviewStopped,
  takeReviewPromptSeed,
  takeReviewResumeError,
  shortOid,
  workspacePagePath,
} from "../components/workspace/reviewWorkspace";
import {
  personalWorkspaceMePath,
  personalWorkspaceUserPath,
  taskRoutePath,
} from "../lib/workspacePagePath";
import {
  WORKSPACE_ROUTE_FAMILIES,
  type WorkspaceRouteFamily,
} from "../lib/shellRoutes";
import {
  findWorkspaceIssueLink,
  parseIssueNumberParam,
} from "../lib/issuePage";
import {
  useReviewPreview,
  useWorkspaceReviewContext,
} from "../hooks/useWorkspaceReview";
import {
  useFileSuggestions,
  type SuggestionOverlayProps,
} from "../components/workspace/ReviewSuggestionOverlay";
import { useReviewSuggestionCounts } from "../hooks/useWorkspaceReview";
import { recordWorkspaceOpen } from "../lib/recentWorkspaces";
import type { ChatPromptHistoryScope } from "../lib/chatPromptHistory";
import {
  applyDeploymentWorkspaceUiCapabilities,
  getWorkspaceUiCapabilities,
} from "../lib/capabilities";
import { routes } from "../lib/routes";
import { isScmSettingsReturn, SCM_OAUTH_SETTINGS_PARAM } from "../lib/scmOAuth";
import { displayNodeName, fileNameFromPath, isUuid } from "../lib/utils";
import {
  DEFAULT_GUIDE_DOC_ID,
  GETTING_STARTED_AUTO_OPENED_STORAGE_KEY,
  USER_GUIDE_BASE_PATH,
  getGuideDocByHref,
  getGuideDocById,
  guideDocTab,
  stripGuideMarkdownComments,
} from "../lib/userGuide";

// Child panel components (from sibling workspace-page specs)
import {
  type ChatComposerDraft,
  type ChatComposerSeed,
  ChatPanel,
  type ChatSendResult,
  createChatComposerDraft,
} from "../components/chat/ChatPanel";
import { ChatBubbleStack } from "../components/chat/ChatBubbleStack";
import {
  agentLaunchPreferencesForConfig,
  completeAgentConfig,
} from "../components/agent/AgentLaunchConfigControls";
import type { ResolveAgentLaunchPreferences } from "../components/agent/AgentLaunchConfigControls";
import { WorkspaceTerminalPanel } from "../components/execution/WorkspaceTerminalPanel";
import { DirectCleanMergeChoiceModal } from "../components/workspace-merge-workflow/DirectCleanMergeChoiceModal";
import {
  WorkspaceMergeConfirmModal,
  type WorkspaceMergeTargetOption,
} from "../components/workspace-merge-workflow/WorkspaceMergeConfirmModal";
import { PreviewControl } from "../components/preview/PreviewControl";
import { NotificationPanel } from "../components/notifications/NotificationPanel";
import { NotificationToastStack } from "../components/notifications/NotificationToastStack";
import { PlatformIssueReportChromePending } from "../components/platform-issues/PlatformIssueReportChromePending";
import { UserSettingsPanel } from "../components/settings/UserSettingsPanel";
import { WorkspaceSettingsContent } from "../components/settings/WorkspaceSettingsContent";
import { WorkerHostsPageContent } from "../components/org/WorkerHostsPageContent";
import { OrgMembersPageContent } from "../components/org/OrgMembersPageContent";
import { OrgApprovalsPageContent } from "../components/org/OrgApprovalsPageContent";
import { GithubAppBindingStatusPageContent } from "../components/org/GithubAppBindingStatusPageContent";
import { OrgAdminPageContent } from "../components/org/OrgAdminPageContent";
import { ImportTaskPicker } from "../components/external-issues/ImportTaskPicker";
import { IssueUnlinkConfirmation } from "../components/external-issues/WorkspaceIssueLinkParts";
import {
  ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE,
  issueStateBlockedReason,
  nextIssueState,
} from "../lib/issueState";
import {
  BASE_ISSUE_LINK_REASON,
  REVIEW_COPY_DETAILS_REASON,
  REVIEW_IDENTITY_PENDING_REASON,
} from "../components/workspace/WorkspaceDetailsPanel";
import { startMessageFromIssue } from "../lib/startMessageFromIssue";
import type {
  ExternalIssueLocatorDTO,
  WorkspaceIssueLinkDTO,
  ExternalIssueSummaryDTO,
  WorkbenchActions,
  PreviewOpenRequest,
  WorkspacePhase,
} from "../lib/types";
import {
  ExternalIssueDetailPanel,
  type ExternalIssueDetailPanelProps,
} from "../components/external-issues/ExternalIssueDetailPanel";
import {
  useChangeIssueState,
  useExternalIssuesFeature,
  useUnlinkWorkspaceIssue,
  useWorkspaceIssueLinks,
} from "../hooks/useExternalIssues";
import { useGithubConnection } from "../hooks/useGithubConnection";
import {
  EXTERNAL_ISSUES_OFF_MESSAGE,
  describeIssueStateError,
} from "../lib/issueCommentErrors";
import { WorkspaceApprovalsPageContent } from "../components/org/WorkspaceApprovalsPageContent";
import {
  LeftSidebar,
  isViewDoc,
  type SidebarSectionId,
  type SourceControlDirtyFile,
} from "../components/tree/TreeNode";
import { WorkspaceHealthBanners } from "../components/workspace/WorkspaceHealthBanners";
import { WorkspaceVmStatusStrip } from "../components/workspace/WorkspaceVmStatusStrip";
import { WorkspaceChangesView } from "../components/workspace/WorkspaceChangesView";
import { IntentLogView } from "../components/intent-log/IntentLogView";
import { CloneDialog } from "../components/workspace/CloneDialog";
import { PrepareToShipModal } from "../components/workspace/PrepareToShipModal";
import { CreateWorkspaceDialog } from "../components/workspace/CreateWorkspaceDialog";
import { RemoveWorkspaceDialog } from "../components/workspace/RemoveWorkspaceDialog";
import { PullRequestPanel } from "../components/workspace/PullRequestPanel";
import {
  TopBar,
  type TopBarPrimaryIssue,
} from "../components/workspace/TopBar";
import {
  canRemoveWorkspaceFromSwitcher,
  workspaceIdsWithLiveChildren,
} from "../components/workspace/workspaceSwitcherPolicy";
import { ReadOnlyBadge } from "../components/workspace/ReadOnlyBadge";
import { WorkspaceDetailsPanel } from "../components/workspace/WorkspaceDetailsPanel";
import { WorkspaceActivityPanel } from "../components/external-issues/WorkspaceActivityPanel";

// Editor components (center panel content dispatch)
import { FileView } from "../components/files/FileView";
import { formatByteSize } from "../components/files/fileBytes";
import {
  isHtmlPath,
  MAX_EDITABLE_TEXT_BYTES,
} from "../components/files/fileText";
import { draftPerson } from "../components/files/fileDrafts";
import { useFileDraftsAsDirtyEditors } from "../components/files/useFileDraftsAsDirtyEditors";
import { WorkspaceCommitDiffView } from "../components/editor/WorkspaceCommitDiffView";
import { SourceControlChangeGraphView } from "../components/source-control/SourceControlChangeGraphView";
import { diffFileKey } from "../lib/diff";
import { TaskWorkspaceSwitchPrompt } from "../components/editor/TaskWorkspaceSwitchPrompt";
import { currentFileTab } from "../lib/currentFileTab";
import { HtmlPreviewShell } from "../components/files/HtmlPreviewShell";
import { parseG5FileHref } from "../components/shared/sandboxedHtml";
import { JsonlTranscriptView } from "../components/shared/JsonlTranscriptView";
import {
  MAX_SECTION_TITLE_CHARS,
  sanitizeSectionText,
} from "../components/shared/htmlDocAnswerBridge";
import { recommendedOption, type ParsedQuestion } from "../lib/questionBlock";
import type { NavigateTarget } from "../components/editor/IntentTraceabilityPanel";

// Types
import type {
  AgentAccountSelectionV1,
  AgentApprovalResolutionInput,
  ChatAgentProvider,
  AgentLaunchSelectionV1,
  ChatMessageContext,
  ChatMessageDTO,
  ChatProjectReference,
  PublishWorkspaceReviewResponse,
  ChatUploadAttachment,
  ChatSessionDTO,
  ChatSessionResultDTO,
  BranchSummary,
  CommentDTO,
  CommentPermissions,
  CommitHistoryEntry,
  CreateWorkspacePullRequestResponse,
  DocNode,
  FileTreeEntry,
  NotificationAction,
  SyncAgentConfig,
  PendingSyncFile,
  TabItem,
  GuideDocId,
  MergePreflightResponse,
  StartWorkspaceMergeResponse,
  TaskDetail,
  TaskSummary,
  TimelineEntry,
  WorkspaceDetail,
  SubscriptionUsage,
  UserSettings,
  WorkspacePullRequestSummary,
  WorkspaceStopOutcome,
  WorkspaceSummary,
  WorkspaceVmView,
  SourceControlChangeGraphRoute,
} from "../lib/types";
import {
  isIsolatedPendingSyncDocFile,
  READ_ONLY_COMMENT_PERMISSIONS,
  workspacePullRequestProviderOutcome,
} from "../lib/types";
import type { RibbonStyle, WorkbenchAction } from "../lib/types";
import {
  forkLandingQuery,
  forkLandingState,
  readForkLandingState,
} from "../lib/workspaceLanding";
import { sha256Utf8 } from "../lib/commentAnchors";
import {
  defaultAgentProviderFromConfig,
  getAvailableSyncAgentConfigs,
  getPreferredSyncAgentConfig,
} from "../lib/agentLaunchDefaults";
import type { PickerHarness, ProviderAccountDTO } from "../lib/types";
import {
  resolveCitationBranch,
  resolveContentBranch,
  resolveDeepLinkRepoId,
  resolveWorkspaceDocsBranchId,
  sortWorkspaceBranches,
  summarizeRepositoryComparisons,
} from "../lib/workspaceBranches";
import {
  useCommentsKeptWhileAnswered,
  type EditorCommentDraft,
} from "../components/comments/commentThreadModel";
import { useActiveComposerSubjects } from "../hooks/useComposerStore";
import { ComposerOwner } from "../components/comments/ComposerOwner";
import { describeCapacityWait } from "../components/workspace/vmCapacityWait";
import {
  capacityWaitKey,
  forgetCapacityWaitAnnouncement,
  forgetEndedCapacityWaits,
  markCapacityWaitAnnounced,
} from "../lib/capacityWaitNotice";
import {
  isPersonalWorkspace,
  withoutPersonalWorkspaces,
} from "../lib/workspaceKind";
import { StartAndSendNotice } from "../components/chat/StartAndSendNotice";
import { PersonalWorkspaceNotice } from "../components/personal/PersonalWorkspaceNotice";
import {
  sourceThreadWrites,
  workspaceAcceptsThreadWrites,
} from "../lib/threadWrites";
import {
  liveGateBanner,
  liveGateCanStart,
  liveGateKind,
  liveGateReason,
  reviewIdentityKnownFrom,
  useWorkspaceLiveGateValue,
  WorkspaceLiveGateProvider,
  type WorkspaceLiveGate,
} from "../lib/workspaceLiveGate";

/**
 * The page's contexts in one wrapper: workspace names for labels and the
 * live gate every control reads (`g5.workspace.ui.gates`).
 */
function WorkspacePageProviders({
  names,
  liveGate,
  workspaceId,
  children,
}: {
  names: Record<string, string>;
  workspaceId: string;
  liveGate: WorkspaceLiveGate;
  children: React.ReactNode;
}) {
  return (
    <WorkspaceNamesProvider value={names}>
      <WorkspaceLiveGateProvider value={liveGate}>
        <SourceControlProvider workspaceId={workspaceId}>
          {children}
        </SourceControlProvider>
      </WorkspaceLiveGateProvider>
    </WorkspaceNamesProvider>
  );
}

// =============================================================================
// Constants
// =============================================================================

function safeDownloadStem(input: string): string {
  const sanitized = input
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^[-._]+|[-._]+$/g, "");
  return sanitized || "workspace";
}

export interface WorkspacePullRequestCreateToastDecision {
  severity: "info" | "warning";
  title: string;
  message: string;
}

export function workspacePullRequestCreateToast(
  result: CreateWorkspacePullRequestResponse,
): WorkspacePullRequestCreateToastDecision {
  const providerOutcome = workspacePullRequestProviderOutcome(
    result.pullRequest,
  );
  const githubPrSkipped = providerOutcome.hasSkippedRepos;
  const providerIssueLabel = providerOutcome.allReposSkipped
    ? "GitHub PR not created"
    : "some GitHub PRs not created";
  return {
    severity: githubPrSkipped ? "warning" : "info",
    title: githubPrSkipped
      ? result.alreadyExisted
        ? `Local PR updated — ${providerIssueLabel}`
        : `Local PR created — ${providerIssueLabel}`
      : result.alreadyExisted
        ? "PR already exists"
        : "PR created",
    message: githubPrSkipped
      ? `${result.pullRequest.sourceWorkspaceName} into ${result.pullRequest.targetWorkspaceName}: ${result.pullRequest.localMergeStatus}. Open the PR dialog for per-repo recovery steps.`
      : `${result.pullRequest.sourceWorkspaceName} into ${result.pullRequest.targetWorkspaceName}: ${result.pullRequest.localMergeStatus}.`,
  };
}

function filenameFromContentDisposition(header: string | null): string | null {
  if (!header) return null;
  const match = /filename="?([^";]+)"?/i.exec(header);
  return match?.[1] ?? null;
}

// Browser-tab title shown when no workspace is open (matches the static
// fallback in index.html). Workspace pages override this with the current
// workspace name via `workspaceDocumentTitle`.
export const DEFAULT_DOCUMENT_TITLE =
  "G5 — Ontology-Centric Agentic Development";

// Build the browser-tab title for an open workspace: "G5 - {name}" with the
// workspace name trimmed for surrounding whitespace only. Falls back to the
// default title when no usable name exists.
export function workspaceDocumentTitle(
  name: string | null | undefined,
): string {
  const trimmed = (name ?? "").trim();
  return trimmed ? `G5 - ${trimmed}` : DEFAULT_DOCUMENT_TITLE;
}

// Stable empty fallback so memos depending on the workspaces list don't
// recompute on every render while the query is loading/errored.
const EMPTY_WORKSPACE_SUMMARIES: WorkspaceSummary[] = [];
const EMPTY_COMMENTS: CommentDTO[] = [];

/** Why a list of threads shown may be missing or out of date, if it may. */
function threadListProblem(state: RefreshingState<unknown>): string | null {
  if (state.status === "error") {
    return `Could not load comments: ${state.error.message}`;
  }
  if (state.status === "ok" && state.refreshError) {
    return `Could not refresh comments: ${state.refreshError.message}. Showing the threads loaded earlier.`;
  }
  return null;
}

const LEFT_SIDEBAR_MIN = 200;
const LEFT_SIDEBAR_MAX = 480;
const LEFT_SIDEBAR_DEFAULT = 280;
const WORKSPACE_UI_CAPABILITIES = getWorkspaceUiCapabilities();
const EMPTY_TASK_LIST: import("../lib/types").TaskListEntry[] = [];
const EMPTY_PR_TARGETS: WorkspaceMergeTargetOption[] = [];

type WorkspaceMergeReviewChoice = {
  targetWorkspaceId: string;
  label: string;
  preflight: MergePreflightResponse;
  agentConfig: SyncAgentConfig;
  agentLaunch: AgentLaunchSelectionV1;
};

export function shouldShowDirectMergePreviewAction(
  preflight: MergePreflightResponse,
): boolean {
  return preflight.decision === "cleanMergeNoGitConflicts";
}

export function shouldShowWorkspaceMergeReview(
  preflight: MergePreflightResponse,
): boolean {
  return (
    preflight.decision === "cleanMergeNoGitConflicts" ||
    preflight.decision === "fastForwardOrNoOp" ||
    preflight.decision === "gitConflicts" ||
    preflight.decision === "workflowRequired"
  );
}

export function workspaceMergeReviewApprovalAction(
  _preflight: MergePreflightResponse,
): "start-route" {
  return "start-route";
}

function workspaceMergeReviewTitle(choice: WorkspaceMergeReviewChoice): string {
  if (choice.preflight.decision === "gitConflicts") {
    return `Resolve Git Conflicts: ${choice.label}`;
  }
  if (choice.preflight.decision === "workflowRequired") {
    return `Workspace Update Needs Attention: ${choice.label}`;
  }
  return `No Git Conflicts: ${choice.label}`;
}
export function taskOutputTabForTask(
  task: Pick<TaskSummary, "id" | "name"> | null | undefined,
): TabItem | null {
  return task ? { type: "workspace_changes", name: "Workspace changes" } : null;
}

/** The read-only external issue tab (id `external-issue:<repo>:<number>`). */
export function externalIssueTab(
  locator: ExternalIssueLocatorDTO,
  displayKey: string,
): Extract<TabItem, { type: "external_issue" }> {
  return {
    type: "external_issue",
    repositoryId: locator.repositoryId,
    number: locator.number,
    name: displayKey,
  };
}

/** The singleton commit diff tab of a commit on a repository branch. */
export function commitDiffTab(
  branch: Pick<BranchSummary, "repoId" | "repoName">,
  sha: string,
  name: string,
): Extract<TabItem, { type: "source_control_commit_diff" }> {
  return {
    type: "source_control_commit_diff",
    name,
    repoId: branch.repoId,
    repoName: branch.repoName,
    branchId: branch.repoId,
    sha,
  };
}

/**
 * Approval ids that must block the chat composer. Legacy request liveness is
 * scoped to its owning assistant message; native request liveness is scoped to
 * the exact active task/attempt. The API rejects late resolutions for dead
 * approvals; this gate is UX. An idle_timeout settlement settles its
 * approval, so no timed-out question sub-state (D25 timed_out_answerable /
 * late_answered / timed_out_superseded) ever re-blocks the composer — the
 * answerable card is actionable WITHOUT blocking sends.
 */
export function pendingAgentApprovalIdsForTimeline(
  entries: TimelineEntry[],
  messages: ReadonlyArray<{ id: string; role: string; status?: string }>,
  activeTask?: TaskSummary | null,
): string[] {
  return classifyAgentApprovals(entries, messages, activeTask).pending;
}

/**
 * True when the active center tab is a full-width admin/audit surface with no
 * execution context — the org/workspace admin surfaces (Settings / Workers /
 * Members) and the read-only Intent Log audit view. The stacked sidebar
 * ignores legacy rail compatibility props.
 */
export function isAdminCenterTabItem(tab: TabItem | null | undefined): boolean {
  return (
    tab?.type === "workspace_settings" ||
    tab?.type === "worker_hosts" ||
    tab?.type === "org_members" ||
    tab?.type === "org_approvals" ||
    tab?.type === "org_github_app" ||
    tab?.type === "org_admin" ||
    tab?.type === "workspace_approvals" ||
    tab?.type === "workspace_review" ||
    tab?.type === "intent_log" ||
    tab?.type === "workspace_details"
  );
}

/** Only these content surfaces consume the shell's resolved branch identity. */
export function requiresContentBranch(
  tab: TabItem | null | undefined,
): boolean {
  if (!tab) return false;
  return tab.type === "doc" || tab.type === "file" || tab.type === "intent_log";
}

/**
 * Resolve the current workspace's clone/fork source summary (workspace-page D25).
 *
 * Prefers the cached org-list item for aggregate stats (cost/tasks the detail
 * summary lacks), but overlays the workspace-detail `branches` because the detail
 * is the authoritative, freshest per-branch projection for the current
 * workspace. The org-wide `useWorkspaces` list is stale-while-revalidate and can
 * lag a just-activated `remote_tracking` repo; without this overlay the first
 * fork after a repo becomes remote-tracking would hand `CloneDialog` a source
 * whose branches lack `remote_tracking`, hiding the isolate/co-track selector
 * (top-bar D4) until a background list refetch lands (chat C17).
 */
export function resolveCurrentWorkspaceSummary(
  listed: WorkspaceSummary | undefined,
  detail: WorkspaceDetail | null,
): WorkspaceSummary | null {
  if (listed) {
    return detail
      ? {
          ...listed,
          isBase: detail.isBase,
          readOnly: detail.readOnly,
          createdBy: detail.createdBy,
          branches: detail.branches,
        }
      : listed;
  }
  if (!detail) return null;
  return {
    id: detail.id,
    name: detail.name,
    description: detail.description,
    isBase: detail.isBase,
    readOnly: detail.readOnly,
    createdBy: detail.createdBy,
    clonedFromId: detail.clonedFromId,
    branches: detail.branches,
    tasksCompleted: 0,
    totalTasks: 0,
    runningTime: null,
    cost: 0,
    concurrentTasks: 0,
    lastUpdated: detail.updatedAt,
    createdAt: detail.createdAt,
  };
}

export { workspaceIdsWithLiveChildren };

export function workspaceMergeTargetsForCurrentWorkspace(
  current:
    | (Pick<WorkspaceSummary, "id" | "clonedFromId"> &
        Partial<Pick<WorkspaceSummary, "isBase" | "kind">>)
    | null,
  allWorkspaces: (Pick<WorkspaceSummary, "id" | "name" | "clonedFromId"> &
    Partial<Pick<WorkspaceSummary, "isBase" | "kind">>)[],
): WorkspaceMergeTargetOption[] {
  // A personal workspace is never a merge source or target
  // (g5.personal.refusals).
  if (!current?.clonedFromId || isPersonalWorkspace(current)) return [];
  return withoutPersonalWorkspaces(allWorkspaces)
    .filter((candidate) => candidate.id === current.clonedFromId)
    .map((candidate) => ({ id: candidate.id, name: candidate.name }));
}

export function openWorkspacePullRequestForTarget<
  T extends Pick<WorkspacePullRequestSummary, "targetWorkspaceId" | "status">,
>(pullRequests: readonly T[], targetWorkspaceId: string): T | null {
  if (!targetWorkspaceId) return null;
  return (
    pullRequests.find(
      (pullRequest) =>
        pullRequest.targetWorkspaceId === targetWorkspaceId &&
        pullRequest.status === "open",
    ) ?? null
  );
}

export function workspaceMergeErrorMessage(error: unknown): string | null {
  if (!error) return null;
  if (error instanceof ApiError) {
    const detailMessage =
      typeof error.details.message === "string" ? error.details.message : null;
    if (
      detailMessage ===
      "Target workspace has no materialized branch row for source repo"
    ) {
      return "The selected target workspace does not contain a materialized branch for one of this workspace's repos. Open the forked workspace and merge it back to its source, or fork again after seeding the repo.";
    }
    return detailMessage ?? error.message;
  }
  return error instanceof Error ? error.message : String(error);
}

/** Minimum width reserved for the central content area */
const CENTER_MIN_WIDTH = 300;

const TAB_PERSISTENCE_DEBOUNCE_MS = 100;

function formatQueryErrorMessage(error: Error, fallback: string): string {
  const message = error.message.trim();
  return message.length > 0 ? message : fallback;
}

// =============================================================================
// getTabId — unique tab identity
// =============================================================================

function getTabId(item: TabItem): string {
  switch (item.type) {
    case "doc":
      return `doc:${item.docId}`;
    case "guide_doc":
      return `guide:${item.guideId}`;
    case "intent_log":
      return "intent-log";
    case "pull_requests":
      return "pull-requests";
    case "file":
      return item.branchId
        ? `file:${item.branchId}:${item.filePath}`
        : `file:${item.filePath}`;
    case "ontology_changes":
      return "ontology-changes";
    case "workspace_changes":
      return "workspace-changes";
    case "source_control_diff":
      return "source-control-diff";
    case "source_control_change_graph":
      return "source-control-change-graph";
    case "source_control_commit_diff":
      return "source-control-commit-diff";
    case "workspace_terminal":
      return "workspace-terminal";
    case "workspace_settings":
      // Singleton per workspace — re-opening focuses the existing tab.
      return "workspace-settings";
    case "worker_hosts":
      // Singleton per org — re-opening focuses the existing tab.
      return "worker-hosts";
    case "org_members":
      // Singleton per org — re-opening focuses the existing tab.
      return "org-members";
    case "org_approvals":
      // Singleton per org — re-opening focuses the existing tab.
      return "org-approvals";
    case "org_github_app":
      // Singleton per org — re-opening focuses the existing tab.
      return "org-github-app";
    case "org_admin":
      // Singleton per org — re-opening focuses the existing tab.
      return "org-admin";
    case "workspace_approvals":
      // Singleton per workspace — re-opening focuses the existing tab.
      return "workspace-approvals";
    case "workspace_review":
      // Singleton per workspace — re-opening focuses the existing tab.
      return "workspace-review";
    case "workspace_details":
      // Singleton per workspace — re-opening focuses the existing tab.
      return "workspace-details";
    case "workspace_activity":
      // Singleton per workspace — re-opening focuses the existing tab.
      return "workspace-activity";
    case "external_issue":
      // One tab per issue, keyed by GitHub repository id and number.
      return `external-issue:${item.repositoryId}:${item.number}`;
  }
}

export function dedupeTabsById(tabs: TabItem[]): TabItem[] {
  const seen = new Set<string>();
  const deduped: TabItem[] = [];
  for (const tab of tabs) {
    const tabId = getTabId(tab);
    if (seen.has(tabId)) continue;
    seen.add(tabId);
    deduped.push(tab);
  }
  return deduped;
}

// Exhaustive over TabItem["type"] so adding a tab type without listing it here
// is a compile error. Restore-time filter: persisted layouts can carry tabs
// whose type was retired (e.g. the removed `ontology_explainer` human-view
// tab), and getTabId/the center-panel renderer are defined only over live
// types (workspace-page-shell spec D5).
const KNOWN_TAB_TYPES: Record<TabItem["type"], true> = {
  doc: true,
  guide_doc: true,
  intent_log: true,
  pull_requests: true,
  file: true,
  ontology_changes: true,
  source_control_diff: true,
  source_control_change_graph: true,
  source_control_commit_diff: true,
  workspace_changes: true,
  workspace_settings: true,
  workspace_terminal: true,
  worker_hosts: true,
  org_members: true,
  org_approvals: true,
  org_github_app: true,
  org_admin: true,
  workspace_approvals: true,
  workspace_review: true,
  workspace_details: true,
  workspace_activity: true,
  external_issue: true,
};

export function knownTabsOnly(tabs: TabItem[]): TabItem[] {
  return tabs.filter((tab) => KNOWN_TAB_TYPES[(tab as TabItem).type] === true);
}

/** The singleton Source Control (dirty diff) tab, without a selected file. */
const SOURCE_CONTROL_DIFF_TAB: Extract<
  TabItem,
  { type: "source_control_diff" }
> = {
  type: "source_control_diff",
  name: "Source Control",
};
/**
 * The Activity center tab: the workspace's landing timeline, named for the
 * workspace (workbench g5.workbench.landing). One name on every path that
 * opens it, so a later open never renames the pinned tab.
 */
const WORKSPACE_ACTIVITY_TAB: TabItem = {
  type: "workspace_activity",
  name: "Workspace",
};

/**
 * Center tabs a workbench `view` action may open (workbench
 * g5.workbench.actions `target: view`), keyed by its `tab` value. An unknown
 * tab leaves the button disabled; it never executes.
 */
const WORKBENCH_VIEW_TABS: Record<string, TabItem> = {
  workspace_activity: WORKSPACE_ACTIVITY_TAB,
  workspace_approvals: { type: "workspace_approvals", name: "Approvals" },
  workspace_details: { type: "workspace_details", name: "Workspace Details" },
  pull_requests: { type: "pull_requests", name: "Pull Requests" },
  // The Diff Graph (guided journey g5.workbench.journey.steps; singleton tab).
  source_control_change_graph: {
    type: "source_control_change_graph",
    name: "Diff Graph",
  },
};
const WORKSPACE_ACTIVITY_TAB_ID = getTabId(WORKSPACE_ACTIVITY_TAB);

/**
 * `tabs` with the Activity tab first, added when absent. On a review copy the pinned Review tab keeps the first place and Activity
 * follows it: the copy exists to be reviewed from (workbench
 * g5.workbench.landing).
 */
export function withActivityTabFirst(
  tabs: TabItem[],
  reviewPinned = false,
): TabItem[] {
  const index = tabs.findIndex(
    (tab) => getTabId(tab) === WORKSPACE_ACTIVITY_TAB_ID,
  );
  const rest = index === -1 ? tabs : tabs.filter((_, i) => i !== index);
  const activity = WORKSPACE_ACTIVITY_TAB;
  const at = reviewPinned && rest[0]?.type === "workspace_review" ? 1 : 0;
  if (index === at && tabs[index].name === activity.name) return tabs;
  return [...rest.slice(0, at), activity, ...rest.slice(at)];
}

export function upsertTab(tabs: TabItem[], item: TabItem): TabItem[] {
  const deduped = dedupeTabsById(tabs);
  const tabId = getTabId(item);
  if (deduped.some((tab) => getTabId(tab) === tabId)) {
    return deduped.map((tab) => {
      if (getTabId(tab) !== tabId) return tab;
      return item;
    });
  }
  return [...deduped, item];
}

/**
 * Compute the tabs that remain after a Chrome-style bulk-close operation
 * anchored on `anchorTabId`:
 * - `"others"` keeps only the anchor tab.
 * - `"right"` keeps the anchor tab and every tab to its left, closing the ones
 *   positioned after it in left-to-right order.
 *
 * When the anchor is not present (stale id) the list is returned unchanged. The
 * anchor tab always survives both operations, so callers can safely fall back
 * to it for the next selection.
 */
export function tabsToKeepForBulkClose(
  tabs: TabItem[],
  anchorTabId: string,
  scope: "others" | "right",
  /** Tabs that survive every close (a review copy's Review tab). */
  pinnedTabIds: readonly string[] = [],
): TabItem[] {
  const anchorIdx = tabs.findIndex((tab) => getTabId(tab) === anchorTabId);
  if (anchorIdx === -1) return tabs;
  const pinned = (tab: TabItem) => pinnedTabIds.includes(getTabId(tab));
  if (scope === "others") {
    return tabs.filter((tab) => getTabId(tab) === anchorTabId || pinned(tab));
  }
  return tabs.filter((tab, idx) => idx <= anchorIdx || pinned(tab));
}

const MAX_CHAT_OPEN_FILE_CONTEXT = 10;

function dedupeChatProjectReferences(
  references: ChatProjectReference[],
): ChatProjectReference[] {
  const seen = new Set<string>();
  const deduped: ChatProjectReference[] = [];
  for (const reference of references) {
    if (seen.has(reference.id)) continue;
    seen.add(reference.id);
    deduped.push(reference);
  }
  return deduped;
}

export const workspaceReadableDocPath = (
  filePath: string | null | undefined,
) => {
  const trimmed = filePath?.trim();
  if (!trimmed) return undefined;
  const normalized = trimmed.replaceAll("\\", "/").replace(/^\/+/, "");
  if (
    normalized.startsWith("docs/docs/") ||
    normalized.startsWith("docs/designs/") ||
    normalized.startsWith("docs/views/")
  ) {
    return normalized;
  }
  if (
    normalized.startsWith("docs/") ||
    normalized.startsWith("designs/") ||
    normalized.startsWith("views/")
  ) {
    return `docs/${normalized}`;
  }
  return normalized;
};

function chatReferenceForDoc(doc: DocNode): ChatProjectReference {
  const isView = isViewDoc(doc);
  return {
    id: `${isView ? "view" : "project_doc"}:${doc.id}`,
    kind: isView ? "view" : "project_doc",
    label: doc.name,
    path: workspaceReadableDocPath(doc.filePath),
    docId: doc.id,
    branchId: doc.repoId ?? undefined,
  };
}

/**
 * Host-owned Discuss composer template for a doc question card (center-panel
 * D69, seeded through the chat_panel_runtime D30 one-shot non-executing
 * contract). Every doc-derived field is sanitized like the D60 section titles
 * (control characters and backticks stripped, whitespace collapsed); the
 * question title is additionally capped at the D60 title limit. Missing
 * fields degrade the template: no title drops the parenthetical, no prompt
 * drops the colon clause, zero options drop the options sentence, and no
 * recommendation drops that clause. The Send keystroke stays the human gate.
 */
export function buildQuestionDiscussPrompt(
  question: ParsedQuestion,
  docPath: string,
  questionTitle?: string,
): string {
  // Neutral placeholders, never the raw value: `sanitizeSectionText` returns
  // "" when the input is entirely control characters, backticks, or
  // whitespace, so a `|| raw` fallback would push exactly the unsanitized
  // strings this function exists to strip back into the prompt.
  const id = sanitizeSectionText(question.id) || "(unnamed question)";
  const title = sanitizeSectionText(questionTitle ?? "").slice(
    0,
    MAX_SECTION_TITLE_CHARS,
  );
  const prompt = sanitizeSectionText(question.prompt);
  const labels = question.options
    .map((option) => sanitizeSectionText(option.label))
    .filter((label) => label.length > 0);
  const recommended = recommendedOption(question);
  const recommendedLabel = recommended
    ? sanitizeSectionText(recommended.label)
    : "";

  let text = `Let's discuss question ${id}`;
  if (title) text += ` ("${title}")`;
  text += ` in Project Docs/${sanitizeSectionText(docPath) || "(unnamed document)"}`;
  text += prompt ? `: ${prompt}` : ".";
  if (prompt && !/[.!?]$/.test(text)) text += ".";
  if (labels.length > 0) {
    text += ` The proposed options are ${labels.join(", ")}`;
    text += recommendedLabel
      ? `; the current recommendation is ${recommendedLabel}.`
      : ".";
  }
  text +=
    " Help me reason about the tradeoffs before I answer — do not edit the doc until I decide.";
  return text;
}

/**
 * Merge the seed-attached chat reference (D69 Discuss attaches the doc as a
 * `project_doc` reference alongside the composer seed) into the references a
 * send carries, deduplicating by reference id.
 */
export function mergeSeededChatReference(
  references: ChatProjectReference[] | undefined,
  seeded: ChatProjectReference | null,
): ChatProjectReference[] | undefined {
  if (!seeded) return references;
  return dedupeChatProjectReferences([...(references ?? []), seeded]);
}

function collectFileChatReferences(
  entries: FileTreeEntry[],
  branchId: string,
  out: ChatProjectReference[] = [],
): ChatProjectReference[] {
  for (const entry of entries) {
    if (entry.type === "file") {
      out.push({
        id: branchId
          ? `source_file:${branchId}:${entry.path}`
          : `source_file:${entry.path}`,
        kind: "source_file",
        label: entry.name || entry.path.split("/").pop() || entry.path,
        path: entry.path,
        branchId: branchId || undefined,
      });
    }
    if (entry.children?.length) {
      collectFileChatReferences(entry.children, branchId, out);
    }
  }
  return out;
}

function chatReferenceForTab(
  tab: TabItem | null,
  docs: DocNode[],
  activeBranchId: string,
): ChatProjectReference | null {
  if (!tab) return null;
  switch (tab.type) {
    case "doc": {
      const doc = docs.find((candidate) => candidate.id === tab.docId);
      return doc
        ? chatReferenceForDoc(doc)
        : {
            id: `project_doc:${tab.docId}`,
            kind: "project_doc",
            label: tab.name,
            docId: tab.docId,
          };
    }
    case "file":
      return {
        id: tab.branchId
          ? `source_file:${tab.branchId}:${tab.filePath}`
          : `source_file:${tab.filePath}`,
        kind: "source_file",
        label: tab.name || tab.filePath.split("/").pop() || tab.filePath,
        path: tab.filePath,
        branchId: tab.branchId ?? activeBranchId,
      };
    case "guide_doc":
    case "intent_log":
    case "pull_requests":
    case "ontology_changes":
    case "workspace_changes":
    case "source_control_diff":
    case "source_control_change_graph":
    case "source_control_commit_diff":
    case "workspace_settings":
    case "workspace_terminal":
    case "worker_hosts":
    case "org_members":
    case "org_approvals":
    case "org_github_app":
    case "org_admin":
    case "workspace_approvals":
    case "workspace_review":
    case "workspace_details":
    case "workspace_activity":
    case "external_issue":
      return null;
  }
}

type NavigationOrigin = {
  tabType: "spec" | "code" | "file" | "doc";
  nodeId: string;
  name: string;
  /** The statement to return to; empty for the top of the document. */
  intentRecordNumber: string;
  filePath?: string;
  branchId?: string;
  /** The file path or doc the navigation opened, which offers Back. */
  target?: string;
};

export function buildNavigationOrigin(
  currentTab: TabItem | null,
  target: NavigateTarget,
): NavigationOrigin | null {
  if (!target.sourceFileName) return null;
  // A link outside any statement returns to the document itself
  // (`g5.documents.ontology.links`).
  if (
    !target.sourceIntentRecordNumber &&
    currentTab?.type !== "file" &&
    currentTab?.type !== "doc"
  )
    return null;

  const fallbackName =
    target.sourceFileName.split("/").pop() ?? target.sourceFileName;

  if (currentTab?.type === "file") {
    return {
      tabType: "file",
      nodeId: currentTab.filePath,
      name: currentTab.name,
      intentRecordNumber: target.sourceIntentRecordNumber ?? "",
      filePath: currentTab.filePath,
      branchId: currentTab.branchId,
    };
  }

  if (currentTab?.type === "doc") {
    return {
      tabType: "doc",
      nodeId: currentTab.docId,
      name: currentTab.name,
      intentRecordNumber: target.sourceIntentRecordNumber ?? "",
    };
  }

  return {
    tabType:
      target.sourceFileType === "code"
        ? "file"
        : target.sourceFileType === "doc"
          ? "doc"
          : "spec",
    nodeId: target.sourceFileName,
    name: fallbackName,
    intentRecordNumber: target.sourceIntentRecordNumber ?? "",
    filePath:
      target.sourceFileType === "code" ? target.sourceFileName : undefined,
  };
}

export function tabFromNavigationOrigin(origin: NavigationOrigin): TabItem {
  if (origin.tabType === "doc") {
    return {
      type: "doc",
      docId: origin.nodeId,
      name: origin.name,
    };
  }

  if (origin.tabType === "file") {
    return {
      ...workspaceFileTab(
        origin.filePath ?? origin.nodeId,
        origin.name,
        origin.branchId,
      ),
    };
  }

  return workspaceFileTab(
    origin.filePath ?? origin.nodeId,
    origin.name,
    origin.branchId,
  );
}

export function workspaceFileTab(
  filePath: string,
  name?: string,
  branchId?: string,
  lineRange?: [number, number],
): TabItem {
  return {
    type: "file",
    filePath,
    name: name ?? (filePath.split("/").pop() || filePath),
    branchId,
    // Only carry a line range when one was provided (e.g. a citation click), so
    // file-tree opens keep their existing tab identity.
    ...(lineRange ? { lineRange } : {}),
  };
}

export function workspaceOntologyFileTab(
  path: string,
  repoId: string,
): TabItem {
  return workspaceFileTab(path, undefined, repoId);
}

function normalizeNewFileEntryName(input: string | null): string | null {
  const normalized = input
    ?.trim()
    .replace(/^\/+/, "")
    .replace(/\/{2,}/g, "/")
    .replace(/\/+$/, "");
  if (!normalized || normalized.includes("..")) return null;
  return normalized;
}

function joinFileEntryPath(
  parentPath: string | null,
  childPath: string,
): string {
  const parent = parentPath?.trim().replace(/\/+$/, "");
  return parent ? `${parent}/${childPath}` : childPath;
}

function branchIdForTabItem(
  item: TabItem,
  docsBranchId: string,
  fallbackBranchId: string,
): string {
  if (item.type === "doc") {
    return docsBranchId || fallbackBranchId;
  }
  if (item.type === "file") {
    return item.branchId || fallbackBranchId;
  }
  return fallbackBranchId;
}
export function resolveFileBranchId(
  branchHint?: string | null,
  confirmedContentBranchId?: string | null,
): string | undefined {
  return branchHint || confirmedContentBranchId || undefined;
}
export function resolveTraceFileNavigation(
  target: Pick<NavigateTarget, "type" | "id" | "workspaceId" | "repoId">,
  workspaceId: string,
  fallbackBranchId?: string | null,
): { filePath: string; branchId?: string } | null {
  if (target.workspaceId && target.workspaceId !== workspaceId) return null;
  const identity = target.id.replace(/^\/+/, "");
  if (!identity || identity.split("/").includes("..")) return null;
  const filePath =
    target.type === "spec" &&
    !identity.startsWith(".g5/") &&
    !identity.startsWith(".semlift/")
      ? ".semlift/specs/" +
        identity.replace(/\.semlift\.md$/, "") +
        ".semlift.md"
      : identity;
  return { filePath, branchId: target.repoId ?? fallbackBranchId ?? undefined };
}

/**
 * Map a TabItem to a shareable URL path segment (appended after `/:org/:workspace`).
 * Content with an identity owns a path (g5.workbench.tabs); singleton
 * surfaces are named by `?view=` instead and return `null` here.
 */
export function tabToPath(tab: TabItem): string | null {
  switch (tab.type) {
    case "doc":
      return `/doc/${tab.docId}`;
    case "intent_log":
      return "/intent-log";
    case "file":
      return `/file/${tab.filePath}`;
    case "guide_doc":
      return `/guide/${encodeURIComponent(tab.guideId)}`;
    case "external_issue":
      return `/issue/${encodeURIComponent(tab.repositoryId)}/${tab.number}`;
    case "source_control_commit_diff":
      return `/commit/${encodeURIComponent(tab.repoId)}/${encodeURIComponent(tab.sha)}`;
    default:
      return null;
  }
}

export function tabToViewParam(tab: TabItem): string | null {
  return tabTypeToViewParam(tab.type);
}
function tabTypeToViewParam(type: TabItem["type"]): string | null {
  switch (type) {
    case "workspace_terminal":
      return "terminal";
    case "workspace_settings":
      return "settings";
    case "worker_hosts":
      return "workers";
    case "org_members":
      return "members";
    case "org_approvals":
      return "approval-definitions";
    case "org_github_app":
      return "github-app";
    case "org_admin":
      return "admin";
    case "workspace_approvals":
      return "approvals";
    case "workspace_details":
      return "details";
    case "workspace_activity":
      return "activity";
    case "workspace_review":
      return "review";
    case "pull_requests":
      return "pull-requests";
    case "workspace_changes":
      return "changes";
    case "source_control_diff":
      return "source-control";
    case "ontology_changes":
      return "ontology-changes";
    case "source_control_change_graph":
      return "diff-graph";
    default:
      return null;
  }
}

/**
 * Center surfaces a personal workspace does not offer
 * (g5.personal.workbench): a link naming one opens nothing and falls back
 * to the workspace root (g5.workbench.tabs).
 */
export const PERSONAL_UNOFFERED_VIEW_PARAMS = new Set<string>([
  "approvals",
  "reviews",
  "review",
  "pull-requests",
  "diff-graph",
]);

/** Every `?view=` value the center tab sync owns (and may clear): each
 *  tab's value plus the scroll aliases `reviews` (Approvals) and `issues`
 *  (Workspace Details). */
const CENTER_VIEW_PARAMS = new Set<string>([
  ...(Object.keys(KNOWN_TAB_TYPES) as TabItem["type"][])
    .map(tabTypeToViewParam)
    .filter((value): value is string => value !== null),
  "reviews",
  "issues",
  "chat",
]);

/** The fresh composer's placeholder after a fork (g5.workspace.ui.fork-landing). */
const FORK_LANDING_COMPOSER_PLACEHOLDER = "What's your idea?";

/**
 * Resolve a doc tab's file name for its label. A doc tab carries only the
 * backend-derived `name` (which can be the doc's first heading), so resolve the
 * doc's file path from the loaded docs list and use its file name (excluding
 * directories). Returns null when no file path is available, so the caller
 * falls back to the tab's name.
 */
/**
 * Tab label for a managed explainer file.
 *
 * Managed explainer names are `<slug>-<YYYY-MM-DD-HHMM>.html`, so the raw
 * basename spends most of a tab's width on a timestamp and every explainer
 * tab truncates to a near-identical stub. Drop the stamp and the extension
 * and show the slug; the full path is still on the tab's title attribute and
 * in the document's own provenance footer.
 */
export function explainerTabLabel(fileName: string): string {
  const withoutExtension = fileName.replace(/\.(html?|md)$/i, "");
  // Only strip a trailing managed stamp (`-2026-08-07-0521`, optionally with
  // a `-2` collision suffix) — never a slug that merely ends in digits.
  const withoutStamp = withoutExtension.replace(
    /-\d{4}-\d{2}-\d{2}-\d{4}(-\d+)?$/,
    "",
  );
  return withoutStamp || withoutExtension;
}

function tabFileName(tab: TabItem, docs: DocNode[]): string | null {
  if (tab.type !== "doc") return null;
  const doc = docs.find((d) => d.id === tab.docId);
  const filePath = doc?.filePath ?? null;
  if (!filePath) return null;
  const fileName = fileNameFromPath(filePath);
  return isManagedExplainerPath(filePath)
    ? explainerTabLabel(fileName)
    : fileName;
}

function findChangeGraphDocument(
  docs: DocNode[],
  route: SourceControlChangeGraphRoute,
): DocNode | null {
  if (!route.filePath) return null;
  const canonical = route.filePath.replace(/^\.\//, "");
  return (
    docs.find((doc) => doc.filePath?.replace(/^\.\//, "") === canonical) ?? null
  );
}
export {
  assistantMessageToStop,
  chatSuccessorExists,
  getPreferredAvailableAgentProvider,
  shouldAttachTaskTimelineToChatSession,
  stopHandoffBridging,
  supportedCloudAgentProviders,
} from "../lib/chatTurns";
import {
  getPreferredAvailableAgentProvider,
  hasLiveAssistantTurn,
  supportedCloudAgentProviders,
} from "../lib/chatTurns";

export {
  defaultAgentProviderFromConfig,
  getAvailableSyncAgentConfigs,
  getPreferredSyncAgentConfig,
} from "../lib/agentLaunchDefaults";

function sessionMatchesChatScope(
  session: ChatSessionDTO | null | undefined,
  scopeType: "workspace" | "task",
  taskId: string | null,
): session is ChatSessionDTO {
  if (!session) return false;
  if (scopeType === "workspace" && session.scopeType === "workspace") {
    return true;
  }
  // The workspace Chat surface also presents the conversation attached to the
  // selected task. Keep that exact task session active when the user returns
  // from its output; unrelated task sessions remain outside the workspace list.
  return (
    Boolean(taskId) &&
    session.scopeType === "task" &&
    session.scopeRef === taskId
  );
}

function sessionMatchesRestorableTaskUrl(
  session: ChatSessionDTO | null | undefined,
  workspaceId: string,
  activeSessionId: string | null,
  sessionHint: string | null,
): session is ChatSessionDTO {
  return Boolean(
    session &&
    activeSessionId &&
    session.id === activeSessionId &&
    sessionHint === activeSessionId &&
    session.workspaceId === workspaceId &&
    session.scopeType === "task",
  );
}

/** Direct-merge modal submission gate (CodeRabbit 2026-08-22). True while ANY
 * launch key or merge mutation that pins the `DirectCleanMergeChoiceModal`
 * open is in flight — including the `merge-review` approval, which the
 * original cancel guard missed. While true, cancel must no-op. Exported for the
 * regression tests in `WorkspacePageDirectCleanMergeGate.test.ts`. */
export function isDirectCleanMergeSubmitting(
  launchSubmissionKeys: ReadonlySet<string>,
  anyMergeMutationLoading: boolean,
): boolean {
  return (
    launchSubmissionKeys.has("direct-clean-full") ||
    launchSubmissionKeys.has("workspace-rebase") ||
    launchSubmissionKeys.has("merge-review") ||
    anyMergeMutationLoading
  );
}

export function getTaskDialogDisabledReason(
  sessionStatus: string | null | undefined,
  _task: Pick<TaskSummary, "status"> | null | undefined,
): string | null {
  if (sessionStatus === "completed") {
    return "This task dialog is closed because the session has completed.";
  }
  if (sessionStatus === "archived") {
    return "This task dialog is archived.";
  }
  // Inputs enqueue behind a queued, running, or held logical task. After a
  // terminal task, the same session accepts a follow-up as a new task.
  return null;
}

function isTaskDialogStreamingAllowed(
  sessionStatus: string | null | undefined,
  task: Pick<TaskSummary, "status"> | null | undefined,
): boolean {
  if (task?.status === "waiting") return false;
  return getTaskDialogDisabledReason(sessionStatus, task) === null;
}

export function hasExplicitTaskWaitForUserInput(
  timelineEntries: TimelineEntry[] | undefined,
  sessionId?: string | null,
  messages?: Pick<ChatMessageDTO, "role" | "createdAt">[],
): boolean {
  const latestUserMessageAtMs = Math.max(
    ...((messages ?? [])
      .filter((message) => message.role === "user")
      .map((message) => Date.parse(message.createdAt))
      .filter((time) => Number.isFinite(time)) as number[]),
  );
  const hasUserMessageTimestamp = Number.isFinite(latestUserMessageAtMs);

  return (timelineEntries ?? []).some((entry) => {
    const metadata = entry.metadata ?? {};
    const entrySessionId = metadata.sessionId;
    if (
      sessionId &&
      typeof entrySessionId === "string" &&
      entrySessionId !== sessionId
    ) {
      return false;
    }
    return (
      metadata.kind === "task_wait" &&
      metadata.source === "g5d.task.wait" &&
      (!hasUserMessageTimestamp ||
        Date.parse(entry.createdAt) > latestUserMessageAtMs)
    );
  });
}

// Question-parked sync wait (workspace-page-shell D26, issue #1391): a sync
// `waiting` task stays steerable while an idle_timeout-settled AskUserQuestion
// for this session is still answerable (not late-answered, not superseded).
// Reuses the D25 sub-state classifier so gate and card cannot disagree.
export function hasAnswerableTimedOutQuestion(
  timelineEntries: TimelineEntry[] | undefined,
  messages?: Pick<ChatMessageDTO, "role" | "createdAt">[],
): boolean {
  const subStates = classifyTimedOutQuestions(
    timelineEntries ?? [],
    messages ?? [],
  );
  for (const subState of subStates.values()) {
    if (subState === "timed_out_answerable") return true;
  }
  return false;
}

/**
 * Verify that a Home AskUserQuestion focus hint belongs to the selected
 * task/session and still names an actionable card. This is deliberately
 * fail-closed: a stale, cross-session, settled, or superseded hint never falls
 * back to another pending question.
 */
export function verifiedAskUserQuestionFocus(input: {
  approvalId: string | null;
  sessionHint: string | null;
  routeTaskId: string | null | undefined;
  selectedTaskId: string | null | undefined;
  activeSessionId: string | null | undefined;
  activeSession: Pick<ChatSessionDTO, "id"> | null | undefined;
  timelineEntries: TimelineEntry[];
  messages: Pick<ChatMessageDTO, "id" | "role" | "status" | "createdAt">[];
  // Retained for caller compatibility; liveness is now judged per owning
  // assistant message via classifyAgentApprovals (#2318), not a session-global
  // streaming flag, so this field is no longer read.
  isStreaming: boolean;
}): string | null {
  const {
    approvalId,
    sessionHint,
    routeTaskId,
    selectedTaskId,
    activeSessionId,
    activeSession,
    timelineEntries,
    messages,
  } = input;
  if (
    !approvalId ||
    !sessionHint ||
    !routeTaskId ||
    !activeSession ||
    activeSession.id !== sessionHint ||
    activeSessionId !== sessionHint ||
    selectedTaskId !== routeTaskId
  ) {
    return null;
  }
  const ownsAskRequest = timelineEntries.some(
    (entry) =>
      entry.metadata?.providerEvent === "provided_agent.approval.requested" &&
      entry.metadata?.toolName === "AskUserQuestion" &&
      entry.metadata?.approvalId === approvalId &&
      entry.metadata?.sessionId === sessionHint,
  );
  if (!ownsAskRequest) return null;

  const pending = classifyAgentApprovals(timelineEntries, messages).pending;
  if (pending.includes(approvalId)) return approvalId;
  return classifyTimedOutQuestions(timelineEntries, messages).get(
    approvalId,
  ) === "timed_out_answerable"
    ? approvalId
    : null;
}

export function shouldClearAskUserQuestionFocus(input: {
  focusHint: string | null;
  selectionReady: boolean;
  activeSessionReady: boolean;
  timelineReady: boolean;
  messagesReady: boolean;
  verifiedApprovalId: string | null;
}): boolean {
  return Boolean(
    input.focusHint &&
    input.selectionReady &&
    input.activeSessionReady &&
    input.timelineReady &&
    input.messagesReady &&
    !input.verifiedApprovalId,
  );
}

export function getRightPanelChatScope(): "workspace" {
  return "workspace";
}

// =============================================================================
// useMediaQuery — responsive breakpoint detection
// =============================================================================

function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => {
    if (typeof window === "undefined") return false;
    return window.matchMedia(query).matches;
  });

  useEffect(() => {
    const mql = window.matchMedia(query);
    const handler = (e: MediaQueryListEvent) => setMatches(e.matches);
    mql.addEventListener("change", handler);
    return () => mql.removeEventListener("change", handler);
  }, [query]);

  return matches;
}

// =============================================================================
// useTabPersistence — localStorage tab state management
// =============================================================================

interface PersistedTabState {
  tabs: TabItem[];
  activeTabId: string | null;
}

function useTabPersistence(workspaceId: string | undefined) {
  const storageKey = workspaceId ? `g5:tabs:${workspaceId}` : null;

  const readPersistedTabs = useCallback((): PersistedTabState | null => {
    if (!storageKey) return null;
    try {
      const raw = localStorage.getItem(storageKey);
      if (!raw) return null;
      const parsed = JSON.parse(raw) as PersistedTabState;
      if (!Array.isArray(parsed.tabs)) return null;
      return parsed;
    } catch {
      return null;
    }
  }, [storageKey]);

  const writePersistedTabs = useCallback(
    (state: PersistedTabState) => {
      if (!storageKey) return;
      try {
        localStorage.setItem(storageKey, JSON.stringify(state));
      } catch {
        // QuotaExceededError — degrade silently
      }
    },
    [storageKey],
  );

  return { readPersistedTabs, writePersistedTabs };
}

// =============================================================================
// DragHandle — resizable panel drag handle
// =============================================================================

interface DragHandleProps {
  orientation: "vertical" | "horizontal";
  onResize: (delta: number) => void;
  onResizeEnd?: () => void;
}

function DragHandle({
  orientation,
  onResize,
  onResizeEnd,
}: DragHandleProps): ReactElement {
  const handleRef = useRef<HTMLDivElement>(null);
  const startPosRef = useRef<number>(0);
  const draggingRef = useRef(false);
  const rafRef = useRef<number>(0);
  const pendingDeltaRef = useRef<number>(0);

  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      e.preventDefault();
      const el = handleRef.current;
      if (!el) return;
      el.setPointerCapture(e.pointerId);
      draggingRef.current = true;
      startPosRef.current = orientation === "vertical" ? e.clientX : e.clientY;
      pendingDeltaRef.current = 0;
      document.body.style.userSelect = "none";
    },
    [orientation],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!draggingRef.current) return;
      const current = orientation === "vertical" ? e.clientX : e.clientY;
      const delta = current - startPosRef.current;
      startPosRef.current = current;
      pendingDeltaRef.current += delta;
      // Throttle state updates to at most one per animation frame
      if (!rafRef.current) {
        rafRef.current = requestAnimationFrame(() => {
          rafRef.current = 0;
          const d = pendingDeltaRef.current;
          pendingDeltaRef.current = 0;
          if (d !== 0) onResize(d);
        });
      }
    },
    [orientation, onResize],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent) => {
      if (!draggingRef.current) return;
      draggingRef.current = false;
      // Cancel any pending rAF and flush remaining delta
      if (rafRef.current) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = 0;
      }
      const d = pendingDeltaRef.current;
      pendingDeltaRef.current = 0;
      if (d !== 0) onResize(d);
      const el = handleRef.current;
      if (el) el.releasePointerCapture(e.pointerId);
      document.body.style.userSelect = "";
      onResizeEnd?.();
    },
    [onResize, onResizeEnd],
  );

  const isVertical = orientation === "vertical";

  return (
    <div
      ref={handleRef}
      className={classNames(
        "shrink-0 bg-transparent hover:bg-[var(--g5-info)] transition-colors duration-150",
        isVertical
          ? "w-1.5 cursor-col-resize h-full px-[2px] bg-clip-content"
          : "h-1.5 cursor-row-resize w-full py-[2px] bg-clip-content",
      )}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      role="separator"
      aria-orientation={orientation}
      data-testid={isVertical ? "sidebar-resize-handle" : undefined}
    />
  );
}

// =============================================================================
// FloatingPanel — detachable panel that floats over the workspace
// =============================================================================

/** Global z-index counter for floating panels — increments on each focus.
 *  Starts above dropdown z-50 so floats always layer on top. */
let floatingPanelZCounter = 200;

/** Size bounds enforced on both the initialSize seed and the resize handle, so
 *  a panel can never open outside the range the user can drag it back into.
 *  @ref software_architecture/client/workspace-page/workspace-page-shell#D38 */
const FLOATING_PANEL_MIN_W = 280;
const FLOATING_PANEL_MAX_W = 800;
const FLOATING_PANEL_MIN_H = 300;
/** Height of the panel kept inside the viewport by drag and top-edge resize,
 *  so the title bar with Dock and Close stays reachable. */
const FLOATING_PANEL_REACHABLE_H = 40;

/** Fallback for consumers with no natural width to carry in. */
const FLOATING_PANEL_DEFAULT_SIZE = { w: 380, h: 520 };

function clampFloatingPanelWidth(w: number): number {
  return Math.max(FLOATING_PANEL_MIN_W, Math.min(FLOATING_PANEL_MAX_W, w));
}

/** The edges a resize handle moves; the opposite edges stay where they are. */
interface FloatingPanelResizeEdges {
  left?: boolean;
  right?: boolean;
  top?: boolean;
  bottom?: boolean;
}

interface FloatingPanelFrame {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * A handle on every edge and corner, so a panel against a viewport edge still
 * resizes from the edges that face the content: the right-edge Chat grows
 * from its left and top edges (g5.workspace.ui.chat-corner-button).
 */
const FLOATING_PANEL_RESIZE_HANDLES: ReadonlyArray<{
  id: string;
  edges: FloatingPanelResizeEdges;
  className: string;
}> = [
  {
    id: "top",
    edges: { top: true },
    className: "top-0 left-3 right-3 h-1.5 cursor-ns-resize",
  },
  {
    id: "bottom",
    edges: { bottom: true },
    className: "bottom-0 left-3 right-3 h-1.5 cursor-ns-resize",
  },
  {
    id: "left",
    edges: { left: true },
    className: "left-0 top-3 bottom-3 w-1.5 cursor-ew-resize",
  },
  {
    id: "right",
    edges: { right: true },
    className: "right-0 top-3 bottom-3 w-1.5 cursor-ew-resize",
  },
  {
    id: "top-left",
    edges: { top: true, left: true },
    className: "top-0 left-0 h-3 w-3 cursor-nwse-resize",
  },
  {
    id: "top-right",
    edges: { top: true, right: true },
    className: "top-0 right-0 h-3 w-3 cursor-nesw-resize",
  },
  {
    id: "bottom-left",
    edges: { bottom: true, left: true },
    className: "bottom-0 left-0 h-3 w-3 cursor-nesw-resize",
  },
  {
    id: "bottom-right",
    edges: { bottom: true, right: true },
    className: "bottom-0 right-0 h-4 w-4 cursor-nwse-resize",
  },
];

/**
 * Move the dragged edges of `start` by (dx, dy) within the size bounds and the
 * viewport. The opposite edges keep their position, so a panel anchored to the
 * right edge stays anchored while its left edge is dragged.
 */
function resizeFloatingPanelFrame(
  start: FloatingPanelFrame,
  edges: FloatingPanelResizeEdges,
  dx: number,
  dy: number,
  viewport: { w: number; h: number },
): FloatingPanelFrame {
  let { x, y, w, h } = start;
  if (edges.right) {
    w = clampFloatingPanelWidth(Math.min(start.w + dx, viewport.w - start.x));
  } else if (edges.left) {
    const right = start.x + start.w;
    x = Math.max(0, right - clampFloatingPanelWidth(start.w - dx));
    w = right - x;
  }
  if (edges.bottom) {
    h = Math.max(
      FLOATING_PANEL_MIN_H,
      Math.min(start.h + dy, viewport.h - start.y),
    );
  } else if (edges.top) {
    // The bottom may already sit below the viewport after a drag; the title
    // bar still has to stay reachable.
    const bottom = start.y + start.h;
    y = Math.max(
      0,
      Math.min(
        bottom - FLOATING_PANEL_MIN_H,
        viewport.h - FLOATING_PANEL_REACHABLE_H,
        start.y + dy,
      ),
    );
    h = bottom - y;
  }
  return { x, y, w, h };
}

/** Per-browser width the corner Chat panel last had; it reopens at that width. */
const CHAT_CORNER_WIDTH_STORAGE_KEY = "g5:chat-corner-width";
/** Where the corner Chat button was dragged to in this browser. */
const CHAT_CORNER_BUTTON_OFFSET_STORAGE_KEY = "g5:chat-corner-button-offset";

// Even reading `localStorage` can throw where storage is denied, so the
// access itself stays inside the try: a denied store must not break the page.
function readStoredChatCornerWidth(): number {
  try {
    const value = Number(localStorage.getItem(CHAT_CORNER_WIDTH_STORAGE_KEY));
    return Number.isFinite(value) && value > 0
      ? clampFloatingPanelWidth(value)
      : FLOATING_PANEL_DEFAULT_SIZE.w;
  } catch {
    return FLOATING_PANEL_DEFAULT_SIZE.w;
  }
}

function writeStoredChatCornerWidth(w: number): void {
  try {
    localStorage.setItem(CHAT_CORNER_WIDTH_STORAGE_KEY, String(Math.round(w)));
  } catch {
    // Per-browser bookkeeping must never break the panel.
  }
}

/**
 * Where a floating panel opens: centered (Pop out), or anchored to the right
 * edge below the shell chrome and stretched to the bottom (the corner Chat
 * button, workspaces g5.workspace.ui.chat-corner-button).
 */
type FloatingPanelPlacement = "center" | "right-edge";

/**
 * Bottom of the lowest workspace chrome row: the tab bar (or the phone's
 * title bar) and the workbench header under it (phase chevrons and the action
 * ribbon with its Chat toggle). A right-edge Chat must leave every control in
 * those rows reachable.
 */
function shellChromeBottom(): number {
  let bottom = 0;
  document
    .querySelectorAll<HTMLElement>(
      '[data-surface="titlebar"], [data-testid="workspace-tab-bar"], [data-testid="workbench-header"]',
    )
    .forEach((element) => {
      bottom = Math.max(bottom, element.getBoundingClientRect().bottom);
    });
  return Math.round(bottom);
}

interface FloatingPanelProps {
  title: string;
  children: React.ReactNode;
  onDock: () => void;
  onClose: () => void;
  /** Starting position; defaults to centered. */
  initialPlacement?: FloatingPanelPlacement;
  /**
   * Starting size. Omit to use the primitive's default — this default is a
   * fallback, not a house style: a consumer that is lifted out of a docked
   * surface seeds its own width so popping out preserves it (shell D38).
   */
  initialSize?: { w: number; h: number };
  /** Called with the final size when a resize drag ends. */
  onResizeEnd?: (size: { w: number; h: number }) => void;
}

function FloatingPanel({
  title,
  children,
  onDock,
  onClose,
  initialSize,
  initialPlacement = "center",
  onResizeEnd,
}: FloatingPanelProps): ReactElement {
  const panelRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: 0, y: 0 });
  const [size, setSize] = useState(() =>
    initialSize
      ? {
          w: clampFloatingPanelWidth(initialSize.w),
          h: Math.max(FLOATING_PANEL_MIN_H, initialSize.h),
        }
      : FLOATING_PANEL_DEFAULT_SIZE,
  );
  const [zIndex, setZIndex] = useState(() => ++floatingPanelZCounter);
  const draggingRef = useRef(false);
  const resizingRef = useRef(false);
  const dragStartRef = useRef({ x: 0, y: 0, px: 0, py: 0 });
  const resizeStartRef = useRef<{
    x: number;
    y: number;
    frame: FloatingPanelFrame;
    edges: FloatingPanelResizeEdges;
  }>({ x: 0, y: 0, frame: { x: 0, y: 0, w: 0, h: 0 }, edges: {} });
  const resizeFrameRef = useRef<FloatingPanelFrame | null>(null);

  // Place on mount: centered, or against the right edge under the chrome.
  useEffect(() => {
    if (initialPlacement === "right-edge") {
      const top = shellChromeBottom();
      const h = Math.max(FLOATING_PANEL_MIN_H, window.innerHeight - top);
      setSize((current) => ({ w: current.w, h }));
      setPos({ x: Math.max(0, window.innerWidth - size.w), y: top });
      return;
    }
    setPos({
      x: Math.max(40, Math.round((window.innerWidth - size.w) / 2)),
      y: Math.max(40, Math.round((window.innerHeight - size.h) / 2)),
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Drag handlers
  const onTitlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      e.preventDefault();
      draggingRef.current = true;
      dragStartRef.current = {
        x: e.clientX,
        y: e.clientY,
        px: pos.x,
        py: pos.y,
      };
      document.body.style.userSelect = "none";
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
    },
    [pos],
  );

  const onTitlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!draggingRef.current) return;
      const dx = e.clientX - dragStartRef.current.x;
      const dy = e.clientY - dragStartRef.current.y;
      setPos({
        x: Math.max(
          0,
          Math.min(window.innerWidth - size.w, dragStartRef.current.px + dx),
        ),
        y: Math.max(
          0,
          Math.min(
            window.innerHeight - FLOATING_PANEL_REACHABLE_H,
            dragStartRef.current.py + dy,
          ),
        ),
      });
    },
    [size.w],
  );

  const onTitlePointerUp = useCallback((e: React.PointerEvent) => {
    draggingRef.current = false;
    document.body.style.userSelect = "";
    (e.target as HTMLElement).releasePointerCapture(e.pointerId);
  }, []);

  // Resize handlers: one set for every edge and corner handle.
  const onResizePointerDown = useCallback(
    (edges: FloatingPanelResizeEdges, e: React.PointerEvent) => {
      e.preventDefault();
      e.stopPropagation();
      resizingRef.current = true;
      resizeStartRef.current = {
        x: e.clientX,
        y: e.clientY,
        frame: { x: pos.x, y: pos.y, w: size.w, h: size.h },
        edges,
      };
      resizeFrameRef.current = null;
      document.body.style.userSelect = "none";
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
    },
    [pos, size],
  );

  const onResizePointerMove = useCallback((e: React.PointerEvent) => {
    if (!resizingRef.current) return;
    const start = resizeStartRef.current;
    const frame = resizeFloatingPanelFrame(
      start.frame,
      start.edges,
      e.clientX - start.x,
      e.clientY - start.y,
      { w: window.innerWidth, h: window.innerHeight },
    );
    resizeFrameRef.current = frame;
    setPos({ x: frame.x, y: frame.y });
    setSize({ w: frame.w, h: frame.h });
  }, []);

  const onResizePointerUp = useCallback(
    (e: React.PointerEvent) => {
      if (!resizingRef.current) return;
      resizingRef.current = false;
      document.body.style.userSelect = "";
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
      const frame = resizeFrameRef.current;
      if (frame) onResizeEnd?.({ w: frame.w, h: frame.h });
    },
    [onResizeEnd],
  );

  /** Bring this panel to the front of all floating panels */
  const bringToFront = useCallback(() => {
    setZIndex((prev) => {
      const next = ++floatingPanelZCounter;
      return next === prev ? prev : next;
    });
  }, []);

  return (
    <div
      ref={panelRef}
      className="fixed flex flex-col rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] shadow-2xl overflow-hidden"
      style={{
        left: pos.x,
        top: pos.y,
        width: size.w,
        height: size.h,
        zIndex,
      }}
      onPointerDown={bringToFront}
    >
      {/* Title bar — draggable */}
      <div
        className="flex items-center gap-2 px-3 h-8 bg-[var(--g5-bg2)] cursor-move select-none shrink-0 border-b border-[var(--g5-stroke)]"
        onPointerDown={onTitlePointerDown}
        onPointerMove={onTitlePointerMove}
        onPointerUp={onTitlePointerUp}
      >
        <span className="text-xs font-medium text-[var(--g5-text0)] flex-1">
          {title}
        </span>
        <button
          data-testid="floating-panel-dock"
          className="p-0.5 rounded hover:bg-[var(--g5-bg0)] text-[var(--g5-text2)] hover:text-[var(--g5-text0)] transition-colors"
          title="Dock panel"
          onClick={onDock}
        >
          <PanelRightClose size={13} />
        </button>
        <button
          data-testid="floating-panel-close"
          className="p-0.5 rounded hover:bg-[var(--g5-blocked)]/20 text-[var(--g5-text2)] hover:text-[var(--g5-blocked)] transition-colors"
          title="Close"
          onClick={onClose}
        >
          ✕
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-hidden flex flex-col">{children}</div>

      {/* Resize handles — every edge and corner; the bottom-right one
          carries the visible grip. */}
      {FLOATING_PANEL_RESIZE_HANDLES.map(({ id, edges, className }) => (
        <div
          key={id}
          data-testid={`floating-panel-resize-${id}`}
          className={classNames("absolute", className)}
          onPointerDown={(e) => onResizePointerDown(edges, e)}
          onPointerMove={onResizePointerMove}
          onPointerUp={onResizePointerUp}
        >
          {id === "bottom-right" && (
            <svg
              width="16"
              height="16"
              viewBox="0 0 16 16"
              className="text-[var(--g5-text2)] opacity-50"
            >
              <path d="M14 14L8 14L14 8Z" fill="currentColor" />
            </svg>
          )}
        </div>
      ))}
    </div>
  );
}

// =============================================================================
// MobileLayout — chat-focused mobile layout (below lg breakpoint)
// =============================================================================

/**
 * One visit to a workspace by this page, replaced on every workspace change
 * and marked inactive on unmount; retained callbacks and the pending composer
 * seed are bound to it.
 */
interface WorkspaceViewScope {
  key: string;
  active: boolean;
}

/**
 * The start a seeded chat action's send must make (ui
 * g5.workbench.chat-actions): the persona and ribbon phase of the clicked
 * action, and the action whose Guided step reads Running once the send is
 * accepted. No configuration revision: the send pins the version loaded when
 * the person sends, because the browser already expanded the template and
 * every docs commit in between (each question answer is one) would otherwise
 * make a click-time pin stale (issue #1142). `sessionId` names the open chat
 * the seed went into, null for a fresh chat.
 */
interface ChatComposerSeededStart {
  actionId: string;
  persona: WorkspacePhase;
  phase: WorkspacePhase;
  sessionId: string | null;
}

/**
 * The page's one pending composer seed (ui g5.workbench.chat-actions): the
 * one-shot seed and what its send carries, bound to the workspace visit that
 * made it. Another visit never sees it, and a chat other than the one it went
 * into drops it (issue #1190).
 */
interface ChatComposerSeedRequest {
  scope: WorkspaceViewScope;
  seed: ChatComposerSeed;
  /** Rides the seeded fresh chat's first send once (D69 Discuss). */
  reference: ChatProjectReference | null;
  start: ChatComposerSeededStart | null;
}

interface MobileLayoutProps {
  /** See ChatPanel `promptHistoryScope`. */
  chatPromptHistoryScope?: ChatPromptHistoryScope;
  /** See ChatPanel `surfaceVisible`: false while a dialog covers the chat. */
  chatSurfaceVisible: boolean;
  /** See ChatPanel `onAcknowledgeResult`. */
  onAcknowledgeChatResult?: (
    sessionId: string,
    result: ChatSessionResultDTO,
  ) => Promise<unknown> | void;
  unreadNotificationCount: number;
  onOpenNotifications: () => void;
  /**
   * Identity banner rendered under the mobile header (the review workspace
   * banner with Finish review), so a review can be finished on a phone.
   */
  banner?: ReactNode;
  // Chat props
  chatSession: ChatSessionDTO | null;
  chatSessions: ChatSessionDTO[];
  chatMessages: ChatMessageDTO[];
  isChatStreaming: boolean;
  stopControlSuppressed: boolean;
  stopState?: ChatStopState | null;
  liveTailState?: LiveTailState;
  chatTimelineEntries?: TimelineEntry[];
  /** Backward timeline pagination for the chat panel (chat_panel_runtime D35). */
  chatHasEarlierHistory?: boolean;
  chatIsLoadingEarlierHistory?: boolean;
  chatTimelineCoverage?: TimelineCoverage;
  chatEarlierHistoryError?: Error | null;
  onLoadEarlierChatHistory?: () => void;
  /** Messages-source scrollback (job_chat_draft_hooks D39; chat_panel_runtime D38). */
  chatHasOlderMessages?: boolean;
  chatIsLoadingOlderMessages?: boolean;
  onLoadOlderChatMessages?: () => void;
  /** Active chat session's task summary (chat_panel_runtime D31, #1597). */
  activeChatTaskSummary?: TaskSummary | null;
  workspaceId: string;
  workspaceSlug: string;
  selectedAgentProvider: ChatAgentProvider;
  availableAgentProviders: ChatAgentProvider[];
  providerAccountsEnabled: boolean;
  providerAccounts?: ProviderAccountDTO[];
  providerAccountAssignments: Record<PickerHarness, string>;
  runtimeVariablesAvailable: Record<PickerHarness, boolean>;
  onSelectAgentProvider: (provider: ChatAgentProvider) => void;
  resolveAgentLaunch: ResolveAgentLaunchPreferences;
  setupError?: string | null;
  composerDisabledReason?: string | null;
  startAndSend?: boolean;
  composerNotice?: ReactNode;
  canManageSessionInputs: boolean;
  projectReferences?: ChatProjectReference[];
  activeProjectReference?: ChatProjectReference | null;
  composerSeed?: ChatComposerSeed | null;
  /** The workspace's unsent draft, the owner of its text (ChatPanel `draft`). */
  composerDraft?: ChatComposerDraft;
  onComposerSeedEmptied?: (token: number) => void;
  composerPlaceholder?: string;
  composerScope?: object;
  defaultClaudeModel?: string;
  defaultAgentProviderChoices?: UserSettings["defaultAgentProviderChoices"];
  /** Composer usage-chip window setting (agent_interaction_ux D8). */
  subscriptionUsageWindow?: UserSettings["subscriptionUsageWindow"];
  /** Pre-session workspace provider-quota snapshot for the usage chip (D8). */
  workspaceSubscriptionUsage?: SubscriptionUsage | null;
  onResolveAgentApproval?: (
    input: AgentApprovalResolutionInput,
  ) => void | Promise<unknown>;
  onSendProviderControl?: (input: {
    sessionId: string;
    action: string;
    payload?: Record<string, unknown>;
  }) => void | Promise<unknown>;
  onSendChatMessage: (
    content: string,
    options?: {
      model?: string;
      permissionMode?: string;
      effort?: string;
      projectReferences?: ChatProjectReference[];
    },
  ) => ChatSendResult | Promise<ChatSendResult>;
  onStopStreaming?: () => void;
  /** Owner-only End for a Claude background wait; omitted for readers. */
  onEndBackgroundWait?: (
    sessionId: string,
    wait: { taskId: string; attempt: number },
  ) => Promise<unknown> | void;
  onSelectChatSession: (sessionId: string) => void;
  onCreateChatSession?: () => void;
  onRenameChatSession?: (
    sessionId: string,
    title: string | null,
  ) => void | Promise<unknown>;
}

function MobileLayout({
  chatPromptHistoryScope: promptHistoryScope,
  onAcknowledgeChatResult,
  chatSurfaceVisible,
  unreadNotificationCount,
  onOpenNotifications,
  chatSession,
  chatSessions,
  chatMessages,
  isChatStreaming,
  stopControlSuppressed,
  stopState,
  liveTailState,
  chatTimelineEntries,
  chatHasEarlierHistory,
  chatIsLoadingEarlierHistory,
  chatTimelineCoverage,
  chatEarlierHistoryError,
  onLoadEarlierChatHistory,
  chatHasOlderMessages,
  chatIsLoadingOlderMessages,
  onLoadOlderChatMessages,
  activeChatTaskSummary = null,
  workspaceId,
  workspaceSlug,
  selectedAgentProvider,
  availableAgentProviders,
  providerAccountsEnabled,
  providerAccounts,
  providerAccountAssignments,
  runtimeVariablesAvailable,
  onSelectAgentProvider,
  resolveAgentLaunch,
  setupError,
  composerDisabledReason,
  startAndSend = false,
  composerNotice = null,
  canManageSessionInputs,
  projectReferences,
  activeProjectReference,
  composerSeed,
  composerDraft,
  onComposerSeedEmptied,
  composerPlaceholder,
  composerScope,
  defaultClaudeModel,
  defaultAgentProviderChoices,
  subscriptionUsageWindow,
  workspaceSubscriptionUsage = null,
  onResolveAgentApproval,
  onSendProviderControl,
  onSendChatMessage,
  onStopStreaming,
  onEndBackgroundWait,
  onSelectChatSession,
  onCreateChatSession,
  onRenameChatSession,
  banner,
}: MobileLayoutProps): ReactElement {
  return (
    <div className="flex flex-col h-screen bg-[var(--g5-bg0)]">
      {/* Mobile header */}
      <div className="flex items-center gap-2 px-3 h-12 bg-[var(--g5-bg1)] border-b border-[var(--g5-stroke)]">
        <div className="flex-1" />

        <button
          data-testid="mobile-notifications-open"
          className="relative p-1.5 rounded hover:bg-[var(--g5-bg2)] text-[var(--g5-text1)]"
          onClick={onOpenNotifications}
          title="Notifications"
        >
          <Bell className="w-4 h-4" />
          {unreadNotificationCount > 0 && (
            <span className="absolute -top-0.5 -right-0.5 w-3.5 h-3.5 rounded-full bg-[var(--g5-blocked)] text-[9px] text-white flex items-center justify-center font-medium">
              {unreadNotificationCount > 9 ? "9+" : unreadNotificationCount}
            </span>
          )}
        </button>
      </div>

      {banner}

      {/* Full-height ChatPanel */}
      <div className="flex-1 overflow-hidden">
        <ChatPanel
          // One panel per workspace, as on desktop: its draft and applied
          // seed belong to that workspace (g5.workbench.chat-actions).
          key={workspaceId}
          draft={composerDraft}
          promptHistoryScope={promptHistoryScope}
          surfaceVisible={chatSurfaceVisible}
          onAcknowledgeResult={onAcknowledgeChatResult}
          session={chatSession}
          messages={chatMessages}
          isStreaming={isChatStreaming}
          stopControlSuppressed={stopControlSuppressed}
          stopState={stopState}
          liveTailState={liveTailState}
          timelineEntries={chatTimelineEntries}
          hasEarlierHistory={chatHasEarlierHistory}
          isLoadingEarlierHistory={chatIsLoadingEarlierHistory}
          timelineCoverage={chatTimelineCoverage}
          earlierHistoryError={chatEarlierHistoryError}
          onLoadEarlierHistory={onLoadEarlierChatHistory}
          hasOlderMessages={chatHasOlderMessages}
          isLoadingOlderMessages={chatIsLoadingOlderMessages}
          onLoadOlderMessages={onLoadOlderChatMessages}
          activeTaskSummary={activeChatTaskSummary}
          scopeType="workspace"
          scopeRef={workspaceId}
          scopeLabel={workspaceSlug}
          sessions={chatSessions}
          selectedAgentProvider={selectedAgentProvider}
          availableAgentProviders={availableAgentProviders}
          providerAccountsEnabled={providerAccountsEnabled}
          providerAccounts={providerAccounts}
          providerAccountAssignments={providerAccountAssignments}
          runtimeVariablesAvailable={runtimeVariablesAvailable}
          onSelectAgentProvider={onSelectAgentProvider}
          resolveAgentLaunch={resolveAgentLaunch}
          setupError={setupError}
          composerDisabledReason={composerDisabledReason}
          startAndSend={startAndSend}
          composerNotice={composerNotice}
          canManageSessionInputs={canManageSessionInputs}
          projectReferences={projectReferences}
          activeProjectReference={activeProjectReference}
          composerSeed={composerSeed}
          onComposerSeedEmptied={onComposerSeedEmptied}
          composerPlaceholder={composerPlaceholder}
          composerScope={composerScope}
          defaultClaudeModel={defaultClaudeModel}
          defaultAgentProviderChoices={defaultAgentProviderChoices}
          subscriptionUsageWindow={subscriptionUsageWindow}
          workspaceSubscriptionUsage={workspaceSubscriptionUsage}
          onResolveAgentApproval={onResolveAgentApproval}
          onSendProviderControl={onSendProviderControl}
          onSendMessage={onSendChatMessage}
          onStopStreaming={onStopStreaming}
          onEndBackgroundWait={onEndBackgroundWait}
          onForkSession={onCreateChatSession ? () => {} : undefined}
          onSummarize={() => {}}
          onCreateSession={onCreateChatSession}
          onSelectSession={onSelectChatSession}
          onRenameSession={onRenameChatSession}
        />
      </div>
    </div>
  );
}

// LeftSidebar is imported from ../components/tree/TreeNode (stacked collapsible sections)

// =============================================================================
// CenterPanel — tab bar + content + draft bar
// =============================================================================

function EmptyWorkspacePrompt({
  onOpenGuideDoc,
}: {
  onOpenGuideDoc: (guideId: GuideDocId) => void;
}): ReactElement {
  return (
    <div className="flex items-center justify-center h-full px-4 text-[var(--g5-text2)] text-sm text-center">
      <span>
        Select a node from the sidebar, or start with{" "}
        <button
          type="button"
          data-testid="empty-workspace-getting-started"
          className="font-medium text-[var(--g5-active)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-active)]/40 rounded-sm"
          onClick={() => onOpenGuideDoc(DEFAULT_GUIDE_DOC_ID)}
        >
          Getting Started
        </button>{" "}
        for a quick tour.
      </span>
    </div>
  );
}

/** The review a copy was forked for, as the center panel needs it. */
interface ReviewCopyOrigin {
  reviewWorkspaceId: string;
  sourceWorkspaceId: string;
  sourceWorkspaceName: string;
}

type IssuePageControls = (
  tab: Extract<TabItem, { type: "external_issue" }>,
) =>
  | Pick<ExternalIssueDetailPanelProps, "actions" | "onRequestApproval">
  | undefined;

interface CenterPanelProps {
  tabs: TabItem[];
  selectedTabId: string | null;
  /** Chat is a persistent first-class center surface, separate from content tabs. */
  chatActive: boolean;
  chatAvailable: boolean;
  /** Render the tab strip (workspaces g5.workspace.ui.center-tabs); off by default. */
  showTabBar: boolean;
  /** Select the persistent Chat surface. */
  onSelectChat: () => void;
  /** Existing workspace ChatPanel rendered in the center content area. */
  chatContent: ReactNode;
  unresolvedContentBranches: BranchSummary[] | null;
  onSelectContentBranch: (branchId: string) => void;
  onSelectTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  /** Close every tab except the right-clicked one (context-menu action). */
  onCloseOtherTabs: (tabId: string) => void;
  /** Close every tab positioned after the right-clicked one (context-menu action). */
  onCloseTabsToRight: (tabId: string) => void;
  /** Tabs that stay open like Chat: no close control, kept by bulk closes. */
  pinnedTabIds?: readonly string[];
  onOpenTab: (item: TabItem) => void;
  /** Download the workspace source as a zip (Workspace Details). */
  onDownloadSource?: () => void;
  downloadSourceInProgress?: boolean;
  /** Workbench status header above the Activity feed (g5.workbench.landing). */
  activityHeader?: ReactNode;
  issuePageControls?: IssuePageControls;
  /** Open the Pull Requests tab from a pull-request Activity row. */
  onOpenPullRequests?: () => void;
  /** Open the preview dialog from a preview Activity row (ui g5.ui.activity.preview-row). */
  onOpenPreview?: (request: PreviewOpenRequest) => void;
  showComments: boolean;
  onToggleComments: () => void;
  /** The viewer owns this workspace (accepts review suggestions in place). */
  viewerOwnsWorkspace?: boolean;
  /** The owner's VM is running (accepting writes into it). */
  ownerVmRunning?: boolean;
  /** What the viewer may do in anchored comment threads. */
  commentPermissions?: CommentPermissions;
  /** Set when this workspace is a review copy (see ContentDispatch). */
  copyOf?: ReviewCopyOrigin | null;
  // Draft bar
  workspaceId: string;
  workspaceTerminalEnabled: boolean;
  workspaceTerminalKey: string;
  /** Authoritative Git change count for nested final-stage actions. */
  workspaceDirtyCount: number;
  sourceControlChangeGraphAvailable: boolean;
  sourceControlChangeGraphDefaultComparison: "workspace" | "uncommitted";
  branchId: string;
  repoId: string;
  docs: DocNode[];

  hasActiveTask: boolean;
  /** When set, scroll to the block with this intent record label (seq ensures uniqueness) */
  scrollToIntentRecord?: { label: string; seq: number } | null;
  /** Callback to update scrollToIntentRecord state */
  onScrollToIntentRecord?: (label: string | null) => void;
  /** Navigation origin for back-button after eye-button navigation */
  navigationOrigin?: NavigationOrigin | null;
  /** Callback to update navigationOrigin state */
  onSetNavigationOrigin?: (origin: NavigationOrigin | null) => void;
  /**
   * D60: seed the chat composer with a host-templated prompt (prefill-only).
   * Threaded to the HTML doc preview's answer bridge.
   */
  onSeedChatPrompt?: (text: string, reference?: ChatProjectReference) => void;
  /**
   * Seed a fresh chat beside the document, for a question card's Discuss and
   * a finding's chat action (`g5.workbench.chat-actions`).
   */
  onDiscussInChat?: (text: string, reference?: ChatProjectReference) => void;
  onOpenWorkspaceFile?: (
    filePath: string,
    repoRef?: string,
    line?: number,
  ) => void;
}

const CenterPanel = React.memo(function CenterPanel({
  tabs,
  selectedTabId,
  chatActive,
  chatAvailable,
  showTabBar,
  onSelectChat,
  chatContent,
  unresolvedContentBranches,
  onSelectContentBranch,
  onSelectTab,
  onCloseTab,
  onCloseOtherTabs,
  onCloseTabsToRight,
  pinnedTabIds = [],
  onOpenTab,
  onDownloadSource,
  downloadSourceInProgress,
  activityHeader,
  issuePageControls,
  onOpenPullRequests,
  onOpenPreview,
  showComments,
  onToggleComments,
  viewerOwnsWorkspace = false,
  ownerVmRunning = false,
  commentPermissions = READ_ONLY_COMMENT_PERMISSIONS,
  copyOf = null,
  workspaceId,
  workspaceTerminalEnabled,
  workspaceTerminalKey,
  workspaceDirtyCount,
  sourceControlChangeGraphAvailable,
  sourceControlChangeGraphDefaultComparison,
  branchId,
  repoId,
  docs,
  hasActiveTask,
  scrollToIntentRecord,
  onScrollToIntentRecord,
  navigationOrigin,
  onSetNavigationOrigin,
  onSeedChatPrompt,
  onDiscussInChat,
  onOpenWorkspaceFile,
}: CenterPanelProps): ReactElement {
  const selectedTabItem = selectedTabId
    ? (tabs.find((tab) => getTabId(tab) === selectedTabId) ?? null)
    : null;
  // A link that opens another file offers Back to where it was followed
  // (`g5.documents.ontology.links`). Stable while the tab is, so a rendered
  // document keyed on it does not render again.
  const openFileWithBack = useCallback(
    (filePath: string, repoRef?: string, line?: number, from?: string) => {
      const source =
        selectedTabItem?.type === "file" ? selectedTabItem.filePath : null;
      if (source && source !== filePath) {
        const origin = buildNavigationOrigin(selectedTabItem, {
          type: "file",
          id: filePath,
          sourceFileName: source,
          sourceIntentRecordNumber: from,
        });
        onSetNavigationOrigin?.(origin && { ...origin, target: filePath });
      }
      onOpenWorkspaceFile?.(filePath, repoRef, line);
    },
    [selectedTabItem, onSetNavigationOrigin, onOpenWorkspaceFile],
  );
  const terminalOpen = tabs.some((tab) => tab.type === "workspace_terminal");
  const terminalActive =
    !chatActive && selectedTabItem?.type === "workspace_terminal";
  const [activatedTerminalKey, setActivatedTerminalKey] = useState<
    string | null
  >(null);
  useEffect(() => {
    setActivatedTerminalKey((previous) => {
      if (!terminalOpen) return null;
      if (terminalActive) return workspaceTerminalKey;
      return previous === workspaceTerminalKey ? previous : null;
    });
  }, [terminalOpen, terminalActive, workspaceTerminalKey]);
  const mountTerminal =
    terminalOpen &&
    (terminalActive || activatedTerminalKey === workspaceTerminalKey);
  const contentActive = !chatActive && !terminalActive;
  const selectedContentTabControlId = selectedTabItem
    ? `center-content-tab-${encodeURIComponent(getTabId(selectedTabItem))}`
    : undefined;
  const openGuideDoc = useCallback(
    (guideId: GuideDocId) => {
      const tab = guideDocTab(guideId);
      if (tab) onOpenTab(tab);
    },
    [onOpenTab],
  );

  /** Return a lucide icon element based on tab name extension */
  const getTabIcon = (tab: TabItem) => {
    if (tab.type === "workspace_terminal") {
      return <SquareTerminal className="w-3.5 h-3.5 shrink-0" />;
    }
    if (tab.type === "guide_doc") {
      return (
        <BookOpen className="w-3.5 h-3.5 shrink-0 text-[var(--g5-active)]" />
      );
    }
    if (tab.type === "workspace_review") {
      return (
        <ClipboardList className="w-3.5 h-3.5 shrink-0 text-[var(--g5-active)]" />
      );
    }
    const name = tab.name.toLowerCase();
    if (
      name.endsWith(".tsx") ||
      name.endsWith(".ts") ||
      name.endsWith(".rs") ||
      name.endsWith(".js") ||
      name.endsWith(".jsx")
    ) {
      return (
        <FileCode className="w-3.5 h-3.5 shrink-0 text-[var(--g5-active)]" />
      );
    }
    return (
      <FileText className="w-3.5 h-3.5 shrink-0 text-[var(--g5-info)]/60" />
    );
  };

  // Right-click tab context menu (Chrome-like Close / Close Others / Close Right)
  const [tabMenu, setTabMenu] = useState<{
    tabId: string;
    x: number;
    y: number;
  } | null>(null);

  useEffect(() => {
    if (!tabMenu) return;
    const dismiss = () => setTabMenu(null);
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setTabMenu(null);
    };
    // This effect runs after the render triggered by the opening right-click,
    // so that right-click's own mousedown has already fired before the dismiss
    // listener is attached — it is the *next* mousedown that closes the menu.
    // Clicks inside the menu call stopPropagation, so they never reach here.
    window.addEventListener("mousedown", dismiss);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("scroll", dismiss, true);
    window.addEventListener("resize", dismiss);
    return () => {
      window.removeEventListener("mousedown", dismiss);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("scroll", dismiss, true);
      window.removeEventListener("resize", dismiss);
    };
  }, [tabMenu]);

  const tabMenuIndex = tabMenu
    ? tabs.findIndex((t) => getTabId(t) === tabMenu.tabId)
    : -1;
  const canCloseOthers = tabs.length > 1;
  const canCloseRight = tabMenuIndex >= 0 && tabMenuIndex < tabs.length - 1;

  return (
    <div
      data-testid="center-panel"
      data-surface="client"
      className="flex-1 min-h-0 min-w-0 flex flex-col overflow-hidden bg-[var(--g5-bg0)]"
    >
      {/* Tab bar — rendered only while the person's Workspace tabs setting
          is on (workspaces g5.workspace.ui.center-tabs). */}
      {showTabBar && (
        <div
          data-testid="tab-bar"
          data-surface="chrome"
          role="tablist"
          aria-label="Workspace views"
          onKeyDown={(event) => {
            if (
              event.key !== "ArrowLeft" &&
              event.key !== "ArrowRight" &&
              event.key !== "Home" &&
              event.key !== "End"
            ) {
              return;
            }
            const tabElements = Array.from(
              event.currentTarget.querySelectorAll<HTMLButtonElement>(
                '[role="tab"]',
              ),
            );
            const currentIndex = tabElements.findIndex(
              (tab) => tab === event.currentTarget.ownerDocument.activeElement,
            );
            if (currentIndex < 0 || tabElements.length === 0) return;
            event.preventDefault();
            const nextIndex =
              event.key === "Home"
                ? 0
                : event.key === "End"
                  ? tabElements.length - 1
                  : (currentIndex +
                      (event.key === "ArrowRight" ? 1 : -1) +
                      tabElements.length) %
                    tabElements.length;
            const nextTab = tabElements[nextIndex];
            nextTab.focus();
            nextTab.click();
          }}
          className="flex items-center gap-px px-1.5 h-9 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] overflow-x-auto shrink-0"
        >
          {chatAvailable && (
            <button
              type="button"
              id="center-chat-tab-control"
              data-testid="center-chat-tab"
              role="tab"
              aria-selected={chatActive}
              aria-controls="center-chat-panel"
              tabIndex={chatActive || !selectedTabItem ? 0 : -1}
              className={classNames(
                "group flex items-center gap-1.5 px-3 py-1.5 text-[12px] cursor-pointer shrink-0 transition-colors",
                chatActive
                  ? "bg-[var(--g5-bg0)] text-[var(--g5-text0)] border-b-2 border-b-[var(--g5-active)]"
                  : "text-[var(--g5-text2)] hover:text-[var(--g5-text1)] hover:bg-white/[0.04]",
              )}
              onClick={onSelectChat}
            >
              <MessageCircle className="w-3.5 h-3.5 shrink-0 text-[var(--g5-info)]/70" />
              <span className="font-medium">Chat</span>
            </button>
          )}
          {tabs.map((tab) => {
            const tabId = getTabId(tab);
            const isActive = !chatActive && tabId === selectedTabId;
            const pinned = pinnedTabIds.includes(tabId);
            return (
              <div
                key={tabId}
                data-workspace-activity={
                  tab.type === "workspace_terminal" ? "ignore" : undefined
                }
                className={classNames(
                  "group flex items-center gap-1.5 px-3 py-1.5 text-[12px] cursor-pointer shrink-0 transition-colors",
                  isActive
                    ? "bg-[var(--g5-bg0)] text-[var(--g5-text0)] border-b-2 border-b-[var(--g5-active)]"
                    : "text-[var(--g5-text2)] hover:text-[var(--g5-text1)] hover:bg-white/[0.04]",
                )}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setTabMenu({ tabId, x: e.clientX, y: e.clientY });
                }}
              >
                <button
                  type="button"
                  data-testid="center-content-tab"
                  id={`center-content-tab-${encodeURIComponent(tabId)}`}
                  role="tab"
                  aria-selected={isActive}
                  aria-controls={
                    tab.type === "workspace_terminal"
                      ? "center-terminal-panel"
                      : "center-content-panel"
                  }
                  tabIndex={isActive ? 0 : -1}
                  className="flex min-w-0 items-center gap-1.5"
                  onClick={() => onSelectTab(tabId)}
                >
                  {getTabIcon(tab)}
                  <span className="truncate max-w-[140px] font-medium">
                    {displayNodeName(tabFileName(tab, docs) ?? tab.name)}
                  </span>
                </button>
                {!pinned && (
                  <button
                    type="button"
                    data-testid="center-tab-close"
                    className={classNames(
                      "ml-1 w-4 h-4 flex items-center justify-center rounded hover:bg-[var(--g5-stroke)] text-[var(--g5-text2)]",
                      isActive
                        ? "opacity-100"
                        : "opacity-0 group-hover:opacity-100",
                      "focus-visible:opacity-100 transition-opacity duration-150",
                    )}
                    onClick={(e) => {
                      e.stopPropagation();
                      onCloseTab(tabId);
                    }}
                    title="Close tab"
                  >
                    ×
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Tab right-click context menu */}
      {tabMenu && tabMenuIndex >= 0 && (
        <div
          data-testid="tab-context-menu"
          role="menu"
          className="fixed z-50 min-w-[180px] py-1 rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] shadow-lg text-[12px]"
          style={{
            top: Math.min(tabMenu.y, window.innerHeight - 120),
            left: Math.min(tabMenu.x, window.innerWidth - 200),
          }}
          // Keep clicks inside the menu from bubbling to the window dismiss
          // handler before the item's onClick runs.
          onMouseDown={(e) => e.stopPropagation()}
        >
          {(
            [
              {
                key: "close",
                label: "Close",
                enabled: !pinnedTabIds.includes(tabMenu.tabId),
                run: () => onCloseTab(tabMenu.tabId),
              },
              {
                key: "close-others",
                label: "Close Other Tabs",
                enabled: canCloseOthers,
                run: () => onCloseOtherTabs(tabMenu.tabId),
              },
              {
                key: "close-right",
                label: "Close Tabs to the Right",
                enabled: canCloseRight,
                run: () => onCloseTabsToRight(tabMenu.tabId),
              },
            ] as const
          ).map((item) => (
            <button
              key={item.key}
              data-testid="center-tab-context-menu-item"
              role="menuitem"
              disabled={!item.enabled}
              className={classNames(
                "w-full text-left px-3 py-1.5 transition-colors",
                item.enabled
                  ? "text-[var(--g5-text1)] hover:bg-white/[0.06] hover:text-[var(--g5-text0)] cursor-pointer"
                  : "text-[var(--g5-text2)]/50 cursor-default",
              )}
              onClick={() => {
                if (!item.enabled) return;
                item.run();
                setTabMenu(null);
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}

      {/* Keep Chat mounted while a content tab is active so its unsent draft,
          references, and attachments survive center-surface switches. */}
      <div className="flex-1 min-h-0 min-w-0 overflow-hidden">
        {chatAvailable && (
          <div
            id="center-chat-panel"
            data-testid="center-chat-panel"
            role="tabpanel"
            aria-labelledby={showTabBar ? "center-chat-tab-control" : undefined}
            aria-label={showTabBar ? undefined : "Chat"}
            hidden={!chatActive}
            className={chatActive ? "h-full" : "hidden"}
          >
            {chatContent}
          </div>
        )}
        {/* Connect on first selection, including for restored tabs, then keep
            the PTY alive across center-tab switches. Closing the tab or
            replacing the VM incarnation clears that activation. */}
        {mountTerminal && (
          <div
            id="center-terminal-panel"
            data-testid="center-terminal-panel"
            role="tabpanel"
            aria-labelledby={
              showTabBar ? "center-content-tab-workspace-terminal" : undefined
            }
            aria-label={showTabBar ? undefined : "Terminal"}
            hidden={!terminalActive}
            className={terminalActive ? "h-full min-h-0" : "hidden"}
          >
            <WorkspaceTerminalPanel
              key={workspaceTerminalKey}
              workspaceId={workspaceId}
              enabled={workspaceTerminalEnabled}
              visible={terminalActive}
            />
          </div>
        )}
        <div
          id="center-content-panel"
          data-testid="center-content-panel"
          role="tabpanel"
          aria-labelledby={showTabBar ? selectedContentTabControlId : undefined}
          aria-label={showTabBar ? undefined : selectedTabItem?.name}
          hidden={!contentActive}
          className={contentActive ? "h-full" : "hidden"}
        >
          {contentActive &&
          unresolvedContentBranches &&
          requiresContentBranch(selectedTabItem) ? (
            <ContentBranchSelectionState
              branches={unresolvedContentBranches}
              onSelect={onSelectContentBranch}
            />
          ) : contentActive && selectedTabId ? (
            <ContentDispatch
              key={selectedTabId}
              tab={selectedTabItem}
              onDownloadSource={onDownloadSource}
              downloadSourceInProgress={downloadSourceInProgress}
              onSeedChatPrompt={onSeedChatPrompt}
              onDiscussInChat={onDiscussInChat}
              onOpenWorkspaceFile={
                onOpenWorkspaceFile ? openFileWithBack : undefined
              }
              issuePageControls={issuePageControls}
              activityHeader={activityHeader}
              onOpenPullRequests={onOpenPullRequests}
              onOpenPreview={onOpenPreview}
              workspaceId={workspaceId}
              workspaceDirtyCount={workspaceDirtyCount}
              sourceControlChangeGraphAvailable={
                sourceControlChangeGraphAvailable
              }
              sourceControlChangeGraphDefaultComparison={
                sourceControlChangeGraphDefaultComparison
              }
              branchId={branchId}
              repoId={repoId}
              viewerOwnsWorkspace={viewerOwnsWorkspace}
              ownerVmRunning={ownerVmRunning}
              commentPermissions={commentPermissions}
              copyOf={copyOf}
              showComments={showComments}
              onToggleComments={onToggleComments}
              hasActiveTask={hasActiveTask}
              scrollToIntentRecord={scrollToIntentRecord}
              navigationOrigin={navigationOrigin}
              onOpenGuideDoc={openGuideDoc}
              onOpenTab={onOpenTab}
              docs={docs}
              onScrollToIntentRecord={onScrollToIntentRecord}
              onNavigateBack={() => {
                if (!navigationOrigin) return;
                onOpenTab(tabFromNavigationOrigin(navigationOrigin));
                onScrollToIntentRecord?.(navigationOrigin.intentRecordNumber);
                onSetNavigationOrigin?.(null);
              }}
              onNavigate={(target) => {
                const resolvedTraceFile =
                  target.type === "file" || target.type === "spec"
                    ? resolveTraceFileNavigation(target, workspaceId, branchId)
                    : null;
                if (
                  (target.type === "file" || target.type === "spec") &&
                  !resolvedTraceFile
                )
                  return;
                // Capture source context as navigation origin for back-button.
                // Preserve the exact current tab kind so "Back" returns to the
                // original spec/code/file tab rather than coercing everything
                // through ontology resolution.
                const currentTab =
                  tabs.find((t) => getTabId(t) === selectedTabId) ?? null;
                const origin = buildNavigationOrigin(currentTab, target);
                const remember = (opened: string) =>
                  onSetNavigationOrigin?.(
                    origin && { ...origin, target: opened },
                  );
                if (
                  (target.type === "file" || target.type === "spec") &&
                  resolvedTraceFile
                ) {
                  remember(resolvedTraceFile.filePath);
                  // Open source files directly via the branch file API
                  // rather than resolving through the ontology tree (which
                  // would incorrectly match the parent spec node via its
                  // src_files frontmatter content).
                  onOpenTab({
                    ...workspaceFileTab(
                      resolvedTraceFile.filePath,
                      undefined,
                      resolvedTraceFile.branchId,
                    ),
                  });
                  onScrollToIntentRecord?.(target.intentRecordNumber ?? null);
                } else if (target.type === "doc") {
                  const resolvedDoc = docs.find(
                    (doc) => doc.id === target.id || doc.filePath === target.id,
                  );
                  remember(resolvedDoc?.id ?? target.id);
                  onOpenTab({
                    type: "doc",
                    docId: resolvedDoc?.id ?? target.id,
                    name: resolvedDoc?.name ?? target.id,
                  });
                  onScrollToIntentRecord?.(target.intentRecordNumber ?? null);
                }
              }}
            />
          ) : !chatActive ? (
            <EmptyWorkspacePrompt onOpenGuideDoc={openGuideDoc} />
          ) : null}
        </div>
      </div>
    </div>
  );
});

// =============================================================================
// =============================================================================
// ContentDispatch — routes tab type to the correct editor component
// =============================================================================

// JSONL project docs get a transcript rich preview (center-panel D61) — same
// display-path predicate rule as isHtmlPath/D42.
function isJsonlDocumentPath(path: string | null | undefined): boolean {
  return (path ?? "").toLowerCase().endsWith(".jsonl");
}

function htmlPreviewTitle(displayPath: string, fallbackName?: string): string {
  const base = displayPath.split("/").pop() || fallbackName || "HTML document";
  return `${base} preview`;
}

interface ContentDispatchProps {
  tab: TabItem | null;
  workspaceId: string;
  /** Download the workspace source as a zip (Workspace Details). */
  onDownloadSource?: () => void;
  downloadSourceInProgress?: boolean;
  /** Authoritative Git change count for nested final-stage actions. */
  workspaceDirtyCount: number;
  sourceControlChangeGraphAvailable: boolean;
  sourceControlChangeGraphDefaultComparison: "workspace" | "uncommitted";
  branchId: string;
  repoId: string;
  /** The viewer owns this workspace (accepts review suggestions). */
  viewerOwnsWorkspace?: boolean;
  /** The owner's VM is running (accepting writes into it). */
  ownerVmRunning?: boolean;
  /**
   * What the viewer may do in anchored comment threads: editors offer only
   * these controls (`g5.documents.comments.controls`).
   */
  commentPermissions?: CommentPermissions;
  /**
   * Set when this workspace is a review copy: files show the source's
   * published proposals and the copy's own changed lines.
   */
  copyOf?: ReviewCopyOrigin | null;
  showComments: boolean;
  onToggleComments: () => void;
  onNavigate: (target: NavigateTarget) => void;
  hasActiveTask: boolean;
  scrollToIntentRecord?: { label: string; seq: number } | null;
  navigationOrigin?: NavigationOrigin | null;
  onNavigateBack?: () => void;
  onOpenGuideDoc: (guideId: GuideDocId) => void;
  /**
   * Open (or focus) a tab. Used by tab contents that themselves trigger
   * navigation to a sibling tab.
   */
  onOpenTab: (item: TabItem) => void;
  docs: DocNode[];
  onScrollToIntentRecord?: (label: string | null) => void;
  /** D60: seed the chat composer with a host-templated prompt (prefill-only). */
  onSeedChatPrompt?: (text: string, reference?: ChatProjectReference) => void;
  /** Seed a fresh chat beside the document (Discuss, a finding's chat). */
  onDiscussInChat?: (text: string, reference?: ChatProjectReference) => void;
  onOpenWorkspaceFile?: (
    filePath: string,
    repoRef?: string,
    line?: number,
    /** The statement the link was followed from, for Back. */
    from?: string,
  ) => void;
  /** Workbench status header above the Activity feed (g5.workbench.landing). */
  activityHeader?: ReactNode;
  issuePageControls?: IssuePageControls;
  /** Open the Pull Requests tab from a pull-request Activity row. */
  onOpenPullRequests?: () => void;
  /** Open the preview dialog from a preview Activity row (ui g5.ui.activity.preview-row). */
  onOpenPreview?: (request: PreviewOpenRequest) => void;
}

type GuideDocLoadState =
  | { status: "loading"; content: "" }
  | { status: "ready"; content: string }
  | { status: "error"; content: ""; message: string };

function decodeAnchorId(rawHash: string): string {
  const hash = rawHash.startsWith("#") ? rawHash.slice(1) : rawHash;
  try {
    return decodeURIComponent(hash);
  } catch {
    return hash;
  }
}

function scrollGuideAnchor(container: HTMLElement | null, rawHash: string) {
  if (!container || !rawHash.startsWith("#")) return;
  const targetId = decodeAnchorId(rawHash);
  if (!targetId) return;

  const target = Array.from(
    container.querySelectorAll<HTMLElement>("[id]"),
  ).find((element) => element.id === targetId);
  target?.scrollIntoView?.({ block: "start" });
}

function GuideDocView({
  guideId,
  onOpenGuideDoc,
}: {
  guideId: GuideDocId;
  onOpenGuideDoc: (guideId: GuideDocId) => void;
}): ReactElement {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const guide = getGuideDocById(guideId);
  const [loadState, setLoadState] = useState<GuideDocLoadState>({
    status: "loading",
    content: "",
  });

  useEffect(() => {
    if (!guide) return;

    let cancelled = false;
    setLoadState({ status: "loading", content: "" });

    fetch(guide.url)
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(`Guide request failed with ${response.status}`);
        }
        return response.text();
      })
      .then((markdown) => {
        if (cancelled) return;
        setLoadState({
          status: "ready",
          content: stripGuideMarkdownComments(markdown),
        });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setLoadState({
          status: "error",
          content: "",
          message: error instanceof Error ? error.message : "Guide unavailable",
        });
      });

    return () => {
      cancelled = true;
    };
  }, [guide]);

  const handleClick = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      const target = event.target as Element | null;
      const anchor = target?.closest?.("a[href]");
      if (!anchor) return;

      const rawHref = anchor.getAttribute("href") ?? "";
      if (!rawHref) return;
      if (rawHref.startsWith("#")) {
        event.preventDefault();
        scrollGuideAnchor(containerRef.current, rawHref);
        return;
      }

      const linkedGuide = getGuideDocByHref(rawHref);
      if (!linkedGuide) return;

      event.preventDefault();
      if (linkedGuide.id === guideId && rawHref.includes("#")) {
        scrollGuideAnchor(containerRef.current, `#${rawHref.split("#")[1]}`);
        return;
      }
      onOpenGuideDoc(linkedGuide.id);
    },
    [guideId, onOpenGuideDoc],
  );

  if (!guide) {
    return (
      <div className="flex items-center justify-center h-full text-[var(--g5-blocked)] text-sm">
        Guide not found
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      data-testid="guide-doc-view"
      title={guide.title}
      className="h-full w-full overflow-y-auto bg-[var(--g5-bg0)]"
      onClick={handleClick}
    >
      {loadState.status === "loading" ? (
        <div
          data-testid="guide-doc-loading"
          className="flex h-full items-center justify-center gap-2 text-sm text-[var(--g5-text2)]"
        >
          <div className="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent" />
          Loading guide…
        </div>
      ) : null}
      {loadState.status === "error" ? (
        <div
          data-testid="guide-doc-error"
          className="flex h-full items-center justify-center px-6 text-center text-sm text-[var(--g5-blocked)]"
        >
          {loadState.message || "Guide unavailable"}
        </div>
      ) : null}
      {loadState.status === "ready" ? (
        <div data-testid="guide-doc-content" className="w-full px-8 py-6">
          <MarkdownRenderer
            content={loadState.content}
            basePath={USER_GUIDE_BASE_PATH}
            headingIdMode="slug"
            className={classNames(
              "text-base leading-relaxed",
              "[&_a]:font-medium [&_a]:text-[var(--g5-active)]",
              "[&_a]:underline [&_a]:underline-offset-4",
              "[&_a:hover]:text-[var(--g5-text0)]",
              "[&_a:focus-visible]:outline [&_a:focus-visible]:outline-2",
              "[&_a:focus-visible]:outline-offset-2",
              "[&_a:focus-visible]:outline-[var(--g5-active)]",
            )}
          />
        </div>
      ) : null}
    </div>
  );
}

/**
 * The open tab's content. When refreshing it fails transiently the content
 * stays as last loaded, with what is typed in it, and a status says so
 * beside it rather than in its place (`g5.workspace.page.refresh`).
 */
function ContentDispatch(props: ContentDispatchProps): ReactElement {
  const [refreshError, setRefreshError] = useState<Error | null>(null);
  // Threads and suggestion cards whose composers hold unsent words, a send,
  // a decision, a reason or an edit (the page's owner): they stay shown if
  // they are deleted, decided or committed past elsewhere meanwhile
  // (`g5.documents.comments.failure`).
  const composingThreadIds = useActiveComposerSubjects("thread");
  const composingSuggestionIds = useActiveComposerSubjects("suggestion");
  // The status is a strip above the content, covering nothing; the content
  // keeps its place in the tree whether the strip shows or not.
  return (
    <div className="flex h-full min-h-0 flex-col">
      {refreshError && (
        <p
          role="status"
          data-testid="content-refresh-error"
          className="shrink-0 border-b border-[var(--g5-warn)]/40 bg-[var(--g5-bg1)] px-4 py-1 text-xs text-[var(--g5-warn)]"
        >
          Could not refresh this file: showing it as last loaded.
        </p>
      )}
      <div className="min-h-0 flex-1">
        <OpenTabContent
          {...props}
          onRefreshError={setRefreshError}
          composingSuggestionIds={composingSuggestionIds}
          composingThreadIds={composingThreadIds}
        />
      </div>
    </div>
  );
}

interface OpenTabContentProps extends ContentDispatchProps {
  /** Reports a transient refresh failure of the open file, or its end. */
  onRefreshError: (error: Error | null) => void;
  /** Suggestion cards whose composers hold unsent words, a send or a decision. */
  composingSuggestionIds: ReadonlySet<string>;
  /** Threads whose reply box holds unsent words or a send. */
  composingThreadIds: ReadonlySet<string>;
}

function OpenTabContent({
  tab: requestedTab,
  workspaceId,
  workspaceDirtyCount,
  sourceControlChangeGraphAvailable,
  sourceControlChangeGraphDefaultComparison,
  repoId: resolvedRepoId,
  viewerOwnsWorkspace = false,
  ownerVmRunning = false,
  commentPermissions = READ_ONLY_COMMENT_PERMISSIONS,
  copyOf = null,
  showComments,
  onToggleComments,
  onNavigate,
  scrollToIntentRecord,
  navigationOrigin,
  onNavigateBack,
  onOpenGuideDoc,
  onOpenTab,
  onDownloadSource,
  downloadSourceInProgress,
  docs,
  onScrollToIntentRecord,
  onSeedChatPrompt,
  onDiscussInChat,
  onOpenWorkspaceFile,
  activityHeader,
  issuePageControls,
  onOpenPullRequests,
  onOpenPreview,
  onRefreshError,
  composingSuggestionIds,
  composingThreadIds,
}: OpenTabContentProps): ReactElement {
  const { user: currentUser, principalKey } = useAuth();
  const tab = requestedTab
    ? currentFileTab(requestedTab, docs, workspaceId)
    : null;
  const repoId =
    tab?.type === "file" ? (tab.branchId ?? resolvedRepoId) : resolvedRepoId;
  const filePath = tab?.type === "file" ? tab.filePath : "";
  const workspaceFile = useWorkspaceFileContent(
    workspaceId,
    repoId,
    filePath || "",
  );
  const fileContent = workspaceFile.state;
  const saveWorkspaceFile = useSaveWorkspaceFile();
  // Every current opener shares the same repository/path discussion identity.
  const commentFile =
    tab?.type === "file" ? { repoId, filePath } : { repoId: "", filePath: "" };
  const fileComments = useFileComments(
    workspaceId,
    commentFile.repoId,
    commentFile.filePath,
  );
  const fileCommentsData =
    fileComments.status === "ok" ? fileComments.data : EMPTY_COMMENTS;
  // A failed refresh keeps the threads and cards already shown, and says so
  // beside them (`g5.documents.comments.failure`).
  const threadsNotice = threadListProblem(fileComments);
  const commentsData = fileCommentsData;
  const isShownThread = useCallback(
    (comment: CommentDTO) =>
      !!commentFile.filePath &&
      comment.repoId === commentFile.repoId &&
      comment.filePath === commentFile.filePath,
    [commentFile.repoId, commentFile.filePath],
  );
  const shownComments = useCommentsKeptWhileAnswered(
    commentsData,
    composingThreadIds,
    isShownThread,
  );
  // The file a new-thread draft belongs to in the page's owner, so it
  // outlives Source/Preview and tab switches.
  const commentDraftKey = commentFile.filePath
    ? `${commentFile.repoId}:${commentFile.filePath}`
    : undefined;

  // Review suggestions on the open file, shown in place by the editors
  // (flat: every reviewer's open proposal at its line), in every view of
  // the file: document or raw file
  // (`g5.governance.review-suggestions.display`).
  const overlayPath = commentFile.filePath;
  const overlayRepoId = commentFile.repoId;
  // On a review copy the proposals shown are the source's: every
  // reviewer's published suggestions on this file, re-anchored on the
  // copy's text, read-only (a copy never accepts).
  const proposalsWorkspaceId = copyOf?.sourceWorkspaceId ?? workspaceId;
  const fileSuggestions = useFileSuggestions(
    proposalsWorkspaceId,
    overlayRepoId,
    overlayPath,
    !!overlayPath,
    composingSuggestionIds,
  );
  const viewerUserId = currentUser?.id ?? "";
  // The cards' controls follow their source's writability: this workspace's
  // on the source itself, the source's own on a review copy.
  const copySource = useWorkspace(copyOf?.sourceWorkspaceId ?? "");
  // The proposals' source, named on a card kept from an earlier commit.
  const ownWorkspace = useWorkspace(copyOf ? "" : workspaceId);
  const proposalsSourceName = copyOf
    ? copyOf.sourceWorkspaceName
    : ownWorkspace.status === "ok"
      ? ownWorkspace.data.name
      : undefined;
  // A copy's cards are the source's: only its known, writable state offers
  // their controls (`g5.documents.comments.controls`).
  const copySourceWrites = sourceThreadWrites(copySource);
  const sourceWritable = copyOf
    ? copySourceWrites.writable
    : commentPermissions.canReply;
  const sourceClosedNote = copyOf ? copySourceWrites.closedNote : "";
  const suggestionOverlay = useMemo<SuggestionOverlayProps | null>(
    () =>
      overlayPath
        ? {
            workspaceId: proposalsWorkspaceId,
            suggestions: fileSuggestions.suggestions,
            viewerOwnsWorkspace: copyOf ? false : viewerOwnsWorkspace,
            viewerUserId,
            ownerVmRunning: copyOf ? false : ownerVmRunning,
            sourceWritable,
            sourceClosedNote,
            onCopyOf: copyOf?.sourceWorkspaceName,
            earlierCommitIds: fileSuggestions.earlierCommitIds,
            sourceName: proposalsSourceName,
          }
        : null,
    [
      overlayPath,
      proposalsWorkspaceId,
      copyOf,
      fileSuggestions.suggestions,
      fileSuggestions.earlierCommitIds,
      proposalsSourceName,
      viewerOwnsWorkspace,
      viewerUserId,
      ownerVmRunning,
      sourceWritable,
      sourceClosedNote,
    ],
  );
  // The copy's own changed lines: the same preview the reviewer's panel
  // shows (one query, one save), so the editor marks what the next push
  // would send.
  // The preview saves the copy (commit, push), which only its owner may
  // do: a member reading someone else's copy gets no preview and no
  // changed-line marks.
  const copyPreview = useReviewPreview(
    copyOf?.reviewWorkspaceId ?? "",
    copyOf !== null && viewerOwnsWorkspace,
  );
  const changedLinesFor = useCallback(
    (fileRepoId: string, path: string) =>
      copyOf && copyPreview.status === "ok" && path
        ? changedLineRanges(copyPreview.data, fileRepoId, path)
        : undefined,
    [copyOf, copyPreview],
  );

  // Mutation hooks for comments
  const { mutateAsync: addCommentAsync } = useAddComment();
  const { mutateAsync: resolveCommentAsync } = useResolveComment();
  const { mutateAsync: unresolveCommentAsync } = useResolveComment(false);
  const { mutateAsync: replyToCommentAsync } = useReplyToComment();
  const { mutateAsync: deleteCommentAsync } = useDeleteComment();

  const activeFileIsHtml = tab?.type === "file" && isHtmlPath(filePath ?? "");

  // Comment actions, each handed to the editors only when the viewer may
  // perform it (`g5.documents.comments.controls`). Each returns its promise,
  // so a refusal is shown where it happened and keeps what was typed.
  // Editors anchor a new thread to the text they display and hand it over
  // complete; the page adds the file it belongs to. Every thread action is
  // sent only for the principal this page was rendered for, even when a send
  // completes, or a click arrives, after another principal's token has
  // (`g5.security.login.principal`).
  const commentContent =
    tab?.type === "file" && fileContent.status === "ok"
      ? fileContent.data
      : null;
  const pushedCommentContent =
    commentContent &&
    commentContent.source === "committed" &&
    commentContent.repoOid &&
    typeof commentContent.content === "string"
      ? commentContent
      : null;
  const canStartThreadHere =
    commentPermissions.canStartThread &&
    !!commentFile.repoId &&
    !!commentFile.filePath &&
    (viewerOwnsWorkspace || !!pushedCommentContent);
  const handleAddComment = useCallback(
    async (draft: EditorCommentDraft) => {
      // A checkout HEAD does not identify live or unsaved bytes. Only link a
      // timeline thread to an immutable revision when its exact text came
      // from the pushed read. Current-work threads retain their hash anchor.
      const commitSha =
        pushedCommentContent &&
        draft.baseOid === pushedCommentContent.repoOid &&
        draft.anchor.contentHash ===
          (await sha256Utf8(pushedCommentContent.content ?? ""))
          ? pushedCommentContent.repoOid
          : undefined;
      if (!viewerOwnsWorkspace && !commitSha) {
        throw new Error("Open the pushed file to start a comment.");
      }
      return addCommentAsync({
        id: draft.id,
        workspaceId,
        scope: "line",
        content: draft.content,
        repoId: commentFile.repoId,
        filePath: commentFile.filePath,
        lineNumber: draft.anchor.lineNumber,
        lineHash: draft.anchor.lineHash,
        contentHash: draft.anchor.contentHash,
        anchorContext: draft.anchor.anchorContext,
        commitSha: commitSha ?? undefined,
        principal: principalKey,
      });
    },
    [
      addCommentAsync,
      pushedCommentContent,
      viewerOwnsWorkspace,
      workspaceId,
      commentFile.repoId,
      commentFile.filePath,
      principalKey,
    ],
  );
  const handleReplyToComment = useCallback(
    (commentId: string, content: string) =>
      replyToCommentAsync({ commentId, content, principal: principalKey }),
    [replyToCommentAsync, principalKey],
  );
  const handleResolveComment = useCallback(
    (commentId: string) =>
      resolveCommentAsync({ commentId, principal: principalKey }),
    [resolveCommentAsync, principalKey],
  );
  const handleUnresolveComment = useCallback(
    (commentId: string) =>
      unresolveCommentAsync({ commentId, principal: principalKey }),
    [unresolveCommentAsync, principalKey],
  );
  const handleDeleteComment = useCallback(
    (commentId: string) =>
      deleteCommentAsync({ commentId, principal: principalKey }),
    [deleteCommentAsync, principalKey],
  );
  const commentActions = {
    onAddComment: canStartThreadHere ? handleAddComment : undefined,
    onReplyToComment: commentPermissions.canReply
      ? handleReplyToComment
      : undefined,
    onResolveComment: commentPermissions.canModerate
      ? handleResolveComment
      : undefined,
    onUnresolveComment: commentPermissions.canModerate
      ? handleUnresolveComment
      : undefined,
    onDeleteComment: commentPermissions.canModerate
      ? handleDeleteComment
      : undefined,
    commentsNotice: threadsNotice ?? fileSuggestions.problem,
  };

  // A transient refresh failure keeps the file as last loaded, and anything
  // typed in it; the wrapper says so beside it.
  const contentRefreshError =
    tab?.type === "file" && fileContent.status === "ok"
      ? fileContent.refreshError
      : null;
  useEffect(() => {
    onRefreshError(contentRefreshError);
  }, [contentRefreshError, onRefreshError]);

  if (!tab) {
    return <EmptyWorkspacePrompt onOpenGuideDoc={onOpenGuideDoc} />;
  }

  if (tab.type === "guide_doc") {
    return (
      <GuideDocView guideId={tab.guideId} onOpenGuideDoc={onOpenGuideDoc} />
    );
  }

  switch (tab.type) {
    case "workspace_terminal":
      return <></>; // Rendered persistently by CenterPanel.
    case "intent_log": {
      return (
        <IntentLogView
          workspaceId={workspaceId}
          repoId={repoId}
          onNavigate={onNavigate}
        />
      );
    }

    case "doc":
      return <div role="alert">Document has no resolved repository file.</div>;

    case "ontology_changes":
      return (
        <OntologyChangesView
          workspaceId={workspaceId}
          focusFile={tab.focusFile}
          onNavigate={onNavigate}
          onOpenFile={(file) =>
            onOpenTab(workspaceOntologyFileTab(file.path, file.repoId))
          }
        />
      );
    case "workspace_changes":
      return <WorkspaceChangesView workspaceId={workspaceId} />;

    case "source_control_diff":
      return <WorkspaceReviewContent workspaceId={workspaceId} />;
    case "source_control_change_graph":
      if (!sourceControlChangeGraphAvailable) {
        return (
          <div className="flex h-full items-center justify-center text-sm text-[var(--g5-text2)]">
            Diff Graph is not available for Base workspaces.
          </div>
        );
      }
      return (
        <SourceControlChangeGraphView
          workspaceId={workspaceId}
          defaultComparison={sourceControlChangeGraphDefaultComparison}
          onOpenDocument={(route) => {
            const doc = findChangeGraphDocument(docs, route);
            if (!doc) return;
            onOpenTab({ type: "doc", docId: doc.id, name: doc.name });
          }}
          onOpenDiff={(route) => {
            if (!route.filePath) return;
            onOpenTab({
              type: "source_control_diff",
              name: SOURCE_CONTROL_DIFF_TAB.name,
              selectedFileKey: diffFileKey({
                repoId: route.repoId,
                file: route.filePath,
              }),
            });
          }}
          onOpenOntology={({ route, statementId, deleted }) => {
            if (!route.filePath) return;
            // A deleted file has no editor: All changes shows it at its place.
            if (deleted && !statementId) {
              onOpenTab({
                type: "ontology_changes",
                name: "Ontology changes",
                focusFile: {
                  repoId: route.repoId,
                  path: route.filePath,
                  seq: Date.now(),
                },
              });
              return;
            }
            // The ontology editor opens with Compare on; a deleted statement
            // resolves to its read-only Before row.
            onNavigate({
              type: "spec",
              id: route.filePath,
              repoId: route.repoId,
              intentRecordNumber: statementId ? `#${statementId}` : undefined,
            });
          }}
        />
      );

    case "source_control_commit_diff":
      return (
        <WorkspaceCommitDiffView
          repoId={tab.repoId}
          repoName={tab.repoName}
          workspaceId={workspaceId}
          sha={tab.sha}
        />
      );

    case "pull_requests":
      return (
        <PullRequestPanel
          workspaceId={workspaceId}
          dirtyCount={workspaceDirtyCount}
        />
      );

    case "file": {
      if (
        fileContent.status === "ok" &&
        fileContent.data.source === "live" &&
        (!viewerOwnsWorkspace || !ownerVmRunning)
      ) {
        return <p role="status">Checking current file access…</p>;
      }
      if (fileContent.status === "loading") {
        return (
          <div className="flex items-center justify-center h-full text-[var(--g5-text2)] text-sm gap-2">
            <div className="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" />
            Loading file…
          </div>
        );
      }
      if (fileContent.status === "error") {
        return (
          <div className="flex items-center justify-center h-full text-[var(--g5-blocked)] text-sm gap-2">
            <AlertCircle className="w-4 h-4" />
            {fileContent.error instanceof ApiError &&
            fileContent.error.status === 404
              ? "This file is unavailable in the current authorized source. It may have been deleted."
              : fileContent.error instanceof ApiError &&
                  fileContent.error.is("FILE_TOO_LARGE")
                ? // The reader refuses the file before its bytes load
                  // (`g5.documents.read.bounds`); no viewer or download
                  // can be offered for it here (`g5.documents.viewers`).
                  `This file is larger than ${formatByteSize(
                    Number(fileContent.error.details.maxBytes) ||
                      MAX_EDITABLE_TEXT_BYTES,
                  )}, the most the workspace file reader loads, so it cannot be shown or downloaded here.`
                : "Failed to load file"}
          </div>
        );
      }
      // Infer language from extension
      const ext = tab.filePath.split(".").pop() ?? "";
      const langMap: Record<string, string> = {
        rs: "rust",
        ts: "typescript",
        tsx: "typescript",
        js: "javascript",
        jsx: "javascript",
        py: "python",
        toml: "toml",
        yaml: "yaml",
        yml: "yaml",
        json: "json",
        md: "markdown",
        mdx: "markdown",
        tf: "hcl",
        hcl: "hcl",
        sh: "shell",
        bash: "shell",
        zsh: "shell",
        sql: "sql",
        css: "css",
        scss: "css",
        html: "html",
        xml: "xml",
        dockerfile: "dockerfile",
        makefile: "makefile",
        graphql: "graphql",
        gql: "graphql",
        proto: "protobuf",
        go: "go",
        java: "java",
        kt: "kotlin",
        swift: "swift",
        rb: "ruby",
        c: "c",
        cpp: "cpp",
        h: "c",
        hpp: "cpp",
      };
      const language = activeFileIsHtml ? "html" : (langMap[ext] ?? ext);
      // FILES opens files read-first, like GitHub; owners edit and Save,
      // readers suggest (`g5.documents.files-view`).
      const fileEditor = (
        <FileView
          key={`${principalKey}:${workspaceId}:${repoId}:${tab.filePath}`}
          previewProps={{
            onNavigate,
            currentUserId: currentUser?.id,
            currentUserEmail: currentUser?.email,
            currentUserAvatarUrl: currentUser?.avatarUrl,
            onDiscussQuestion: onDiscussInChat
              ? (question, title) =>
                  onDiscussInChat(
                    buildQuestionDiscussPrompt(question, tab.filePath, title),
                    {
                      id: `source_file:${repoId}:${tab.filePath}`,
                      kind: "source_file",
                      label: tab.name,
                      path: tab.filePath,
                      branchId: repoId,
                    },
                  )
              : undefined,
            onDiscussFinding: onDiscussInChat
              ? (text) =>
                  onDiscussInChat(text, {
                    id: `source_file:${repoId}:${tab.filePath}`,
                    kind: "source_file",
                    label: tab.name,
                    path: tab.filePath,
                    branchId: repoId,
                  })
              : undefined,
          }}
          renderPreview={
            activeFileIsHtml
              ? (html, actions) => (
                  <HtmlPreviewShell
                    displayPath={tab.filePath}
                    title={htmlPreviewTitle(tab.filePath, tab.name)}
                    html={html}
                    onOpenWorkspaceFile={onOpenWorkspaceFile}
                    questionWorkspaceId={
                      isManagedExplainerPath(tab.filePath)
                        ? workspaceId
                        : undefined
                    }
                    answerBridge={{
                      workspaceId,
                      filePath: tab.filePath,
                      displayed: html,
                      canSave: actions.canSave,
                      onSave: actions.save,
                      onSeedChatPrompt,
                    }}
                  />
                )
              : isJsonlDocumentPath(tab.filePath)
                ? (content) => (
                    <JsonlTranscriptView
                      displayPath={tab.filePath}
                      content={content}
                    />
                  )
                : undefined
          }
          workspaceId={workspaceId}
          repoId={repoId}
          filePath={tab.filePath}
          language={language}
          file={fileContent.data}
          principal={principalKey}
          lineTarget={tab.lineRange?.[0] ?? null}
          viewerOwnsWorkspace={viewerOwnsWorkspace}
          ownerVmRunning={ownerVmRunning}
          workspaceWritable={commentPermissions.canReply}
          onSave={async (nextContent, expectedHash, expectedVmId, options) =>
            saveWorkspaceFile.mutateAsync({
              workspaceId,
              repoId,
              path: tab.filePath,
              content: nextContent,
              expectedHash,
              expectedVmId,
              commitAndPush: options?.commitAndPush,
            })
          }
          onReload={workspaceFile.reload}
          onFetchCurrent={() =>
            fetchWorkspaceFile(workspaceId, repoId, tab.filePath)
          }
          onOpenWorkspaceFile={onOpenWorkspaceFile}
          onStartWorkspace={requestWorkspaceStart}
          editorProps={{
            showComments,
            onToggleComments,
            comments: shownComments,
            commentDraftKey,
            ...commentActions,
            scrollToIntentRecord: scrollToIntentRecord?.label ?? null,
            scrollToIntentRecordSeq: scrollToIntentRecord?.seq,
            // Back shows only on the file the link opened.
            navigationOrigin:
              navigationOrigin?.target === tab.filePath
                ? navigationOrigin
                : null,
            onNavigateBack,
            suggestionOverlay,
            changedLines: changedLinesFor(repoId, tab.filePath),
          }}
        />
      );
      return fileEditor;
    }

    case "workspace_settings":
      // Workspace settings rendered inside the workspace chrome as a center
      // tab. Preserve the `workspace-settings-page` testid that e2e asserts on,
      // and let the content scroll within the center panel.
      // The content is the settings sheet, which owns its own scroll
      // (ui g5.ui.settings-page).
      return (
        <div data-testid="workspace-settings-page" className="h-full min-h-0">
          <WorkspaceSettingsContent workspaceId={workspaceId} />
        </div>
      );

    case "worker_hosts":
      // Org worker-host admin embedded as a center tab. The content reads the
      // `:org` slug from the route and renders its own gating notices.
      return (
        <div className="h-full overflow-y-auto">
          <WorkerHostsPageContent />
        </div>
      );

    case "org_members":
      // Org members admin embedded as a center tab. The content reads the
      // `:org` slug from the route and renders its own gating.
      return (
        <div className="h-full overflow-y-auto">
          <OrgMembersPageContent />
        </div>
      );

    case "org_approvals":
      // Org approval-definition library embedded as a center tab. The content
      // reads the `:org` slug from the route and renders its own gating.
      return (
        <div className="h-full overflow-y-auto">
          <OrgApprovalsPageContent />
        </div>
      );

    case "org_github_app":
      // Org GitHub App binding embedded as a center tab. The content reads
      // the `:org` slug from the route and renders its own admin gating.
      return (
        <div className="h-full overflow-y-auto">
          <GithubAppBindingStatusPageContent />
        </div>
      );

    case "org_admin":
      // The organization's settings sheet embedded as a center tab; it owns
      // its own scroll (ui g5.ui.settings-page). The content reads the `:org`
      // slug from the route and links out to each configuration surface.
      return (
        <div className="h-full min-h-0">
          <OrgAdminPageContent />
        </div>
      );

    case "workspace_approvals":
      // Workspace approvals: merge-into requirements + outgoing decisions
      // (B6 PR-2).
      return (
        <div className="h-full overflow-y-auto">
          <WorkspaceApprovalsPageContent
            workspaceId={workspaceId}
            dirtyCount={workspaceDirtyCount}
            onOpenWorkspaceFile={onOpenWorkspaceFile}
          />
        </div>
      );

    case "workspace_review":
      // A review copy's own page: the reviewer's changes, notes and holds,
      // their comments and last round, then the source's reviews.
      return (
        <div className="h-full overflow-y-auto">
          <ReviewCopyPage
            workspaceId={workspaceId}
            viewerUserId={viewerUserId}
            canPush={viewerOwnsWorkspace}
            onOpenFile={onOpenWorkspaceFile}
          />
        </div>
      );

    case "workspace_details":
      // Read-only Workspace Details overview embedded as a center tab. The
      // panel derives the owning org from the loaded workspace detail and
      // hosts the external Issues section (`?view=issues` scrolls to it).
      return (
        <div className="h-full overflow-y-auto">
          <WorkspaceDetailsPanel
            workspaceId={workspaceId}
            onOpenIssue={(link) =>
              onOpenTab(externalIssueTab(link.locator, link.displayKey))
            }
            onDownloadSource={onDownloadSource}
            downloadSourceInProgress={downloadSourceInProgress}
          />
        </div>
      );

    case "workspace_activity":
      // The primary issue's discussion with workspace context
      // (`?view=activity`); the panel scrolls itself. Under the alignment
      // layout it is the landing timeline with its status header
      // (workbench g5.workbench.landing).
      return (
        <WorkspaceActivityPanel
          workspaceId={workspaceId}
          header={activityHeader}
          onOpenPullRequests={onOpenPullRequests}
          onOpenPreview={onOpenPreview}
          onOpenCommit={(commit) =>
            onOpenTab(
              commitDiffTab(
                commit,
                commit.sha,
                commit.summary || commit.shortSha,
              ),
            )
          }
        />
      );

    case "external_issue":
      // Read-only external GitHub issue (live read of a configured domain).
      return (
        <div className="h-full overflow-y-auto">
          <ExternalIssueDetailPanel
            {...issuePageControls?.(tab)}
            repositoryId={tab.repositoryId}
            number={tab.number}
            name={tab.name}
          />
        </div>
      );

    default:
      return (
        <div className="flex items-center justify-center h-full text-[var(--g5-text2)] text-sm">
          Unknown tab type
        </div>
      );
  }
}

// =============================================================================
// WorkspacePage — main exported shell component
// =============================================================================

export function countSyncReadyFiles(
  files: PendingSyncFile[],
  workspaceId: string,
): number {
  return files.filter((file) => isIsolatedPendingSyncDocFile(file, workspaceId))
    .length;
}

function ContentBranchSelectionState({
  branches,
  onSelect,
}: {
  branches: BranchSummary[];
  onSelect: (branchId: string) => void;
}): ReactElement {
  return (
    <div
      data-testid="content-branch-unresolved"
      className="flex h-full min-h-0 items-center justify-center overflow-auto bg-[var(--g5-bg0)] p-6"
    >
      <div className="w-full max-w-lg rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-5">
        <h2 className="text-sm font-semibold text-[var(--g5-text0)]">
          Select a branch to view content
        </h2>
        <p className="mt-1 text-xs text-[var(--g5-text2)]">
          This workspace has more than one visible branch, so content cannot be
          selected from repository order.
        </p>
        <div className="mt-4 flex flex-col gap-2">
          {branches.map((branch) => (
            <button
              key={branch.repoId}
              type="button"
              data-testid={`content-branch-option-${branch.repoId}`}
              className="flex items-center justify-between rounded-md border border-[var(--g5-stroke)] px-3 py-2 text-left text-xs text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]"
              onClick={() => onSelect(branch.repoId)}
            >
              <span className="font-medium">{branch.repoName}</span>
              <span className="font-mono text-[var(--g5-text2)]">
                {branch.branchName}
              </span>
            </button>
          ))}
          {branches.length === 0 && (
            <p className="text-xs text-[var(--g5-text2)]">
              No visible branches are available.
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

export interface WorkspacePageProps {
  /**
   * How the route names the workspace: by name (`/:org/:workspace`), or a
   * personal workspace by kind — `/:org/me` and `/:org/users/:handle`
   * (personal_workspace g5.personal.routes).
   */
  routeFamily?: WorkspaceRouteFamily;
}

export function WorkspacePage({
  routeFamily = "workspace",
}: WorkspacePageProps = {}): ReactElement {
  // ---------------------------------------------------------------------------
  // Route parameters
  // ---------------------------------------------------------------------------

  const {
    org,
    workspace: workspaceNameParam,
    handle: personalHandleParam,
  } = useParams<{
    org: string;
    workspace: string;
    handle: string;
  }>();
  const { taskId: routeTaskId } = useParams<{
    taskId?: string;
  }>();
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams] = useSearchParams();
  const pageRoutes = WORKSPACE_ROUTE_FAMILIES[routeFamily];
  const docMatch = useMatch(pageRoutes.doc);
  const fileMatch = useMatch(pageRoutes.file);
  const intentLogMatch = useMatch(pageRoutes.intentLog);
  const guideMatch = useMatch(pageRoutes.guide);
  const issueMatch = useMatch(pageRoutes.issue);
  const commitMatch = useMatch(pageRoutes.commit);
  const docPath = docMatch?.params["*"] ?? null;
  const filePath = fileMatch?.params["*"] ?? null;
  const isIntentLogPath = !!intentLogMatch;
  // Identity routes of the center surfaces that are not a spec, doc or file
  // (g5.workbench.tabs). `useMatch` already decodes the segments.
  const guideRouteId = guideMatch?.params.guideId ?? null;
  const issueRouteRepositoryId = issueMatch?.params.repositoryId ?? null;
  const issueRouteNumber = issueMatch?.params.number ?? null;
  const commitRouteRepoId = commitMatch?.params.repoId ?? null;
  const commitRouteSha = commitMatch?.params.sha ?? null;
  const identityRoutePath =
    !!guideRouteId || !!issueRouteRepositoryId || !!commitRouteRepoId;

  // Resolve the route to UUIDs: a name via the resolve endpoint, a personal
  // route by registry kind (g5.personal.routes). A personal route's org id
  // comes from the caller's memberships.
  const personalRoute = routeFamily !== "workspace";
  const { user: routeUser } = useAuth();
  const personalRouteOrgId =
    (personalRoute &&
      routeUser?.orgs.find((membership) => membership.orgSlug === org)
        ?.orgId) ||
    "";
  const routeTarget = useMemo<WorkspaceRouteTarget>(
    () =>
      routeFamily === "me"
        ? { family: "me", orgId: personalRouteOrgId }
        : routeFamily === "user"
          ? {
              family: "user",
              orgId: personalRouteOrgId,
              handle: personalHandleParam,
            }
          : {
              family: "workspace",
              orgSlug: org,
              workspaceSlug: workspaceNameParam,
            },
    [
      org,
      personalHandleParam,
      personalRouteOrgId,
      routeFamily,
      workspaceNameParam,
    ],
  );
  const routeResolution = useResolveWorkspaceRoute(routeTarget);
  // A personal route in an organization the caller is not a member of has
  // nothing to resolve: show it as not found rather than loading forever.
  const resolvedId: typeof routeResolution =
    personalRoute && !!routeUser && !personalRouteOrgId
      ? {
          status: "error",
          error: new Error(`Organization "${org ?? ""}" not found`),
        }
      : routeResolution;
  // The workspace's name for labels: the route segment, or the resolved
  // personal name (`users/<handle>`).
  const workspaceSlug =
    routeFamily === "workspace"
      ? workspaceNameParam
      : routeFamily === "user" && personalHandleParam
        ? `users/${personalHandleParam}`
        : resolvedId.status === "ok"
          ? resolvedId.data.name
          : undefined;
  // The root path this page's routes hang under; every in-page navigation
  // stays on the route family it was opened by.
  const workspaceRoutePath: string | null = !org
    ? null
    : routeFamily === "me"
      ? personalWorkspaceMePath(org)
      : routeFamily === "user"
        ? personalHandleParam
          ? personalWorkspaceUserPath(org, personalHandleParam)
          : null
        : workspaceNameParam
          ? workspacePagePath(org, workspaceNameParam)
          : null;
  const workspaceId =
    resolvedId.status === "ok" ? resolvedId.data.workspaceId : "";
  const resolvedOrgId = resolvedId.status === "ok" ? resolvedId.data.orgId : "";
  const resolveAgentLaunch = useResolveAgentLaunch(workspaceId);
  const resolveAgentLaunchPreferences = resolveAgentLaunch.mutateAsync;
  const confirmAgentLaunch = useCallback(
    async (config: SyncAgentConfig) => {
      const preferences = agentLaunchPreferencesForConfig(config);
      const resolved = await resolveAgentLaunchPreferences(preferences);
      return {
        preferences,
        confirmedResolutionId: resolved.resolutionId,
      };
    },
    [resolveAgentLaunchPreferences],
  );
  const launchSubmissionLocksRef = useRef(new Set<string>());
  const [launchSubmissionKeys, setLaunchSubmissionKeys] = useState<Set<string>>(
    () => new Set(),
  );
  const acquireLaunchSubmission = useCallback((key: string): boolean => {
    if (launchSubmissionLocksRef.current.has(key)) return false;
    launchSubmissionLocksRef.current.add(key);
    setLaunchSubmissionKeys(new Set(launchSubmissionLocksRef.current));
    return true;
  }, []);
  const releaseLaunchSubmission = useCallback((key: string): void => {
    launchSubmissionLocksRef.current.delete(key);
    setLaunchSubmissionKeys(new Set(launchSubmissionLocksRef.current));
  }, []);
  const instanceInfo = useInstanceInfo();
  // Provider-accounts kill-switch (right-panel D29) and the chat Account
  // field's data (session_provider_controls D30): the visible accounts plus
  // the effective per-harness assignment, threaded into ChatPanel. Both
  // reads stay dormant while the provider-accounts switch is off.
  const providerAccountsEnabled = isProviderAccountsEnabled(
    instanceInfo.status === "ok" ? instanceInfo.data : null,
  );
  const providerAccountsOrgId = providerAccountsEnabled ? resolvedOrgId : "";
  const chatProviderAccounts = useProviderAccounts(
    providerAccountsOrgId,
    workspaceId,
  );
  const chatAccountAssignmentConfigs = useAssignmentConfigs(
    providerAccountsOrgId,
    workspaceId,
  );
  const providerAccounts =
    chatProviderAccounts.status === "ok"
      ? chatProviderAccounts.data
      : undefined;
  const providerAccountAssignments = useMemo<Record<PickerHarness, string>>(
    () => ({
      claude_code: effectiveAssignment(
        chatAccountAssignmentConfigs.triples.claude_code,
      ).value,
      codex: effectiveAssignment(chatAccountAssignmentConfigs.triples.codex)
        .value,
    }),
    [chatAccountAssignmentConfigs.triples],
  );
  const chatRuntimeVariablesAvailable =
    chatAccountAssignmentConfigs.runtimeVariablesAvailable;
  // Agents that can authenticate as configured (chat
  // g5.chat.account-defaults): Auto prefers one of them.
  const chatAuthenticatableAgents = useMemo(
    () =>
      authenticatableAgentProviders(
        providerAccountAssignments,
        chatRuntimeVariablesAvailable,
      ),
    [providerAccountAssignments, chatRuntimeVariablesAvailable],
  );
  // Runtime UI v3 gate (layout_and_navigation D5): fail-closed while loading.
  // Since D7/D55 it gates only the home/fork-governance surfaces — the status
  // band rides the top-panel gate + layout resolution below.
  const { settings: userSettings, update: updateUserSettings } =
    useUserSettings();
  // Open workspaces are tabs in the strip (g5.workbench.tabs): the strip owns
  // Home, and removing a workspace closes its tab.
  const workspaceTabs = useWorkspaceTabs();
  // Shown again from the strip this session: the surface the workspace
  // restores (its URL and the snapshot left with the strip) stands, not the
  // landing.
  const returnedToTab =
    workspaceTabs.enabled &&
    workspaceTabs.active.kind === "workspace" &&
    workspaceTabs.active.returned;
  const activeStripTab =
    workspaceTabs.enabled && workspaceTabs.active.kind === "workspace"
      ? workspaceTabs.active.tab
      : null;
  const activeTabSnapshot = returnedToTab
    ? (
        workspaceTabs.active as Extract<
          typeof workspaceTabs.active,
          { kind: "workspace" }
        >
      ).snapshot
    : null;
  const rememberTabSnapshot = workspaceTabs.rememberSnapshot;
  // A former name in the URL resolved to the workspace: replace it with the
  // current name, keeping the sub-route, query and fragment, and move the
  // open tab (g5.workspace.former-names, g5.workbench.tabs). The page stays
  // mounted, so its state is untouched. A personal route resolves by kind
  // and keeps its own path (g5.personal.routes).
  const canonicalWorkspaceName =
    routeFamily === "workspace" && resolvedId.status === "ok"
      ? resolvedId.data.workspaceName
      : null;
  const renameTab = workspaceTabs.renameTab;
  // The page's query client is declared further down; this binding serves
  // the redirect only.
  const resolutionQueryClient = useQueryClient();
  useEffect(() => {
    if (
      !org ||
      !workspaceSlug ||
      !workspaceId ||
      !resolvedOrgId ||
      !canonicalWorkspaceName ||
      canonicalWorkspaceName === workspaceSlug
    ) {
      return;
    }
    const segments = location.pathname.split("/");
    if (segments.length < 3) return;
    segments[2] = encodeURIComponent(canonicalWorkspaceName);
    renameTab(
      { orgSlug: org, name: workspaceSlug },
      { orgSlug: org, name: canonicalWorkspaceName },
    );
    // The canonical route resolves to the same workspace: seed it so the
    // resolution never goes pending and the page keeps its state.
    resolutionQueryClient.setQueryData(
      resolveWorkspaceKey(org, canonicalWorkspaceName),
      {
        orgId: resolvedOrgId,
        workspaceId,
        workspaceName: canonicalWorkspaceName,
      },
    );
    navigate(`${segments.join("/")}${location.search}${location.hash}`, {
      replace: true,
      state: location.state as unknown,
    });
  }, [
    canonicalWorkspaceName,
    location.hash,
    location.pathname,
    location.search,
    location.state,
    navigate,
    org,
    renameTab,
    resolutionQueryClient,
    resolvedOrgId,
    workspaceId,
    workspaceSlug,
  ]);
  // Owner-only phase move (g5.workbench.phases.state); optimistic on the
  // detail cache, rolled back on refusal.
  const updateWorkspacePhase = useUpdateWorkspacePhase(workspaceId);
  const workspaceUiCapabilities = useMemo(
    () =>
      applyDeploymentWorkspaceUiCapabilities(
        WORKSPACE_UI_CAPABILITIES,
        instanceInfo.status === "ok" ? instanceInfo.data : null,
      ),
    [instanceInfo],
  );
  const showCloudWorkspaceWorkflows =
    workspaceUiCapabilities.cloudWorkspaceWorkflows;

  // ---------------------------------------------------------------------------
  // Auth
  // ---------------------------------------------------------------------------

  const { user, hasScope, switchOrg } = useAuth();
  const { toasts, pushToast, dismiss: dismissToast } = useClientToasts();
  const [routeOrgSwitchTarget, setRouteOrgSwitchTarget] = useState<
    string | null
  >(null);
  const activeOrgId = user?.activeOrgId ?? "";
  const userId = user?.id ?? "";
  // Record the open into the per-user recently-opened MRU once the slug→UUID
  // resolution succeeds and the user is known — fire-and-forget localStorage
  // bookkeeping for organization Home (shell D45, routing-and-auth D24).
  useEffect(() => {
    if (!userId || !workspaceId) return;
    recordWorkspaceOpen(userId, workspaceId);
  }, [userId, workspaceId]);
  // The route org counts as active only when the active JWT org is KNOWN
  // (non-empty) and equals the route-resolved org. A hydrating/unknown active
  // org ("") must NOT be treated as active — otherwise org-scoped reads could
  // fire with a not-yet-confirmed default-org JWT. See routing-and-auth D6.
  const isRouteOrgActive =
    !!resolvedOrgId && !!activeOrgId && activeOrgId === resolvedOrgId;

  useEffect(() => {
    if (!resolvedOrgId || !activeOrgId) {
      setRouteOrgSwitchTarget(null);
      return;
    }
    if (activeOrgId === resolvedOrgId) {
      setRouteOrgSwitchTarget(null);
      return;
    }

    let cancelled = false;
    const targetOrgId = resolvedOrgId;
    setRouteOrgSwitchTarget(targetOrgId);

    void switchOrg(targetOrgId)
      .catch((error: unknown) => {
        if (cancelled) return;
        console.error("Failed to switch to the route organization:", error);
        pushToast(
          "error",
          "Could not switch organization",
          "Reload the page or switch organizations from the menu.",
        );
      })
      .finally(() => {
        if (cancelled) return;
        setRouteOrgSwitchTarget((current) =>
          current === targetOrgId ? null : current,
        );
      });

    return () => {
      cancelled = true;
    };
  }, [activeOrgId, resolvedOrgId, switchOrg, pushToast]);

  const isRouteOrgSwitching = routeOrgSwitchTarget === resolvedOrgId;

  const routeOrgMembership = useMemo(
    () =>
      user?.orgs.find(
        (membership) =>
          membership.orgSlug === org || membership.orgId === resolvedOrgId,
      ) ?? null,
    [user, org, resolvedOrgId],
  );

  // Worker-host admin access for the current org — mirrors WorkerHostsPage's
  // own gate (org admin role, or active-org `workers:admin` scope; the
  // backend enforces `workers:admin`). Gates the top-bar Workers entry.
  const canManageWorkers = useMemo(() => {
    return (
      showCloudWorkspaceWorkflows &&
      (routeOrgMembership?.role === "admin" ||
        (routeOrgMembership?.orgId === user?.activeOrgId &&
          hasScope("workers:admin")))
    );
  }, [routeOrgMembership, user, showCloudWorkspaceWorkflows, hasScope]);

  // ---------------------------------------------------------------------------
  // Responsive
  // ---------------------------------------------------------------------------

  const isMobile = !useMediaQuery("(min-width: 1024px)");

  // ---------------------------------------------------------------------------
  // Data hooks
  // ---------------------------------------------------------------------------

  const workspace = useWorkspace(workspaceId);
  // FILES drafts outlive their editors; the page lists them as unsaved.
  useFileDraftsAsDirtyEditors(draftPerson(useRenderedPrincipal()), workspaceId);
  const workspaceDisplayName =
    workspace.status === "ok"
      ? workspace.data.name
      : (workspaceSlug ?? "workspace");
  const allSortedBranches = useMemo(() => {
    if (workspace.status !== "ok") return [];
    return sortWorkspaceBranches(workspace.data.branches ?? []);
  }, [workspace]);
  const currentWorkspaceBranchIds = useMemo(
    () => new Set(allSortedBranches.map((branch) => branch.repoId)),
    [allSortedBranches],
  );
  const isCurrentWorkspaceBranchId = useCallback(
    (branchId: string | null | undefined) =>
      !!branchId && currentWorkspaceBranchIds.has(branchId),
    [currentWorkspaceBranchIds],
  );
  const docsBranchId = useMemo(() => {
    if (workspace.status !== "ok") return "";
    return resolveWorkspaceDocsBranchId(workspace.data, allSortedBranches);
  }, [allSortedBranches, workspace]);
  const sortedBranches = useMemo(() => {
    if (!docsBranchId) return allSortedBranches;
    return allSortedBranches.filter((branch) => branch.repoId !== docsBranchId);
  }, [allSortedBranches, docsBranchId]);
  const [activeBranchId, setActiveBranchId] = useState("");
  const [explicitContentBranchId, setExplicitContentBranchId] = useState("");
  const codeBranchIds = useMemo(
    () =>
      workspace.status === "ok"
        ? sortedBranches
            .filter((branch) => branch.repoId !== workspace.data.docsRepoId)
            .map((branch) => branch.repoId)
        : [],
    [sortedBranches, workspace],
  );
  useEffect(() => {
    if (sortedBranches.length === 0) {
      if (activeBranchId !== "") setActiveBranchId("");
      return;
    }
    const stillExists = sortedBranches.some((b) => b.repoId === activeBranchId);
    if (!stillExists) {
      // Presentation default only. Content consumers receive the separate
      // explicitContentBranchId below, so this first-sorted choice cannot
      // authorize a draft/live workspace file/file request (workspace-page-shell D60).
      setActiveBranchId(sortedBranches[0].repoId);
    }
  }, [activeBranchId, sortedBranches]);
  useEffect(() => {
    if (
      explicitContentBranchId &&
      !allSortedBranches.some(
        (branch) => branch.repoId === explicitContentBranchId,
      )
    ) {
      setExplicitContentBranchId("");
    }
  }, [allSortedBranches, explicitContentBranchId]);
  const activeBranch = useMemo(
    () => sortedBranches.find((b) => b.repoId === activeBranchId) ?? null,
    [sortedBranches, activeBranchId],
  );
  const taskList = useWorkspaceTasks(workspaceId);
  const tasksData = taskList.status === "ok" ? taskList.data : EMPTY_TASK_LIST;
  const hasActiveTask = useWorkspaceHasActiveTasks(workspaceId);
  // Persisted default agent provider (codex_support I11/D3) — a non-secret
  // scoped-config value, resolved against advertised worker capabilities by the
  // preferred-provider helpers below; falls back to first-available.
  const workspaceConfigForDefaultAgent = useWorkspaceScopedConfig(
    workspaceId,
    resolvedOrgId,
  );
  // Live subscription observations arrive only after a session starts work.
  const workspaceSubscriptionUsage = null;
  const defaultAgentProvider = useMemo(
    () =>
      defaultAgentProviderFromConfig(
        workspaceConfigForDefaultAgent.status === "ok"
          ? workspaceConfigForDefaultAgent.data.effectiveSettings
          : null,
      ),
    [workspaceConfigForDefaultAgent],
  );
  // Chat default-model read (the launch modals consume the full per-harness
  // choices map since #1370, so this claude_code-entry read serves only the
  // ChatPanel default-model inheritance).
  const chatDefaultClaudeModel =
    userSettings.defaultAgentProviderChoices.claude_code.model;
  const launchChoices = userSettings.defaultAgentProviderChoices;
  const syncAgentConfig = useMemo(
    () =>
      getPreferredSyncAgentConfig(
        defaultAgentProvider,
        launchChoices,
        chatAuthenticatableAgents,
      ),
    [defaultAgentProvider, launchChoices, chatAuthenticatableAgents],
  );
  const syncAgentConfigOptions = useMemo(
    () => getAvailableSyncAgentConfigs(launchChoices),
    [launchChoices],
  );
  const notifications = useNotifications(workspaceId, user?.id);
  // Shared read-only mutation-error surface (M31): any mutation rejected with a
  // WORKSPACE_READ_ONLY (403) ApiError re-emits through the module bus (wired to
  // the app-wide MutationCache in main.tsx). Surface one consistent warning toast
  // so blocked actions from stale tabs / races always explain themselves without
  // swallowing other errors.
  useEffect(
    () =>
      subscribeReadOnlyNotice(() =>
        pushToast(
          "warning",
          "Read-only workspace",
          WORKSPACE_READ_ONLY_MESSAGE,
        ),
      ),
    [pushToast],
  );
  const browserSectionNonce = useRef(0);
  const [browserSectionRequest, setBrowserSectionRequest] = useState<{
    section: SidebarSectionId;
    nonce: number;
  } | null>(null);
  const [leftSidebarOpen, setLeftSidebarOpen] = useState(true);
  const [activeSidebarSection, setActiveSidebarSection] =
    useState<SidebarSectionId | null>(null);
  const shouldLoadFileTree =
    leftSidebarOpen && activeSidebarSection === "files";
  const writeFileEntry = useWriteFileEntry();
  const deleteFileEntry = useDeleteFileEntry();
  const handleCreateFileEntry = useCallback(
    async (branchId: string, parentPath: string | null) => {
      const name = normalizeNewFileEntryName(window.prompt("New file name"));
      if (!name) return;
      const path = joinFileEntryPath(parentPath, name);
      try {
        await writeFileEntry.mutateAsync({
          workspaceId,
          repoId: branchId,
          path,
          content: "",
        });
        pushToast("info", "File created", path);
      } catch (error) {
        pushToast(
          "error",
          "Could not create file",
          error instanceof Error ? error.message : String(error),
        );
      }
    },
    [pushToast, writeFileEntry],
  );
  const handleCreateFolderEntry = useCallback(
    async (branchId: string, parentPath: string | null) => {
      const name = normalizeNewFileEntryName(window.prompt("New folder name"));
      if (!name) return;
      const path = joinFileEntryPath(parentPath, `${name}/G5_FOLDER.md`);
      try {
        await writeFileEntry.mutateAsync({
          workspaceId,
          repoId: branchId,
          path,
          content:
            "# Folder\n\nThis visible placeholder keeps the folder selectable until it contains another file.\n",
        });
        pushToast(
          "info",
          "Folder created",
          joinFileEntryPath(parentPath, name),
        );
      } catch (error) {
        pushToast(
          "error",
          "Could not create folder",
          error instanceof Error ? error.message : String(error),
        );
      }
    },
    [pushToast, writeFileEntry],
  );
  const handleDeleteFileEntry = useCallback(
    async (branchId: string, entry: SidebarFileTreeEntry) => {
      const confirmed = window.confirm(
        `Delete ${entry.type === "file" ? "file" : "folder"} "${entry.path}"?`,
      );
      if (!confirmed) return;
      try {
        await deleteFileEntry.mutateAsync({
          workspaceId,
          repoId: branchId,
          path: entry.path,
        });
        pushToast("info", "Deleted", entry.path);
      } catch (error) {
        pushToast(
          "error",
          "Could not delete file",
          error instanceof Error ? error.message : String(error),
        );
      }
    },
    [deleteFileEntry, pushToast],
  );
  const workspaceVm = useWorkspaceVm(workspaceId);
  // Whether anything the page shows of the workspace is as last loaded: its
  // name's resolution, its detail or its VM state could
  // not be refreshed (`g5.workspace.page.refresh`).
  const workspaceShownAsLastLoaded =
    (resolvedId.status === "ok" && resolvedId.refreshError !== null) ||
    (workspace.status === "ok" && workspace.refreshError !== null) ||
    (workspaceVm.status === "ok" && workspaceVm.refreshError !== null);
  const resumeWorkspaceVm = useResumeWorkspaceVm(workspaceId);
  // Review workspace identity: set when this workspace is a reviewer-owned
  // fork of another workspace (Approvals → Review Workspace). Drives the
  // review banner, the switcher badge and the one-shot review prompt seed.
  const workspaceReview = useWorkspaceReviewContext(workspaceId, !!workspaceId);
  const reviewOrigin =
    workspaceReview.status === "ok" ? workspaceReview.data.asReview : null;
  // The file opener the review panel uses is declared later in this
  // component (it needs the branch and docs metadata); the panel calls it
  // through a ref kept current below.
  // The banner is composed before the tab helpers exist; it reaches the
  // Review tab opener through a ref the tab code fills in.
  const openReviewTabRef = useRef<(() => void) | null>(null);
  const openReviewTabFromBanner = useCallback(() => {
    openReviewTabRef.current?.();
  }, []);
  const reviewCopyOrigin = useMemo<ReviewCopyOrigin | null>(
    () =>
      reviewOrigin
        ? {
            reviewWorkspaceId: reviewOrigin.reviewWorkspaceId,
            sourceWorkspaceId: reviewOrigin.sourceWorkspaceId,
            sourceWorkspaceName: reviewOrigin.sourceWorkspaceName,
          }
        : null,
    [reviewOrigin],
  );
  const stopWorkspaceVm = useStopWorkspaceVm(workspaceId);
  const transferWorkspace = useTransferWorkspace(workspaceId);
  const orgMembers = useOrgMembers(resolvedOrgId);
  const workspaceReadAuthority = JSON.stringify([
    user?.id,
    workspace.status === "ok" ? workspace.data.ownerUserId : null,
    workspaceVm.status === "ok"
      ? `${workspaceVm.data.ownerUserId}:${workspaceVm.data.vmId}:${workspaceVm.data.epoch}:${workspaceVm.data.status}`
      : "unavailable",
  ]);
  const docListState = useDocList(workspaceId, workspaceReadAuthority);
  // Workspace-wide approvals state for the chevron meters and the Prepare-to-
  // Ship surface (g5.workbench.gates).
  const workspaceApprovalsState = useWorkspaceApprovals(workspaceId);
  const shipApprovals =
    workspaceApprovalsState.status === "ok" &&
    !workspaceApprovalsState.refreshError
      ? workspaceApprovalsState.data
      : null;
  const reviewFlow = useWorkspaceReviewFlow(workspaceId);
  const reviewSubmitted =
    reviewFlow.isSuccess && !reviewFlow.error ? !!reviewFlow.data.pr : null;
  const reviewApprovalsStatus = reviewFlow.error
    ? "error"
    : reviewSubmitted === null
      ? "loading"
      : workspaceApprovalsState.status === "ok"
        ? workspaceApprovalsState.refreshError
          ? "error"
          : "ok"
        : workspaceApprovalsState.status === "error"
          ? "error"
          : "loading";
  const docs = useMemo(
    () => (docListState.status === "ok" ? docListState.data : []),
    [docListState],
  );
  const [createWorkspaceDialogOpen, setCreateWorkspaceDialogOpen] =
    useState(false);
  const [cloneSourceWorkspace, setCloneSourceWorkspace] =
    useState<WorkspaceSummary | null>(null);
  const [removeWorkspaceRequest, setRemoveWorkspaceRequest] =
    useState<WorkspaceSummary | null>(null);
  const deleteWorkspace = useDeleteWorkspace(
    removeWorkspaceRequest?.id ?? "",
    resolvedOrgId,
  );
  const [sourceDownloadInProgress, setSourceDownloadInProgress] =
    useState(false);
  // Workspace list for project switcher navigation
  const allWorkspaces = useWorkspaces(resolvedOrgId);
  const allWorkspacesData =
    allWorkspaces.status === "ok"
      ? allWorkspaces.data
      : EMPTY_WORKSPACE_SUMMARIES;
  const workspaceDetailData =
    workspace.status === "ok" && workspace.data.id === workspaceId
      ? workspace.data
      : null;
  const currentWorkspaceIsBase = workspaceDetailData?.isBase === true;
  // The personal presentation follows the registry kind, never the name
  // (personal_workspace g5.personal.workbench).
  const currentWorkspaceIsPersonal = isPersonalWorkspace(workspaceDetailData);
  // Workbench phase (g5.workbench.phases.state): the DTO's explicit value;
  // Base is always Business Engineering, and an older server (no `phase`)
  // reads as the default `be`.
  const workbenchPhase = currentWorkspaceIsBase
    ? "be"
    : isWorkspacePhase(workspaceDetailData?.phase)
      ? workspaceDetailData.phase
      : "be";
  const workspaceChatAvailable = workspaceDetailData?.isBase === false;
  // Re-run Verification is greyed until the organization's administrator turns
  // it on (ui g5.workbench.review-prompts). Read once the session has adopted
  // the workspace's organization, like the Issues switch.
  const verificationOrgConfig = useOrgScopedConfig(
    resolvedOrgId && resolvedOrgId === user?.activeOrgId ? resolvedOrgId : "",
  );
  const verificationActionEnabled =
    verificationOrgConfig.status === "ok" &&
    isOrgVerificationActionEnabled(verificationOrgConfig.data);
  // Retained callbacks must belong to this selected view, including an
  // A → B → A switch and unmount while an accepted operation finishes. The
  // scope is the workspace's identity once resolved: a former name in the
  // URL is replaced by the current name (g5.workspace.former-names) without
  // leaving the workspace, so an accepted operation still completes here.
  const workspaceViewScopeKey = workspaceId
    ? `${org}/${workspaceId}`
    : `${org}/${workspaceSlug}/`;
  const workspaceViewScopeRef = useRef<WorkspaceViewScope>({
    key: workspaceViewScopeKey,
    active: true,
  });
  if (workspaceViewScopeRef.current.key !== workspaceViewScopeKey) {
    workspaceViewScopeRef.current = {
      key: workspaceViewScopeKey,
      active: true,
    };
  }
  const workspaceViewScope = workspaceViewScopeRef.current;
  useEffect(() => {
    workspaceViewScope.active = true;
    return () => {
      workspaceViewScope.active = false;
    };
  }, [workspaceViewScope]);
  const workspaceIdsWithChildren = useMemo(
    () => workspaceIdsWithLiveChildren(allWorkspacesData),
    [allWorkspacesData],
  );
  const canRemoveWorkspace = useCallback(
    (candidate: WorkspaceSummary) =>
      canRemoveWorkspaceFromSwitcher(candidate, {
        workspaceListLoaded: allWorkspaces.status === "ok",
        workspaceIdsWithChildren,
        routeOrgActive: routeOrgMembership?.orgId === user?.activeOrgId,
        hasWorkspaceWrite: hasScope("workspaces:write"),
        userId: user?.id,
      }),
    [
      allWorkspaces.status,
      hasScope,
      routeOrgMembership,
      user?.activeOrgId,
      user?.id,
      workspaceIdsWithChildren,
    ],
  );
  // A review copy publishes only through Push review
  // (`g5.governance.review-workspace.exclusions`). The list projection
  // derives `isReview` from the fork record and usually answers before the
  // review context does, so the excluded controls never flicker in.
  const isReviewCopy =
    reviewOrigin !== null ||
    allWorkspacesData.some(
      (candidate) =>
        candidate.id === workspaceId && candidate.isReview === true,
    );
  // Until the list or the review context has answered, the page does not
  // know whether this is a copy: the controls a copy excludes are withheld
  // rather than shown and then taken away.
  const reviewIdentityKnown = reviewIdentityKnownFrom({
    reviewContextStatus: workspaceReview.status,
    listStatus: allWorkspaces.status,
    listHasCurrent: allWorkspacesData.some(
      (candidate) => candidate.id === workspaceId,
    ),
  });
  const reviewExcluded = isReviewCopy || !reviewIdentityKnown;
  const isRouteOrgAdmin = routeOrgMembership?.role === "admin";
  // Three gates, by what each operation needs (`g5.workspace.ui.gates`):
  // durable authority admits platform-record and Git actions without a VM;
  // live attachment admits only what the owner handle requires; the VM
  // state names the reason and the one valid lifecycle action. One
  // derivation, shared with Home's personal panel (g5.personal.home).
  const {
    gate: liveGate,
    isOwner: isWorkspaceOwner,
    canManageDurableOwnerState,
    ownerHasRunningAttachment,
    writable: workspaceWritable,
    liveEditable,
    kind: liveGateKindNow,
    workspaceNoun,
  } = useWorkspaceLiveGateValue({
    runtimeKey: workspaceReadAuthority,
    vmOwnerUserId:
      workspaceVm.status === "ok"
        ? (workspaceVm.data.ownerUserId ?? null)
        : undefined,
    workspace: workspaceDetailData,
    vmStatus: workspaceVm.status === "ok" ? workspaceVm.data.status : null,
    userId: user?.id,
    orgAdmin: isRouteOrgAdmin,
    reviewCopy: isReviewCopy,
    reviewIdentityKnown,
  });
  useWorkspaceUserActivity(
    workspaceId,
    workspaceVm.status === "ok" ? workspaceVm.data : null,
    isWorkspaceOwner,
  );
  // A review copy is never transferred: its review reaches the source as
  // suggestions, and the copy stays the reviewer's. A personal workspace is
  // never transferred (g5.personal.refusals).
  const canTransferCurrentWorkspace =
    !reviewExcluded &&
    !currentWorkspaceIsPersonal &&
    (isWorkspaceOwner || isRouteOrgAdmin);
  // Start and send (g5.chat.start-and-send): while the owner's VM is stopped
  // or failed, the composer stays open and a send records Start consent
  // together with the input, in any workspace the owner can start.
  const startAndSend = isWorkspaceOwner && liveGateCanStart(liveGateKindNow);
  // Fork, publication, pull requests and rebase are never offered from a
  // personal workspace (g5.personal.refusals, g5.personal.workbench).
  const publicationWorkflows =
    showCloudWorkspaceWorkflows && !currentWorkspaceIsPersonal;
  // Editing content requires live attachment: a stopped owner sees the
  // durable source and must explicitly Start the workspace; readers and
  // non-owning admins never receive live edit controls.
  const workspaceReadOnly = !liveEditable;
  // Why the current start waits, while it does; the banner, the pop-up notice
  // and the personal composer all read this one value.
  const capacityWaitReason =
    workspaceVm.status === "ok" &&
    workspaceVm.data.status === "queued_for_capacity"
      ? describeCapacityWait(workspaceVm.data.lastError)
      : null;
  // The banner names the state in its own words (`g5.workspace.ui.vm-state`);
  // Base keeps its exact message (`g5.workspace.base-ui.message`).
  const workspaceStateBanner = currentWorkspaceIsBase
    ? {
        message: "Read-only workspace. Fork workspace to make edits.",
        action: null as "start" | "stop" | null,
      }
    : liveGateBanner(liveGateKindNow, { waitReason: capacityWaitReason });
  // Stop and End reach a live reply through owner-only routes that accept
  // any mutable workspace the actor owns (`ensure_workspace_mutable`: not read
  // only, not freezing) without needing a running VM, so a loading or failed
  // VM status read must not take them away during a long reply
  // (`g5.agents.lifecycle.owner-stop`). Readers never get them.
  const canControlLiveChatTurn =
    canManageDurableOwnerState && !currentWorkspaceIsBase;
  // Comment threads are platform records: they need no running VM, only a
  // workspace that accepts writes. Readers start pushed-file threads and
  // reply; only the current owner moderates or comments on current work
  // (`g5.documents.comments.authority`).
  const commentsWritable =
    workspaceDetailData !== null &&
    workspaceAcceptsThreadWrites(workspaceDetailData);
  const commentPermissions = useMemo<CommentPermissions>(
    () => ({
      canStartThread: commentsWritable,
      canReply: commentsWritable,
      canModerate: commentsWritable && isWorkspaceOwner,
    }),
    [commentsWritable, isWorkspaceOwner],
  );
  const workspaceVmBusy =
    resumeWorkspaceVm.isLoading ||
    stopWorkspaceVm.isLoading ||
    transferWorkspace.isLoading;
  const workspaceVmActionError =
    resumeWorkspaceVm.error ?? stopWorkspaceVm.error ?? transferWorkspace.error;
  const notifyWorkspaceVmAction = useCallback(
    (...args: Parameters<typeof pushToast>): boolean => {
      if (
        workspaceViewScope.active &&
        workspaceViewScopeRef.current === workspaceViewScope
      ) {
        pushToast(...args);
        return true;
      }
      return false;
    },
    [pushToast, workspaceViewScope],
  );
  // A queued start is announced as a pop-up once per Start, from the Start's
  // own answer and from the VM poll, however the workspace entered the wait
  // (the owner's Start, a Start and send, `g5d`, or a page opened on a
  // waiting workspace); a changed reason updates the banner and tooltip, and
  // the header row never lays the reason out beside its buttons
  // (`g5.ui.capacity-wait-notice`). Only the owner sees the VM's state, like
  // its controls.
  const announceCapacityWait = useCallback(
    (vm: WorkspaceVmView) => {
      if (
        vm.status !== "queued_for_capacity" ||
        !isWorkspaceOwner ||
        currentWorkspaceIsBase
      ) {
        return;
      }
      const key = capacityWaitKey(vm);
      if (
        markCapacityWaitAnnounced(key) &&
        !notifyWorkspaceVmAction(
          "info",
          "Workspace waiting for capacity",
          describeCapacityWait(vm.lastError),
        )
      ) {
        // Not shown because the owner has moved on: still owed here.
        forgetCapacityWaitAnnouncement(key);
      }
    },
    [currentWorkspaceIsBase, isWorkspaceOwner, notifyWorkspaceVmAction],
  );
  const polledVm = workspaceVm.status === "ok" ? workspaceVm.data : null;
  useEffect(() => {
    if (!polledVm) return;
    if (polledVm.status === "queued_for_capacity") {
      announceCapacityWait(polledVm);
    } else {
      forgetEndedCapacityWaits(polledVm);
    }
  }, [announceCapacityWait, polledVm]);
  const handleResumeWorkspace = useCallback(async () => {
    try {
      const result = await resumeWorkspaceVm.mutateAsync(undefined);
      if (result.outcome === "queued_for_capacity") {
        announceCapacityWait(result.vm);
      } else if (result.outcome === "attached") {
        notifyWorkspaceVmAction(
          "info",
          "Workspace running",
          "The workspace VM is attached.",
        );
      } else {
        notifyWorkspaceVmAction(
          "info",
          "Workspace starting",
          "The workspace VM is attaching.",
        );
      }
    } catch (error) {
      notifyWorkspaceVmAction(
        "error",
        "Could not start workspace",
        error instanceof Error ? error.message : String(error),
      );
    }
  }, [announceCapacityWait, notifyWorkspaceVmAction, resumeWorkspaceVm]);
  // The Reviews page's "Start" (next to "Start the workspace to accept
  // suggestions") is the header's Start, asked for through an event.
  useEffect(() => {
    if (!isWorkspaceOwner) return;
    const start = () => void handleResumeWorkspace();
    window.addEventListener(WORKSPACE_START_EVENT, start);
    return () => window.removeEventListener(WORKSPACE_START_EVENT, start);
  }, [handleResumeWorkspace, isWorkspaceOwner]);
  const handleStopWorkspace = useCallback(async () => {
    try {
      const outcome = await stopWorkspaceVm.mutateAsync(undefined);
      const toast = describeStopOutcome(outcome);
      notifyWorkspaceVmAction(toast.severity, toast.title, toast.message);
    } catch (error) {
      notifyWorkspaceVmAction(
        "error",
        "Could not stop workspace",
        error instanceof Error ? error.message : String(error),
      );
    }
  }, [notifyWorkspaceVmAction, stopWorkspaceVm]);
  const handleTransferWorkspace = useCallback(
    async (toUserId: string) => {
      try {
        const outcome = await transferWorkspace.mutateAsync({
          toUserId,
          idempotencyKey: randomUuid(),
        });
        if (outcome.status === "completed") {
          notifyWorkspaceVmAction(
            "info",
            "Workspace transferred",
            "The prior incarnation was stopped and ownership changed.",
          );
        } else {
          notifyWorkspaceVmAction(
            "info",
            "Workspace transfer in progress",
            "G5 is stopping and purging the prior incarnation before ownership changes.",
          );
        }
      } catch (error) {
        notifyWorkspaceVmAction(
          "error",
          "Could not transfer workspace",
          error instanceof Error ? error.message : String(error),
        );
      }
    },
    [notifyWorkspaceVmAction, transferWorkspace],
  );
  // Dedicated workspace docs folder ("docs/<workspace_id>") — drives Project
  // Docs filtering, All Docs current-folder exclusion, and the default
  // create-doc folder.
  const docsFolderPath = workspaceDetailData?.docsFolderPath ?? null;
  // Workspace display names by id, for labeling UUID folder segments under
  // docs/ in the All Docs sidebar section.
  const workspaceNamesById = useMemo(() => {
    const names: Record<string, string> = {};
    for (const ws of allWorkspacesData) names[ws.id] = ws.name;
    // The list covers every workspace in the org once it resolves; this
    // explicit re-add covers the load window, when `allWorkspacesData` is still
    // empty but the single workspace-detail query has landed. Without it, doc
    // paths would briefly render the raw UUID fallback for the current
    // workspace.
    if (workspaceDetailData) {
      names[workspaceDetailData.id] = workspaceDetailData.name;
    }
    return names;
  }, [allWorkspacesData, workspaceDetailData]);
  const currentWorkspaceSummary = useMemo<WorkspaceSummary | null>(() => {
    const summary = resolveCurrentWorkspaceSummary(
      allWorkspacesData.find((w) => w.id === workspaceId),
      workspaceDetailData,
    );
    // The detail-built fallback carries no `isReview`; the review context
    // may know the copy before the list does.
    return summary && isReviewCopy && summary.isReview !== true
      ? { ...summary, isReview: true }
      : summary;
  }, [allWorkspacesData, isReviewCopy, workspaceId, workspaceDetailData]);
  // Reflect the currently-open workspace in the browser-tab title as
  // "G5 - {workspace-name}", using the route slug until workspace data loads.
  // Restore the static default title when leaving the workspace so
  // non-workspace pages are unaffected.
  const currentWorkspaceName =
    currentWorkspaceSummary?.name ?? workspaceDisplayName;
  useEffect(() => {
    document.title = workspaceDocumentTitle(currentWorkspaceName);
    return () => {
      document.title = DEFAULT_DOCUMENT_TITLE;
    };
  }, [currentWorkspaceName]);

  const handleDownloadSource = useCallback(async () => {
    if (sourceDownloadInProgress) return;
    const workspaceName =
      currentWorkspaceSummary?.name ??
      (workspace.status === "ok" ? workspace.data.name : "workspace");

    setSourceDownloadInProgress(true);
    try {
      const response = await api.getRawResponse(
        routes.workspaces.sourceDownload(workspaceId),
      );
      const blob = await response.blob();
      const objectUrl = URL.createObjectURL(blob);
      const filename =
        filenameFromContentDisposition(
          response.headers.get("content-disposition"),
        ) ?? `${safeDownloadStem(workspaceName)}-source.zip`;
      const anchor = document.createElement("a");
      anchor.href = objectUrl;
      anchor.download = filename;
      anchor.style.display = "none";
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(objectUrl);
    } catch (error) {
      pushToast(
        "error",
        "Source download failed",
        error instanceof Error
          ? error.message
          : "Could not download workspace source.",
      );
    } finally {
      setSourceDownloadInProgress(false);
    }
  }, [
    currentWorkspaceSummary?.name,
    pushToast,
    sourceDownloadInProgress,
    workspace,
    workspaceId,
  ]);

  // The selected task: the left-panel highlight and the task's chat session.
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const pendingSelectedTaskIdRef = useRef<string | null>(null);
  // The `?session=` hint the deep-link effect saw on its previous run, so a
  // link to the same content path without it (a query-only navigation) can
  // hand the center back to the content.
  const prevSessionHintRef = useRef<string | null>(null);
  // Right-panel chat is standard workspace chat. Task selection does not
  // create/switch chat scope.
  const chatScopeType = getRightPanelChatScope();

  // Chat hooks
  const chatSessions = useChatSessions(
    workspaceChatAvailable ? workspaceId : "",
    chatScopeType,
  );
  const [activeChatSessionId, setActiveChatSessionId] = useState<string | null>(
    null,
  );
  const suppressedSessionHintRef = useRef<string | null>(null);
  const activeChatQuerySessionId = workspaceChatAvailable
    ? (activeChatSessionId ?? "")
    : "";
  const activeChatSessionState = useChatSession(activeChatQuerySessionId);
  const chatSessionData = useChatMessages(activeChatQuerySessionId);
  const activeChatTaskState = useSessionTask(activeChatQuerySessionId);
  const activeChatTaskSummary =
    activeChatTaskState.status === "ok" ? activeChatTaskState.data : null;
  const activeTaskSessionTaskId = activeChatTaskSummary?.id ?? null;
  const sendChatMessage = useSendChatMessage();
  const startChatMessage = useStartChatMessage();
  const renameChatSession = useRenameChatSession();
  const replyToProviderRequest = useReplyToProviderRequest();
  const endBackgroundWait = useEndBackgroundWait();
  const endBackgroundWaitAsync = endBackgroundWait.mutateAsync;
  const forkSession = useForkSession();
  // Workbench (g5.workbench.actions, g5.workbench.persona): the served action
  // set and personas, templated starts and "follow view" forks. Only the
  // ribbon reads it only while the workspace offers Chat.
  const workbenchConfig = useWorkbenchConfig(
    workspaceId,
    workspaceChatAvailable,
  );
  const startWorkbenchAction = useStartWorkbenchAction();
  const followView = useFollowView();
  // Guided journey (ui g5.ui.guided-toggle): the section exists with the
  // feature on, never on Base, and only while the person keeps the guided
  // experience on; its marks and the source pull requests load only then.
  const guidedJourneyEnabled = useGuidedJourneyEnabled();
  // A personal workspace has no phases or journey (g5.personal.workbench).
  const guidedJourneyShown =
    guidedJourneyEnabled &&
    workspaceChatAvailable &&
    userSettings.showGuidedJourney &&
    !currentWorkspaceIsPersonal;
  const journeyMarks = useJourneyMarks(workspaceId, guidedJourneyShown);
  const setJourneyMark = useSetJourneyMark();
  const [journeyMarkPendingStepId, setJourneyMarkPendingStepId] = useState<
    string | null
  >(null);
  // A mark that failed to save stays on its row with a Retry until the next
  // attempt on that step (ui g5.ui.guided-panel), instead of a toast.
  const [journeyMarkFailure, setJourneyMarkFailure] =
    useState<JourneyMarkFailure | null>(null);
  // The sessions this page started for a ribbon action, by action id: a
  // step reads Running while its session still has a running task. Page
  // memory only (sessions do not record the action), so a reload forgets it.
  const [journeyActionSessions, setJourneyActionSessions] = useState<
    Readonly<Record<string, string>>
  >({});
  const journeyActiveSessionIds = useWorkspaceActiveSessionIds(
    workspaceId,
    guidedJourneyShown,
  );
  const journeyRunningActionIds = useMemo(() => {
    const ids = new Set<string>();
    for (const [actionId, sessionId] of Object.entries(journeyActionSessions)) {
      if (journeyActiveSessionIds.has(sessionId)) ids.add(actionId);
    }
    return ids;
  }, [journeyActionSessions, journeyActiveSessionIds]);
  const rememberJourneyActionSession = useCallback(
    (actionId: string, sessionId: string) =>
      setJourneyActionSessions((current) =>
        current[actionId] === sessionId
          ? current
          : { ...current, [actionId]: sessionId },
      ),
    [],
  );
  // Loaded pull requests survive a transient refresh failure like the marks
  // do, so a completed step never reads as to-do while the API is unreachable.
  const journeyPullRequests = toRefreshingState(
    useQuery({
      ...workspacePullRequestsQueryOptions(workspaceId, "source", "all"),
      enabled: guidedJourneyShown && !!workspaceId,
    }),
  );
  const journeyFacts: JourneyFacts = useMemo(
    () => ({
      docs,
      docsFolderPath,
      pullRequests:
        journeyPullRequests.status === "ok" ? journeyPullRequests.data : [],
      marks: journeyMarks.status === "ok" ? journeyMarks.data : [],
    }),
    [docs, docsFolderPath, journeyMarks, journeyPullRequests],
  );
  const handleJourneyMark = useCallback(
    async (stepId: string, mark: JourneyMark) => {
      setJourneyMarkPendingStepId(stepId);
      setJourneyMarkFailure((current) =>
        current?.stepId === stepId ? null : current,
      );
      try {
        await setJourneyMark.mutateAsync({
          workspaceId,
          stepId,
          done: mark.done,
          skipped: mark.skipped,
        });
      } catch (error) {
        setJourneyMarkFailure({
          stepId,
          mark,
          message: error instanceof Error ? error.message : String(error),
        });
      } finally {
        setJourneyMarkPendingStepId((current) =>
          current === stepId ? null : current,
        );
      }
    },
    [setJourneyMark, workspaceId],
  );
  const [pendingWorkbenchActionId, setPendingWorkbenchActionId] = useState<
    string | null
  >(null);
  const handleRenameChatSession = useCallback(
    (sessionId: string, title: string | null) =>
      renameChatSession.mutateAsync({ sessionId, title }),
    [renameChatSession.mutateAsync],
  );

  // Reset chat session on workspace switch
  useEffect(() => {
    setActiveChatSessionId(null);
  }, [workspaceId]);
  const handleSelectTask = useCallback(
    (taskId: string) => {
      setSelectedTaskId(taskId);
      pendingSelectedTaskIdRef.current = taskId;
      if (workspaceRoutePath)
        navigate(workspaceRoutePath + taskRoutePath(taskId), {
          replace: true,
        });
    },
    [workspaceRoutePath, navigate],
  );

  const taskDetail = useTask(selectedTaskId ?? "");

  const chatScopeRef = workspaceId;
  const chatScopeLabel = workspaceSlug ?? "Workspace";

  const queryClient = useQueryClient();
  // A cached name resolution can outlive its workspace (deleted in another
  // tab or by someone else, possibly with the name reused). When the
  // resolved id is gone, resolve the name again once instead of polling a
  // dead id; a second 404 for the same route and id is shown as the error it
  // is. The page stays mounted across workspace switches, so the retry is
  // tracked per route resolution, not per id.
  const reresolvedRef = useRef<string | null>(null);
  const workspaceNotFound =
    workspace.status === "error" &&
    workspace.error instanceof ApiError &&
    workspace.error.status === 404;
  // Deleting a workspace closes its tabs (g5.workbench.tabs.deleted): when
  // the workspace this page showed is gone — the name no longer resolves, or
  // still names the dead id — its tab closes as the page's own delete closes
  // it, showing the left neighbour, else Home. A new workspace under the
  // name is shown instead. The route and the tab list may have moved on by
  // the time the re-resolution answers, so the strip is read then; a page
  // that was left by then has no active tab to close.
  const closeStripTab = workspaceTabs.closeTab;
  const stripRef = useRef({
    activeTab: activeStripTab,
    closeTab: closeStripTab,
  });
  useLayoutEffect(() => {
    stripRef.current = { activeTab: activeStripTab, closeTab: closeStripTab };
    return () => {
      stripRef.current = { activeTab: null, closeTab: closeStripTab };
    };
  }, [activeStripTab, closeStripTab]);
  useEffect(() => {
    if (!workspaceNotFound) return;
    const attempt = JSON.stringify([routeTarget, workspaceId]);
    if (reresolvedRef.current === attempt) return;
    reresolvedRef.current = attempt;
    const resolutionKey = workspaceRouteResolutionKey(routeTarget);
    const stripTab = stripRef.current.activeTab;
    void queryClient.invalidateQueries({ queryKey: resolutionKey }).then(() => {
      if (!stripTab || routeTarget.family !== "workspace") return;
      const resolved = queryClient.getQueryData<ResolvedIds>(resolutionKey);
      if (resolved && resolved.workspaceId !== workspaceId) return;
      const { activeTab, closeTab } = stripRef.current;
      if (!activeTab || !sameWorkspaceTab(activeTab, stripTab)) return;
      closeTab(stripTab);
    });
  }, [workspaceNotFound, workspaceId, queryClient, routeTarget]);
  // The route names a workspace that does not exist: a stale link, or a tab
  // kept for a workspace deleted while this browser was away. The tab is not
  // kept (g5.workbench.tabs.deleted); the page stays, under a transient tab,
  // to say the workspace was not found.
  const unlistStripTab = workspaceTabs.unlistTab;
  const routeWorkspaceMissing =
    routeFamily === "workspace" &&
    resolvedId.status === "error" &&
    resolvedId.error instanceof ApiError &&
    resolvedId.error.status === 404;
  useEffect(() => {
    if (!routeWorkspaceMissing || !activeStripTab) return;
    unlistStripTab(activeStripTab);
  }, [activeStripTab, routeWorkspaceMissing, unlistStripTab]);
  useEffect(() => {
    if (ownerHasRunningAttachment) return;
    // A stop, transfer, demotion, or revocation makes every live payload
    // ineligible. Drop file bodies/trees immediately and refetch the durable
    // Git/projection views under the newly observed authority.
    queryClient.removeQueries({ queryKey: ["workspace-file", workspaceId] });
    queryClient.removeQueries({ queryKey: ["file-tree"] });
    queryClient.invalidateQueries({
      queryKey: ["workspace-changes", workspaceId],
    });
    queryClient.invalidateQueries({ queryKey: ["docs", workspaceId] });
    queryClient.invalidateQueries({ queryKey: ["views", workspaceId] });
  }, [ownerHasRunningAttachment, queryClient, workspaceId]);
  const replyToProviderRequestAsync = replyToProviderRequest.mutateAsync;
  // Provider choices are submitted with the next session input. The retired
  // mutable provider-control endpoint no longer persists an out-of-band draft.
  const acceptProviderSettingsLocally = useCallback(async () => {}, []);
  const handleResolveAgentApproval = useCallback(
    async (input: AgentApprovalResolutionInput) => {
      if (
        !input.taskId ||
        input.attempt === undefined ||
        !input.providerRequestId ||
        !input.requestKind
      ) {
        throw new Error("This historical approval is no longer actionable.");
      }
      const validatedResponse =
        input.validatedResponse ??
        (input.requestKind === "permission"
          ? input.decision === "approved"
            ? {
                behavior: "allow",
                ...(input.updatedInput && typeof input.updatedInput === "object"
                  ? { updated_input: input.updatedInput }
                  : {}),
              }
            : {
                behavior: "deny",
                ...(input.reason ? { message: input.reason } : {}),
              }
          : null);
      if (!validatedResponse) {
        throw new Error("The provider question response is missing answers.");
      }
      return replyToProviderRequestAsync({
        sessionId: input.sessionId,
        taskId: input.taskId,
        attempt: input.attempt,
        providerRequestId: input.providerRequestId,
        validatedResponse,
        ...(input.startCompute ? { startCompute: true } : {}),
      });
    },
    [replyToProviderRequestAsync],
  );

  const deleteDoc = useDeleteDoc(workspaceId);
  const deleteViewDoc = useDeleteViewDoc(workspaceId);
  const [projectDocDeleteRequest, setProjectDocDeleteRequest] = useState<{
    docId: string;
    name: string;
  } | null>(null);
  const targetWorkspacePullRequests = useWorkspacePullRequests(
    workspaceId,
    "target",
    "open",
  );
  const sourceWorkspacePullRequests = useWorkspacePullRequests(
    workspaceId,
    "source",
    "open",
  );
  const preflightWorkspaceMerge = usePreflightWorkspaceMerge(workspaceId);
  const startWorkspaceMerge = useStartWorkspaceMerge(workspaceId);
  const [directCleanChoice, setDirectCleanChoice] =
    useState<WorkspaceMergeReviewChoice | null>(null);
  const [workspaceMergeConfirmOpen, setWorkspaceMergeConfirmOpen] =
    useState(false);
  const [workspaceMergeTargetId, setWorkspaceMergeTargetId] =
    useState<string>("");
  // Prepare-to-Ship modal (workspace-page-shell D52), opened from the
  // ribbon's Create PR and Merge actions (g5.workbench.ship-actions).
  const [prepareToShipOpen, setPrepareToShipOpen] = useState(false);
  const workspaceMergeTargets = useMemo(
    () =>
      workspaceMergeTargetsForCurrentWorkspace(
        currentWorkspaceSummary,
        allWorkspacesData,
      ),
    [allWorkspacesData, currentWorkspaceSummary],
  );
  const pullRequestTargetChoices = useWorkspacePullRequestTargets(workspaceId);
  const workspacePullRequestTargets =
    pullRequestTargetChoices.status === "ok"
      ? pullRequestTargetChoices.data
      : EMPTY_PR_TARGETS;
  const workspaceMergeError = workspaceMergeErrorMessage(
    preflightWorkspaceMerge.error ?? startWorkspaceMerge.error,
  );
  const pullRequestTargetError =
    pullRequestTargetChoices.status === "error"
      ? pullRequestTargetChoices.error
      : pullRequestTargetChoices.status === "ok"
        ? pullRequestTargetChoices.refreshError
        : null;
  const pullRequestDestinationStatus =
    showCloudWorkspaceWorkflows &&
    !currentWorkspaceIsBase &&
    !currentWorkspaceIsPersonal &&
    !reviewExcluded &&
    workspaceWritable &&
    pullRequestTargetError ? (
      <div
        role="alert"
        className="flex items-center gap-3 px-4 py-2 text-sm text-[var(--g5-blocked)]"
      >
        <span>
          {pullRequestTargetChoices.status === "ok"
            ? "Could not refresh pull request destinations. Showing previously loaded destinations. "
            : "Could not load pull request destinations: "}
          {workspaceMergeErrorMessage(pullRequestTargetError)}
        </span>
        <button
          type="button"
          data-testid="workspace-pr-destinations-retry"
          className="underline"
          onClick={() =>
            void queryClient.invalidateQueries({
              queryKey: ["workspaces", "pull-request-targets", workspaceId],
            })
          }
        >
          Retry PR destinations
        </button>
      </div>
    ) : null;
  // Prepare-to-Ship modal target: the direct parent first, else Base
  // (shell D52).
  const prepareToShipTarget = workspacePullRequestTargets[0] ?? null;
  const prepareToShipExistingPullRequest = openWorkspacePullRequestForTarget(
    sourceWorkspacePullRequests.status === "ok"
      ? sourceWorkspacePullRequests.data
      : [],
    prepareToShipTarget?.id ?? "",
  );
  // Preserve each repository's independent comparison from the already-fetched
  // detail. The modal renders behind-only readiness and never sums unrelated
  // Git histories (prepare-to-ship-modal D3, issue #1995).
  const repositoryComparison = useMemo(
    () =>
      workspace.status === "ok"
        ? summarizeRepositoryComparisons(workspace.data.branches)
        : null,
    [workspace],
  );

  useEffect(() => {
    if (!workspaceMergeConfirmOpen) return;
    if (
      workspaceMergeTargetId &&
      workspaceMergeTargets.some(
        (target) => target.id === workspaceMergeTargetId,
      )
    ) {
      return;
    }
    setWorkspaceMergeTargetId(workspaceMergeTargets[0]?.id ?? "");
  }, [
    workspaceMergeConfirmOpen,
    workspaceMergeTargetId,
    workspaceMergeTargets,
  ]);

  const handleOpenWorkspaceMerge = useCallback(() => {
    preflightWorkspaceMerge.reset();
    startWorkspaceMerge.reset();
    setDirectCleanChoice(null);
    setWorkspaceMergeConfirmOpen(true);
  }, [preflightWorkspaceMerge, startWorkspaceMerge]);

  const [rightPanelOpen, setRightPanelOpen] = useState(true);
  const [chatSetupError, setChatSetupError] = useState<string | null>(null);
  const [chatComposerResetToken, setChatComposerResetToken] = useState(0);
  // Prompt history is this person's, per workspace and chat; the panel adds
  // the chat (chat g5.chat.ui.prompt-history.store).
  const workspaceChatPromptHistoryScope = useMemo<
    ChatPromptHistoryScope | undefined
  >(
    () => (userId && workspaceId ? { userId, workspaceId } : undefined),
    [userId, workspaceId],
  );
  // Unsent composer text per workspace, held by the page so a remount of the
  // ChatPanel (Dock, Pop out, a tab switch and back) keeps what was typed
  // (g5.workspace.ui.chat-corner-button).
  const chatComposerDraftsRef = useRef(new Map<string, ChatComposerDraft>());
  // A fork lands on a fresh composer (ui g5.workspace.ui.fork-landing). Its
  // router state carries the fork's draft once: the first draft this page
  // creates for the landed workspace starts as that text, unsent, with Send
  // pulsing as a seeded draft does, under the landing's seed header when it
  // carries one (a Base re-verification run's cost notice,
  // g5.reverification.workspace). The landing is then held for the
  // workspace, since in-page URL writes drop router state, so the composer
  // keeps the fork's placeholder until a chat exists.
  const forkLanding = useMemo(
    () => readForkLandingState(location.state),
    [location.state],
  );
  const forkLandingRef = useRef(forkLanding);
  forkLandingRef.current = forkLanding;
  const [forkLandingWorkspaceId, setForkLandingWorkspaceId] = useState<
    string | null
  >(null);
  useEffect(() => {
    if (forkLanding && workspaceId) setForkLandingWorkspaceId(workspaceId);
  }, [forkLanding, workspaceId]);
  const chatComposerDraftFor = useCallback((id: string): ChatComposerDraft => {
    let draft = chatComposerDraftsRef.current.get(id);
    if (!draft) {
      draft = createChatComposerDraft();
      const landing = forkLandingRef.current;
      if (landing?.draft) {
        draft.text = landing.draft;
        draft.seedPulse = true;
        // Sending or emptying the draft drops it with the pulse.
        if (landing.header) draft.seedHeader = landing.header;
      }
      chatComposerDraftsRef.current.set(id, draft);
    }
    return draft;
  }, []);
  const chatComposerPlaceholder =
    forkLandingWorkspaceId !== null &&
    forkLandingWorkspaceId === workspaceId &&
    !activeChatSessionId
      ? FORK_LANDING_COMPOSER_PLACEHOLDER
      : undefined;

  // ---------------------------------------------------------------------------
  // URL View Params Sync — Fix 9 (§13)
  // ---------------------------------------------------------------------------

  /** Update URL query params without navigating away (replace: true) */
  const updateViewParams = useCallback(
    (
      updates: Record<string, string | null>,
      pathOverride?: string,
      resetParams = false,
    ) => {
      if (!workspaceRoutePath) return;
      const params = new URLSearchParams(resetParams ? "" : searchParams);
      for (const [key, value] of Object.entries(updates)) {
        if (value === null) params.delete(key);
        else params.set(key, value);
      }
      const qs = params.toString();
      const basePath = pathOverride || location.pathname || workspaceRoutePath;
      navigate(`${basePath}${qs ? "?" + qs : ""}`, { replace: true });
    },
    [workspaceRoutePath, location.pathname, searchParams, navigate],
  );

  // ---------------------------------------------------------------------------
  // Fix 1 (§2): View session in chat panel
  // ---------------------------------------------------------------------------

  // Stale-hint fence (chat intent 7010a91e): navigate() updates searchParams
  // asynchronously, so a render can still carry the PREVIOUS ?session= hint
  // AFTER an imperative selection cleared the suppression — URL hydration then
  // flipped the panel back to the old session (live flip right after the
  // first attached send, dev 2026-08-24 02:48:31, session e247e9d6). The ref
  // records the imperative selection; hydration ignores any non-matching hint
  // until the route catches up, mirroring pendingSelectedTaskIdRef.
  const pendingSessionSelectionRef = useRef<string | null>(null);
  const pendingSessionRevealRef = useRef<boolean | null>(null);

  const handleViewSession = useCallback(
    (
      sessionId: string,
      pathOverride?: string,
      resetParams = false,
      revealChat = true,
    ) => {
      if (
        !workspaceChatAvailable ||
        !workspaceViewScope.active ||
        workspaceViewScopeRef.current !== workspaceViewScope
      )
        return;
      suppressedSessionHintRef.current = null;
      pendingSessionSelectionRef.current = sessionId;
      pendingSessionRevealRef.current = revealChat;
      setActiveChatSessionId(sessionId);
      setRightPanelOpen(revealChat);
      updateViewParams(
        { session: sessionId, ...(revealChat ? { view: null } : {}) },
        pathOverride,
        resetParams,
      );
    },
    [updateViewParams, workspaceChatAvailable, workspaceViewScope],
  );

  const handleWorkspaceMergeStarted = useCallback(
    (response?: StartWorkspaceMergeResponse) => {
      if (response?.resolution) {
        setPrepareToShipOpen(false);
        handleViewSession(response.resolution.sessionId);
        // Start and resolve with agent (g5.git.stopped-rebase.resolve-with-agent):
        // the brief waits for the Start it is bound to.
        const start = response.startCompute;
        if (start && start.status !== "running") {
          pushToast(
            "info",
            "Conflict resolution queued",
            start.status === "queued_for_capacity"
              ? "The workspace is waiting for capacity to start. An agent resolves the conflicts once it is running; review its result before shipping."
              : "The workspace is starting. An agent resolves the conflicts once it is running; review its result before shipping.",
          );
          return;
        }
        pushToast(
          "info",
          "Conflict resolution started",
          "An agent is resolving conflicts in this workspace. Review its result before shipping to Base.",
        );
        return;
      }
      if (response?.run?.status && response.run.status !== "completed") {
        pushToast(
          "info",
          "Workspace rebase workflow started",
          "The full rebase workflow is running for the current workspace. The target workspace will not be advanced.",
        );
        return;
      }

      if (response?.rebase) {
        const changed = response.rebase.members.some(
          (member) => member.published,
        );
        pushToast(
          "info",
          changed
            ? "Workspace rebase completed"
            : "Workspace already up to date",
          changed
            ? "The workspace was rebased in its running VM and saved to G5. The target workspace was not advanced."
            : "The workspace already contains the target; nothing was changed.",
        );
        return;
      }

      pushToast(
        "info",
        "Workspace rebase completed",
        "The current workspace was rebased without advancing the target workspace.",
      );
    },
    [handleViewSession, pushToast],
  );

  const startWorkspaceRebase = useCallback(
    async (
      choice: WorkspaceMergeReviewChoice,
      options?: { startCompute: boolean },
    ) => {
      if (!acquireLaunchSubmission("workspace-rebase")) return false;
      try {
        const response = await startWorkspaceMerge.mutateAsync({
          targetWorkspaceId: choice.targetWorkspaceId,
          agentLaunch: choice.agentLaunch,
          ...(options?.startCompute ? { startCompute: true } : {}),
        });
        handleWorkspaceMergeStarted(response);
        return true;
      } finally {
        releaseLaunchSubmission("workspace-rebase");
      }
    },
    [
      acquireLaunchSubmission,
      handleWorkspaceMergeStarted,
      releaseLaunchSubmission,
      startWorkspaceMerge,
    ],
  );

  // Shared rebase confirm flow (preflight → direct-clean review choice →
  // start). Callers supply the target: the Rebase & Merge dialog uses its
  // selected target, the Prepare-to-Ship modal its PR target (shell D52).
  const confirmWorkspaceMergeForTarget = useCallback(
    async (
      targetWorkspaceId: string,
      targetWorkspaceName: string | null,
      agentConfig: SyncAgentConfig,
      agentLaunch: AgentLaunchSelectionV1,
    ) => {
      if (!targetWorkspaceId) return;
      // The dialog can outlive the gate (`g5.workspace.ui.reader-publication`).
      if (!workspaceWritable || reviewExcluded) return;
      if (!acquireLaunchSubmission("workspace-merge")) return;
      const launchConfig = completeAgentConfig(agentConfig);
      try {
        const preflightResponse = await preflightWorkspaceMerge.mutateAsync({
          targetWorkspaceId,
        });
        const { preflight } = preflightResponse;
        if (shouldShowWorkspaceMergeReview(preflight)) {
          setDirectCleanChoice({
            targetWorkspaceId,
            label: `${workspaceDisplayName} onto ${targetWorkspaceName ?? "target workspace"}`,
            preflight,
            agentConfig: launchConfig,
            agentLaunch,
          });
          setWorkspaceMergeConfirmOpen(false);
          return;
        }
        const response = await startWorkspaceMerge.mutateAsync({
          targetWorkspaceId,
          agentLaunch,
        });
        setWorkspaceMergeConfirmOpen(false);
        startWorkspaceMerge.reset();
        handleWorkspaceMergeStarted(response);
      } catch (error) {
        pushToast(
          "error",
          "Could not start workspace merge",
          error instanceof Error
            ? error.message
            : "Failed to start workspace merge.",
        );
      } finally {
        releaseLaunchSubmission("workspace-merge");
      }
    },
    [
      handleWorkspaceMergeStarted,
      acquireLaunchSubmission,
      preflightWorkspaceMerge,
      pushToast,
      releaseLaunchSubmission,
      reviewExcluded,
      startWorkspaceMerge,
      workspaceDisplayName,
      workspaceWritable,
    ],
  );

  const handleConfirmWorkspaceMerge = useCallback(
    async (
      agentConfig: SyncAgentConfig,
      agentLaunch: AgentLaunchSelectionV1,
    ) => {
      if (!workspaceMergeTargetId) return;
      const target = workspaceMergeTargets.find(
        (candidate) => candidate.id === workspaceMergeTargetId,
      );
      await confirmWorkspaceMergeForTarget(
        workspaceMergeTargetId,
        target?.name ?? null,
        agentConfig,
        agentLaunch,
      );
    },
    [
      confirmWorkspaceMergeForTarget,
      workspaceMergeTargetId,
      workspaceMergeTargets,
    ],
  );

  // Any in-flight direct-merge submission (either modal button, or the
  // merge-review approval) pins the modal open: cancel and the modal's
  // preview-attach affordance are both gated on this (CodeRabbit 2026-08-22).
  const directCleanMergeSubmitting = isDirectCleanMergeSubmitting(
    launchSubmissionKeys,
    startWorkspaceMerge.isLoading,
  );
  const handleDirectCleanChoiceCancel = useCallback(() => {
    if (directCleanMergeSubmitting) {
      return;
    }
    setDirectCleanChoice(null);
    startWorkspaceMerge.reset();
  }, [directCleanMergeSubmitting, startWorkspaceMerge]);

  const handleMergeReviewApproval = useCallback(async () => {
    if (!directCleanChoice) return;
    if (!acquireLaunchSubmission("merge-review")) return;
    try {
      await startWorkspaceRebase(directCleanChoice);
      setDirectCleanChoice(null);
      startWorkspaceMerge.reset();
    } catch (error) {
      pushToast(
        "error",
        "Could not rebase workspace",
        error instanceof Error
          ? error.message
          : "Failed to rebase the current workspace.",
      );
    } finally {
      releaseLaunchSubmission("merge-review");
    }
  }, [
    acquireLaunchSubmission,
    directCleanChoice,
    pushToast,
    releaseLaunchSubmission,
    startWorkspaceRebase,
    startWorkspaceMerge,
  ]);

  const handleDirectCleanFullWorkflow = useCallback(
    async (options: { startCompute: boolean }) => {
      if (!directCleanChoice) return;
      if (!acquireLaunchSubmission("direct-clean-full")) return;
      try {
        await startWorkspaceRebase(directCleanChoice, options);
        setDirectCleanChoice(null);
      } catch (error) {
        pushToast(
          "error",
          "Could not start merge workflow",
          error instanceof Error
            ? error.message
            : "Failed to start the full merge workflow.",
        );
      } finally {
        releaseLaunchSubmission("direct-clean-full");
      }
    },
    [
      acquireLaunchSubmission,
      directCleanChoice,
      pushToast,
      releaseLaunchSubmission,
      startWorkspaceRebase,
    ],
  );

  // WebSocket subscriptions
  useWSChannel(isUuid(workspaceId) ? `workspace:${workspaceId}` : "");
  useWSChannel(
    workspaceChatAvailable &&
      activeChatSessionId &&
      activeChatSessionState.status === "ok" &&
      activeChatSessionState.data.workspaceId === workspaceId
      ? `chat:${activeChatSessionId}`
      : "",
  );
  // §38.357: Subscribe to task-specific events for live timeline updates.
  useWSChannel(selectedTaskId ? `task:${selectedTaskId}` : "");

  const handleCloseChatPanel = useCallback(() => {
    const sessionHint = searchParams.get("session");
    if (sessionHint) {
      suppressedSessionHintRef.current = sessionHint;
      pendingSessionSelectionRef.current = null;
      pendingSessionRevealRef.current = null;
      updateViewParams({ session: null });
    }
    setRightPanelOpen(false);
  }, [searchParams, updateViewParams]);

  // Content navigation removes the chat query hint asynchronously. Suppress
  // that exact stale value until the URL catches up so it cannot reopen Chat
  // for one render after a sidebar or content-tab selection.
  const hideChatForContent = useCallback(() => {
    const sessionHint = searchParams.get("session");
    if (sessionHint) {
      suppressedSessionHintRef.current = sessionHint;
      pendingSessionSelectionRef.current = null;
      pendingSessionRevealRef.current = null;
    }
    setRightPanelOpen(false);
  }, [searchParams]);
  const findTaskSummaryById = useCallback(
    (taskId: string) => tasksData.find((task) => task.id === taskId) ?? null,
    [tasksData],
  );

  const pendingTaskChatSelectionRef = useRef<string | null>(null);
  const handleSelectTaskAndOpenChat = useCallback(
    (taskId: string) => {
      pendingTaskChatSelectionRef.current = taskId;
      const selectedTaskPath = workspaceRoutePath
        ? `${workspaceRoutePath}${taskRoutePath(taskId)}`
        : undefined;
      handleSelectTask(taskId);
      const summarySessionId = findTaskSummaryById(taskId)?.sessionId;
      const detailSessionId =
        taskDetail.status === "ok" && taskDetail.data.id === taskId
          ? taskDetail.data.sessionId
          : null;
      const sessionId = summarySessionId ?? detailSessionId;
      if (sessionId) {
        // Select the attached conversation for the persistent Chat tab while
        // keeping the task output visible in the center.
        handleViewSession(
          sessionId,
          selectedTaskPath,
          Boolean(selectedTaskPath),
          false,
        );
        return;
      }

      void api
        .get<TaskDetail>(`/api/v1/tasks/${taskId}`)
        .then((task) => {
          if (
            pendingTaskChatSelectionRef.current !== taskId ||
            task.workspaceId !== workspaceId
          )
            return;
          if (!task.sessionId) return;
          handleViewSession(
            task.sessionId,
            selectedTaskPath,
            Boolean(selectedTaskPath),
            false,
          );
        })
        .catch(() => {
          // Task detail loading is already surfaced through the task query.
        });
    },
    [
      findTaskSummaryById,
      handleSelectTask,
      handleViewSession,
      taskDetail,
      workspaceId,
      workspaceRoutePath,
    ],
  );

  const autoOpenedTaskSessionRef = useRef<string | null>(null);

  useEffect(() => {
    if (!selectedTaskId || taskDetail.status !== "ok") return;
    if (taskDetail.data.id !== selectedTaskId) return;
    if (
      taskDetail.data.workspaceId !== workspaceId ||
      taskDetail.data.taskType === "preview"
    )
      return;
    const sessionId = taskDetail.data.sessionId;
    if (!sessionId) return;

    const key = `${selectedTaskId}:${sessionId}`;
    if (autoOpenedTaskSessionRef.current === key) return;
    autoOpenedTaskSessionRef.current = key;
    // A URL that already names a session (a deep link's hint, or the
    // selection a click wrote) owns the chat: the route hydration applies
    // the hint, and rewriting the query here would drop the link's
    // `askUserQuestion` and replace a different explicit session. The key
    // is recorded first, so content navigation that later drops the hint
    // does not pull the URL back to this task.
    if (searchParams.get("session")) return;
    // Pass the selected task's own route as the path override. Without it,
    // `handleViewSession` -> `updateViewParams` falls back to
    // `location.pathname`, which is stale within the render batch that just
    // navigated to the new task — re-selecting a task (whose `taskDetail` is
    // cached) would then navigate the route back to the previous task.
    const selectedTaskPath = workspaceRoutePath
      ? `${workspaceRoutePath}${taskRoutePath(selectedTaskId)}`
      : undefined;
    handleViewSession(
      sessionId,
      selectedTaskPath,
      Boolean(selectedTaskPath),
      false,
    );
  }, [
    handleViewSession,
    selectedTaskId,
    taskDetail,
    routeTaskId,
    location.pathname,
    routeTaskId,
    searchParams,
    workspaceRoutePath,
  ]);

  const handleCreateChatSession = useCallback(() => {
    if (
      !workspaceChatAvailable ||
      !workspaceViewScope.active ||
      workspaceViewScopeRef.current !== workspaceViewScope
    )
      return;
    // `navigate(..., { replace: true })` updates searchParams asynchronously. If
    // the old `?session=...` is still visible for one render, URL hydration would
    // immediately re-select the old chat and make the first + click appear to do
    // nothing. Suppress only that exact stale hint; allow any different/deep-link
    // session hint to hydrate normally.
    suppressedSessionHintRef.current = activeChatSessionId;
    pendingSessionSelectionRef.current = null;
    pendingSessionRevealRef.current = null;
    setActiveChatSessionId(null);
    setChatComposerResetToken((token) => token + 1);
    setChatSetupError(null);
    setRightPanelOpen(true);
    updateViewParams({ session: null });
  }, [
    activeChatSessionId,
    updateViewParams,
    workspaceChatAvailable,
    workspaceViewScope,
  ]);

  // The page's one pending composer seed (ui g5.workbench.chat-actions),
  // made by every consumer of the shared seed path below: Explain in chat,
  // Discuss, the ribbon and Guided chat actions, the review prompts and the
  // dialogs. It is bound to the workspace visit that made it, and the seed
  // either ChatPanel receives is derived in render from that binding, so a
  // panel mounted for another workspace sees no seed in its very first
  // render, whatever the order of effects (issue #1190). The ref mirrors the
  // state for the send handler and the effects, which read the request as it
  // is now rather than as it was rendered.
  const chatComposerSeedRequestRef = useRef<ChatComposerSeedRequest | null>(
    null,
  );
  const [chatComposerSeedRequest, setChatComposerSeedRequestState] =
    useState<ChatComposerSeedRequest | null>(null);
  const setChatComposerSeedRequest = useCallback(
    (next: ChatComposerSeedRequest | null) => {
      chatComposerSeedRequestRef.current = next;
      setChatComposerSeedRequestState(next);
    },
    [],
  );
  const chatComposerSeed =
    chatComposerSeedRequest &&
    chatComposerSeedRequest.scope === workspaceViewScope
      ? chatComposerSeedRequest.seed
      : null;
  // Monotonic token: Date.now() can repeat across rapid clicks, which would
  // make the second node's seed look already-consumed to the one-shot guard.
  const chatComposerSeedTokenRef = useRef(0);
  // Leaving the workspace drops the seed with its start and reference. The
  // render-time binding above already keeps it from the next workspace's
  // panel; this frees the payload.
  useEffect(() => {
    const pending = chatComposerSeedRequestRef.current;
    if (pending && pending.scope !== workspaceViewScope) {
      setChatComposerSeedRequest(null);
    }
  }, [setChatComposerSeedRequest, workspaceViewScope]);
  // The seed belongs to the chat it went into: a fresh chat's seed lives
  // while no chat is selected, an open chat's while that chat is. Choosing
  // another chat drops it, so its start and reference never reach that
  // chat's sends; the text already applied stays the person's draft.
  useEffect(() => {
    const pending = chatComposerSeedRequestRef.current;
    if (pending && (pending.start?.sessionId ?? null) !== activeChatSessionId) {
      setChatComposerSeedRequest(null);
    }
  }, [activeChatSessionId, setChatComposerSeedRequest]);
  // Emptying the seeded draft abandons the seed (g5.workbench.chat-actions).
  // The panel names the applied token, so a draft seeded earlier, or by a
  // fork landing, drops nothing newer.
  const handleComposerSeedEmptied = useCallback(
    (token: number) => {
      if (chatComposerSeedRequestRef.current?.seed.token === token) {
        setChatComposerSeedRequest(null);
      }
    },
    [setChatComposerSeedRequest],
  );
  // Shared seed path: fresh new-session composer prefilled with `text` —
  // never auto-sent. Consumers: the HTML
  // doc answer bridge's D60 explainer prompts, and the D69 question Discuss
  // action (which also attaches the doc as a project_doc reference).
  const handleSeedChatPrompt = useCallback(
    (
      text: string,
      reference?: ChatProjectReference,
      /**
       * A chat action's seeded start (g5.workbench.chat-actions); with a
       * `sessionId` the seed goes into that open chat instead of a fresh one.
       */
      seededStart?: ChatComposerSeededStart,
      /** Shown at the top of the seeded chat (g5.workbench.review-prompts). */
      header?: ComposerSeedHeader,
    ) => {
      if (
        !workspaceChatAvailable ||
        !workspaceViewScope.active ||
        workspaceViewScopeRef.current !== workspaceViewScope
      )
        return false;
      if (!seededStart?.sessionId) handleCreateChatSession();
      // A seed into an open chat brings that chat back into view as a fresh
      // one would: a document opened meanwhile hides Chat, and the prompt
      // has to be where the person looks (review of PR #971).
      else setRightPanelOpen(true);
      chatComposerSeedTokenRef.current += 1;
      // A newer seed replaces the pending one, whatever it attached.
      setChatComposerSeedRequest({
        scope: workspaceViewScope,
        seed: {
          token: chatComposerSeedTokenRef.current,
          text,
          ...(header ? { header } : {}),
        },
        reference: reference ?? null,
        start: seededStart ?? null,
      });
      return true;
    },
    [
      handleCreateChatSession,
      setChatComposerSeedRequest,
      workspaceChatAvailable,
      workspaceViewScope,
    ],
  );
  // Code review and Re-run Verification (g5.workbench.review-prompts): a
  // fresh chat seeded with the prompt and the header it shows on top.
  const seedReviewPrompt = useCallback(() => {
    const seed = reviewWorkspaceSeed();
    handleSeedChatPrompt(seed.text, undefined, undefined, seed.header);
  }, [handleSeedChatPrompt]);
  const seedVerificationPrompt = useCallback(() => {
    const seed = verificationSeed();
    handleSeedChatPrompt(seed.text, undefined, undefined, seed.header);
  }, [handleSeedChatPrompt]);
  // Review workspace hand-off (ReviewWorkspaceCard → this page): the Approvals
  // page marks a one-shot seed before navigating here, and the reviewer who
  // owns the copy gets the review prompt prefilled in a fresh composer once.
  const reviewOriginWorkspaceId = reviewOrigin?.reviewWorkspaceId ?? null;
  // Open review suggestion counts per file (Files section badges). On a
  // review copy the badges follow the file overlays: the source's
  // suggestions, since publications belong to the source.
  const suggestionCountsWorkspaceId = reviewOrigin
    ? reviewOrigin.sourceWorkspaceId
    : workspaceId;
  const suggestionCounts = useReviewSuggestionCounts(
    suggestionCountsWorkspaceId,
    !!suggestionCountsWorkspaceId,
  );
  const suggestionCountPaths =
    suggestionCounts.status === "ok" ? suggestionCounts.data.paths : undefined;
  useEffect(() => {
    if (!reviewOriginWorkspaceId || !isWorkspaceOwner) return;
    // Start succeeded but the VM was not resumed: say why, once, so the
    // disabled composer is not a mystery. The VM strip offers Start.
    const resumeError = takeReviewResumeError(reviewOriginWorkspaceId);
    if (resumeError) {
      pushToast(
        "error",
        "Review workspace VM did not start",
        `${resumeError} Use Start workspace in the VM strip to try again.`,
      );
    }
    if (!takeReviewPromptSeed(reviewOriginWorkspaceId)) return;
    seedReviewPrompt();
  }, [reviewOriginWorkspaceId, isWorkspaceOwner, seedReviewPrompt, pushToast]);

  // A client toast's action is a path of this app: open it and drop the
  // toast. Shared by the desktop and mobile toast stacks.
  const handleClientToastAction = useCallback(
    (id: string, action: NotificationAction) => {
      if (action.action === "navigate" && action.target) {
        navigate(action.target);
      }
      dismissToast(id);
    },
    [dismissToast, navigate],
  );
  // Push review outcome, shared by the desktop and mobile banners: say what
  // round went out and stay in the copy (the dialog promised as much); the
  // Review tab refreshes on its own, and the toast offers the source's
  // Reviews page for whoever wants to see the round next to the others.
  const handleReviewPushed = useCallback(
    (result: PublishWorkspaceReviewResponse) => {
      const skipped = result.skipped.length;
      const name = reviewOrigin?.sourceWorkspaceName ?? "the source";
      pushToast(
        "info",
        `Round ${result.publication.round} pushed`,
        `Sent to ${name}${
          skipped > 0
            ? `; ${skipped} file${skipped === 1 ? "" : "s"} could not be compared`
            : ""
        }.`,
        reviewOrigin
          ? [
              {
                label: `Open on ${name}`,
                action: "navigate",
                target: sourceReviewsPath(org ?? "", reviewOrigin),
              },
            ]
          : [],
      );
    },
    [org, pushToast, reviewOrigin],
  );
  const handleReviewUpdated = useCallback(
    (outcome: { ok: boolean; message: string }) => {
      pushToast(
        outcome.ok ? "info" : "error",
        outcome.ok
          ? "Latest changes picked up"
          : "Latest changes not picked up",
        outcome.message,
      );
    },
    [pushToast],
  );
  const handleRequestRemoveWorkspace = useCallback(
    (candidate: WorkspaceSummary) => {
      if (!canRemoveWorkspace(candidate)) return;
      deleteWorkspace.reset();
      setRemoveWorkspaceRequest(candidate);
    },
    [canRemoveWorkspace, deleteWorkspace],
  );
  // Finish review (`g5.governance.review-workspace.finish`): the ordinary
  // Stop of the copy, then the ordinary Remove offered for it. Pushed rounds
  // stay on the source either way.
  const handleFinishReview = useCallback(async () => {
    if (!reviewOrigin || !currentWorkspaceSummary) return;
    const status = workspaceVm.status === "ok" ? workspaceVm.data.status : null;
    const step = finishReviewStep(status);
    if (step === "unknown") {
      pushToast(
        "info",
        "Review workspace state not known yet",
        "Finish review once the workspace's state has loaded.",
      );
      return;
    }
    if (step === "stopping") {
      pushToast(
        "info",
        "Review workspace is stopping",
        `Rounds you pushed stay on ${reviewOrigin.sourceWorkspaceName}. Finish review again once it has stopped to remove the workspace, or keep it to reopen later.`,
      );
      return;
    }
    if (step === "stop") {
      let outcome: WorkspaceStopOutcome;
      try {
        outcome = await stopWorkspaceVm.mutateAsync(undefined);
      } catch (error) {
        if (
          !workspaceViewScope.active ||
          workspaceViewScopeRef.current !== workspaceViewScope
        )
          return;
        pushToast(
          "error",
          "Could not stop the review workspace",
          error instanceof Error ? error.message : String(error),
        );
        return;
      }
      if (
        !workspaceViewScope.active ||
        workspaceViewScopeRef.current !== workspaceViewScope
      )
        return;
      // Stop is accepted before the VM has released; removal needs a
      // stopped workspace, so it is offered once the stop has completed.
      if (!finishReviewStopped(outcome)) {
        pushToast(
          "info",
          "Review workspace is stopping",
          `Rounds you pushed stay on ${reviewOrigin.sourceWorkspaceName}. Finish review again once it has stopped to remove the workspace, or keep it to reopen later.`,
        );
        return;
      }
    }
    pushToast(
      "info",
      "Review finished",
      `Your review workspace is stopped. Rounds you pushed stay on ${reviewOrigin.sourceWorkspaceName}; remove the workspace or keep it to reopen later.`,
    );
    handleRequestRemoveWorkspace(currentWorkspaceSummary);
  }, [
    currentWorkspaceSummary,
    workspaceViewScope,
    handleRequestRemoveWorkspace,
    pushToast,
    reviewOrigin,
    stopWorkspaceVm,
    workspaceVm,
  ]);
  // Workspace state banner (M25, `g5.workspace.ui.vm-state`). Persistent
  // while content is not editable: Base and readers read the fork text; the
  // owner reads the VM state in its own words, with the one valid lifecycle
  // action. Both layouts render it, so a phone can stop a starting VM too.
  const workspaceStateBannerElement = workspaceStateBanner ? (
    <div
      data-testid="workspace-readonly-banner"
      data-surface="banner"
      role="status"
      className={classNames(
        "flex flex-wrap items-center gap-2 px-4 py-1.5",
        "border-b border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10",
        "text-xs text-[var(--g5-text1)]",
      )}
    >
      <ReadOnlyBadge />
      <span>{workspaceStateBanner.message}</span>
      {workspaceStateBanner.action === "start" && (
        <button
          type="button"
          data-testid="workspace-readonly-banner-start"
          onClick={() => void handleResumeWorkspace()}
          disabled={workspaceVmBusy}
          className="rounded border border-[var(--g5-stroke)] px-2 py-0.5 text-xs font-medium text-[var(--g5-active)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
        >
          Start
        </button>
      )}
      {workspaceStateBanner.action === "stop" && (
        <button
          type="button"
          data-testid="workspace-readonly-banner-stop"
          onClick={() => void handleStopWorkspace()}
          disabled={workspaceVmBusy}
          className="rounded border border-[var(--g5-stroke)] px-2 py-0.5 text-xs font-medium text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
        >
          Stop
        </button>
      )}
    </div>
  ) : null;
  const reviewWorkspaceBanner = reviewOrigin ? (
    <ReviewWorkspaceBanner
      review={reviewOrigin}
      canPush={isWorkspaceOwner}
      onSeedReviewPrompt={
        workspaceWritable ? () => seedReviewPrompt() : undefined
      }
      onFinishReview={
        workspaceWritable &&
        currentWorkspaceSummary !== null &&
        canRemoveWorkspace(currentWorkspaceSummary)
          ? () => void handleFinishReview()
          : undefined
      }
      onPushed={handleReviewPushed}
      onUpdated={handleReviewUpdated}
      onOpenReviewTab={openReviewTabFromBanner}
      onBackToSource={() =>
        navigate(workspacePagePath(org ?? "", reviewOrigin.sourceWorkspaceName))
      }
      mobilePush={isMobile}
    />
  ) : null;

  // ---------------------------------------------------------------------------
  // Fix 8 (§9): Open in IDE / external tab
  // ---------------------------------------------------------------------------

  // ---------------------------------------------------------------------------
  // Chat streaming state (platform notices leave the latest turn live)
  // ---------------------------------------------------------------------------

  const chatMessages =
    chatSessionData.status === "ok" ? chatSessionData.data : [];
  const isChatStreaming = useMemo(
    () => hasLiveAssistantTurn(chatMessages),
    [chatMessages],
  );
  // One Stop state per turn, shared with the Home chat
  // (chat g5.chat.controls): `Stopping…` at once, one request in flight,
  // Retry on failure, the A→B handoff fence as before.
  const chatStop = useChatStopController({
    sessionId: activeChatSessionId,
    messages: chatMessages,
    messagesReady: chatSessionData.status === "ok",
  });
  const isChatStopControlSuppressed = chatStop.stopControlSuppressed;
  const handleStopStreaming = chatStop.onStop;
  const chatStopState = chatStop.stopState;

  const refreshActiveChatTurn = useCallback(
    (sessionId: string) => {
      // Single-flight (chat_panel_runtime D21): must not cancel an in-flight
      // window read — with cancelRefetch: true a read slower than the 3s tick
      // is aborted and restarted forever and recovery never converges.
      void refreshChatMessages(queryClient, sessionId);
      queryClient.invalidateQueries({
        queryKey: ["session-timeline", sessionId],
      });
      queryClient.invalidateQueries({
        queryKey: ["chat-session", sessionId],
      });
      queryClient.invalidateQueries({ queryKey: ["chat-sessions"] });
      queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
      if (activeTaskSessionTaskId) {
        queryClient.invalidateQueries({
          queryKey: ["task", activeTaskSessionTaskId],
        });
      }
    },
    [activeTaskSessionTaskId, queryClient],
  );

  useEffect(() => {
    if (!activeChatSessionId || !isChatStreaming) return;

    refreshActiveChatTurn(activeChatSessionId);
    const timer = window.setInterval(() => {
      refreshActiveChatTurn(activeChatSessionId);
    }, 3000);

    return () => window.clearInterval(timer);
  }, [activeChatSessionId, isChatStreaming, refreshActiveChatTurn]);

  // ---------------------------------------------------------------------------
  // Unified timeline — API-fetched history + live WS entries
  // ---------------------------------------------------------------------------

  // Fetch persisted timeline entries from the session-level API (survives reload)
  const apiTimeline = useSessionTimeline(activeChatSessionId ?? "");
  const apiTimelineEntries: TimelineEntry[] = useMemo(() => {
    if (apiTimeline.status !== "ok") return [];
    return apiTimeline.data.items;
  }, [apiTimeline]);
  // Backward timeline pagination for the chat panel (job_chat_draft_hooks D41,
  // chat_panel_runtime D35, issue #2333). `fetchNextPage` changes identity each
  // render, so hold it behind a ref + stable callback for the memoized
  // ChatPanel; the loaded pages already flow through the id-deduped merge below.
  const timelineHasEarlierHistory = apiTimeline.hasOlder;
  const timelineIsLoadingEarlierHistory = apiTimeline.isLoadingOlder;
  const loadOlderTimelineRef = useRef(apiTimeline.loadOlder);
  loadOlderTimelineRef.current = apiTimeline.loadOlder;
  const handleLoadEarlierTimelineHistory = useCallback(() => {
    void loadOlderTimelineRef.current();
  }, []);
  // Messages-source scrollback (job_chat_draft_hooks D39; chat_panel_runtime
  // D38): same ref + stable-callback pattern as the timeline loader so the
  // memoized ChatPanel sites keep identity-stable props.
  const chatMessagesHasOlder = chatSessionData.hasOlder;
  const chatMessagesIsLoadingOlder = chatSessionData.isLoadingOlder;
  const loadOlderChatMessagesRef = useRef(chatSessionData.loadOlder);
  loadOlderChatMessagesRef.current = chatSessionData.loadOlder;
  const handleLoadOlderChatMessages = useCallback(() => {
    void loadOlderChatMessagesRef.current();
  }, []);

  // Live rows arrive through `chat.timeline` into the session-timeline cache
  // itself (chat g5.chat.live-delivery), so the transcript reads one window
  // for saved and live rows; the unsaved tail is read back after each saved
  // read while the turn is active (g5.chat.live-tail-recovery).
  const chatLiveTailState = useLiveTail(
    activeChatSessionId,
    isChatStreaming,
    apiTimeline.historyReads,
  );

  // §38.1066 + §38.1260: Invalidate doc/view caches on server-side changes
  const handleDocContentChanged = useCallback(
    (event: {
      type: "doc.content_changed";
      workspaceId: string;
      docId: string;
    }) => {
      if (event.workspaceId !== workspaceId) return;
      queryClient.invalidateQueries({
        queryKey: ["view", workspaceId, event.docId],
      });
    },
    [queryClient, workspaceId],
  );
  useWSEventListener(
    "doc.content_changed",
    handleDocContentChanged as (event: unknown) => void,
  );

  const handleDocCreated = useCallback(
    (event: { type: "doc.created"; workspaceId: string; docId: string }) => {
      if (event.workspaceId !== workspaceId) return;
      queryClient.invalidateQueries({ queryKey: ["docs", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["views", workspaceId] });
    },
    [queryClient, workspaceId],
  );
  useWSEventListener(
    "doc.created",
    handleDocCreated as (event: unknown) => void,
  );

  const handleDocDeleted = useCallback(
    (event: { type: "doc.deleted"; workspaceId: string; docId: string }) => {
      if (event.workspaceId !== workspaceId) return;
      queryClient.invalidateQueries({ queryKey: ["docs", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["views", workspaceId] });
    },
    [queryClient, workspaceId],
  );
  useWSEventListener(
    "doc.deleted",
    handleDocDeleted as (event: unknown) => void,
  );

  // The held window's rows for the active session (saved and live alike).
  const chatTimelineEntries: TimelineEntry[] = useMemo(() => {
    const seenIds = new Set<string>();
    const merged: TimelineEntry[] = [];
    for (const entry of apiTimelineEntries) {
      const entrySessionId = entry.metadata?.sessionId;
      const belongsToActiveSession =
        !activeChatSessionId ||
        typeof entrySessionId !== "string" ||
        entrySessionId === activeChatSessionId;
      if (!belongsToActiveSession || seenIds.has(entry.id)) continue;
      seenIds.add(entry.id);
      merged.push(entry);
    }
    return merged;
  }, [activeChatSessionId, apiTimelineEntries]);

  // ---------------------------------------------------------------------------
  // Core UI state
  // ---------------------------------------------------------------------------

  const [retainedTabs, setOpenTabs] = useState<TabItem[]>([]);
  useLayoutEffect(() => {
    if (!ownerHasRunningAttachment) return;
    setOpenTabs((tabs) => {
      const unbound = (tab: TabItem) =>
        (tab.type === "file" || tab.type === "doc") && !tab.liveAuthorityKey;
      return tabs.some(unbound)
        ? tabs.map((tab) =>
            unbound(tab)
              ? { ...tab, liveAuthorityKey: workspaceReadAuthority }
              : tab,
          )
        : tabs;
    });
  }, [ownerHasRunningAttachment, workspaceReadAuthority, retainedTabs]);
  // A revoked runtime's private file names and bodies are ineligible in the
  // same render. Browser drafts remain in their person's separate draft store.
  const openTabs = useMemo(
    () =>
      retainedTabs.filter(
        (tab) =>
          (tab.type !== "file" && tab.type !== "doc") ||
          !tab.liveAuthorityKey ||
          (ownerHasRunningAttachment &&
            tab.liveAuthorityKey === workspaceReadAuthority),
      ),
    [retainedTabs, ownerHasRunningAttachment, workspaceReadAuthority],
  );
  const [selectedTab, setSelectedTab] = useState<string | null>(null);
  const [tabStateWorkspaceId, setTabStateWorkspaceId] = useState<string | null>(
    null,
  );
  // Ref mirror of selectedTab — lets openTab skip redundant scroll-clear when
  // the deep-link useEffect re-fires for an already-selected tab.
  const selectedTabRef = useRef<string | null>(null);
  selectedTabRef.current = selectedTab;
  const previousTabWorkspaceIdRef = useRef<string | null>(null);
  // A tab-URL write (`syncUrlToTab`) that has not landed, with the location
  // key it was issued under: the deep-link effect waits for it, and a
  // different navigation landing first (a review copy's Review landing, a
  // link) supersedes it rather than leaving the page waiting for a URL that
  // never comes.
  const pendingTabUrlRef = useRef<{ url: string; key: string } | null>(null);
  const locationRef = useRef(location);
  locationRef.current = location;
  // The selected tab item, for effects that must not re-run on selection.
  const selectedTabItemRef = useRef<TabItem | null>(null);
  const lastResolvedRouteKeyRef = useRef<string | null>(null);
  // Tracks the `?view=` value (settings|workers|members) already consumed by the
  // URL→tab effect, so the deep-link redirect opens the matching admin tab
  // exactly once (and navigating away does not immediately re-open it).
  const consumedViewParamRef = useRef<string | null>(null);
  // The `?view=` value the tab→URL mirror itself last wrote. It names a tab
  // that was selected, not a hint: when it lands after the user has already
  // selected another tab, opening it would undo that selection.
  const mirroredViewParamRef = useRef<string | null>(null);
  const autoOpenedReviewTabRef = useRef<string | null>(null);
  // The `?view=` value seen on the previous render, so the tab→URL mirror effect
  // can tell whether the URL changed externally this render (browser
  // back/forward, manual edit, redirect → URL wins) versus the active tab
  // changing (rail click/close → mirror the URL). Avoids a bounce-back race.
  const prevViewParamRef = useRef<string | null>(null);
  // The tab a consumed hint asked for; mirroring waits until it is selected.
  const pendingHintTabTypeRef = useRef<string | null>(null);
  const [leftSidebarWidth, setLeftSidebarWidth] =
    useState(LEFT_SIDEBAR_DEFAULT);
  const [rightPanelDetached, setRightPanelDetached] = useState(false);
  // The corner Chat button opens the floating panel at the right edge; Pop
  // out keeps opening it centered (g5.workspace.ui.chat-corner-button).
  const [floatingChatPlacement, setFloatingChatPlacement] =
    useState<FloatingPanelPlacement>("center");
  // The right-edge Chat reopens at the width it last had in this browser,
  // read on each open so another tab's resize counts too.
  const [chatCornerWidth, setChatCornerWidth] = useState(
    FLOATING_PANEL_DEFAULT_SIZE.w,
  );
  const handleChatCornerResizeEnd = useCallback((size: { w: number }) => {
    setChatCornerWidth(size.w);
    writeStoredChatCornerWidth(size.w);
  }, []);
  const [chatFocusRequest, setChatFocusRequest] = useState(0);
  const coveringOverlayOpen = useCoveringOverlayOpen();
  const centerChatActive =
    workspaceChatAvailable && rightPanelOpen && !rightPanelDetached;
  const chatComposerSurface = !workspaceChatAvailable
    ? "unavailable"
    : isMobile
      ? "mobile"
      : rightPanelDetached
        ? "floating"
        : centerChatActive
          ? "center"
          : "hidden";
  // A hidden center tab keeps the panel mounted; only a shown, uncovered
  // chat can display a result (chat g5.chat.inspection.acknowledge).
  const chatSurfaceVisible =
    chatComposerSurface !== "unavailable" &&
    chatComposerSurface !== "hidden" &&
    !coveringOverlayOpen;
  // Each visit owns its pending sends, including fresh composers whose session
  // ID is still null and panels replaced by docking or responsive layout.
  const chatComposerScope = useMemo(
    () => ({
      sessionId: activeChatSessionId,
      resetToken: chatComposerResetToken,
      surface: chatComposerSurface,
    }),
    [activeChatSessionId, chatComposerResetToken, chatComposerSurface],
  );
  const chatComposerScopeRef = useRef(chatComposerScope);
  useLayoutEffect(() => {
    chatComposerScopeRef.current = chatComposerScope;
  }, [chatComposerScope]);
  useEffect(() => setChatSetupError(null), [chatComposerScope]);
  const handleOpenChatCorner = useCallback(() => {
    setChatCornerWidth(readStoredChatCornerWidth());
    setFloatingChatPlacement("right-edge");
    setRightPanelOpen(false);
    setRightPanelDetached(true);
    setChatFocusRequest((token) => token + 1);
  }, []);
  // Discuss on a question card and a finding's chat action seed a fresh chat
  // in the right-edge panel, keeping the document in view; on a phone, where
  // there is no panel, Chat takes the center as other seeds do
  // (`g5.workbench.chat-actions`).
  const handleDiscussInChat = useCallback(
    (text: string, reference?: ChatProjectReference) => {
      if (handleSeedChatPrompt(text, reference) && !isMobile)
        handleOpenChatCorner();
    },
    [handleOpenChatCorner, handleSeedChatPrompt, isMobile],
  );
  // A bubble of the stack (ui g5.workspace.ui.chat-bubble-stack) opens its
  // chat exactly as the corner button opens Chat, with that chat active.
  const handleOpenChatBubble = useCallback(
    (sessionId: string) => {
      handleViewSession(sessionId, undefined, false, false);
      handleOpenChatCorner();
    },
    [handleOpenChatCorner, handleViewSession],
  );
  // While Chat is the center surface the stack stays and a bubble opens its
  // chat there.
  const handleOpenChatBubbleInCenter = useCallback(
    (sessionId: string) => handleViewSession(sessionId),
    [handleViewSession],
  );
  // The durable gate alone decides whether the corner button can act: a
  // stopped VM still opens Chat (its composer explains), while Base, a reader
  // and a read-only or freezing policy keep the button visible but disabled.
  const chatCornerGateKind = liveGateKind({
    isOwner: isWorkspaceOwner,
    isBase: currentWorkspaceIsBase,
    readOnly: workspaceDetailData?.readOnly === true,
    freezing: workspaceDetailData?.mutationPolicyTransition === "freezing",
    vmStatus: "running",
  });
  const chatCornerDisabledReason =
    chatCornerGateKind === "live"
      ? null
      : liveGateReason(chatCornerGateKind, "chat", { workspaceNoun });
  // Activate the center Chat surface: dock a popped-out panel and restore the
  // active session's URL hint so a reload returns to the conversation rather
  // than the last content tab. Shared by the Chat tab and, while the tab strip
  // is hidden, the title-bar Chat toggle (workspaces
  // g5.workspace.ui.center-tabs).
  const handleSelectCenterChat = useCallback(() => {
    setRightPanelDetached(false);
    if (activeChatSessionId) {
      handleViewSession(activeChatSessionId);
    } else {
      setRightPanelOpen(true);
      updateViewParams({ view: null });
    }
  }, [activeChatSessionId, handleViewSession, updateViewParams]);
  const handleToggleChatPanel = useCallback(() => {
    if (
      !workspaceChatAvailable ||
      !workspaceViewScope.active ||
      workspaceViewScopeRef.current !== workspaceViewScope
    )
      return;
    if (centerChatActive) {
      handleCloseChatPanel();
    } else {
      handleSelectCenterChat();
    }
  }, [
    centerChatActive,
    handleCloseChatPanel,
    handleSelectCenterChat,
    workspaceChatAvailable,
    workspaceViewScope,
  ]);
  const resetTaskSelection = useCallback(() => {
    setSelectedTaskId(null);
  }, []);
  useEffect(() => {
    resetTaskSelection();
    setRightPanelDetached(false);
    setRightPanelOpen(true);
    setActiveChatSessionId(null);
    pendingTaskChatSelectionRef.current = null;
    pendingSessionSelectionRef.current = null;
    pendingSessionRevealRef.current = null;
    suppressedSessionHintRef.current = null;
  }, [resetTaskSelection, workspaceId]);
  useEffect(() => {
    if (!currentWorkspaceIsBase) return;
    setRightPanelDetached(false);
    setRightPanelOpen(false);
    setActiveChatSessionId(null);
    const chatView = searchParams.get("view") === "chat";
    if (searchParams.has("session") || chatView) {
      updateViewParams({ session: null, ...(chatView ? { view: null } : {}) });
    }
  }, [currentWorkspaceIsBase, searchParams, updateViewParams]);
  const handleCloseRemoveWorkspaceDialog = useCallback(() => {
    if (deleteWorkspace.isLoading) return;
    deleteWorkspace.reset();
    setRemoveWorkspaceRequest(null);
  }, [deleteWorkspace]);
  const handleConfirmRemoveWorkspace = useCallback(async () => {
    const target = removeWorkspaceRequest;
    if (!target || !resolvedOrgId || !org) return;

    await deleteWorkspace.mutateAsync(undefined);
    queryClient.removeQueries({ queryKey: ["workspace", target.id] });
    setRemoveWorkspaceRequest(null);

    if (workspaceTabs.enabled) {
      // The strip shows the closed tab's left neighbour, else Home
      // (g5.workbench.tabs).
      if (target.id === workspaceId) resetTaskSelection();
      workspaceTabs.closeTab({ orgSlug: org, name: target.name });
      return;
    }
    if (target.id === workspaceId) {
      resetTaskSelection();
      // `allWorkspacesData` is the pre-delete list, which is sufficient: the
      // `.find` already excludes the deleted workspace, and any other entry is
      // a valid navigation target. `useDeleteWorkspace.onSuccess` invalidates
      // the list, so the cache refreshes without a second concurrent GET here.
      const nextWorkspace = allWorkspacesData.find(
        (candidate) => candidate.id !== target.id,
      );
      if (nextWorkspace) {
        navigate(`/${org}/${nextWorkspace.name}`);
      } else {
        navigate(`/${org}`);
      }
    }
  }, [
    allWorkspacesData,
    deleteWorkspace,
    navigate,
    org,
    queryClient,
    removeWorkspaceRequest,
    resetTaskSelection,
    workspaceTabs,
    resolvedOrgId,
    workspaceId,
  ]);
  // Comments are rarely used, so documents opened in the center panel default to
  // comment visibility off; users opt in per session via the annotations toggle.
  const [showComments, setShowComments] = useState(false);
  // scrollToIntentRecord uses a counter to guarantee uniqueness — React skips
  // re-renders when the new state value === old value, so navigating to "I1"
  // twice in a row would silently no-op without the counter.
  const scrollSeqRef = useRef(0);
  const [scrollToIntentRecord, setScrollToIntentRecordRaw] = useState<{
    label: string;
    seq: number;
  } | null>(null);
  const setScrollToIntentRecord = useCallback((label: string | null) => {
    if (!label) {
      setScrollToIntentRecordRaw(null);
      return;
    }
    scrollSeqRef.current += 1;
    setScrollToIntentRecordRaw({ label, seq: scrollSeqRef.current });
  }, []);
  const [navigationOrigin, setNavigationOrigin] =
    useState<NavigationOrigin | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const closeSettings = useCallback(() => {
    setSettingsOpen(false);

    if (isScmSettingsReturn(searchParams)) {
      const next = new URLSearchParams(searchParams);
      next.delete(SCM_OAUTH_SETTINGS_PARAM);
      const search = next.toString();
      navigate(
        `${location.pathname}${search ? `?${search}` : ""}${location.hash}`,
        {
          replace: true,
        },
      );
    }
  }, [location.hash, location.pathname, navigate, searchParams]);
  const [notificationsOpen, setNotificationsOpen] = useState(false);
  const [createProjectDocModal, setCreateProjectDocModal] = useState<{
    open: boolean;
    title: string;
    defaultFolderPath?: string;
    initialUploadFiles?: readonly File[];
    initialMode?: "plain" | "upload" | "transcript";
  }>({
    open: false,
    title: "Create Doc",
  });
  // Shared opener for the Create Project Doc modal — used by the sidebar's
  // create action and the status band's Requirements "Upload docs" CTA
  // (shell D42). The `launchContext` prop main passed here is gone with the
  // Create View retirement: project-docs is now the modal's only launch path,
  // so there is no context left to select.
  const handleOpenCreateProjectDoc = useCallback(() => {
    setCreateProjectDocModal({
      open: true,
      title: "Create Doc",
    });
  }, []);
  // Views-section "Create explainer" entries (left-sidebar D34): the selected
  // The request uses workspace files as its context.
  const [createExplainerModal, setCreateExplainerModal] = useState<{
    open: boolean;
  }>({
    open: false,
  });
  // The ribbon's Build mock dialog (g5.workbench.mock).
  const [buildMockOpen, setBuildMockOpen] = useState(false);

  useEffect(() => {
    setCreateExplainerModal({ open: false });
    setBuildMockOpen(false);
  }, [workspaceId]);

  // ---------------------------------------------------------------------------
  // Tab persistence
  // ---------------------------------------------------------------------------

  const { readPersistedTabs, writePersistedTabs } =
    useTabPersistence(workspaceId);
  const tabPersistTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // The write the debounce still owes; run when the page leaves the
  // workspace, by a switch within this mounted page or an unmount, so a
  // selection made just before a switch is what the return restores
  // (g5.workbench.tabs).
  const pendingTabWriteRef = useRef<(() => void) | null>(null);
  // What a return from the tab strip restored from the strip's snapshot:
  // the selected center tab (route hydration leaves it in place) and
  // whether Chat was in front (read by the landing).
  const restoredSelectionRef = useRef<string | null>(null);
  // The path and raw `?branch=` hint the selection was restored at; the
  // guard holds while the visit stays on them (the hint validated later
  // re-resolves the same route) and expires for good once either changes,
  // so a later route (a link, a row — back to that same path included, or
  // the same file on another branch) resolves as usual.
  const restoredRouteRef = useRef<{
    pathname: string;
    branchHint: string | null;
  } | null>(null);
  const restoredChatCenterRef = useRef(false);
  const flushPersistedTabs = useCallback(() => {
    if (tabPersistTimerRef.current) clearTimeout(tabPersistTimerRef.current);
    tabPersistTimerRef.current = null;
    const write = pendingTabWriteRef.current;
    pendingTabWriteRef.current = null;
    write?.();
  }, []);
  useEffect(() => flushPersistedTabs, [flushPersistedTabs]);

  // Every workspace shown by this mounted page restores its OWN persisted
  // tabs (keyed by workspace id), so a switch never leaks doc, branch, live
  // file or overlay ids from the previous workspace: the in-memory tabs are
  // replaced wholesale by the destination's record, or cleared when it has
  // none. Only the first workspace may open the getting-started guide.
  useEffect(() => {
    if (!workspaceId) return;

    const previousWorkspaceId = previousTabWorkspaceIdRef.current;
    if (previousWorkspaceId === workspaceId) return;

    previousTabWorkspaceIdRef.current = workspaceId;
    flushPersistedTabs();
    pendingTabUrlRef.current = null;
    lastResolvedRouteKeyRef.current = null;
    consumedViewParamRef.current = null;
    mirroredViewParamRef.current = null;
    prevViewParamRef.current = null;
    setScrollToIntentRecord(null);
    setNavigationOrigin(null);

    // A return from the tab strip (g5.workbench.tabs) restores the snapshot
    // the strip kept for this session — the open tabs, the selection and
    // Chat's placement — over the shared localStorage record, which another
    // browser window may have written since. A fresh open restores the
    // persisted record.
    const snapshotTabs =
      activeTabSnapshot && activeTabSnapshot.workspaceId === workspaceId
        ? dedupeTabsById(knownTabsOnly(activeTabSnapshot.tabs as TabItem[]))
        : [];
    restoredSelectionRef.current = null;
    restoredRouteRef.current = null;
    restoredChatCenterRef.current = false;
    const persisted = readPersistedTabs();
    const persistedTabs = persisted
      ? dedupeTabsById(knownTabsOnly(persisted.tabs))
      : [];
    if (snapshotTabs.length > 0 && activeTabSnapshot) {
      const activeTabId =
        activeTabSnapshot.activeTabId &&
        snapshotTabs.some(
          (tab) => getTabId(tab) === activeTabSnapshot.activeTabId,
        )
          ? activeTabSnapshot.activeTabId
          : getTabId(snapshotTabs[0]);
      setOpenTabs(snapshotTabs);
      setSelectedTab(activeTabId);
      restoredSelectionRef.current = activeTabId;
      restoredChatCenterRef.current = activeTabSnapshot.chatCenter;
    } else if (persistedTabs.length > 0) {
      const activeTabId =
        persisted?.activeTabId &&
        persistedTabs.some((tab) => getTabId(tab) === persisted.activeTabId)
          ? persisted.activeTabId
          : getTabId(persistedTabs[0]);
      setOpenTabs(persistedTabs);
      setSelectedTab(activeTabId);
    } else if (previousWorkspaceId === null) {
      if (
        !docPath &&
        !filePath &&
        !isIntentLogPath &&
        !routeTaskId &&
        !identityRoutePath
      ) {
        let alreadyOpened = false;
        try {
          alreadyOpened =
            localStorage.getItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY) ===
            "1";
        } catch {
          alreadyOpened = false;
        }

        const defaultGuideTab = guideDocTab(DEFAULT_GUIDE_DOC_ID);
        if (!alreadyOpened && defaultGuideTab) {
          try {
            localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
          } catch {
            // The tab can still open even when browser storage is unavailable.
          }
          setOpenTabs([defaultGuideTab]);
          setSelectedTab(getTabId(defaultGuideTab));
        } else {
          setOpenTabs([]);
          setSelectedTab(null);
        }
      } else {
        setOpenTabs([]);
        setSelectedTab(null);
      }
    } else {
      setOpenTabs([]);
      setSelectedTab(null);
    }

    setTabStateWorkspaceId(workspaceId);
  }, [
    activeTabSnapshot,
    docPath,
    filePath,
    flushPersistedTabs,
    identityRoutePath,
    routeTaskId,
    readPersistedTabs,
    setScrollToIntentRecord,
    isIntentLogPath,
    workspaceId,
  ]);

  // Persist tabs on change (debounced). The strip's session snapshot
  // (g5.workbench.tabs) is written alongside, unfiltered: a return shows
  // exactly what was left, a reload what the persisted record keeps.
  useEffect(() => {
    if (!workspaceId || tabStateWorkspaceId !== workspaceId) return;
    const write = () => {
      writePersistedTabs({
        tabs: openTabs,
        activeTabId: selectedTab,
      });
      if (activeStripTab) {
        rememberTabSnapshot(activeStripTab, {
          workspaceId,
          tabs: openTabs,
          activeTabId: selectedTab,
          chatCenter: centerChatActive,
        });
      }
    };
    pendingTabWriteRef.current = write;
    if (tabPersistTimerRef.current) clearTimeout(tabPersistTimerRef.current);
    tabPersistTimerRef.current = setTimeout(() => {
      tabPersistTimerRef.current = null;
      pendingTabWriteRef.current = null;
      write();
    }, TAB_PERSISTENCE_DEBOUNCE_MS);
    return () => {
      if (tabPersistTimerRef.current) clearTimeout(tabPersistTimerRef.current);
    };
  }, [
    activeStripTab,
    centerChatActive,
    openTabs,
    rememberTabSnapshot,
    selectedTab,
    tabStateWorkspaceId,
    workspaceId,
    writePersistedTabs,
  ]);

  const selectedTabItem = useMemo(
    () => openTabs.find((tab) => getTabId(tab) === selectedTab) ?? null,
    [openTabs, selectedTab],
  );
  selectedTabItemRef.current = selectedTabItem;
  // The org/workspace admin tabs (Settings / Workers / Members) are full-content
  // surfaces with no execution context.
  const isAdminCenterTab = isAdminCenterTabItem(selectedTabItem);
  // Legacy compatibility value for the removed rail action highlight. The
  // current stacked sidebar ignores it while shell callers are being cleaned up.
  const activeRailAction: "workers" | "settings" | "intent_log" | null =
    selectedTabItem?.type === "worker_hosts"
      ? "workers"
      : selectedTabItem?.type === "workspace_settings"
        ? "settings"
        : selectedTabItem?.type === "intent_log"
          ? "intent_log"
          : null;
  // Remember the most recent non-admin tab for the legacy content-section
  // compatibility callback; the current stacked sidebar does not use rail-only
  // navigation.
  const lastNonAdminTabIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (selectedTab && !isAdminCenterTab) {
      lastNonAdminTabIdRef.current = selectedTab;
    }
  }, [selectedTab, isAdminCenterTab]);
  const selectedDoc = useMemo(() => {
    if (selectedTabItem?.type !== "doc") return null;
    return (
      docs.find((candidate) => candidate.id === selectedTabItem.docId) ?? null
    );
  }, [docs, selectedTabItem]);
  const activeChatProjectReference = useMemo(
    () => chatReferenceForTab(selectedTabItem, docs, activeBranchId),
    [activeBranchId, docs, selectedTabItem],
  );
  const chatMessageContext = useMemo<ChatMessageContext | undefined>(() => {
    const pathForTab = (tab: TabItem | null): string | null => {
      if (!tab) return null;
      switch (tab.type) {
        case "file":
          return tab.filePath;
        case "doc": {
          const doc = docs.find((candidate) => candidate.id === tab.docId);
          return workspaceReadableDocPath(doc?.filePath) ?? null;
        }
        case "guide_doc":
        case "intent_log":
        case "pull_requests":
          return null;
        case "ontology_changes":
        case "workspace_changes":
        case "source_control_diff":
        case "source_control_change_graph":
        case "source_control_commit_diff":
          return null;
        case "workspace_settings":
        case "workspace_terminal":
          return null;
        case "worker_hosts":
          return null;
        case "org_members":
          return null;
        case "org_approvals":
          return null;
        case "org_github_app":
          return null;
        case "org_admin":
          return null;
        case "workspace_approvals":
          return null;
        case "workspace_review":
          return null;
        case "workspace_details":
          return null;
        case "workspace_activity":
        case "external_issue":
          return null;
      }
    };

    const activeFilePath = pathForTab(selectedTabItem) ?? undefined;
    const openFilePaths = Array.from(
      new Set(
        openTabs.map(pathForTab).filter((path): path is string => !!path),
      ),
    ).slice(0, MAX_CHAT_OPEN_FILE_CONTEXT);

    if (!activeFilePath && openFilePaths.length === 0) {
      return undefined;
    }

    return {
      activeFilePath,
      openFilePaths,
    };
  }, [docs, openTabs, selectedTabItem]);
  const requestedContentBranchId = useMemo(() => {
    if (selectedTabItem?.type === "doc") {
      const persistedDocBranchId = selectedDoc?.repoId ?? "";
      return isCurrentWorkspaceBranchId(persistedDocBranchId)
        ? persistedDocBranchId
        : docsBranchId || persistedDocBranchId;
    }
    if (selectedTabItem?.type === "file") {
      return selectedTabItem.branchId ?? "";
    }
    return "";
  }, [
    docsBranchId,
    isCurrentWorkspaceBranchId,
    selectedDoc?.repoId,
    selectedTabItem,
  ]);
  const contentBranchResolution = useMemo(() => {
    return resolveContentBranch(
      workspace.status === "ok" ? allSortedBranches : undefined,
      explicitContentBranchId,
      docsBranchId,
      requestedContentBranchId,
    );
  }, [
    allSortedBranches,
    docsBranchId,
    explicitContentBranchId,
    requestedContentBranchId,
    workspace.status,
  ]);
  const effectiveContentBranchId =
    contentBranchResolution.status === "resolved"
      ? contentBranchResolution.branchId
      : "";
  const fileTree = useFileTree(
    workspaceId,
    effectiveContentBranchId,
    undefined,
    -1,
    {
      enabled: shouldLoadFileTree,
      authorityKey: workspaceReadAuthority,
    },
  );
  const fileTreeEntries = useMemo(
    () => (fileTree.status === "ok" ? fileTree.data : []),
    [fileTree],
  );
  const chatProjectReferences = useMemo(
    () =>
      dedupeChatProjectReferences([
        ...docs.map(chatReferenceForDoc),
        ...collectFileChatReferences(fileTreeEntries, effectiveContentBranchId),
      ]),
    [docs, effectiveContentBranchId, fileTreeEntries],
  );
  const selectExplicitContentBranch = useCallback(
    (branchId: string) => {
      if (!isCurrentWorkspaceBranchId(branchId)) return;
      setExplicitContentBranchId(branchId);
      if (codeBranchIds.includes(branchId)) {
        setActiveBranchId(branchId);
      }
    },
    [codeBranchIds, isCurrentWorkspaceBranchId],
  );
  const selectedContentBranch = useMemo(
    () =>
      allSortedBranches.find((b) => b.repoId === effectiveContentBranchId) ??
      null,
    [allSortedBranches, effectiveContentBranchId],
  );
  const workspaceChanges = useWorkspaceChanges(workspaceId);
  const sourceControlDirtyFiles = useMemo<SourceControlDirtyFile[]>(() => {
    if (workspaceChanges.status !== "ok") return [];
    return workspaceChanges.data.members
      .flatMap((member) =>
        (member.dirty ?? []).map((change) => ({
          repoId: member.repo_id,
          repoName: member.root_path || member.repo_id,
          file: change.path,
        })),
      )
      .sort((a, b) => a.file.localeCompare(b.file));
  }, [workspaceChanges]);
  // ---------------------------------------------------------------------------
  // Tab management callbacks
  // ---------------------------------------------------------------------------

  /** Build a full URL path for a tab item, e.g. /acme/my-workspace/file/src/main.rs */
  const buildTabUrl = useCallback(
    (tab: TabItem): string | null => {
      if (!workspaceRoutePath) return null;
      const suffix = tabToPath(tab);
      if (!suffix) return null;
      return `${workspaceRoutePath}${suffix}`;
    },
    [workspaceRoutePath],
  );
  const isSelectableBranchId = useCallback(
    (branchId: string) =>
      sortedBranches.some((branch) => branch.repoId === branchId),
    [sortedBranches],
  );

  /** Navigate the URL bar to reflect the given tab (or workspace root if null).
   *  Content tabs own the primary route; only tab-neutral query params carry over. */
  // Query parameters an opener attached to a route-less tab (the Approvals
  // phase filter, g5.workbench.gates), merged into the `?view=` mirror that
  // names that tab — not one that still names the previously selected tab.
  const pendingTabSearchParamsRef = useRef<{
    view: string | null;
    params: Record<string, string | null>;
  } | null>(null);
  const syncUrlToTab = useCallback(
    (
      tab: TabItem | null,
      preserveSessionHint = false,
      updates?: Record<string, string | null>,
    ) => {
      if (!workspaceRoutePath) return;
      if (!tab) {
        const workspaceUrl = workspaceRoutePath;
        pendingTabUrlRef.current = { url: workspaceUrl, key: location.key };
        navigate(workspaceUrl, { replace: true });
        return;
      }
      const url = buildTabUrl(tab);
      if (!url) {
        const keepsPath = tab.type === "workspace_changes" && !!routeTaskId;
        const workspaceUrl = workspaceRoutePath;
        const needsRoot = !keepsPath && location.pathname !== workspaceUrl;
        // Singleton content tabs leave Chat just like path-based tabs. Drop
        // its hint so a later link to the same session can reveal it again.
        const clearsSession =
          !preserveSessionHint && searchParams.has("session");
        const contentUpdates = clearsSession
          ? { session: null, ...updates }
          : updates;
        if (!contentUpdates && !needsRoot) return;
        // The `?view=` mirror below runs on the stale pre-navigation params
        // once the tab is selected; it merges these so they survive it.
        if (contentUpdates) {
          pendingTabSearchParamsRef.current = {
            view: tabToViewParam(tab),
            params: contentUpdates,
          };
        }
        if (needsRoot) {
          // A write to the root from a content path is awaited like a
          // content write: the mirror would otherwise name the tab on the
          // stale content path, and the consumer then drop that hint there.
          const params = new URLSearchParams(searchParams);
          for (const [key, value] of Object.entries(contentUpdates ?? {})) {
            if (value === null) params.delete(key);
            else params.set(key, value);
          }
          const qs = params.toString();
          pendingTabUrlRef.current = {
            url: `${workspaceUrl}${qs ? "?" + qs : ""}`,
            key: location.key,
          };
        }
        updateViewParams(
          contentUpdates ?? {},
          needsRoot ? workspaceUrl : undefined,
        );
        return;
      }

      // Carry forward tab-neutral query params (branch). A `session` hint is
      // NOT content-tab-neutral: `searchParams` here is the pre-navigation
      // value, so a stale task-chat `?session=` would be re-attached to the
      // new tab's route and re-hydrate the task selection for the
      // previously-selected task. Drop it on user-driven content-tab syncs;
      // route hydration preserves an explicit session URL so its authorized
      // detail can restore the conversation after reload.
      const params = new URLSearchParams(searchParams);
      // Remove tab-specific params that should reset on tab switch.
      if (!preserveSessionHint) {
        params.delete("session");
      }
      params.delete("tab");
      params.delete("editor");
      // A center-view hint names a singleton tab; this content tab replaces
      // it now rather than through the `?view=` mirror a render later,
      // which would rewrite the URL the deep-link effect then waits for.
      const view = params.get("view");
      if (view && CENTER_VIEW_PARAMS.has(view)) params.delete("view");
      if (tab.type === "doc") {
        // Project docs always live on the dedicated docs branch. Carrying a
        // stale non-doc branch hint into /doc/... routes can trap the editor
        // in "Preparing draft…" after reload/navigation.
        params.delete("branch");
      } else if (tab.type === "file" && tab.branchId) {
        params.set("branch", tab.branchId);
      }
      for (const [key, value] of Object.entries(updates ?? {})) {
        if (value === null) params.delete(key);
        else params.set(key, value);
      }
      const qs = params.toString();
      const nextUrl = `${url}${qs ? "?" + qs : ""}`;
      pendingTabUrlRef.current = { url: nextUrl, key: location.key };
      navigate(nextUrl, { replace: true });
    },
    [
      workspaceRoutePath,
      navigate,
      buildTabUrl,
      routeTaskId,
      searchParams,
      location.pathname,
      location.key,
      updateViewParams,
    ],
  );

  const openTab = useCallback(
    (
      item: TabItem,
      options?: {
        preserveSessionHint?: boolean;
        /** Query parameters the opened tab's URL carries (null deletes). */
        searchParams?: Record<string, string | null>;
      },
    ) => {
      item = currentFileTab(item, docs, workspaceId);
      if (item.type === "file" || item.type === "doc") {
        item = {
          ...item,
          liveAuthorityKey: ownerHasRunningAttachment
            ? workspaceReadAuthority
            : undefined,
        };
      }
      // Content opened from the sidebar, a deep link, or another center view
      // replaces Chat in the center. The persistent Chat tab remains available
      // as the return path.
      if (!options?.preserveSessionHint) {
        hideChatForContent();
      }

      const ownerBranchId = branchIdForTabItem(
        item,
        docsBranchId,
        activeBranchId,
      );
      if (
        ownerBranchId &&
        activeBranchId !== ownerBranchId &&
        isSelectableBranchId(ownerBranchId)
      ) {
        setActiveBranchId(ownerBranchId);
      }
      const tabId = getTabId(item);
      setOpenTabs((prev) => upsertTab(prev, item));
      // Skip state changes if this tab is already selected — prevents the
      // deep-link useEffect (which re-fires when syncUrlToTab changes the
      // URL) from clearing scrollToIntentRecord that was just set by the
      // onNavigate handler in the same React batch.
      if (selectedTabRef.current !== tabId) {
        setSelectedTab(tabId);
        // Clear stale intent-scroll state when switching tabs. If the tab
        // switch was triggered by intent navigation, the caller will re-set
        // this immediately after (in the same React batch), so the new
        // value wins.
        setScrollToIntentRecord(null);
      }
      const selectedFile =
        item.type === "source_control_diff"
          ? item.selectedFileKey?.split("\u0000")
          : undefined;
      syncUrlToTab(
        item,
        options?.preserveSessionHint,
        selectedFile
          ? {
              mode: "base",
              reviewRepo: selectedFile[0],
              reviewPath: selectedFile[1],
              comment: null,
              ...options?.searchParams,
            }
          : options?.searchParams,
      );
    },
    [
      workspaceId,
      activeBranchId,
      docsBranchId,
      docs,
      hideChatForContent,
      isSelectableBranchId,
      syncUrlToTab,
      setScrollToIntentRecord,
      ownerHasRunningAttachment,
      workspaceReadAuthority,
    ],
  );

  // Restore old document tabs once their file identities are available.
  useEffect(() => {
    const resolve = (item: TabItem) => {
      const resolved = currentFileTab(item, docs, workspaceId);
      // A legacy unqualified tab can join the shared identity only after
      // content ownership has resolved, never from sidebar presentation order.
      return resolved.type === "file" &&
        !resolved.branchId &&
        getTabId(item) === selectedTab &&
        contentBranchResolution.status === "resolved"
        ? { ...resolved, branchId: contentBranchResolution.branchId }
        : resolved;
    };
    setOpenTabs((previous) => {
      const next = dedupeTabsById(previous.map(resolve));
      return JSON.stringify(next) === JSON.stringify(previous)
        ? previous
        : next;
    });
    const selected = openTabs.find((t) => getTabId(t) === selectedTab);
    if (selected) {
      const id = getTabId(resolve(selected));
      if (id !== selectedTab) setSelectedTab(id);
    }
  }, [workspaceId, docs, contentBranchResolution, openTabs, selectedTab]);

  const openPullRequestsTab = useCallback(() => {
    openTab({ type: "pull_requests", name: "Pull Requests" });
  }, [openTab]);

  const openSourceControlDiffTab = useCallback(
    (target?: SourceControlDirtyFile) => {
      if (target) {
        openTab(
          {
            ...SOURCE_CONTROL_DIFF_TAB,
            selectedFileKey: diffFileKey(target),
          },
          { searchParams: { mode: "uncommitted" } },
        );
      } else {
        openTab({ type: "workspace_changes", name: "Current changes" });
      }
    },
    [openTab],
  );

  // Returns whether the tab opened: Base has no Diff Graph.
  const openSourceControlChangeGraphTab = useCallback((): boolean => {
    if (workspace.status !== "ok" || workspace.data.isBase) return false;
    openTab({
      type: "source_control_change_graph",
      name: "Diff Graph",
    });
    return true;
  }, [openTab, workspace]);

  const openSourceControlCommitTab = useCallback(
    (branch: BranchSummary, commit: CommitHistoryEntry) => {
      openTab(
        commitDiffTab(branch, commit.sha, commit.summary || commit.shortSha),
      );
    },
    [openTab],
  );

  const [switchPromptTaskId, setSwitchPromptTaskId] = useState<string | null>(
    null,
  );

  // Open (or focus) the Workspace Settings center tab. Settings/admin tabs are
  // singletons (one per workspace/org) — `getTabId` returns a fixed id so
  // re-clicking the rail focuses the existing tab instead of duplicating it.
  // These tabs do NOT own a URL route (`tabToPath` returns null), so `openTab`
  // leaves the URL on the current workspace route and never activates a rail
  // sidebar section.
  const openWorkspaceSettingsTab = useCallback(() => {
    openTab({ type: "workspace_settings", name: "Settings" });
  }, [openTab]);

  const openWorkspaceTerminalTab = useCallback(() => {
    openTab({ type: "workspace_terminal", name: "Terminal" });
  }, [openTab]);

  // Open (or focus) the org Worker-Hosts admin center tab.
  const openWorkerHostsTab = useCallback(() => {
    openTab({ type: "worker_hosts", name: "Workers" });
  }, [openTab]);

  // Open (or focus) the org Members admin center tab.
  const openOrgMembersTab = useCallback(() => {
    openTab({ type: "org_members", name: "Members" });
  }, [openTab]);

  // Open (or focus) the org approval-definition LIBRARY center tab
  // (`?view=approval-definitions`).
  const openOrgApprovalsTab = useCallback(() => {
    openTab({ type: "org_approvals", name: "Approval Definitions" });
  }, [openTab]);

  // Open (or focus) the org GitHub App binding center tab
  // (`?view=github-app`).
  const openOrgGithubAppTab = useCallback(() => {
    openTab({ type: "org_github_app", name: "GitHub App" });
  }, [openTab]);

  // Open (or focus) the org Admin hub center tab (`?view=admin`).
  const openOrgAdminTab = useCallback(() => {
    openTab({ type: "org_admin", name: "Admin" });
  }, [openTab]);

  // Open (or focus) the workspace Approvals center tab (`?view=approvals`).
  // Approvals is shared by the whole workspace, regardless of phase.
  const openWorkspaceApprovalsTab = useCallback(() => {
    openTab({ type: "workspace_approvals", name: "Approvals" });
  }, [openTab]);

  // Open (or focus) the read-only Workspace Details center tab.
  const openWorkspaceDetailsTab = useCallback(() => {
    openTab({ type: "workspace_details", name: "Workspace Details" });
  }, [openTab]);

  // Open (or focus) the workspace Activity center tab (`?view=activity`).
  const openWorkspaceActivityTab = useCallback(() => {
    openTab(WORKSPACE_ACTIVITY_TAB);
  }, [openTab]);

  // External issues (`external_issues` instance feature, fail-closed): the
  // top bar's Activity entrypoint and primary-issue chip.
  const externalIssuesFeatureStatus =
    useExternalIssuesFeature(resolvedOrgId).status;
  const externalIssuesEnabled = externalIssuesFeatureStatus === "enabled";
  const workspaceIssueLinks = useWorkspaceIssueLinks(
    workspaceId,
    externalIssuesEnabled,
  );
  const [importTaskWorkspaceId, setImportTaskWorkspaceId] = useState<
    string | null
  >(null);
  const [detachIssue, setDetachIssue] = useState<WorkspaceIssueLinkDTO | null>(
    null,
  );
  const [createWorkspaceIssueRef, setCreateWorkspaceIssueRef] = useState<
    string | null
  >(null);
  const unlinkWorkspaceIssue = useUnlinkWorkspaceIssue(workspaceId);
  const issueWriteBlockedReason = reviewExcluded
    ? !reviewIdentityKnown && !isReviewCopy
      ? REVIEW_IDENTITY_PENDING_REASON
      : REVIEW_COPY_DETAILS_REASON
    : null;
  // Base takes no new issue links (g5.issue.base-links): Import task and
  // quick-add are offered disabled with the reason; Detach stays available.
  const issueLinkBlockedReason = currentWorkspaceIsBase
    ? BASE_ISSUE_LINK_REASON
    : null;
  const issueLinkCreationBlockedReason =
    issueWriteBlockedReason ?? issueLinkBlockedReason;
  const canWriteIssueLinks = isWorkspaceOwner && hasScope("workspaces:write");
  // Mark as done / Reopen on the issue page: the viewer's own GitHub write
  // (g5.external-issues.transition); the row and page re-read from the
  // answer, and a refusal is announced.
  const githubConnection = useGithubConnection(externalIssuesEnabled).status;
  const changeIssueState = useChangeIssueState(workspaceId);
  const changeLinkedIssueState = useCallback(
    async (link: WorkspaceIssueLinkDTO) => {
      const state = nextIssueState(link);
      try {
        const result = await changeIssueState.mutateAsync({
          linkId: link.id,
          state,
        });
        if (result.outcome === "unknown") {
          pushToast(
            "warning",
            `${link.displayKey}: change not confirmed`,
            ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE,
          );
        }
      } catch (error) {
        pushToast(
          "error",
          state === "closed"
            ? `Could not mark ${link.displayKey} as done`
            : `Could not reopen ${link.displayKey}`,
          describeIssueStateError(error, state),
        );
      }
    },
    [changeIssueState, pushToast],
  );
  const openLinkedIssue = useCallback(
    (link: WorkspaceIssueLinkDTO) =>
      openTab(externalIssueTab(link.locator, link.displayKey)),
    [openTab],
  );
  const openImportTaskPicker = useCallback(
    () => setImportTaskWorkspaceId(workspaceId),
    [workspaceId],
  );
  // Sidebar Issues & tasks props (g5.workbench.issues), memoised so the
  // memoised LeftSidebar is not re-rendered by a fresh object each render.
  const issuesSection = useMemo(
    () => ({
      viewerOwnsWorkspace: canWriteIssueLinks,
      writeBlockedReason: issueWriteBlockedReason,
      linkBlockedReason: issueLinkBlockedReason,
      onOpenIssue: openLinkedIssue,
      onImportTask: openImportTaskPicker,
    }),
    [
      canWriteIssueLinks,
      issueLinkBlockedReason,
      issueWriteBlockedReason,
      openLinkedIssue,
      openImportTaskPicker,
    ],
  );
  const primaryIssueLink =
    workspaceIssueLinks.status === "ok"
      ? (workspaceIssueLinks.data.links.find(
          (link) => link.role === "primary",
        ) ?? null)
      : null;
  const topBarPrimaryIssue = useMemo<TopBarPrimaryIssue | null>(() => {
    if (!externalIssuesEnabled || !primaryIssueLink) return null;
    return {
      displayKey: primaryIssueLink.displayKey,
      title: primaryIssueLink.issue?.title ?? null,
      state: primaryIssueLink.issue?.state ?? null,
      onOpen: () =>
        openTab(
          externalIssueTab(
            primaryIssueLink.locator,
            primaryIssueLink.displayKey,
          ),
        ),
    };
  }, [externalIssuesEnabled, openTab, primaryIssueLink]);

  // Open (or focus) a review copy's own page: My review and the source's
  // reviews. Only a copy has one.
  const openWorkspaceReviewTab = useCallback(() => {
    if (!reviewOrigin) return;
    // Names its own hint in its write: the Review landing can open this tab
    // and leave a content path in one render, and a stale hint carried to
    // the root then (a `?view=activity` beside a guide link) would not be
    // mirrored over, since the URL had just changed.
    openTab(
      {
        type: "workspace_review",
        name: `Review of ${reviewOrigin.sourceWorkspaceName}`,
      },
      { searchParams: { view: "review" } },
    );
  }, [openTab, reviewOrigin]);
  useEffect(() => {
    openReviewTabRef.current = openWorkspaceReviewTab;
  }, [openWorkspaceReviewTab]);
  const openTaskOutputForTask = useCallback(
    (taskId: string) => {
      const task = findTaskSummaryById(taskId);
      if (!task) {
        setSwitchPromptTaskId(taskId);
        return;
      }
      const tab = taskOutputTabForTask(task);
      if (tab) openTab(tab);
    },
    [findTaskSummaryById, openTab],
  );
  const handleSelectTaskAndOpenOutput = useCallback(
    (taskId: string) => {
      hideChatForContent();
      openTaskOutputForTask(taskId);
      handleSelectTaskAndOpenChat(taskId);
    },
    [hideChatForContent, openTaskOutputForTask, handleSelectTaskAndOpenChat],
  );

  // A review copy's Review tab stays open like Chat: it is where the copy
  // is reviewed from, and every way of closing a tab leaves it alone. Under
  // the alignment layout the timeline is pinned the same way: it is the
  // landing surface (workbench g5.workbench.landing).
  const pinnedTabIds = useMemo<readonly string[]>(
    () => [
      ...(reviewOrigin ? ["workspace-review"] : []),
      WORKSPACE_ACTIVITY_TAB_ID,
    ],
    [reviewOrigin],
  );
  const closeTab = useCallback(
    (tabId: string) => {
      if (pinnedTabIds.includes(tabId)) return;
      setOpenTabs((prev) => {
        const idx = prev.findIndex((t) => getTabId(t) === tabId);
        if (idx === -1) return prev;
        const next = prev.filter((_, i) => i !== idx);

        // If closing the active tab, select nearest neighbor
        setSelectedTab((currentSelected) => {
          if (currentSelected !== tabId) return currentSelected;
          if (next.length === 0) {
            syncUrlToTab(null);
            return null;
          }
          const newIdx = Math.min(idx, next.length - 1);
          const newTab = next[newIdx];
          const ownerBranchId = branchIdForTabItem(
            newTab,
            docsBranchId,
            activeBranchId,
          );
          if (
            ownerBranchId &&
            activeBranchId !== ownerBranchId &&
            isSelectableBranchId(ownerBranchId)
          ) {
            setActiveBranchId(ownerBranchId);
          }
          syncUrlToTab(newTab);
          return getTabId(newTab);
        });

        return next;
      });
    },
    [
      activeBranchId,
      docsBranchId,
      isSelectableBranchId,
      pinnedTabIds,
      syncUrlToTab,
    ],
  );

  const closeTabsBulk = useCallback(
    (anchorTabId: string, scope: "others" | "right") => {
      setOpenTabs((prev) => {
        const next = tabsToKeepForBulkClose(
          prev,
          anchorTabId,
          scope,
          pinnedTabIds,
        );
        if (next.length === prev.length) return prev;

        setSelectedTab((currentSelected) => {
          // The active tab survived the bulk close — keep the selection.
          if (
            currentSelected &&
            next.some((t) => getTabId(t) === currentSelected)
          ) {
            return currentSelected;
          }
          // The active tab was closed: fall back to the anchor tab, which
          // always survives both "others" and "right" operations.
          const anchor =
            next.find((t) => getTabId(t) === anchorTabId) ??
            next[next.length - 1] ??
            null;
          if (!anchor) {
            syncUrlToTab(null);
            return null;
          }
          const ownerBranchId = branchIdForTabItem(
            anchor,
            docsBranchId,
            activeBranchId,
          );
          if (
            ownerBranchId &&
            activeBranchId !== ownerBranchId &&
            isSelectableBranchId(ownerBranchId)
          ) {
            setActiveBranchId(ownerBranchId);
          }
          syncUrlToTab(anchor);
          return getTabId(anchor);
        });

        return next;
      });
    },
    [
      activeBranchId,
      docsBranchId,
      isSelectableBranchId,
      pinnedTabIds,
      syncUrlToTab,
    ],
  );

  const closeOtherTabs = useCallback(
    (tabId: string) => closeTabsBulk(tabId, "others"),
    [closeTabsBulk],
  );

  const closeTabsToRight = useCallback(
    (tabId: string) => closeTabsBulk(tabId, "right"),
    [closeTabsBulk],
  );

  const handleRequestDeleteProjectDoc = useCallback(
    (docId: string) => {
      const doc = docs.find((candidate) => candidate.id === docId);
      setProjectDocDeleteRequest({
        docId,
        name: doc?.name ?? doc?.filePath ?? "this project doc",
      });
    },
    [docs],
  );

  const handleConfirmDeleteProjectDoc = useCallback(
    async (docId: string) => {
      // The confirmation can outlive the VM; deleting needs the owner
      // handle, so the gate is applied again here (`g5.workspace.ui.unavailable`).
      if (!liveEditable) return;
      const doc = docs.find((candidate) => candidate.id === docId);
      // Retained views delete through the doc-scoped view route — the
      // project-docs route rejects view types server-side. This is the
      // type-gated delete affordance for views after their sidebar section
      // was folded into All Docs (left-sidebar D35).
      const isView = doc ? isViewDoc(doc) : false;
      const label = isView ? "view" : "project doc";
      try {
        if (isView) {
          await deleteViewDoc.mutateAsync({ docId });
        } else {
          await deleteDoc.mutateAsync({ docId });
        }
        closeTab(`doc:${docId}`);
        pushToast(
          "info",
          isView ? "View deleted" : "Project doc deleted",
          doc?.name ? `Deleted ${doc.name}.` : `The ${label} was deleted.`,
        );
      } catch (error) {
        pushToast(
          "error",
          `Could not delete ${label}`,
          error instanceof Error
            ? error.message
            : `The ${label} could not be deleted.`,
        );
      }
    },
    [closeTab, deleteDoc, deleteViewDoc, docs, liveEditable, pushToast],
  );

  const selectTab = useCallback(
    (tabId: string) => {
      hideChatForContent();
      setSelectedTab(tabId);
      // Clear stale intent-scroll state when user manually switches tabs
      setScrollToIntentRecord(null);
      // Find the tab item to sync URL
      setOpenTabs((prev) => {
        const tab = prev.find((t) => getTabId(t) === tabId);
        if (tab) {
          const ownerBranchId = branchIdForTabItem(
            tab,
            docsBranchId,
            activeBranchId,
          );
          if (
            ownerBranchId &&
            activeBranchId !== ownerBranchId &&
            isSelectableBranchId(ownerBranchId)
          ) {
            setActiveBranchId(ownerBranchId);
          }
          syncUrlToTab(tab);
        }
        return prev; // no mutation
      });
    },
    [
      activeBranchId,
      docsBranchId,
      hideChatForContent,
      isSelectableBranchId,
      syncUrlToTab,
      setScrollToIntentRecord,
    ],
  );

  // Legacy callback retained for callers from the removed rail-only sidebar.
  // If invoked while an admin tab is active, leave the admin tab and return to
  // the most recent non-admin tab (or the first available one).
  const handleRailContentSectionSelected = useCallback(() => {
    if (!isAdminCenterTabItem(selectedTabItem)) return;
    const isOpen = (id: string | null): boolean =>
      !!id && openTabs.some((t) => getTabId(t) === id);
    let targetId: string | null = isOpen(lastNonAdminTabIdRef.current)
      ? lastNonAdminTabIdRef.current
      : null;
    if (!targetId) {
      const firstNonAdmin = openTabs.find((t) => !isAdminCenterTabItem(t));
      targetId = firstNonAdmin ? getTabId(firstNonAdmin) : null;
    }
    if (targetId) {
      selectTab(targetId);
    } else {
      // No non-admin tab exists (e.g. the admin tab was opened as the only tab
      // via a deep-link). Deselect so the center falls back to its empty/welcome
      // state.
      setSelectedTab(null);
    }
  }, [selectedTabItem, openTabs, selectTab]);

  const openedTaskRouteRef = useRef<string | null>(null);

  // ---------------------------------------------------------------------------
  // Deep-link resolution
  // ---------------------------------------------------------------------------

  useEffect(() => {
    if (!workspaceId) return;

    const currentUrl = `${location.pathname}${location.search}`;
    const pending = pendingTabUrlRef.current;
    if (pending) {
      if (pending.url === currentUrl || location.key !== pending.key) {
        // Landed, or superseded by another navigation that landed instead.
        pendingTabUrlRef.current = null;
      } else {
        return;
      }
    }

    const branchHint = searchParams.get("branch");
    const validBranchHint = isCurrentWorkspaceBranchId(branchHint)
      ? branchHint
      : null;
    if (
      branchHint &&
      workspace.status === "ok" &&
      allSortedBranches.length > 0 &&
      !validBranchHint
    ) {
      updateViewParams({ branch: null });
    }
    const routeKey = isIntentLogPath
      ? "intent-log"
      : docPath
        ? `doc:${docPath}`
        : filePath
          ? `file:${filePath}:${validBranchHint ?? ""}:${searchParams.get("editor") ?? ""}`
          : guideRouteId
            ? `guide:${guideRouteId}`
            : issueRouteRepositoryId
              ? `issue:${issueRouteRepositoryId}:${issueRouteNumber ?? ""}`
              : commitRouteRepoId
                ? `commit:${commitRouteRepoId}:${commitRouteSha ?? ""}`
                : "root";
    // A content route resolves once this workspace's tabs are restored:
    // before that, the selection still belongs to the previous workspace, and
    // a route it already shows there (the same guide) would resolve as shown
    // while the restoration then clears it.
    const shouldResolveTabRoute =
      tabStateWorkspaceId === workspaceId &&
      lastResolvedRouteKeyRef.current !== routeKey;
    const preserveSessionHint = Boolean(searchParams.get("session"));
    // A return from the tab strip restored the strip's snapshot
    // (g5.workbench.tabs): that selection is the surface, and the route —
    // which may still name content opened earlier, or sit behind Chat — is
    // not opened over it while the visit stays on that path, however often
    // it re-resolves. Hints below still hydrate; a later path resolves.
    if (restoredSelectionRef.current) {
      const restoredRoute = restoredRouteRef.current;
      if (restoredRoute === null) {
        restoredRouteRef.current = { pathname: location.pathname, branchHint };
      } else if (
        restoredRoute.pathname !== location.pathname ||
        restoredRoute.branchHint !== branchHint
      ) {
        restoredSelectionRef.current = null;
        restoredRouteRef.current = null;
      }
    }
    const restoredSelection = returnedToTab
      ? restoredSelectionRef.current
      : null;
    // A deep link shows its content tab. A restored tab can already be the
    // selected one (the document's own URL reloaded), so the open that takes
    // Chat out of the center does not run; the link still means this content.
    const showRouteTab = (tab: TabItem) => {
      if (restoredSelection) return;
      if (
        selectedTabRef.current !== getTabId(tab) ||
        !selectedTabItemRef.current
      ) {
        // A persisted live tab can retain its selected ID after its old
        // authority becomes ineligible. Reopen the freshly resolved route;
        // the selected ID alone does not mean its content tab is visible.
        openTab(tab, { preserveSessionHint });
      } else if (!preserveSessionHint) {
        hideChatForContent();
      }
    };

    if (routeKey === "root") {
      lastResolvedRouteKeyRef.current = routeKey;
    }

    if (shouldResolveTabRoute && isIntentLogPath) {
      const routeIntentLogTab: TabItem = {
        type: "intent_log",
        name: "All intent logs",
      };
      showRouteTab(routeIntentLogTab);
      lastResolvedRouteKeyRef.current = routeKey;
    }
    if (shouldResolveTabRoute && docPath) {
      if (docListState.status === "loading") return;

      const doc = docs.find((d) => d.id === docPath);
      if (doc) {
        const routeDocTab: TabItem = {
          type: "doc",
          docId: docPath,
          name: doc.name || docPath,
        };
        showRouteTab(routeDocTab);
        lastResolvedRouteKeyRef.current = routeKey;
      } else if (docListState.status === "ok" && workspaceRoutePath) {
        // Reject unknown direct deep-links so URL IDs cannot bypass workspace doc list membership.
        lastResolvedRouteKeyRef.current = routeKey;
        navigate(workspaceRoutePath, {
          replace: true,
        });
      }
    }
    if (shouldResolveTabRoute && filePath) {
      // Source files are opened directly via the branch file API.
      // Do NOT resolve through resolveSpecRef — its content-substring
      // fallback incorrectly matches the parent spec node (whose YAML
      // frontmatter contains the file name in src_files).
      const routeFileBranchId = resolveFileBranchId(
        validBranchHint,
        effectiveContentBranchId,
      );
      const routeFileTab = workspaceFileTab(
        filePath,
        undefined,
        routeFileBranchId,
      );
      showRouteTab(routeFileTab);
      lastResolvedRouteKeyRef.current = routeKey;
    }
    // Identity routes (g5.workbench.tabs): a guide doc, an external issue or
    // a commit diff. An unknown identity falls back to the workspace root,
    // like an unknown doc link; the issue panel's live read keeps its own
    // authority.
    // An unknown identity falls back like a fresh open of the root: the
    // timeline under the alignment layout (the landing already counted this
    // visit as deep-linked, and a restored selection must not stand in for
    // the surface the link named), the bare root otherwise. Both are tab-URL
    // writes the `?view=` consumer and mirror wait for, instead of writing
    // the stale route back from this render's query.
    const fallBackToRoot = () => {
      // The root first: a singleton tab selected while the URL still names
      // the stale path would have its hint written and dropped in turns.
      // The timeline names its own hint in its write, over whatever stale
      // hint the link carried: the mirror holds back from a URL that just
      // changed, and nothing else would re-run it.
      syncUrlToTab(null);
      openTab(WORKSPACE_ACTIVITY_TAB, { searchParams: { view: "activity" } });
    };
    if (shouldResolveTabRoute && guideRouteId) {
      const routeGuideTab = guideDocTab(guideRouteId);
      lastResolvedRouteKeyRef.current = routeKey;
      if (routeGuideTab) {
        showRouteTab(routeGuideTab);
      } else {
        fallBackToRoot();
      }
    }
    if (shouldResolveTabRoute && issueRouteRepositoryId) {
      const issueNumber = parseIssueNumberParam(issueRouteNumber);
      if (issueNumber <= 0) {
        lastResolvedRouteKeyRef.current = routeKey;
        fallBackToRoot();
      } else {
        // The tab is labelled by the workspace's link to that issue when it
        // has one; the feature and the links are a short wait, and the
        // links only load when the feature is on.
        if (
          externalIssuesFeatureStatus === "loading" ||
          (externalIssuesEnabled && workspaceIssueLinks.status === "loading")
        ) {
          return;
        }
        // A workspace whose organization offers no external issues has no
        // issue surface: the link falls back to the root like Diff Graph on
        // Base.
        if (!externalIssuesEnabled) {
          lastResolvedRouteKeyRef.current = routeKey;
          fallBackToRoot();
          return;
        }
        const link =
          workspaceIssueLinks.status === "ok"
            ? findWorkspaceIssueLink(workspaceIssueLinks.data.links, {
                repositoryId: issueRouteRepositoryId,
                number: issueNumber,
              })
            : undefined;
        showRouteTab(
          externalIssueTab(
            { repositoryId: issueRouteRepositoryId, number: issueNumber },
            link?.displayKey ?? `#${issueNumber}`,
          ),
        );
        lastResolvedRouteKeyRef.current = routeKey;
      }
    }
    if (shouldResolveTabRoute && commitRouteRepoId && commitRouteSha) {
      if (workspace.status === "loading") return;
      const branch = allSortedBranches.find(
        (candidate) => candidate.repoId === commitRouteRepoId,
      );
      lastResolvedRouteKeyRef.current = routeKey;
      if (branch) {
        // The commit tab is a singleton with one id for every commit, so a
        // link to another commit while it is selected must replace its
        // identity: an open upserts the item where `showRouteTab` would only
        // keep the selection.
        const shown = selectedTabItemRef.current;
        if (
          shown?.type === "source_control_commit_diff" &&
          shown.repoId === branch.repoId &&
          shown.sha === commitRouteSha
        ) {
          // The opener's own tab (titled by the commit summary) stands.
          showRouteTab(shown);
        } else if (!restoredSelection) {
          openTab(
            commitDiffTab(branch, commitRouteSha, shortOid(commitRouteSha)),
            { preserveSessionHint },
          );
        }
      } else {
        fallBackToRoot();
      }
    }
    if (validBranchHint) {
      if (!docPath && isSelectableBranchId(validBranchHint)) {
        setActiveBranchId(validBranchHint);
      }
    }

    const sessionHint = searchParams.get("session");
    // A link to the content path this page already shows, minus the
    // conversation hint it had, names the content: Chat leaves the center.
    // An in-page action that drops the hint (closing Chat, a new chat, a
    // tab selection) has suppressed that hint itself and keeps its own
    // Chat placement.
    const previousSessionHint = prevSessionHintRef.current;
    prevSessionHintRef.current = sessionHint;
    if (
      previousSessionHint &&
      !sessionHint &&
      routeKey !== "root" &&
      !shouldResolveTabRoute &&
      !restoredSelection &&
      suppressedSessionHintRef.current !== previousSessionHint
    ) {
      hideChatForContent();
    }
    if (sessionHint && workspaceChatAvailable) {
      let pendingReveal: boolean | null = null;
      if (
        pendingSessionSelectionRef.current &&
        sessionHint !== pendingSessionSelectionRef.current
      ) {
        // Stale hint from before an imperative selection — the route has not
        // caught up yet; keep the user's pick (chat intent 7010a91e).
      } else {
        if (sessionHint === pendingSessionSelectionRef.current) {
          pendingReveal = pendingSessionRevealRef.current;
          pendingSessionSelectionRef.current = null;
          pendingSessionRevealRef.current = null;
        }
        if (sessionHint !== suppressedSessionHintRef.current) {
          suppressedSessionHintRef.current = null;
          setActiveChatSessionId(sessionHint);
          // Direct task routes keep their task output visible. An explicit
          // Chat-tab/history selection can still override that route default.
          if (pendingReveal ?? !routeTaskId) {
            setRightPanelOpen(true);
          }
        }
      }
    } else {
      suppressedSessionHintRef.current = null;
    }

    if (
      routeTaskId &&
      (!pendingSelectedTaskIdRef.current ||
        pendingSelectedTaskIdRef.current === routeTaskId)
    ) {
      pendingSelectedTaskIdRef.current = null;
      setSelectedTaskId(routeTaskId);
      if (openedTaskRouteRef.current !== routeTaskId) {
        openedTaskRouteRef.current = routeTaskId;
        if (!restoredSelection) openTaskOutputForTask(routeTaskId);
      }
    } else if (!routeTaskId) {
      openedTaskRouteRef.current = null;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    docPath,
    filePath,
    isIntentLogPath,
    tabStateWorkspaceId,
    guideRouteId,
    issueRouteRepositoryId,
    issueRouteNumber,
    commitRouteRepoId,
    commitRouteSha,
    externalIssuesFeatureStatus,
    externalIssuesEnabled,
    workspaceIssueLinks,
    allSortedBranches,
    routeTaskId,
    docListState.status,
    docs,
    docsBranchId,
    searchParams,
    activeBranchId,
    effectiveContentBranchId,
    allSortedBranches.length,
    isCurrentWorkspaceBranchId,
    isSelectableBranchId,
    org,
    workspace.status,
    workspaceId,
    workspaceRoutePath,
    navigate,
    updateViewParams,
    hideChatForContent,
    location.pathname,
    location.search,
    location.key,
    workspaceChatAvailable,
  ]);

  const alignmentLandedWorkspaceRef = useRef<string | null>(null);
  useEffect(() => {
    if (!workspaceId || tabStateWorkspaceId !== workspaceId) return;
    if (alignmentLandedWorkspaceRef.current === workspaceId) return;
    alignmentLandedWorkspaceRef.current = workspaceId;
    const deepLinked =
      !!docPath ||
      !!filePath ||
      isIntentLogPath ||
      !!routeTaskId ||
      identityRoutePath ||
      searchParams.has("view") ||
      searchParams.has("session");
    if (returnedToTab && selectedTabRef.current) {
      // The restored selection is the surface. A workspace change puts Chat
      // back in the center; it stays there only when Chat was in front when
      // the workspace was left (the strip's snapshot).
      if (!restoredChatCenterRef.current) hideChatForContent();
    } else if (!deepLinked && !reviewOrigin) {
      // Selects the tab, mirrors `?view=activity` and takes Chat out of the
      // center, as any content open does.
      openWorkspaceActivityTab();
    }
    setOpenTabs((prev) => withActivityTabFirst(prev, reviewOrigin !== null));
  }, [
    docPath,
    filePath,
    hideChatForContent,
    identityRoutePath,
    isIntentLogPath,
    routeTaskId,
    openWorkspaceActivityTab,
    returnedToTab,
    reviewOrigin,
    searchParams,

    tabStateWorkspaceId,
    workspaceId,
  ]);

  // ---------------------------------------------------------------------------
  // `?view=` ⇄ center tab sync — keep Terminal and admin tabs bookmarkable
  //
  // The standalone `/:org/settings`, `/:org/settings/workers`, and
  // `/:org/settings/members` routes redirect into the org's base workspace with
  // `?view=settings|workers|members`. Two convergent effects keep the URL and
  // the active admin center tab in sync:
  //   1. URL → tab: when `?view` appears, open/focus the matching admin tab.
  //      A ref dedupes per distinct value so navigating away (which clears the
  //      param via effect 2) does not immediately re-open the tab.
  //   2. tab → URL: mirror `?view` to whichever admin tab is the active center
  //      tab (set on open, cleared when a non-admin tab becomes active), so the
  //      URL reflects the open rail surface and is refresh-/bookmark-stable.
  // ---------------------------------------------------------------------------
  useEffect(() => {
    // Wait until restoration has committed; otherwise it can replace a tab
    // opened from the URL while instance features are still loading.
    if (!workspaceId || tabStateWorkspaceId !== workspaceId) return;
    const viewParam = searchParams.get("view");
    if (!viewParam) {
      consumedViewParamRef.current = null;
      return;
    }
    // A content path owns the surface (g5.workbench.tabs): a center-view
    // hint beside a spec, doc, file, intent-log, guide, issue or commit path
    // is stale (the mirror writes hints at the workspace root only) and is
    // dropped rather than opened over the path's content, which would also
    // leave the path's own navigation waiting for a URL that never lands.
    // It counts as consumed at once, so a write that carries it to the root
    // meanwhile (a review copy's Review landing) does not apply it there.
    if (
      CENTER_VIEW_PARAMS.has(viewParam) &&
      (!!docPath || !!filePath || isIntentLogPath || identityRoutePath)
    ) {
      consumedViewParamRef.current = viewParam;
      // The path's own navigation, when one is in flight, already wrote the
      // URL without the hint (and with its own branch hint); a write from
      // this render's stale query would replace it and strand the deep-link
      // effect waiting for it. This runs again once that URL lands.
      if (
        pendingTabUrlRef.current &&
        pendingTabUrlRef.current.url !==
          `${location.pathname}${location.search}`
      ) {
        return;
      }
      updateViewParams({ view: null });
      return;
    }
    // The mirror's own write lands as consumed: the tab it names was already
    // selected when it was written, and may since have been left.
    if (mirroredViewParamRef.current === viewParam) {
      consumedViewParamRef.current = viewParam;
      return;
    }
    if (consumedViewParamRef.current === viewParam) return;
    if (viewParam === "ontology" && workspace.status !== "ok") return;
    // An Approvals hint waits, unconsumed, until the page knows whether this
    // is a review copy; the effect runs again when it does.
    if (
      (viewParam === "approvals" || viewParam === "reviews") &&
      !reviewIdentityKnown
    ) {
      return;
    }
    // The Diff Graph is refused on Base; the hint waits for the workspace.
    if (viewParam === "diff-graph" && workspace.status !== "ok") return;
    // A personal workspace offers none of these surfaces; the hint waits for
    // the workspace's kind, then falls back to the root.
    if (PERSONAL_UNOFFERED_VIEW_PARAMS.has(viewParam)) {
      if (workspace.status !== "ok") return;
      if (currentWorkspaceIsPersonal) {
        consumedViewParamRef.current = viewParam;
        updateViewParams({ view: null });
        return;
      }
    }
    // Chat as the center surface (ui g5.workspace.ui.fork-landing): the
    // hinted session when one is named, else the no-session composer.
    if (viewParam === "chat") {
      consumedViewParamRef.current = viewParam;
      setRightPanelDetached(false);
      setRightPanelOpen(true);
      return;
    }
    // Mark consumed first so an unknown value cannot loop.
    consumedViewParamRef.current = viewParam;
    const hintedTypes: Record<string, string> = {
      settings: "workspace_settings",
      terminal: "workspace_terminal",
      workers: "worker_hosts",
      members: "org_members",
      "approval-definitions": "org_approvals",
      "github-app": "org_github_app",
      admin: "org_admin",
      details: "workspace_details",
      issues: "workspace_details",
      activity: "workspace_activity",
      changes: "workspace_changes",
      ontology: "source_control_change_graph",
      review: "workspace_review",
    };
    pendingHintTabTypeRef.current = hintedTypes[viewParam] ?? null;
    if (viewParam === "settings") {
      openWorkspaceSettingsTab();
    } else if (viewParam === "terminal") {
      openWorkspaceTerminalTab();
    } else if (viewParam === "workers") {
      openWorkerHostsTab();
    } else if (viewParam === "members") {
      openOrgMembersTab();
    } else if (viewParam === "approvals" || viewParam === "reviews") {
      // Reviews live on the Approvals tab; the panel scrolls itself into
      // view when the hint is `reviews`. A review copy has no Approvals
      // page (it never merges or opens a PR): its Review tab is the page.
      // The hint's tab is selected on a later render; until then the
      // previously selected tab must not mirror itself over the hint.
      pendingHintTabTypeRef.current = isReviewCopy
        ? "workspace_review"
        : "workspace_approvals";
      if (isReviewCopy) {
        openWorkspaceReviewTab();
      } else {
        openWorkspaceApprovalsTab();
      }
    } else if (viewParam === "approval-definitions") {
      openOrgApprovalsTab();
    } else if (viewParam === "github-app") {
      openOrgGithubAppTab();
    } else if (viewParam === "admin") {
      openOrgAdminTab();
    } else if (viewParam === "details") {
      openWorkspaceDetailsTab();
    } else if (viewParam === "activity") {
      openWorkspaceActivityTab();
    } else if (viewParam === "changes") {
      openTab({ type: "workspace_changes", name: "Current changes" });
    } else if (viewParam === "docs" || viewParam === "review-docs") {
      setLeftSidebarOpen(true);
      setBrowserSectionRequest({
        section: "project-docs",
        nonce: ++browserSectionNonce.current,
      });
      updateViewParams({ view: null });
    } else if (["source-control", "files"].includes(viewParam)) {
      pendingHintTabTypeRef.current = "source_control_diff";
      // A mirrored Source Control hint names the restored tab and its file.
      // Showing it must not replace that item with a fresh, unselected one.
      if (
        viewParam === "source-control" &&
        selectedTabRef.current === getTabId(SOURCE_CONTROL_DIFF_TAB)
      ) {
        hideChatForContent();
      } else {
        openTab({
          type: "source_control_diff",
          name: "Source Control",
        });
      }
    } else if (viewParam === "ontology") {
      openSourceControlChangeGraphTab();
    } else if (viewParam === "issues") {
      // The Issues section of Workspace Details; the section scrolls itself
      // into view and the hint stays until then.
      openWorkspaceDetailsTab();
    } else if (viewParam === "review") {
      openWorkspaceReviewTab();
    } else if (viewParam === "pull-requests") {
      openPullRequestsTab();
    } else if (viewParam === "ontology-changes") {
      openTab({ type: "ontology_changes", name: "Ontology changes" });
    } else if (viewParam === "diff-graph") {
      // A refused open (Base) lands like a fresh open of the root: the
      // timeline, naming its own hint in its write (the landing counted the
      // visit as deep-linked, and a persisted selection must not stand in
      // for the surface the link named); a bare root otherwise.
      if (!openSourceControlChangeGraphTab()) {
        openTab(WORKSPACE_ACTIVITY_TAB, {
          searchParams: { view: "activity" },
        });
      }
    } else {
      // Removed or unknown surfaces use the same fallback as a missing
      // content target. A document path still owns its content.
      if (docPath || filePath || isIntentLogPath || identityRoutePath) {
        updateViewParams({ view: null });
      } else {
        openTab(WORKSPACE_ACTIVITY_TAB, {
          searchParams: { view: "activity" },
        });
      }
    }
  }, [
    workspaceId,
    tabStateWorkspaceId,
    searchParams,

    docPath,
    filePath,
    isIntentLogPath,
    identityRoutePath,
    location.pathname,
    location.search,
    hideChatForContent,
    openPullRequestsTab,
    openSourceControlDiffTab,
    openSourceControlChangeGraphTab,
    workspace.status,
    currentWorkspaceIsPersonal,
    openTab,
    updateViewParams,
    openWorkspaceSettingsTab,
    openWorkspaceTerminalTab,
    openWorkerHostsTab,
    openOrgMembersTab,
    openOrgApprovalsTab,
    openOrgGithubAppTab,
    openOrgAdminTab,
    openWorkspaceApprovalsTab,
    openWorkspaceDetailsTab,
    openWorkspaceActivityTab,
    openTab,
    openSourceControlChangeGraphTab,
    workspace.status,
    reviewIdentityKnown,
    isReviewCopy,
    openWorkspaceReviewTab,
    reviewOrigin,
  ]);

  // A review copy has no Approvals page: an Approvals tab restored from a
  // previous visit (or opened before the review context arrived) is closed.
  useEffect(() => {
    if (!isReviewCopy || !workspaceId || tabStateWorkspaceId !== workspaceId)
      return;
    const stale = openTabs.find((tab) => tab.type === "workspace_approvals");
    if (stale) closeTab(getTabId(stale));
  }, [isReviewCopy, workspaceId, tabStateWorkspaceId, openTabs, closeTab]);

  // A review copy's Review tab is always there, first in the strip, like
  // Chat: it is added after the tabs are restored (so they do not replace
  // it) and cannot be closed. It is selected once per page load, not when
  // the strip shows the review copy again this session; after that the
  // reviewer's own tab choice stands.
  useEffect(() => {
    if (!reviewOrigin || !workspaceId || tabStateWorkspaceId !== workspaceId)
      return;
    const name = `Review of ${reviewOrigin.sourceWorkspaceName}`;
    setOpenTabs((prev) => {
      const idx = prev.findIndex((tab) => tab.type === "workspace_review");
      if (idx === 0) return prev;
      const item: TabItem =
        idx === -1 ? { type: "workspace_review", name } : prev[idx];
      return [item, ...prev.filter((_, i) => i !== idx)];
    });
  }, [reviewOrigin, workspaceId, tabStateWorkspaceId]);
  useEffect(() => {
    if (!reviewOrigin || !workspaceId || tabStateWorkspaceId !== workspaceId)
      return;
    if (autoOpenedReviewTabRef.current === workspaceId) return;
    autoOpenedReviewTabRef.current = workspaceId;
    if (!returnedToTab) openWorkspaceReviewTab();
  }, [
    reviewOrigin,
    returnedToTab,
    workspaceId,
    tabStateWorkspaceId,
    openWorkspaceReviewTab,
  ]);

  useEffect(() => {
    // Preserve an incoming hint while the workspace's tabs are being restored.
    if (!workspaceId || tabStateWorkspaceId !== workspaceId) return;
    // Whether Chat is the center view is unknown until the workspace detail
    // says whether this is a Base workspace. Mirroring a restored Terminal
    // or admin tab before then wrote `?view=` over the Chat that becomes
    // active once the detail loads, and the hint then opened that tab.
    if (!workspaceDetailData) return;
    // A content tab's own navigation (`syncUrlToTab`) that has not landed
    // yet already names the URL, hint dropped; the router can lag it by a
    // render, and a mirror written from the stale route would replace it.
    if (
      pendingTabUrlRef.current &&
      pendingTabUrlRef.current.url !== `${location.pathname}${location.search}`
    ) {
      return;
    }
    const have = searchParams.get("view");
    const haveIsCenterView = !!have && CENTER_VIEW_PARAMS.has(have);
    // A task's default changes surface is already identified by its path.
    const want =
      centerChatActive ||
      !selectedTabItem ||
      (routeTaskId && selectedTabItem.type === "workspace_changes")
        ? null
        : tabToViewParam(selectedTabItem);
    // Did `?view` change externally this render (vs. the active tab changing)?
    // If so, the URL→tab effect is about to switch tabs — let it win and do NOT
    // mirror the (stale) active tab back over the new hint, or we bounce.
    // The mirror's own write landing is not an external change.
    const ownWrite = have !== null && mirroredViewParamRef.current === have;
    if (ownWrite) mirroredViewParamRef.current = null;
    const urlChanged = prevViewParamRef.current !== have && !ownWrite;
    prevViewParamRef.current = have;

    // A hint the URL→tab effect has not consumed yet (it may be waiting for
    // the review identity) is never mirrored over by the active tab.
    const havePending =
      haveIsCenterView && consumedViewParamRef.current !== have;
    // A consumed hint whose tab is not selected yet is still pending: the
    // tab it asked for arrives on a later render.
    const hintTab = pendingHintTabTypeRef.current;
    if (hintTab) {
      if (!haveIsCenterView) {
        pendingHintTabTypeRef.current = null;
      } else if (selectedTabItem?.type === hintTab && !centerChatActive) {
        pendingHintTabTypeRef.current = null;
      } else {
        // Opening the hinted tab also hides Chat. Both state changes must
        // land before the mirror can own the URL, even when the tab was
        // already restored as selected. Otherwise Chat clears the hint.
        return;
      }
    }
    if (want) {
      // Parameters the opener attached to this tab ride along with the mirror
      // (the mirror's params predate the opener's own navigation). A mirror
      // run before the selection lands still names the previous tab and
      // leaves them for the one that names this tab.
      const attached = pendingTabSearchParamsRef.current;
      if (attached && attached.view === want && !havePending) {
        pendingTabSearchParamsRef.current = null;
        mirroredViewParamRef.current = want;
        updateViewParams({ view: want, ...attached.params });
        return;
      }
      if (have === want) return; // already in sync
      if (havePending) return;
      // `reviews` is the Approvals tab with a scroll hint for its Reviews
      // panel; it stays in the URL until that panel has mounted and read it
      // (the panel may still be loading), never rewritten to `approvals`.
      if (want === "approvals" && have === "reviews") return;
      // Likewise `issues` is Workspace Details scrolled to its Issues section.
      if (want === "details" && have === "issues") return;
      // A new route may remove the previous view hint while its tab selection
      // is still being applied. Do not write the old view back over that URL.
      if (urlChanged) return;
      mirroredViewParamRef.current = want;
      // A `?section=` hint belongs to the settings sheet it was written for
      // (ui g5.ui.settings-page); it does not follow the view it is leaving.
      updateViewParams({ view: want, section: null }); // tab-driven (rail click) → mirror
    } else if (haveIsCenterView) {
      if (urlChanged) return; // URL just set an admin hint — consumer will open it
      if (havePending) return;
      // A session selection's own URL write is in flight: it already drops
      // the view hint, and this render's query predates it. Clearing from
      // here would write the stale query back without the `?session=` hint
      // (Chat took the center before the route caught up).
      if (pendingSessionSelectionRef.current) return;
      // Stale admin hint with a non-admin tab active → clear (never clobber an
      // unrelated `view` value).
      mirroredViewParamRef.current = null;
      updateViewParams({ view: null, section: null });
    }
  }, [
    centerChatActive,
    workspaceDetailData,
    workspaceId,
    tabStateWorkspaceId,
    selectedTabItem,
    routeTaskId,
    searchParams,
    location.pathname,
    location.search,
    updateViewParams,
    reviewIdentityKnown,
  ]);

  // ---------------------------------------------------------------------------
  // Keyboard shortcuts
  // ---------------------------------------------------------------------------

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const isMac = navigator.platform.toUpperCase().indexOf("MAC") >= 0;
      const mod = isMac ? e.metaKey : e.ctrlKey;
      const target = e.target as HTMLElement;
      const isEditing =
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable;

      // Skip other shortcuts when editing
      if (isEditing) return;

      // ⌘\ / Ctrl+\ — Toggle left sidebar
      if (mod && e.key === "\\") {
        e.preventDefault();
        setLeftSidebarOpen((prev) => !prev);
        return;
      }

      // ⌘S / Ctrl+S — Prevent browser save dialog
      if (mod && e.key === "s") {
        e.preventDefault();
        return;
      }

      // Escape — Close overlays
      if (e.key === "Escape") {
        if (settingsOpen) {
          closeSettings();
          return;
        }
        if (notificationsOpen) {
          setNotificationsOpen(false);
          return;
        }
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [settingsOpen, notificationsOpen, closeSettings]);

  // SCM OAuth returns carry `settings=scm` (see buildScmOAuthReturnTo). The SCM
  // connect UI and its `scm_connected`/`scm_error` handler now live in the
  // workspace settings *tab* (WorkspaceSettingsContent), not the account flyout.
  // Open that tab so the callback lands where the user started; the tab content
  // consumes the result params and clears the SCM hint from the URL.
  useEffect(() => {
    if (isScmSettingsReturn(searchParams)) {
      openWorkspaceSettingsTab();
    }
  }, [searchParams, openWorkspaceSettingsTab]);

  // ---------------------------------------------------------------------------
  // Panel resize handlers
  // ---------------------------------------------------------------------------

  const handleLeftSidebarResize = useCallback((delta: number) => {
    setLeftSidebarWidth((prev) => {
      const maxAllowed = Math.min(
        LEFT_SIDEBAR_MAX,
        window.innerWidth - CENTER_MIN_WIDTH,
      );
      return Math.max(LEFT_SIDEBAR_MIN, Math.min(maxAllowed, prev + delta));
    });
  }, []);

  // ---------------------------------------------------------------------------
  // Derived data
  // ---------------------------------------------------------------------------

  const availableAgentProviders = useMemo<ChatAgentProvider[]>(
    () => supportedCloudAgentProviders(),
    [],
  );
  const [selectedAgentProvider, setSelectedAgentProvider] =
    useState<ChatAgentProvider>("claude_code");
  const userSelectedAgentProviderRef = useRef(false);
  useEffect(() => {
    userSelectedAgentProviderRef.current = false;
  }, [workspaceId]);
  const handleSelectAgentProvider = useCallback(
    (provider: ChatAgentProvider) => {
      userSelectedAgentProviderRef.current = true;
      setSelectedAgentProvider(provider);
    },
    [],
  );
  const preferredAvailableAgentProvider = getPreferredAvailableAgentProvider(
    availableAgentProviders,
    defaultAgentProvider,
    chatAuthenticatableAgents,
  );
  useEffect(() => {
    if (!availableAgentProviders.includes(selectedAgentProvider)) {
      setSelectedAgentProvider(preferredAvailableAgentProvider);
      return;
    }
    if (
      !activeChatSessionId &&
      !userSelectedAgentProviderRef.current &&
      selectedAgentProvider !== preferredAvailableAgentProvider
    ) {
      setSelectedAgentProvider(preferredAvailableAgentProvider);
    }
  }, [
    activeChatSessionId,
    availableAgentProviders,
    preferredAvailableAgentProvider,
    selectedAgentProvider,
  ]);
  const visibleChatSessions = useMemo(() => {
    const sessions = chatSessions.status === "ok" ? chatSessions.data : [];
    return sessions.filter((session) =>
      sessionMatchesChatScope(session, chatScopeType, selectedTaskId),
    );
  }, [chatSessions, chatScopeType, selectedTaskId]);

  const activeChatSession = useMemo(() => {
    const sessionDetail =
      activeChatSessionState.status === "ok"
        ? activeChatSessionState.data
        : null;
    if (
      sessionMatchesChatScope(sessionDetail, chatScopeType, selectedTaskId) ||
      sessionMatchesRestorableTaskUrl(
        sessionDetail,
        workspaceId,
        activeChatSessionId,
        searchParams.get("session"),
      )
    ) {
      return sessionDetail;
    }
    if (!activeChatSessionId) return null;
    return (
      visibleChatSessions.find(
        (session) => session.id === activeChatSessionId,
      ) ?? null
    );
  }, [
    activeChatSessionId,
    activeChatSessionState,
    chatScopeType,
    searchParams,
    selectedTaskId,
    visibleChatSessions,
    workspaceId,
  ]);
  const acknowledgeChatResult = useChatResultInspection(
    activeChatSession?.id,
    activeChatSession?.activity ??
      visibleChatSessions.find(
        (session) => session.id === activeChatSession?.id,
      )?.activity,
    chatSessionData.status === "ok" ? chatSessionData.data : [],
  );

  const askUserQuestionFocusHint = searchParams.get("askUserQuestion");
  const verifiedAskUserQuestionFocusId = useMemo(
    () =>
      verifiedAskUserQuestionFocus({
        approvalId: askUserQuestionFocusHint,
        sessionHint: searchParams.get("session"),
        routeTaskId,
        selectedTaskId,
        activeSessionId: activeChatSessionId,
        activeSession: activeChatSession,
        timelineEntries: chatTimelineEntries,
        messages: chatMessages,
        isStreaming: isChatStreaming,
      }),
    [
      activeChatSession,
      activeChatSessionId,
      askUserQuestionFocusHint,
      chatMessages,
      chatTimelineEntries,
      isChatStreaming,
      routeTaskId,
      searchParams,
      selectedTaskId,
    ],
  );
  const consumeAskUserQuestionFocus = useCallback(
    (approvalId: string) => {
      if (searchParams.get("askUserQuestion") !== approvalId) return;
      updateViewParams({ askUserQuestion: null });
    },
    [searchParams, updateViewParams],
  );

  useEffect(() => {
    if (
      shouldClearAskUserQuestionFocus({
        focusHint: askUserQuestionFocusHint,
        // Session queries can finish before the route's task is selected on a
        // hard reload. An unverified hint is not stale until selection catches up.
        selectionReady: Boolean(
          routeTaskId &&
          selectedTaskId === routeTaskId &&
          activeChatSessionId === searchParams.get("session"),
        ),
        activeSessionReady: activeChatSessionState.status === "ok",
        timelineReady: apiTimeline.status === "ok",
        messagesReady: chatSessionData.status === "ok",
        verifiedApprovalId: verifiedAskUserQuestionFocusId,
      })
    ) {
      updateViewParams({ askUserQuestion: null });
    }
  }, [
    activeChatSessionId,
    activeChatSessionState,
    apiTimeline.status,
    askUserQuestionFocusHint,
    chatSessionData.status,
    chatTimelineEntries,
    routeTaskId,
    searchParams,
    selectedTaskId,
    updateViewParams,
    verifiedAskUserQuestionFocusId,
  ]);

  // The waiting panel's End: report a failure here and reject so the panel
  // re-enables its control.
  const handleEndBackgroundWait = useCallback(
    async (sessionId: string, wait: { taskId: string; attempt: number }) => {
      const scope = workspaceViewScope;
      try {
        await endBackgroundWaitAsync({ sessionId, ...wait });
      } catch (error) {
        if (scope.active && workspaceViewScopeRef.current === scope) {
          pushToast(
            "error",
            "Could not end the wait",
            error instanceof Error ? error.message : String(error),
          );
        }
        throw error;
      }
    },
    [endBackgroundWaitAsync, pushToast, workspaceViewScope],
  );

  const effectiveChatScopeType = chatScopeType;
  const effectiveChatScopeRef = chatScopeRef;
  const effectiveChatScopeLabel = chatScopeLabel;

  const pendingAgentApprovalIds = useMemo(
    () =>
      activeChatSession
        ? pendingAgentApprovalIdsForTimeline(
            chatTimelineEntries,
            chatMessages,
            activeChatTaskSummary,
          )
        : [],
    [
      activeChatSession,
      activeChatTaskSummary,
      chatTimelineEntries,
      chatMessages,
    ],
  );
  // The composer follows the live gate (`g5.conversation.composer.stopped`):
  // a stopped owner reads the state and gets a Start beside it, and keeps
  // the draft.
  const composerDisabledReason =
    (startAndSend
      ? null
      : liveGateReason(liveGateKindNow, "send messages", { workspaceNoun })) ??
    (pendingAgentApprovalIds.length > 0
      ? "Resolve the pending approval before sending more steering."
      : null);

  // A link that names a personal workspace by its name (`users/<handle>`)
  // lands on the kind-resolved route, so the tab strip and every in-page
  // link use one identity (g5.personal.routes). The page already resolved
  // the workspace and shows it by kind; this rewrites the URL and moves the
  // name tab the arrival opened onto the personal route, so the workspace
  // stays one tab rather than a name tab beside a personal one (issue #947).
  const workspaceDetailName = workspaceDetailData?.name;
  const personalRouteTab = useMemo<WorkspaceTab | null>(
    () =>
      routeFamily === "workspace" &&
      currentWorkspaceIsPersonal &&
      !!org &&
      !!workspaceRoutePath &&
      workspaceDetailName?.startsWith("users/") === true
        ? isWorkspaceOwner
          ? { orgSlug: org, name: "me", personal: "me" }
          : { orgSlug: org, name: workspaceDetailName, personal: "user" }
        : null,
    [
      currentWorkspaceIsPersonal,
      isWorkspaceOwner,
      org,
      routeFamily,
      workspaceDetailName,
      workspaceRoutePath,
    ],
  );
  const personalRouteRedirect = personalRouteTab
    ? workspaceTabPath(personalRouteTab)
    : null;
  useEffect(() => {
    if (
      !personalRouteRedirect ||
      !personalRouteTab ||
      !workspaceRoutePath ||
      !org ||
      !workspaceSlug
    ) {
      return;
    }
    const suffix = location.pathname.startsWith(workspaceRoutePath)
      ? location.pathname.slice(workspaceRoutePath.length)
      : "";
    renameTab({ orgSlug: org, name: workspaceSlug }, personalRouteTab);
    navigate(`${personalRouteRedirect}${suffix}${location.search}`, {
      replace: true,
    });
  }, [
    location.pathname,
    location.search,
    navigate,
    org,
    personalRouteRedirect,
    personalRouteTab,
    renameTab,
    workspaceRoutePath,
    workspaceSlug,
  ]);

  // The owner's composer says what Start and send does and shows the Start's
  // progress inline (g5.chat.start-and-send).
  const composerNotice = isWorkspaceOwner ? (
    <StartAndSendNotice waitReason={capacityWaitReason} />
  ) : null;

  useEffect(() => {
    if (activeChatSession?.agentProvider) {
      setSelectedAgentProvider(activeChatSession.agentProvider);
    }
  }, [activeChatSession?.agentProvider]);

  useEffect(() => {
    if (chatSessions.status !== "ok" || !activeChatSessionId) return;
    if (
      visibleChatSessions.some((session) => session.id === activeChatSessionId)
    ) {
      return;
    }
    if (activeChatSessionState.status === "loading") {
      return;
    }
    const selectedTaskSessionId =
      taskDetail.status === "ok" ? taskDetail.data.sessionId : null;
    if (selectedTaskSessionId === activeChatSessionId) {
      return;
    }
    const sessionDetail =
      activeChatSessionState.status === "ok"
        ? activeChatSessionState.data
        : null;
    if (
      sessionMatchesChatScope(sessionDetail, chatScopeType, selectedTaskId) ||
      sessionMatchesRestorableTaskUrl(
        sessionDetail,
        workspaceId,
        activeChatSessionId,
        searchParams.get("session"),
      )
    ) {
      return;
    }
    setActiveChatSessionId(null);
  }, [
    activeChatSessionId,
    activeChatSessionState,
    chatScopeType,
    chatSessions.status,
    searchParams,
    selectedTaskId,
    taskDetail,
    visibleChatSessions,
    workspaceId,
  ]);

  const handleSendChatMessage = useCallback(
    async (
      content: string,
      options?: {
        agentLaunch?: AgentLaunchSelectionV1;
        launchSource?: SyncAgentConfig["launchSource"];
        model?: string;
        permissionMode?: string;
        effort?: string;
        account?: AgentAccountSelectionV1;
        projectReferences?: ChatProjectReference[];
        attachments?: ChatUploadAttachment[];
        startCompute?: boolean;
      },
    ): Promise<ChatSendResult> => {
      if (
        !workspaceChatAvailable ||
        !workspaceViewScope.active ||
        workspaceViewScopeRef.current !== workspaceViewScope
      )
        return false;
      const scope = workspaceViewScope;
      setChatSetupError(null);
      const composerScope = chatComposerScopeRef.current;
      const isCurrentComposer = () =>
        scope.active &&
        workspaceViewScopeRef.current === scope &&
        chatComposerScopeRef.current === composerScope;
      // The seed this send carries, as it is now. An accepted send retires
      // exactly this request: a newer seed made while the send was in
      // flight stays, and a late completion never clears it.
      const seedRequest = chatComposerSeedRequestRef.current;
      const retireSeedRequest = () => {
        if (seedRequest && chatComposerSeedRequestRef.current === seedRequest) {
          setChatComposerSeedRequest(null);
        }
      };
      const {
        projectReferences,
        attachments,
        agentLaunch: confirmedAgentLaunch,
        launchSource,
        startCompute: requestedStartCompute,
        ...turnOptions
      } = options ?? {};
      // Only the owner's stopped composer starts on a send.
      const startCompute =
        requestedStartCompute === true && startAndSend ? true : undefined;
      // A session-starting send consumes the seed-attached reference (D69
      // Discuss): the doc rides along as a project_doc reference exactly once.
      const seededReference =
        activeChatSessionId && activeChatSession
          ? null
          : (seedRequest?.reference ?? null);
      const effectiveReferences = mergeSeededChatReference(
        projectReferences,
        seededReference,
      );
      const contextForTurn: ChatMessageContext | undefined =
        effectiveReferences?.length
          ? {
              ...(chatMessageContext ?? {}),
              projectReferences: effectiveReferences,
            }
          : chatMessageContext;
      if (activeChatSessionId && activeChatSession) {
        try {
          await sendChatMessage.mutateAsync({
            sessionId: activeChatSession.id,
            content,
            context: contextForTurn,
            options: turnOptions,
            attachments,
            startCompute,
          });
          // A chat action seeded into this open chat: its Guided step reads
          // Running from this accepted send (g5.workbench.chat-actions).
          const seededStart = seedRequest?.start ?? null;
          if (seededStart && seededStart.sessionId === activeChatSession.id) {
            retireSeedRequest();
            rememberJourneyActionSession(
              seededStart.actionId,
              activeChatSession.id,
            );
          }
          return true;
        } catch (err) {
          if (!isCurrentComposer()) return false;
          const message =
            err instanceof Error ? err.message : "Could not send chat message.";
          setChatSetupError(message);
          pushToast("error", "Chat message failed", message);
          return false;
        }
      }

      if (startChatMessage.isLoading) return false;
      if (!acquireLaunchSubmission("chat-start")) return false;
      try {
        // A chat action seeded into this fresh composer: the send starts the
        // session under the action's persona from the configuration version
        // loaded now, with the text as sent (g5.workbench.chat-actions). The
        // browser expanded the template, so no action id or context travel.
        // A stale version is refused once; the hook reloads the
        // configuration and the seed stays for the next Send.
        const seededStart =
          seedRequest?.start && seedRequest.start.sessionId === null
            ? seedRequest.start
            : null;
        if (seededStart) {
          if (workbenchConfig.status !== "ok") {
            setChatSetupError(RIBBON_FALLBACK_CHAT_TITLE);
            pushToast(
              "error",
              "Workbench unavailable",
              RIBBON_FALLBACK_CHAT_TITLE,
            );
            return false;
          }
          // The user's default harness choices, explicit as the immediate
          // ribbon start resolved them: inheriting would require a
          // workspace-configured permission mode a fresh workspace lacks.
          const agentLaunch =
            confirmedAgentLaunch ??
            (await confirmAgentLaunch({
              ...syncAgentConfig,
              launchSource: "explicit",
            }));
          const started = await startWorkbenchAction.mutateAsync({
            workspaceId,
            persona: seededStart.persona,
            phase: seededStart.phase,
            configRevision: workbenchConfig.data.revision,
            content,
            agentLaunch,
            scopeType: "workspace",
            scopeRef: workspaceId,
            messageContext: contextForTurn,
            attachments,
            // A seeded chat action's fresh chat starts the VM like any
            // other Start and send (g5.chat.start-and-send).
            ...(startCompute ? { startCompute: true } : {}),
          });
          rememberJourneyActionSession(
            seededStart.actionId,
            started.session.id,
          );
          // The seed is spent with its reference and start, wherever the
          // person is by now.
          retireSeedRequest();
          const accepted = { sessionId: started.session.id };
          if (!isCurrentComposer()) return accepted;
          handleViewSession(started.session.id);
          return accepted;
        }
        const agentLaunch =
          confirmedAgentLaunch ??
          (await confirmAgentLaunch({
            launchSource: launchSource ?? "inherit_configured",
            agentProvider: selectedAgentProvider,
            model: turnOptions.model,
            permissionMode: turnOptions.permissionMode,
            effort: turnOptions.effort,
          }));
        const started = await startChatMessage.mutateAsync({
          workspaceId,
          content,
          scopeType: chatScopeType,
          scopeRef: chatScopeRef || undefined,
          agentLaunch,
          context: contextForTurn,
          attachments,
          startCompute,
        });
        // The prompt belongs to the chat this send created, even when the
        // person has moved on (chat g5.chat.ui.prompt-history.store).
        const accepted = { sessionId: started.session.id };
        // One-shot: the seed and its reference belong to exactly this send.
        retireSeedRequest();
        if (!isCurrentComposer()) return accepted;
        handleViewSession(started.session.id);
        return accepted;
      } catch (err) {
        if (!isCurrentComposer()) return false;
        const message =
          err instanceof Error ? err.message : "Could not start chat session.";
        console.error("Failed to start chat session:", err);
        setChatSetupError(message);
        pushToast("error", "Chat session failed", message);
        return false;
      } finally {
        releaseLaunchSubmission("chat-start");
      }
    },
    [
      activeChatSession,
      activeChatSessionId,
      acquireLaunchSubmission,
      chatScopeRef,
      chatScopeType,
      chatMessageContext,
      confirmAgentLaunch,
      handleViewSession,
      startAndSend,
      pushToast,
      releaseLaunchSubmission,
      rememberJourneyActionSession,
      selectedAgentProvider,
      startWorkbenchAction,
      syncAgentConfig,
      workbenchConfig,
      workspaceChatAvailable,
      workspaceViewScope,
      sendChatMessage,
      setChatComposerSeedRequest,
      startChatMessage,
      workspaceId,
    ],
  );

  const selectedFileBranchId = useMemo(() => {
    if (selectedTabItem?.type !== "file") return null;
    return selectedTabItem.branchId ?? null;
  }, [selectedTabItem]);

  useEffect(() => {
    if (!selectedFileBranchId) return;
    if (selectedFileBranchId === activeBranchId) return;
    if (!isSelectableBranchId(selectedFileBranchId)) return;
    setActiveBranchId(selectedFileBranchId);
  }, [activeBranchId, isSelectableBranchId, selectedFileBranchId]);

  // Code nodes remain visible in the sidebar — the spec requires source files
  // to appear as navigable children of their parent spec nodes (FileCode icon).
  // They are also used by resolveSpecRef for navigation from the traceability
  // panel.  Removing them from the tree breaks cross-env editing and hides
  // user-created code nodes.

  const previewDirtyCount = useMemo(
    () =>
      workspaceChanges.status === "ok"
        ? workspaceChanges.data.members.reduce(
            (count, member) => count + (member.dirty?.length ?? 0),
            0,
          )
        : 0,
    [workspaceChanges],
  );

  const collaborators = useMemo(() => [], []);

  // ---------------------------------------------------------------------------
  // Workbench ribbon orchestration (workbench g5.workbench.actions)
  // ---------------------------------------------------------------------------
  // The ribbon is data; this page dispatches its clicks. `chat` actions start
  // a persona session from the served template (or append the same template
  // to the open session that already carries that persona). `open` actions
  // call the handlers that already own each surface: Approvals → the
  // approvals tab; Create PR / Merge to parent → the existing
  // PrepareToShipModal (whose merge path is `useMergeWorkspacePullRequest`,
  // the only target-advancing flow — never the rebase-only confirm handlers);
  // Rebase → the Rebase Workspace dialog
  // (g5.workbench.ship-actions);
  // Preview → PreviewControl; New doc / Upload file → CreateProjectDocModal;
  // view → openTab; Upload transcript → the same modal in transcript mode,
  // whose stored document starts the phase's `extract-decisions` template
  // (g5.workbench.meeting); Build mock → BuildMockModal, whose primary action
  // seeds the phase's hidden `generate-mock` template (g5.workbench.mock).
  const ribbonActions = useMemo(
    () =>
      workspaceRibbonActions(
        workbenchConfig.status === "ok"
          ? workbenchConfig.data.actions
          : WORKBENCH_ACTIONS,
      ),
    [workbenchConfig],
  );
  const ribbonConfigStatus: RibbonConfigStatus =
    workbenchConfig.status === "ok"
      ? workbenchConfig.data.config.status
      : workbenchConfig.status === "loading"
        ? "loading"
        : "fallback";
  const ribbonConfigError =
    workbenchConfig.status === "ok"
      ? (workbenchConfig.data.config.error ?? null)
      : null;
  // Preview enablement: the workspace's code repositories (the docs
  // repository is excluded, as PreviewControl excludes it).
  const workbenchCodeBranches = useMemo(
    () =>
      workspace.status === "ok"
        ? workspace.data.branches
            .filter(
              (branch) =>
                branch.workspaceId === workspaceId &&
                branch.repoId !== workspace.data.docsRepoId,
            )
            .map((branch) => ({
              workspaceId: branch.workspaceId,
              repoId: branch.repoId,
              repoName: branch.repoName,
              branchName: branch.branchName,
            }))
        : [],
    [workspace, workspaceId],
  );
  const shipSurfaceAvailable =
    showCloudWorkspaceWorkflows &&
    !currentWorkspaceIsPersonal &&
    workspaceWritable &&
    !reviewExcluded &&
    !!prepareToShipTarget &&
    !!currentWorkspaceSummary &&
    !currentWorkspaceSummary.isBase;
  // Rebase & Merge gate: a writable source
  // the person owns, with at least one target to rebase onto.
  const rebaseSurfaceAvailable =
    showCloudWorkspaceWorkflows &&
    workspaceWritable &&
    !reviewExcluded &&
    !currentWorkspaceIsBase &&
    workspaceMergeTargets.length > 0;
  const workbenchActionState = useCallback(
    (action: WorkbenchAction): RibbonActionState | undefined => {
      if (action.kind === "chat") {
        if (!workspaceWritable) {
          return {
            disabled: true,
            title: "Chat needs a writable workspace you own",
          };
        }
        // A chat action seeds the composer for the person to send
        // (g5.workbench.chat-actions): it is offered only while the composer
        // can send, greyed with the composer's own reason otherwise.
        return composerDisabledReason
          ? { disabled: true, title: composerDisabledReason }
          : undefined;
      }
      if (action.kind !== "open") return undefined;
      switch (action.target) {
        case "ship":
        case "merge":
          return shipSurfaceAvailable
            ? {
                title:
                  action.target === "merge"
                    ? `Merge the pull request into ${prepareToShipTarget?.name ?? "the parent"}`
                    : `Create or update the pull request to ${prepareToShipTarget?.name ?? "the parent"}`,
              }
            : {
                disabled: true,
                title: "Pull requests are unavailable for this workspace",
              };
        case "rebase":
          return rebaseSurfaceAvailable
            ? {
                title:
                  "Rebase this workspace onto a parent without advancing it",
              }
            : {
                disabled: true,
                title: currentWorkspaceIsBase
                  ? "Base workspace cannot be rebased"
                  : "Workspace rebase is unavailable",
              };
        case "new-doc":
        case "upload":
          return workspaceWritable
            ? undefined
            : { disabled: true, title: "Documents need a writable workspace" };
        case "meeting":
          // The dialog itself needs only a writable workspace: a stored
          // transcript seeds the extraction chat, which Start and send can
          // carry; only the upload path writes through the running VM and
          // the dialog gates that path (documents g5.workbench.meeting).
          return workspaceWritable
            ? undefined
            : { disabled: true, title: "Documents need a writable workspace" };
        case "explainer":
          return showCloudWorkspaceWorkflows && workspaceWritable
            ? undefined
            : { disabled: true, title: "Explainers need a writable workspace" };
        case "review":
          return workspaceWritable && workspaceChatAvailable
            ? undefined
            : {
                disabled: true,
                title: "Code review starts a chat in a writable workspace",
              };
        case "verification":
          if (!verificationActionEnabled) {
            return { disabled: true, title: VERIFICATION_ACTION_OFF_TITLE };
          }
          return workspaceWritable && workspaceChatAvailable
            ? undefined
            : {
                disabled: true,
                title:
                  "Re-run Verification starts a chat in a writable workspace",
              };
        case "preview":
          if (!isWorkspaceOwner) {
            return {
              disabled: true,
              title: "Only the workspace owner starts previews",
            };
          }
          return workbenchCodeBranches.length === 0
            ? {
                disabled: true,
                title: "This workspace has no code repository to preview",
              }
            : undefined;
        case "mock":
          // The dialog lists the mocks for anyone; its primary action carries
          // the composer's own reason (g5.workbench.mock).
          return undefined;
        case "view":
          return action.tab && WORKBENCH_VIEW_TABS[action.tab]
            ? undefined
            : { disabled: true, title: `Unknown view "${action.tab ?? ""}"` };
        default:
          return undefined;
      }
    },
    [
      currentWorkspaceIsBase,
      isWorkspaceOwner,
      prepareToShipTarget?.name,
      rebaseSurfaceAvailable,
      shipSurfaceAvailable,
      showCloudWorkspaceWorkflows,
      verificationActionEnabled,
      workbenchCodeBranches.length,
      workspaceChatAvailable,
      workspaceWritable,
      composerDisabledReason,
    ],
  );
  // The `current_doc` context names the document's repository path: a
  // repository doc's tab id is the `overlay:` handle, which the agent cannot
  // read, so it is decoded here.
  const workbenchCurrentDocPath = useMemo(() => {
    const selected = openTabs.find((tab) => getTabId(tab) === selectedTab);
    const docId = selected?.type === "doc" ? selected.docId : docPath;
    return docId ? (parseRepositoryDocPath(docId) ?? docId) : docId;
  }, [docPath, openTabs, selectedTab]);
  const handleWorkbenchChat = useCallback(
    async (
      action: WorkbenchAction,
      issue?: ExternalIssueSummaryDTO,
      /** A document the caller names instead of the selected tab's. */
      currentDocPath?: string,
    ) => {
      if (issue && !workspaceWritable) return;
      const persona = action.persona ?? workbenchPhase;
      const context = workbenchActionContext(
        action,
        currentDocPath ?? workbenchCurrentDocPath,
      );
      if (workbenchConfig.status !== "ok") {
        pushToast("error", "Workbench unavailable", RIBBON_FALLBACK_CHAT_TITLE);
        return;
      }
      const configRevision = workbenchConfig.data.revision;
      if (!issue) {
        // A chat action never sends on its own (g5.workbench.chat-actions):
        // it seeds the composer with the expanded prompt, in the open chat
        // when that chat already carries the persona, otherwise in a fresh
        // one; the person's send starts or continues the chat.
        handleSeedChatPrompt(
          workbenchActionMessage(action, workbenchPhase, context, ""),
          undefined,
          {
            actionId: action.id,
            persona,
            phase: workbenchPhase,
            sessionId:
              activeChatSession && activeChatSession.persona === persona
                ? activeChatSession.id
                : null,
          },
        );
        return;
      }
      const scope = workspaceViewScopeRef.current;
      setPendingWorkbenchActionId(action.id);
      try {
        if (!acquireLaunchSubmission("chat-start")) {
          pushToast(
            "error",
            "A chat is already starting",
            "Try again when it finishes.",
          );
          return;
        }
        try {
          // The user's default harness choices, explicit like the chat
          // composer's pre-session draft: inheriting would require a
          // workspace-configured permission mode a fresh workspace lacks.
          const agentLaunch = await confirmAgentLaunch({
            ...syncAgentConfig,
            launchSource: "explicit",
          });
          const started = await startWorkbenchAction.mutateAsync({
            workspaceId,
            persona,
            // The clicked phase selects the template; ids repeat across
            // phases and the persona may be overridden to another phase.
            phase: workbenchPhase,
            ...(issue ? {} : { actionId: action.id, context }),
            configRevision,
            content: issue ? startMessageFromIssue(issue) : "",
            agentLaunch,
            scopeType: "workspace",
            scopeRef: workspaceId,
            // Implement from an issue starts at once: while the owner's VM
            // is stopped, the same request starts it (g5.chat.start-and-send).
            ...(startAndSend ? { startCompute: true } : {}),
          });
          rememberJourneyActionSession(action.id, started.session.id);
          if (!scope.active || workspaceViewScopeRef.current !== scope) return;
          handleViewSession(started.session.id);
        } finally {
          releaseLaunchSubmission("chat-start");
        }
      } catch (err) {
        if (!scope.active || workspaceViewScopeRef.current !== scope) return;
        const message =
          err instanceof Error ? err.message : "Could not start the action.";
        pushToast("error", `${action.label} failed`, message);
      } finally {
        setPendingWorkbenchActionId(null);
      }
    },
    [
      acquireLaunchSubmission,
      activeChatSession,
      confirmAgentLaunch,
      handleSeedChatPrompt,
      handleViewSession,
      pushToast,
      releaseLaunchSubmission,
      rememberJourneyActionSession,
      startAndSend,
      startWorkbenchAction,
      syncAgentConfig,
      workbenchConfig,
      workbenchCurrentDocPath,
      workspaceWritable,
      workbenchPhase,
      workspaceId,
    ],
  );
  // Linked-issue page actions (g5.workbench.issues): only for an issue linked
  // to this workspace.
  const issuePageControls = useCallback<IssuePageControls>(
    (tab) => {
      if (workspaceIssueLinks.status !== "ok") return undefined;
      const link = findWorkspaceIssueLink(workspaceIssueLinks.data.links, tab);
      if (!link) return undefined;
      // Implement starts the chat at once; while the owner's VM is stopped
      // the same request starts it, and the label says so
      // (g5.chat.start-and-send, g5.lifecycle.start).
      const implementAction: WorkbenchAction = {
        id: "implement-issue",
        kind: "chat",
        label: startAndSend
          ? "Start and implement in this workspace"
          : "Implement in this workspace",
      };
      const implementReason =
        workbenchActionState(implementAction)?.title ??
        (workbenchConfig.status !== "ok"
          ? RIBBON_FALLBACK_CHAT_TITLE
          : !link.issue
            ? "Issue details are unavailable"
            : undefined);
      const issueStateReason = externalIssuesEnabled
        ? issueStateBlockedReason(
            link,
            issueWriteBlockedReason,
            githubConnection,
          )
        : EXTERNAL_ISSUES_OFF_MESSAGE;
      return {
        actions: [
          {
            id: "implement",
            label: implementAction.label,
            disabled: !!implementReason || pendingWorkbenchActionId !== null,
            title: implementReason,
            onSelect: () => {
              if (link.issue)
                void handleWorkbenchChat(implementAction, link.issue);
            },
          },
          {
            id: "fork",
            label: "Fork in own workspace",
            disabled: !showCloudWorkspaceWorkflows,
            onSelect: () => {
              setCreateWorkspaceIssueRef(link.displayKey);
              setCreateWorkspaceDialogOpen(true);
            },
          },
          {
            id: "done",
            label: link.issue?.state === "closed" ? "Reopen" : "Mark as done",
            disabled: !!issueStateReason || changeIssueState.isLoading,
            title: issueStateReason ?? undefined,
            onSelect: () => void changeLinkedIssueState(link),
          },
          ...(canWriteIssueLinks
            ? [
                {
                  id: "detach",
                  label: "Detach from workspace",
                  disabled: !!issueWriteBlockedReason,
                  title: issueWriteBlockedReason ?? undefined,
                  onSelect: () => {
                    unlinkWorkspaceIssue.reset();
                    setDetachIssue(link);
                  },
                },
                {
                  id: "import",
                  label: "Import task",
                  disabled: !!issueLinkCreationBlockedReason,
                  title: issueLinkCreationBlockedReason ?? undefined,
                  onSelect: openImportTaskPicker,
                },
              ]
            : []),
        ],
        onRequestApproval: () => openWorkspaceApprovalsTab(),
      };
    },
    [
      canWriteIssueLinks,
      changeIssueState.isLoading,
      changeLinkedIssueState,
      externalIssuesEnabled,
      githubConnection,
      handleWorkbenchChat,
      issueWriteBlockedReason,
      openImportTaskPicker,
      openWorkspaceApprovalsTab,
      pendingWorkbenchActionId,
      showCloudWorkspaceWorkflows,
      unlinkWorkspaceIssue,
      startAndSend,
      workbenchActionState,
      workbenchConfig.status,
      workbenchPhase,
      workspaceIssueLinks,
    ],
  );
  // Review meeting (g5.workbench.meeting): the transcripts already stored
  // under meetings/ (not the decisions documents beside them), offered by the
  // dialog as the second source; and the extraction start both sources share.
  const meetingDocs = useMemo(() => {
    const transcripts: DocNode[] = [];
    const decisions: DocNode[] = [];
    if (!docsFolderPath) return { transcripts, decisions };
    const prefix = `${docsFolderPath}/${TRANSCRIPT_FOLDER}/`;
    for (const doc of docs) {
      const path = doc.filePath ?? "";
      if (!path.startsWith(prefix) || !path.endsWith(".md")) continue;
      if (path.endsWith(".decisions.md")) decisions.push(doc);
      else transcripts.push(doc);
    }
    return { transcripts, decisions };
  }, [docs, docsFolderPath]);
  const startDecisionExtraction = useCallback(
    (doc: DocNode) => {
      if (!workspaceWritable) return;
      const extract = findWorkbenchAction(
        ribbonActions[workbenchPhase],
        "extract-decisions",
      );
      if (extract?.kind === "chat") {
        void handleWorkbenchChat(extract, undefined, doc.filePath ?? undefined);
      }
    },
    [handleWorkbenchChat, ribbonActions, workbenchPhase, workspaceWritable],
  );
  // Build mock (g5.workbench.mock): the mocks already stored under mocks/,
  // newest first, offered by the dialog; and the hidden template its primary
  // action seeds, gated as the ribbon would gate the chat action itself.
  const mockDocs = useMemo(
    () => recentMockDocs(docs, docsFolderPath),
    [docs, docsFolderPath],
  );
  // A committed phase that lists Build mock without the hidden template (a
  // file from before the dialog, or one that names only the drawn actions)
  // still builds mocks from the built-in template: the browser expands the
  // prompt itself and sends the text under the persona, so no server-side
  // lookup of the id is involved (review of PR #971).
  const generateMockAction = useMemo(
    () =>
      findWorkbenchAction(ribbonActions[workbenchPhase], "generate-mock") ??
      findWorkbenchAction(WORKBENCH_ACTIONS[workbenchPhase], "generate-mock"),
    [ribbonActions, workbenchPhase],
  );
  const buildMockDisabledReason = useMemo(() => {
    if (generateMockAction?.kind !== "chat") {
      return "This phase has no mock-building template";
    }
    const ribbonState = ribbonActionState(
      generateMockAction,
      ribbonConfigStatus,
    );
    if (ribbonState.disabled) return ribbonState.title ?? "Unavailable";
    const pageState = workbenchActionState(generateMockAction);
    return pageState?.disabled ? (pageState.title ?? "Unavailable") : null;
  }, [generateMockAction, ribbonConfigStatus, workbenchActionState]);
  const startMockGeneration = useCallback(() => {
    if (buildMockDisabledReason !== null || !generateMockAction) return;
    void handleWorkbenchChat(generateMockAction);
  }, [buildMockDisabledReason, generateMockAction, handleWorkbenchChat]);

  const handleWorkbenchAction = useCallback(
    (action: WorkbenchAction) => {
      if (action.kind === "chat") {
        void handleWorkbenchChat(action);
        return;
      }
      if (action.kind !== "open") return;
      switch (action.target) {
        case "approvals":
          // Open the shared workspace approval list.
          openWorkspaceApprovalsTab();
          return;
        case "ship":
        case "merge":
          // Both reach the ship surface; Merge to parent merges the existing
          // PR there (`useMergeWorkspacePullRequest`), never the rebase-only
          // `confirmWorkspaceMergeForTarget` / `handleMergeReviewApproval`.
          if (shipSurfaceAvailable) setPrepareToShipOpen(true);
          return;
        case "rebase":
          // The source-only Rebase Workspace dialog (g5.workbench.ship-actions):
          // it never advances the target.
          if (rebaseSurfaceAvailable) handleOpenWorkspaceMerge();
          return;
        case "new-doc":
          handleOpenCreateProjectDoc();
          return;
        case "upload":
          setCreateProjectDocModal({
            open: true,
            title: "Upload Docs",
            initialMode: "upload",
          });
          return;
        case "meeting":
          // Review meeting (g5.workbench.meeting): one dialog takes an
          // uploaded transcript or one already under meetings/; either way
          // the extraction template starts over it (see the modal's
          // onCreated / onUseExisting).
          setCreateProjectDocModal({
            open: true,
            title: "Review meeting",
            initialMode: "transcript",
          });
          return;
        case "view": {
          const tab = action.tab ? WORKBENCH_VIEW_TABS[action.tab] : undefined;
          if (tab) openTab(tab);
          return;
        }
        case "explainer":
          // The existing Create Explainer modal (g5.workbench.journey.steps).
          setCreateExplainerModal({ open: true });
          return;
        case "mock":
          // Build mock (g5.workbench.mock): one dialog lists the stored mocks
          // and seeds the generation template (see the modal's onBuild).
          setBuildMockOpen(true);
          return;
        case "review":
          // One prompt definition for every Review Workspace entry point.
          seedReviewPrompt();
          return;
        case "verification":
          // Re-run Verification (g5.workbench.review-prompts).
          seedVerificationPrompt();
          return;
        default:
          // `preview` renders its own control; unknown targets are disabled
          // by the ribbon and never reach here.
          return;
      }
    },
    [
      handleOpenCreateProjectDoc,
      handleOpenWorkspaceMerge,
      handleWorkbenchChat,
      openTab,
      openWorkspaceApprovalsTab,
      rebaseSurfaceAvailable,
      seedReviewPrompt,
      seedVerificationPrompt,
      shipSurfaceAvailable,
      workbenchPhase,
    ],
  );
  // A preview Activity or rail row opens the ribbon's preview dialog (ui
  // g5.ui.activity.preview-row): each request is a new sequence number so a
  // repeated click reopens a dialog the owner closed meanwhile.
  const [previewOpenRequest, setPreviewOpenRequest] = useState<
    (PreviewOpenRequest & { seq: number }) | undefined
  >(undefined);
  const handleOpenPreview = useCallback((request: PreviewOpenRequest) => {
    setPreviewOpenRequest((prev) => ({
      ...request,
      seq: (prev?.seq ?? 0) + 1,
    }));
  }, []);
  // Only the owner has a preview dialog to open, and only when the ribbon
  // actually mounts the general preview control (not on Base or personal
  // workspaces, and not while the action is disabled); otherwise the rows
  // stay plain text.
  const previewRowsOpenDialog = useMemo(() => {
    if (
      !isWorkspaceOwner ||
      currentWorkspaceIsBase ||
      currentWorkspaceIsPersonal
    )
      return false;
    const generalPreview = (ribbonActions[workbenchPhase] ?? [])
      .flatMap((group) => group.actions)
      .find(
        (action) =>
          action.kind === "open" &&
          action.target === "preview" &&
          !action.hidden,
      );
    return !!generalPreview && !workbenchActionState(generalPreview)?.disabled;
  }, [
    currentWorkspaceIsBase,
    currentWorkspaceIsPersonal,
    isWorkspaceOwner,
    ribbonActions,
    workbenchActionState,
    workbenchPhase,
  ]);
  const onOpenPreviewRow = previewRowsOpenDialog
    ? handleOpenPreview
    : undefined;
  const renderWorkbenchAction = useCallback(
    (
      action: WorkbenchAction,
      button: RibbonButtonShape,
      idPrefix = "ribbon",
    ) => {
      if (action.kind !== "open" || action.target !== "preview") {
        return undefined;
      }
      // The trigger takes the style's shape so it sits among its neighbours
      // (g5.workbench.header-styles).
      return (
        <PreviewControl
          workspaceId={workspaceId}
          isDirty={previewDirtyCount > 0}
          mode="unmerged"
          label={action.label}
          triggerClassName={button.className}
          triggerTestId={`${idPrefix}-preview-${action.id}`}
          iconSize={button.iconSize}
          labelHidden={button.labelHidden}
          // The ribbon's preview control is the one instance a row's request
          // reaches; the guided journey renders the same action again.
          openRequest={idPrefix === "ribbon" ? previewOpenRequest : undefined}
        />
      );
    },
    [previewDirtyCount, previewOpenRequest, workspaceId],
  );
  // "Follow view" (g5.workbench.persona): fork the open chat under the
  // current phase's persona; the server starts fresh for an empty source.
  const handleFollowView = useCallback(async () => {
    if (!activeChatSession || workbenchConfig.status !== "ok") return;
    const scope = workspaceViewScopeRef.current;
    try {
      const forked = await followView.mutateAsync({
        sessionId: activeChatSession.id,
        workspaceId,
        atIndex: Math.max(0, activeChatSession.messageCount - 1),
        persona: workbenchPhase,
        configRevision: workbenchConfig.data.revision,
      });
      if (!scope.active || workspaceViewScopeRef.current !== scope) return;
      handleViewSession(forked.sessionId);
    } catch (err) {
      if (!scope.active || workspaceViewScopeRef.current !== scope) return;
      pushToast(
        "error",
        "Could not follow the view",
        err instanceof Error ? err.message : "Unknown error",
      );
    }
  }, [
    activeChatSession,
    followView,
    handleViewSession,
    pushToast,
    workbenchConfig,
    workbenchPhase,
    workspaceId,
  ]);
  const activeSessionPersona = isWorkspacePhase(activeChatSession?.persona)
    ? activeChatSession.persona
    : null;

  const handleSelectFile = useCallback(
    (branchId: string, path: string, lineRange?: [number, number]) => {
      // Files selected from the file tree are raw workspace files; do not
      // resolve them through the ontology tree because src_files frontmatter
      // can mention the same path without representing a standalone node.
      // `lineRange` (present only for citation clicks) is carried on the tab so
      // the editor reveals the cited line (chat_service D54).
      openTab({
        ...workspaceFileTab(path, undefined, branchId, lineRange),
      });
    },
    [openTab],
  );

  // Repo-qualified citations use explicit active ownership or unique branch
  // cardinality. They never fall back to another repo or to presentation
  // ordering (workspace-page-shell D39 / chat_service D55). Legacy citations
  // without repoId preserve the historical active-branch behavior.
  const handleOpenCitationFile = useCallback(
    (filePath: string, lineRange?: [number, number], repoId?: string): void => {
      const resolution = resolveCitationBranch(
        allSortedBranches,
        activeBranchId,
        repoId,
      );
      if (resolution.status === "resolved") {
        handleSelectFile(resolution.branchId, filePath, lineRange);
        return;
      }

      const detail =
        resolution.reason === "ambiguous-repo"
          ? "The cited repository has multiple visible branches and no authoritative active branch."
          : resolution.reason === "repo-not-visible"
            ? "The cited repository is not visible in this workspace."
            : "This workspace has no active branch for the legacy citation.";
      pushToast(
        "error",
        "Cannot open citation",
        `${detail} No file was opened.`,
      );
    },
    [activeBranchId, allSortedBranches, handleSelectFile, pushToast],
  );

  // A changes card's link to an ontology statement opens its file at that
  // statement, as the editor's own statement links do; opening the tab
  // first lets the scroll request win the tab switch.
  const handleOpenOntologyStatement = useCallback(
    (filePath: string, repoId: string | undefined, statementId: string) => {
      handleOpenCitationFile(filePath, undefined, repoId);
      setScrollToIntentRecord(`#${statementId}`);
    },
    [handleOpenCitationFile, setScrollToIntentRecord],
  );

  // g5://file deep links resolve repository ownership BEFORE branch
  // selection (center-panel D73): an explicit ?repo= qualifier decodes
  // against the branch inventory, else the managed docs namespace resolves
  // by workspace metadata, else the unique non-docs repo; ambiguity fails
  // closed naming the candidate repositories. The resolved repoId feeds the
  // D39 citation pathway — deep links never hit its legacy-active arm
  // (issue #1970: that arm is always code-backed, so Project Docs targets
  // opened against the wrong repository).
  const deepLinkDocsMeta = useMemo(
    () =>
      workspace.status === "ok"
        ? {
            docsRepoId: workspace.data.docsRepoId,
            docsFolderPath: workspace.data.docsFolderPath,
          }
        : { docsRepoId: null, docsFolderPath: "" },
    [workspace],
  );
  const handleOpenDeepLinkFile = useCallback(
    (filePath: string, repoRef?: string, line?: number): void => {
      const ownership = resolveDeepLinkRepoId(
        allSortedBranches,
        deepLinkDocsMeta,
        filePath,
        repoRef,
      );
      if (ownership.status === "resolved") {
        handleOpenCitationFile(
          filePath,
          line ? [line, line] : undefined,
          ownership.repoId,
        );
        return;
      }
      // Qualified candidates: display names can collide across repos, so
      // every candidate keeps its repo ID — "common (repo-a), common
      // (repo-b)" — giving the reader the identity an explicit `?repo=`
      // qualifier needs (invariant D3: the error lists QUALIFIED candidates).
      const candidateLabels = ownership.candidateRepoIds.map((repoId) => {
        const repoName = allSortedBranches.find(
          (branch) => branch.repoId === repoId,
        )?.repoName;
        return repoName ? `${repoName} (${repoId})` : repoId;
      });
      const detail =
        ownership.reason === "unknown-repo-ref"
          ? `The link names repository "${repoRef}", which is not visible in this workspace.`
          : ownership.reason === "ambiguous-repo-ref"
            ? `The link's repository name matches multiple repositories: ${candidateLabels.join(", ")}.`
            : ownership.reason === "ambiguous-repo"
              ? `The file path is valid in multiple repositories: ${candidateLabels.join(", ")}. Add an explicit repo qualifier to the link.`
              : "No repository in this workspace can own the linked file.";
      pushToast(
        "error",
        "Cannot open file link",
        `${detail} No file was opened.`,
      );
    },
    [allSortedBranches, deepLinkDocsMeta, handleOpenCitationFile, pushToast],
  );

  // Shared Markdown and chat file links use one workspace-level delegate.
  // Revalidate the repository/path before opening an authorized file tab.
  useEffect(() => {
    const onDeepLinkClick = (event: MouseEvent) => {
      if (event.defaultPrevented || event.button !== 0) return;
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
        return;
      }
      const target = event.target as HTMLElement | null;
      const anchor = target?.closest?.("a[data-g5-file-link]");
      if (!anchor) return;
      event.preventDefault();
      const href = anchor.getAttribute("href") ?? "";
      if (href.startsWith("g5://file/")) {
        const parsed = parseG5FileHref(href);
        if (!parsed) return;
        handleOpenDeepLinkFile(parsed.filePath, parsed.repoRef, parsed.line);
      }
    };
    document.addEventListener("click", onDeepLinkClick);
    return () => document.removeEventListener("click", onDeepLinkClick);
  }, [handleOpenDeepLinkFile]);

  // ---------------------------------------------------------------------------
  // Notification handlers
  // ---------------------------------------------------------------------------

  const handleNotificationAction = useCallback(
    (id: string, action: NotificationAction) => {
      notifications.dismiss(id);
      // `workspace:<id>[?query]` targets (review notifications) open that
      // workspace's page with the query preserved, e.g. `?view=reviews`.
      // The user channel delivers notifications from every organization the
      // viewer belongs to, so a workspace outside the current org's list is
      // resolved through its detail (name and org) before navigating.
      if (
        action.action === "navigate" &&
        action.target.startsWith("workspace:")
      ) {
        const rest = action.target.slice("workspace:".length);
        const queryIndex = rest.indexOf("?");
        const targetId = queryIndex === -1 ? rest : rest.slice(0, queryIndex);
        const query = queryIndex === -1 ? "" : rest.slice(queryIndex);
        const name = workspaceNamesById[targetId];
        if (name && org) {
          navigate(`/${org}/${encodeURIComponent(name)}${query}`);
          return;
        }
        void (async () => {
          try {
            const detail = await api.get<WorkspaceDetail>(
              `/api/v1/workspaces/${targetId}`,
            );
            const slug =
              user?.orgs.find((candidate) => candidate.orgId === detail.orgId)
                ?.orgSlug ?? org;
            if (slug && detail.name) {
              navigate(`/${slug}/${encodeURIComponent(detail.name)}${query}`);
            }
          } catch {
            // The notification is dismissed either way; the target may be
            // gone or no longer readable.
          }
        })();
      }
    },
    [notifications, workspaceNamesById, org, navigate, user],
  );

  // ---------------------------------------------------------------------------
  // Render: Workspace not found
  // ---------------------------------------------------------------------------

  if (resolvedId.status === "error") {
    // `/{org}/me` refuses viewers, who own no workspace (g5.personal.lifecycle).
    const personalRefused =
      routeFamily === "me" &&
      resolvedId.error instanceof ApiError &&
      resolvedId.error.status === 403;
    return (
      <div className="flex h-screen w-full flex-col items-center justify-center gap-4 bg-background text-foreground">
        <AlertCircle className="h-12 w-12 text-muted-foreground" />
        <h1 className="text-xl font-semibold">
          {personalRefused
            ? "No personal workspace"
            : personalRoute
              ? "Personal workspace not found"
              : "Workspace not found"}
        </h1>
        <p
          className="max-w-xl text-center text-sm text-muted-foreground"
          data-testid="workspace-not-found-message"
        >
          {personalRefused ? (
            <>
              Viewers have no personal workspace in this organization. An
              administrator can make you a member.
            </>
          ) : personalRoute ? (
            <>
              The personal workspace{" "}
              <span className="font-mono text-foreground">
                {routeFamily === "me"
                  ? `${org}/me`
                  : `${org}/users/${personalHandleParam ?? ""}`}
              </span>{" "}
              does not exist or you do not have access. Only its owner and
              organization administrators can open it.
            </>
          ) : (
            <>
              The workspace{" "}
              <span className="font-mono text-foreground">
                {org}/{workspaceSlug}
              </span>{" "}
              does not exist or you do not have access.
            </>
          )}
        </p>
        <button
          data-testid="workspace-not-found-back-to-projects"
          className="mt-2 rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground hover:bg-primary/90"
          onClick={() => navigate(`/${org}`)}
        >
          Back to projects
        </button>
      </div>
    );
  }

  if (resolvedId.status === "loading" || workspace.status === "loading") {
    // The top bar's report trigger follows; hold off the floating fallback.
    return (
      <div className="flex h-screen w-full items-center justify-center bg-background">
        <PlatformIssueReportChromePending />
        <div className="h-8 w-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
      </div>
    );
  }

  if (workspace.status === "error") {
    return (
      <div className="flex h-screen w-full flex-col items-center justify-center gap-4 bg-background px-6 text-center text-foreground">
        <AlertCircle className="h-12 w-12 text-muted-foreground" />
        <h1 className="text-xl font-semibold">Failed to load workspace</h1>
        <p className="max-w-xl text-sm text-muted-foreground">
          {formatQueryErrorMessage(
            workspace.error,
            "The workspace detail request failed.",
          )}
        </p>
        <button
          data-testid="workspace-load-error-back-to-projects"
          className="mt-2 rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground hover:bg-primary/90"
          onClick={() => navigate(`/${org}`)}
        >
          Back to projects
        </button>
      </div>
    );
  }

  // ---------------------------------------------------------------------------
  // Render: Mobile layout
  // ---------------------------------------------------------------------------

  // The personal workbench says who can see it (g5.personal.access).
  const personalOwnerName =
    orgMembers.status === "ok"
      ? (orgMembers.data.find(
          (member) => member.userId === workspaceDetailData?.ownerUserId,
        )?.displayName ?? null)
      : null;
  const personalWorkspaceNotice = currentWorkspaceIsPersonal ? (
    <PersonalWorkspaceNotice
      isOwner={isWorkspaceOwner}
      ownerName={personalOwnerName}
    />
  ) : null;
  if (isMobile && workspaceChatAvailable) {
    return (
      <ComposerOwner scope={workspaceViewScopeKey}>
        <WorkspacePageProviders
          names={workspaceNamesById}
          liveGate={liveGate}
          workspaceId={workspaceId}
        >
          <ErrorBoundary>
            <MobileLayout
              chatPromptHistoryScope={workspaceChatPromptHistoryScope}
              chatSurfaceVisible={chatSurfaceVisible}
              onAcknowledgeChatResult={acknowledgeChatResult}
              onRenameChatSession={
                workspaceWritable ? handleRenameChatSession : undefined
              }
              unreadNotificationCount={notifications.active.length}
              onOpenNotifications={() => setNotificationsOpen(true)}
              banner={
                <>
                  {reviewWorkspaceBanner}
                  {personalWorkspaceNotice}
                  {workspaceStateBannerElement}
                  {/* The owner's lifecycle control stays through every
                    state, running included (`g5.workspace.ui.vm-state`). */}
                  {isWorkspaceOwner && !currentWorkspaceIsBase && (
                    <div
                      data-testid="workspace-mobile-lifecycle"
                      className="flex flex-wrap items-center gap-2 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-1.5"
                    >
                      <WorkspaceVmStatusStrip
                        vm={workspaceVm}
                        ownerUserId={workspaceDetailData?.ownerUserId ?? null}
                        members={
                          orgMembers.status === "ok" ? orgMembers.data : []
                        }
                        canControl={isWorkspaceOwner}
                        canTransfer={false}
                        isBase={currentWorkspaceIsBase}
                        busy={workspaceVmBusy}
                        error={workspaceVmActionError?.message}
                        onResume={() => void handleResumeWorkspace()}
                        onStop={() => void handleStopWorkspace()}
                        onOpenTerminal={openWorkspaceTerminalTab}
                        onTransfer={() => {}}
                        showTerminal={false}
                      />
                    </div>
                  )}
                </>
              }
              chatSession={activeChatSession}
              chatSessions={visibleChatSessions}
              chatMessages={
                chatSessionData.status === "ok" ? chatSessionData.data : []
              }
              isChatStreaming={isChatStreaming}
              stopControlSuppressed={
                isChatStopControlSuppressed || !canControlLiveChatTurn
              }
              stopState={chatStopState}
              liveTailState={chatLiveTailState}
              chatTimelineEntries={chatTimelineEntries}
              chatHasEarlierHistory={timelineHasEarlierHistory}
              chatIsLoadingEarlierHistory={timelineIsLoadingEarlierHistory}
              chatTimelineCoverage={apiTimeline.coverage}
              chatEarlierHistoryError={apiTimeline.appendError}
              onLoadEarlierChatHistory={handleLoadEarlierTimelineHistory}
              chatHasOlderMessages={chatMessagesHasOlder}
              chatIsLoadingOlderMessages={chatMessagesIsLoadingOlder}
              onLoadOlderChatMessages={handleLoadOlderChatMessages}
              activeChatTaskSummary={activeChatTaskSummary}
              workspaceId={workspaceId}
              workspaceSlug={workspaceSlug ?? "Workspace"}
              selectedAgentProvider={selectedAgentProvider}
              availableAgentProviders={availableAgentProviders}
              providerAccountsEnabled={providerAccountsEnabled}
              providerAccounts={providerAccounts}
              providerAccountAssignments={providerAccountAssignments}
              runtimeVariablesAvailable={chatRuntimeVariablesAvailable}
              onSelectAgentProvider={handleSelectAgentProvider}
              resolveAgentLaunch={resolveAgentLaunchPreferences}
              setupError={chatSetupError}
              composerDisabledReason={composerDisabledReason}
              startAndSend={startAndSend}
              composerNotice={composerNotice}
              canManageSessionInputs={canManageDurableOwnerState}
              projectReferences={chatProjectReferences}
              activeProjectReference={activeChatProjectReference}
              composerSeed={chatComposerSeed}
              composerDraft={
                workspaceId ? chatComposerDraftFor(workspaceId) : undefined
              }
              onComposerSeedEmptied={handleComposerSeedEmptied}
              composerPlaceholder={chatComposerPlaceholder}
              composerScope={chatComposerScope}
              defaultClaudeModel={chatDefaultClaudeModel}
              defaultAgentProviderChoices={
                userSettings.defaultAgentProviderChoices
              }
              subscriptionUsageWindow={userSettings.subscriptionUsageWindow}
              workspaceSubscriptionUsage={workspaceSubscriptionUsage}
              onResolveAgentApproval={
                workspaceWritable ? handleResolveAgentApproval : undefined
              }
              onSendProviderControl={
                workspaceWritable ? acceptProviderSettingsLocally : undefined
              }
              onSendChatMessage={handleSendChatMessage}
              onStopStreaming={
                canControlLiveChatTurn ? handleStopStreaming : undefined
              }
              onEndBackgroundWait={
                canControlLiveChatTurn ? handleEndBackgroundWait : undefined
              }
              onSelectChatSession={handleViewSession}
              onCreateChatSession={
                workspaceWritable ? handleCreateChatSession : undefined
              }
            />
            {/* Finish review offers removal from the mobile banner too. */}
            {removeWorkspaceRequest && (
              <RemoveWorkspaceDialog
                workspace={removeWorkspaceRequest}
                isRemoving={deleteWorkspace.isLoading}
                errorMessage={deleteWorkspace.error?.message ?? null}
                onClose={handleCloseRemoveWorkspaceDialog}
                onConfirm={handleConfirmRemoveWorkspace}
              />
            )}
            {toasts.length > 0 && (
              <NotificationToastStack
                notifications={toasts}
                onDismiss={dismissToast}
                onAction={handleClientToastAction}
              />
            )}
          </ErrorBoundary>
        </WorkspacePageProviders>
      </ComposerOwner>
    );
  }

  // ---------------------------------------------------------------------------
  // Render: Desktop layout
  // ---------------------------------------------------------------------------

  const renderWorkspaceChat = (controls?: {
    layout?: "panel" | "center";
    onDetach?: () => void;
    onClose?: () => void;
  }) => (
    <ChatPanel
      key={workspaceId}
      draft={workspaceId ? chatComposerDraftFor(workspaceId) : undefined}
      promptHistoryScope={workspaceChatPromptHistoryScope}
      surfaceVisible={chatSurfaceVisible}
      onAcknowledgeResult={acknowledgeChatResult}
      layout={controls?.layout}
      // Persona pill (workbench g5.workbench.persona): the open session's
      // persona (the workspace phase before a session exists) beside the
      // Chat label, with "follow view" when the workbench phase differs.
      phaseScope={{
        phase: activeSessionPersona ?? workbenchPhase,
        label: WORKBENCH_PHASES[activeSessionPersona ?? workbenchPhase].short,
        source: activeSessionPersona ? "persona" : "phase",
        followView:
          activeChatSession &&
          workspaceWritable &&
          activeSessionPersona !== workbenchPhase
            ? {
                targetPhase: workbenchPhase,
                onFollow: () => void handleFollowView(),
                pending: followView.isLoading,
              }
            : undefined,
      }}
      session={activeChatSession}
      focusApprovalId={verifiedAskUserQuestionFocusId}
      onFocusApprovalConsumed={consumeAskUserQuestionFocus}
      messages={chatSessionData.status === "ok" ? chatSessionData.data : []}
      isStreaming={isChatStreaming}
      stopControlSuppressed={
        isChatStopControlSuppressed || !canControlLiveChatTurn
      }
      stopState={chatStopState}
      liveTailState={chatLiveTailState}
      timelineEntries={chatTimelineEntries}
      hasEarlierHistory={timelineHasEarlierHistory}
      isLoadingEarlierHistory={timelineIsLoadingEarlierHistory}
      onLoadEarlierHistory={handleLoadEarlierTimelineHistory}
      timelineCoverage={apiTimeline.coverage}
      earlierHistoryError={apiTimeline.appendError}
      hasOlderMessages={chatMessagesHasOlder}
      isLoadingOlderMessages={chatMessagesIsLoadingOlder}
      onLoadOlderMessages={handleLoadOlderChatMessages}
      activeTaskSummary={activeChatTaskSummary}
      scopeType={effectiveChatScopeType}
      scopeRef={effectiveChatScopeRef}
      scopeLabel={effectiveChatScopeLabel}
      sessions={visibleChatSessions}
      selectedAgentProvider={selectedAgentProvider}
      availableAgentProviders={availableAgentProviders}
      providerAccountsEnabled={providerAccountsEnabled}
      providerAccounts={providerAccounts}
      providerAccountAssignments={providerAccountAssignments}
      runtimeVariablesAvailable={chatRuntimeVariablesAvailable}
      onSelectAgentProvider={handleSelectAgentProvider}
      resolveAgentLaunch={resolveAgentLaunchPreferences}
      setupError={chatSetupError}
      composerDisabledReason={composerDisabledReason}
      startAndSend={startAndSend}
      composerNotice={composerNotice}
      canManageSessionInputs={canManageDurableOwnerState}
      projectReferences={chatProjectReferences}
      activeProjectReference={activeChatProjectReference}
      composerSeed={chatComposerSeed}
      onComposerSeedEmptied={handleComposerSeedEmptied}
      composerPlaceholder={chatComposerPlaceholder}
      composerScope={chatComposerScope}
      focusRequest={chatFocusRequest}
      defaultClaudeModel={chatDefaultClaudeModel}
      defaultAgentProviderChoices={userSettings.defaultAgentProviderChoices}
      subscriptionUsageWindow={userSettings.subscriptionUsageWindow}
      workspaceSubscriptionUsage={workspaceSubscriptionUsage}
      // Answering an agent needs the running VM; the panel disables the
      // buttons with the reason while the owner's VM is stopped. Sessions,
      // forks and renames are platform records and follow durable authority.
      onResolveAgentApproval={
        workspaceWritable ? handleResolveAgentApproval : undefined
      }
      onSendProviderControl={
        workspaceWritable ? acceptProviderSettingsLocally : undefined
      }
      onSendMessage={handleSendChatMessage}
      onStopStreaming={canControlLiveChatTurn ? handleStopStreaming : undefined}
      onEndBackgroundWait={
        canControlLiveChatTurn ? handleEndBackgroundWait : undefined
      }
      onOpenOntologyChanges={() =>
        openTab({ type: "ontology_changes", name: "Ontology changes" })
      }
      onForkSession={
        !workspaceWritable
          ? undefined
          : (atIdx) => {
              if (activeChatSession) {
                forkSession.mutate({
                  sessionId: activeChatSession.id,
                  workspaceId: workspaceId || "",
                  atIndex: atIdx,
                });
              }
            }
      }
      onSummarize={() => {}}
      onCreateSession={workspaceWritable ? handleCreateChatSession : undefined}
      onSelectSession={handleViewSession}
      onRenameSession={workspaceWritable ? handleRenameChatSession : undefined}
      onDetach={controls?.onDetach}
      onClose={controls?.onClose}
      onOpenFile={handleOpenCitationFile}
      onOpenOntologyStatement={handleOpenOntologyStatement}
    />
  );

  const activeCenterIsAdmin = !centerChatActive && isAdminCenterTab;

  // On a desktop the title bar is gone (ui g5.ui.workbench-shell):
  // the strip carries the global controls and the ribbon's right end the
  // workspace-bound ones, in the person's action-bar style.
  // On a phone the desktop render path serves only Base (every other
  // workspace takes the mobile layout), and that page keeps its title bar:
  // the strip's and the ribbon's controls are a desktop arrangement.
  const desktopChrome = !isMobile;
  const shellControls = desktopChrome ? (
    <ShellControls
      onOpenWorkerHosts={canManageWorkers ? openWorkerHostsTab : undefined}
      notifications={{
        unreadCount: notifications.active.length,
        onOpen: () => setNotificationsOpen(true),
      }}
      onOpenSettings={openWorkspaceSettingsTab}
      onOpenAccount={() => setSettingsOpen(true)}
    />
  ) : undefined;
  // A render function: the controls take the ribbon's rendered style, which
  // under the dynamic action bar follows the row's room
  // (g5.workbench.header-styles).
  // The Overview control leads the ribbon before the phase's first action
  // and opens the workspace page (workbench g5.workbench.landing).
  const ribbonOverviewControl = desktopChrome
    ? (style: RibbonStyle) => {
        const shape = ribbonButtonShape(style, {});
        return (
          <RibbonShapedButton
            shape={shape}
            icon={
              <LayoutDashboard
                size={shape.iconSize}
                className="shrink-0"
                aria-hidden="true"
              />
            }
            label="Overview"
            title="Open the workspace overview"
            testId="ribbon-overview"
            onClick={openWorkspaceActivityTab}
          />
        );
      }
    : undefined;
  const ribbonWorkspaceControls = desktopChrome
    ? (style: RibbonStyle) => (
        <WorkbenchWorkspaceControls
          // No open menu or picked recipient survives a workspace switch.
          key={workspaceId}
          variant={style}
          vm={workspaceVm}
          ownerUserId={workspaceDetailData?.ownerUserId ?? null}
          members={orgMembers.status === "ok" ? orgMembers.data : []}
          canControl={isWorkspaceOwner}
          canTransfer={canTransferCurrentWorkspace}
          isBase={currentWorkspaceIsBase}
          busy={workspaceVmBusy}
          error={workspaceVmActionError?.message}
          onResume={() => void handleResumeWorkspace()}
          onStop={() => void handleStopWorkspace()}
          onTransfer={(toUserId) => void handleTransferWorkspace(toUserId)}
          onOpenDetails={openWorkspaceDetailsTab}
          // The center Chat tab makes a toggle redundant; with the tab strip
          // hidden it is the way back to Chat (g5.workspace.ui.center-tabs).
          chatToggle={
            workspaceChatAvailable && !userSettings.showTabBar
              ? { open: centerChatActive, onToggle: handleToggleChatPanel }
              : undefined
          }
        />
      )
    : undefined;
  const ribbonApprovalsAction =
    desktopChrome && !reviewExcluded
      ? (style: RibbonStyle) => {
          const shape = ribbonButtonShape(style, {});
          return (
            <RibbonShapedButton
              shape={shape}
              icon={<ShieldCheck size={shape.iconSize} aria-hidden="true" />}
              label="Approvals"
              testId="ribbon-workspace-approvals"
              onClick={() => openWorkspaceApprovalsTab()}
            />
          );
        }
      : undefined;
  // The rail's Files header carries Terminal under alignment, where the
  // title bar's shortcut is gone (g5.workbench.shell-chrome): the owner's
  // control, enabled while the VM runs, else disabled with the reason
  // (`g5.workspace.ui.unavailable`).
  // Plain derivation, not a hook: this sits after the page's early returns.
  const railTerminalAction = (() => {
    if (!desktopChrome || !isWorkspaceOwner || currentWorkspaceIsBase) {
      return undefined;
    }
    const terminal = terminalControlState(workspaceVm);
    return {
      onActivate: openWorkspaceTerminalTab,
      disabled: !terminal.enabled,
      title: terminal.title,
      blockedReason: terminal.enabled ? null : terminal.title,
    };
  })();

  return (
    <ComposerOwner scope={workspaceViewScopeKey}>
      <WorkspacePageProviders
        names={workspaceNamesById}
        liveGate={liveGate}
        workspaceId={workspaceId}
      >
        <ErrorBoundary>
          <AppShell shellControls={shellControls}>
            {/* Title bar — the phone's Base page only: on a desktop the tab
              strip and the ribbon carry its controls and the rail's Activity
              row its collapse control (ui g5.ui.workbench-shell). */}
            {!desktopChrome && (
              <TopBar
                workspace={workspace.status === "ok" ? workspace.data : null}
                orgId={resolvedOrgId || org || ""}
                orgSlug={org || ""}
                leftSidebarOpen={leftSidebarOpen}
                onToggleLeftSidebar={() => setLeftSidebarOpen((prev) => !prev)}
                rightPanelOpen={centerChatActive}
                onToggleChat={handleToggleChatPanel}
                chatAvailable={workspaceChatAvailable}
                // The center Chat tab makes this toggle redundant; with the tab
                // strip hidden it is the way back to Chat from a content tab
                // (workspaces g5.workspace.ui.center-tabs).
                showChatToggle={!userSettings.showTabBar}
                onOpenSettings={openWorkspaceSettingsTab}
                onOpenWorkerHosts={
                  canManageWorkers ? openWorkerHostsTab : undefined
                }
                onOpenWorkspaceDetails={openWorkspaceDetailsTab}
                onOpenOrgManagement={() => setSettingsOpen(true)}
                onOpenNotifications={() => setNotificationsOpen(true)}
                onSelectWorkspace={(id) => {
                  const ws = allWorkspacesData.find((w) => w.id === id);
                  if (ws) {
                    resetTaskSelection();
                    navigate(`/${org}/${encodeURIComponent(ws.name)}`);
                  }
                }}
                onNewWorkspace={() => {
                  if (showCloudWorkspaceWorkflows) {
                    setCreateWorkspaceIssueRef(null);
                    setCreateWorkspaceDialogOpen(true);
                  }
                }}
                onForkCurrentWorkspace={
                  publicationWorkflows && !reviewExcluded
                    ? () => setCloneSourceWorkspace(currentWorkspaceSummary)
                    : undefined
                }
                onForkWorkspace={
                  showCloudWorkspaceWorkflows
                    ? (ws) => setCloneSourceWorkspace(ws)
                    : undefined
                }
                onRemoveWorkspace={handleRequestRemoveWorkspace}
                canRemoveWorkspace={canRemoveWorkspace}
                collaborators={collaborators}
                unreadNotificationCount={notifications.active.length}
                isMobile={false}
                capabilities={workspaceUiCapabilities}
                isReviewWorkspace={isReviewCopy}
                reviewIdentityKnown={reviewIdentityKnown}
                onOpenActivity={
                  externalIssuesEnabled ? openWorkspaceActivityTab : undefined
                }
                primaryIssue={topBarPrimaryIssue}
              />
            )}
            {workspaceShownAsLastLoaded && (
              // A transient failure keeps the page as last loaded, and what is
              // typed in it; a refusal still shows the load error.
              <div
                role="status"
                data-testid="workspace-refresh-error"
                className="border-b border-[var(--g5-warn)]/40 bg-[var(--g5-bg1)] px-4 py-1.5 text-xs text-[var(--g5-warn)]"
              >
                Can&apos;t refresh this workspace: showing it as it was last
                loaded. What you type stays here; actions may fail until it is
                back.
              </div>
            )}

            {/* Review workspace banner: this workspace is a reviewer-owned copy
              of another workspace. Finish review lives only here (and in the
              mobile layout's banner slot). */}
            {reviewWorkspaceBanner}

            {personalWorkspaceNotice}

            {workspaceStateBannerElement}

            {/* Credential warning banners directly below the banners
              (workspace-page-shell D40). Suppressed entirely in local mode. */}
            {showCloudWorkspaceWorkflows && (
              <WorkspaceHealthBanners
                workspaceConfig={workspaceConfigForDefaultAgent}
                settingsHref={
                  workspaceRoutePath
                    ? `${workspaceRoutePath}?view=settings`
                    : undefined
                }
                onOpenSettings={openWorkspaceSettingsTab}
              />
            )}

            {!(prepareToShipOpen && prepareToShipTarget) &&
              pullRequestDestinationStatus}

            {/* Workbench header (ui g5.ui.workbench-shell): the three phase
              chevrons and the per-phase action ribbon. The
              phase is explicit owner-moved state (g5.workbench.phases.state):
              Base and non-owners see the chevrons read-only, Base always at
              BE. Ribbon actions dispatch through the orchestrator above
              (g5.workbench.actions). */}
            {workspace.status === "ok" && (
              <div
                data-testid="workbench-header"
                data-phase-bar={userSettings.phaseBarStyle}
                data-action-bar={userSettings.actionBarStyle}
                className={classNames(
                  "flex shrink-0",
                  // The vertical phase bar (g5.workbench.header-styles) is a
                  // column at the left of the action-bar row; the other
                  // styles stack the two rows.
                  userSettings.phaseBarStyle === "vertical"
                    ? "flex-row items-stretch"
                    : "flex-col",
                )}
              >
                {/* A personal workspace has no phases
                  (g5.personal.workbench). */}
                {!currentWorkspaceIsPersonal && (
                  <PhaseChevrons
                    phase={workbenchPhase}
                    variant={userSettings.phaseBarStyle}
                    readOnly={!isWorkspaceOwner || currentWorkspaceIsBase}
                    pending={updateWorkspacePhase.isLoading}
                    error={updateWorkspacePhase.error?.message ?? null}
                    onSelectPhase={(phase) =>
                      updateWorkspacePhase.mutate(phase)
                    }
                  />
                )}
                {/* Base offers no template actions (workbench
                  g5.workbench.actions; workspaces "Omit Chat from Base"), so
                  its ribbon row carries only the workspace controls
                  (g5.workbench.shell-chrome). */}
                <div className="min-w-0 flex-1">
                  <ActionRibbon
                    phase={workbenchPhase}
                    variant={userSettings.actionBarStyle}
                    actions={
                      currentWorkspaceIsBase || currentWorkspaceIsPersonal
                        ? EMPTY_RIBBON_ACTIONS
                        : ribbonActions
                    }
                    configStatus={
                      currentWorkspaceIsBase || currentWorkspaceIsPersonal
                        ? "builtin"
                        : ribbonConfigStatus
                    }
                    configError={ribbonConfigError}
                    onRetryConfig={workbenchConfig.refetch}
                    actionState={workbenchActionState}
                    onAction={handleWorkbenchAction}
                    pendingActionId={pendingWorkbenchActionId}
                    renderAction={renderWorkbenchAction}
                    endActions={ribbonApprovalsAction}
                    leading={ribbonOverviewControl}
                    trailing={ribbonWorkspaceControls}
                  />
                </div>
              </div>
            )}

            {/* Main content area: 3-column flex */}
            <div className="flex flex-1 overflow-hidden">
              {/* Left Sidebar — stacked/collapsible sections stay mounted according
              to the user's leftSidebarOpen + width preferences. Legacy
              admin-surface compatibility props are still passed until the shell
              API is cleaned up, but LeftSidebar ignores them. */}
              {/* The bare collapsed edge belongs to the full sidebar; in
              `collapse-to-rail` the collapsed sidebar is the icon rail with
              its own expand control (ui g5.ui.sidebar-default). */}
              {!leftSidebarOpen &&
                userSettings.sidebarDisplayMode !== "collapse-to-rail" && (
                  <div
                    data-testid="sidebar-collapsed-edge"
                    className="flex w-7 shrink-0 flex-col items-center border-r border-[var(--g5-stroke)] bg-[var(--g5-bg1)] pt-1"
                  >
                    <button
                      type="button"
                      data-testid="sidebar-expand"
                      className="inline-flex h-6 w-6 items-center justify-center rounded text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]"
                      onClick={() => setLeftSidebarOpen(true)}
                      title="Expand sidebar"
                      aria-label="Expand sidebar"
                    >
                      <ChevronsRight size={14} />
                    </button>
                  </div>
                )}
              {(leftSidebarOpen ||
                userSettings.sidebarDisplayMode === "collapse-to-rail") && (
                <LeftSidebar
                  isOpen={leftSidebarOpen}
                  onCollapse={() => setLeftSidebarOpen(false)}
                  onExpand={() => setLeftSidebarOpen(true)}
                  // Workbench default order; user reordering still wins.
                  sectionOrderPreset="workbench"
                  terminalAction={railTerminalAction}
                  guidedSection={
                    guidedJourneyShown ? (
                      <GuidedJourneySection
                        phase={workbenchPhase}
                        actions={ribbonActions}
                        configStatus={ribbonConfigStatus}
                        actionState={workbenchActionState}
                        onAction={handleWorkbenchAction}
                        renderAction={renderWorkbenchAction}
                        pendingActionId={pendingWorkbenchActionId}
                        runningActionIds={journeyRunningActionIds}
                        facts={journeyFacts}
                        marksStatus={
                          journeyMarks.status === "ok"
                            ? "ok"
                            : journeyMarks.status === "error"
                              ? "error"
                              : "loading"
                        }
                        onRetryMarks={() =>
                          void queryClient.invalidateQueries({
                            queryKey: journeyKey(workspaceId),
                          })
                        }
                        pullRequestsStatus={journeyPullRequests.status}
                        onRetryPullRequests={() =>
                          void queryClient.invalidateQueries({
                            queryKey: workspacePullRequestsQueryOptions(
                              workspaceId,
                              "source",
                              "all",
                            ).queryKey,
                          })
                        }
                        isOwner={isWorkspaceOwner}
                        ownerDisplayName={ownerName(
                          orgMembers.status === "ok" ? orgMembers.data : [],
                          workspaceDetailData?.ownerUserId ?? null,
                        )}
                        approvals={shipApprovals}
                        reviewSubmitted={reviewSubmitted}
                        approvalsStatus={reviewApprovalsStatus}
                        onReviewApprovals={
                          !reviewExcluded
                            ? openWorkspaceApprovalsTab
                            : undefined
                        }
                        // Marks are the owner's durable write under the
                        // mutation policy (g5.workspace.ui.gates).
                        onMark={
                          canManageDurableOwnerState
                            ? handleJourneyMark
                            : undefined
                        }
                        markBlockedReason={
                          !isWorkspaceOwner
                            ? "Only the workspace owner marks steps"
                            : workspaceDetailData?.readOnly
                              ? "This workspace is read-only"
                              : workspaceDetailData?.mutationPolicyTransition ===
                                  "freezing"
                                ? "This workspace is freezing"
                                : null
                        }
                        markPendingStepId={journeyMarkPendingStepId}
                        markFailure={journeyMarkFailure}
                        onMovePhase={
                          isWorkspaceOwner && !currentWorkspaceIsBase
                            ? (phase) => updateWorkspacePhase.mutate(phase)
                            : undefined
                        }
                        movePending={updateWorkspacePhase.isLoading}
                        onOpenDoc={(doc) =>
                          openTab({
                            type: "doc",
                            docId: doc.id,
                            name: doc.name,
                          })
                        }
                      />
                    ) : undefined
                  }
                  issuesSection={issuesSection}
                  displayMode={userSettings.sidebarDisplayMode}
                  railOnly={activeCenterIsAdmin}
                  activeRailAction={activeRailAction}
                  onSelectContentSection={handleRailContentSectionSelected}
                  width={leftSidebarWidth}
                  workspaceId={workspaceId}
                  organization={org ?? ""}
                  branches={allSortedBranches}
                  onExplainOntologyFile={
                    workspaceChatAvailable
                      ? (repoId, path) =>
                          handleSeedChatPrompt(`Explain ${path}`, {
                            id: `source_file:${repoId}:${path}`,
                            kind: "source_file",
                            label: path,
                            path,
                            branchId: repoId,
                          })
                      : undefined
                  }
                  onSelectOntologyFile={(repoId, path) =>
                    openTab(workspaceOntologyFileTab(path, repoId))
                  }
                  ontologyChangesSelected={
                    selectedTabItem?.type === "ontology_changes"
                  }
                  onOpenOntologyChanges={() =>
                    openTab({
                      type: "ontology_changes",
                      name: "Ontology changes",
                    })
                  }
                  suggestionCountPaths={suggestionCountPaths}
                  docsFolderPath={docsFolderPath}
                  workspaceNamesById={workspaceNamesById}
                  selectedDocId={
                    selectedTab?.startsWith("doc:")
                      ? selectedTab.slice(4)
                      : null
                  }
                  selectedTaskId={selectedTaskId}
                  selectedFilePath={
                    selectedTabItem?.type === "file"
                      ? selectedTabItem.filePath
                      : null
                  }
                  selectedFileBranchId={
                    selectedTabItem?.type === "file"
                      ? (selectedTabItem.branchId ?? activeBranchId)
                      : null
                  }
                  onSelectDoc={(docId) => {
                    const doc = docs.find((d) => d.id === docId);
                    openTab({
                      type: "doc",
                      docId,
                      name: doc?.name ?? docId,
                    });
                  }}
                  sourceControlDirtyFiles={sourceControlDirtyFiles}
                  onOpenSourceControlDiff={openSourceControlDiffTab}
                  sourceControlBranches={allSortedBranches}
                  sourceControlActiveBranchId={activeBranchId}
                  onOpenSourceControlCommit={openSourceControlCommitTab}
                  onSelectFile={handleSelectFile}
                  onSelectTask={handleSelectTaskAndOpenOutput}
                  tasks={tasksData}
                  hasActiveTask={hasActiveTask}
                  taskListStatus={taskList.status}
                  taskListError={taskList.paginationError}
                  onRetryTasks={() => {
                    void taskList.retry();
                  }}
                  hasMoreTasks={taskList.hasNextPage}
                  isLoadingMoreTasks={taskList.isFetchingNextPage}
                  onLoadMoreTasks={() => {
                    void taskList.fetchNextPage();
                  }}
                  activateSectionRequest={browserSectionRequest}
                  onSectionRequestApplied={(nonce) =>
                    setBrowserSectionRequest((request) =>
                      request?.nonce === nonce ? null : request,
                    )
                  }
                  onActiveSectionChange={setActiveSidebarSection}
                  onOpenWorkerAdmin={
                    canManageWorkers ? openWorkerHostsTab : undefined
                  }
                  onOpenWorkspaceSettings={openWorkspaceSettingsTab}
                  onOpenIntentLog={() =>
                    openTab({ type: "intent_log", name: "All intent logs" })
                  }
                  docs={docs}
                  // Direct-mutation affordances (create doc/view/folder/file,
                  // delete, view sync) are visibility-gated by callback presence
                  // in LeftSidebar; passing `undefined` for a reader or a
                  // read-only workspace hides them (M30). They all write into
                  // the running workspace, so the sidebar disables them with
                  // the VM's reason while the owner's VM is stopped
                  // (`g5.workspace.ui.unavailable`). Read affordances stay wired.
                  onCreateFileEntry={
                    workspaceWritable ? handleCreateFileEntry : undefined
                  }
                  onCreateFolderEntry={
                    workspaceWritable ? handleCreateFolderEntry : undefined
                  }
                  onDeleteFileEntry={
                    workspaceWritable ? handleDeleteFileEntry : undefined
                  }
                  onCreateProjectDoc={
                    workspaceWritable ? handleOpenCreateProjectDoc : undefined
                  }
                  onDropProjectDocFiles={
                    !workspaceWritable
                      ? undefined
                      : ({ files, folderPath }) =>
                          setCreateProjectDocModal({
                            open: true,
                            title: "Create Doc",
                            defaultFolderPath: folderPath,
                            initialUploadFiles: files,
                          })
                  }
                  onDeleteProjectDoc={
                    !workspaceWritable
                      ? undefined
                      : (doc) => handleRequestDeleteProjectDoc(doc.id)
                  }
                />
              )}

              {/* Left sidebar drag handle — hidden in rail display modes: the
                44px rail is not resizable and the overlay reuses the last
                persisted expanded width (left-sidebar D38). */}
              {leftSidebarOpen &&
                (userSettings.sidebarDisplayMode === "expanded" ||
                  userSettings.sidebarDisplayMode === "collapse-to-rail") && (
                  <DragHandle
                    orientation="vertical"
                    onResize={handleLeftSidebarResize}
                  />
                )}

              {/* Center Panel */}
              <div className="relative flex-1 min-w-0 flex flex-col overflow-hidden">
                <CenterPanel
                  tabs={openTabs}
                  selectedTabId={selectedTab}
                  chatActive={centerChatActive}
                  chatAvailable={workspaceChatAvailable}
                  showTabBar={userSettings.showTabBar}
                  onSelectChat={handleSelectCenterChat}
                  chatContent={
                    !workspaceChatAvailable || rightPanelDetached ? null : (
                      <section
                        data-testid="right-panel"
                        data-chat-placement="center"
                        aria-label="Workspace chat"
                        className="h-full min-h-0 overflow-hidden bg-[var(--g5-bg1)]"
                      >
                        {renderWorkspaceChat({
                          layout: "center",
                          onDetach: () => {
                            setFloatingChatPlacement("center");
                            setRightPanelOpen(false);
                            setRightPanelDetached(true);
                          },
                        })}
                      </section>
                    )
                  }
                  unresolvedContentBranches={
                    contentBranchResolution.status === "unresolved"
                      ? allSortedBranches
                      : null
                  }
                  onSelectContentBranch={selectExplicitContentBranch}
                  onSeedChatPrompt={
                    workspaceChatAvailable ? handleSeedChatPrompt : undefined
                  }
                  onDiscussInChat={
                    workspaceChatAvailable ? handleDiscussInChat : undefined
                  }
                  onOpenWorkspaceFile={handleOpenDeepLinkFile}
                  onSelectTab={(tabId) => {
                    selectTab(tabId);
                  }}
                  onCloseTab={closeTab}
                  onCloseOtherTabs={closeOtherTabs}
                  onCloseTabsToRight={closeTabsToRight}
                  pinnedTabIds={pinnedTabIds}
                  onOpenTab={openTab}
                  onDownloadSource={handleDownloadSource}
                  downloadSourceInProgress={sourceDownloadInProgress}
                  // Workspace landing (g5.workbench.landing): the summary
                  // line, the Workspace task and the Implementation
                  // description above Activity.
                  issuePageControls={issuePageControls}
                  activityHeader={
                    workspace.status === "ok" ? (
                      <WorkspaceLanding
                        workspaceId={workspaceId}
                        workspaceName={workspace.data.name}
                        description={workspace.data.description}
                        docsRepoId={workspace.data.docsRepoId}
                        approvals={
                          currentWorkspaceIsBase ||
                          currentWorkspaceIsPersonal ||
                          reviewExcluded
                            ? null
                            : approvalSummary(
                                shipApprovals,
                                reviewApprovalsStatus,
                              )
                        }
                      />
                    ) : undefined
                  }
                  onOpenPullRequests={
                    publicationWorkflows ? openPullRequestsTab : undefined
                  }
                  onOpenPreview={onOpenPreviewRow}
                  showComments={showComments}
                  onToggleComments={() => setShowComments((prev) => !prev)}
                  viewerOwnsWorkspace={isWorkspaceOwner}
                  ownerVmRunning={ownerHasRunningAttachment}
                  commentPermissions={commentPermissions}
                  copyOf={reviewCopyOrigin}
                  workspaceId={workspaceId}
                  workspaceTerminalEnabled={ownerHasRunningAttachment}
                  workspaceTerminalKey={`${workspaceId}:${workspaceVm.status === "ok" ? `${workspaceVm.data.vmId}:${workspaceVm.data.epoch}` : "unavailable"}`}
                  workspaceDirtyCount={previewDirtyCount}
                  sourceControlChangeGraphAvailable={
                    workspace.status === "ok" && !workspace.data.isBase
                  }
                  sourceControlChangeGraphDefaultComparison="workspace"
                  branchId={effectiveContentBranchId}
                  repoId={
                    selectedContentBranch?.repoId ?? activeBranch?.repoId ?? ""
                  }
                  docs={docs}
                  hasActiveTask={hasActiveTask}
                  scrollToIntentRecord={scrollToIntentRecord}
                  onScrollToIntentRecord={setScrollToIntentRecord}
                  navigationOrigin={navigationOrigin}
                  onSetNavigationOrigin={setNavigationOrigin}
                />
              </div>
            </div>
          </AppShell>

          {/* Detached floating chat panel — history remains available read-only. */}
          {workspaceChatAvailable && rightPanelDetached && (
            <FloatingPanel
              title="Chat"
              initialPlacement={floatingChatPlacement}
              initialSize={
                floatingChatPlacement === "right-edge"
                  ? { w: chatCornerWidth, h: FLOATING_PANEL_DEFAULT_SIZE.h }
                  : undefined
              }
              onResizeEnd={
                floatingChatPlacement === "right-edge"
                  ? handleChatCornerResizeEnd
                  : undefined
              }
              onDock={() => {
                setRightPanelDetached(false);
                setRightPanelOpen(true);
              }}
              onClose={() => {
                setRightPanelDetached(false);
                handleCloseChatPanel();
              }}
            >
              {renderWorkspaceChat()}
            </FloatingPanel>
          )}

          {/* Corner Chat button (g5.workspace.ui.chat-corner-button): desktop
              only, gone while Chat is already the center surface, while its
              floating panel or a covering overlay is open, disabled with the
              durable gate's reason on Base and in a workspace the viewer
              cannot write. It can be dragged out of the way of whatever it
              covers; the spot is remembered in this browser. */}
          {!isMobile &&
            workspaceDetailData &&
            !centerChatActive &&
            !rightPanelDetached &&
            !coveringOverlayOpen && (
              <CornerButton
                corner="right"
                label="Open chat beside the content"
                disabledReason={chatCornerDisabledReason}
                onClick={handleOpenChatCorner}
                testId="chat-corner-button"
                movable={{ storageKey: CHAT_CORNER_BUTTON_OFFSET_STORAGE_KEY }}
                stack={
                  // Only an enabled button carries the stack: a disabled one
                  // explains the read-only state and opens nothing.
                  chatCornerDisabledReason === null &&
                  userSettings.chatBubbleStyle !== "off" ? (
                    <ChatBubbleStack
                      sessions={visibleChatSessions}
                      style={userSettings.chatBubbleStyle}
                      limit={userSettings.chatBubbleLimit}
                      onOpen={handleOpenChatBubble}
                    />
                  ) : undefined
                }
              >
                <MessageSquare className="h-5 w-5" />
              </CornerButton>
            )}

          {/* The bubble stack alone while Chat holds the center (ui
              g5.workspace.ui.chat-bubble-stack): same conditions as the
              button, at its remembered spot; a bubble opens its chat there. */}
          {!isMobile &&
            workspaceDetailData &&
            centerChatActive &&
            !rightPanelDetached &&
            !coveringOverlayOpen &&
            chatCornerDisabledReason === null &&
            userSettings.chatBubbleStyle !== "off" && (
              <CornerStack
                corner="right"
                storageKey={CHAT_CORNER_BUTTON_OFFSET_STORAGE_KEY}
                testId="chat-center-bubble-stack"
              >
                <ChatBubbleStack
                  sessions={visibleChatSessions}
                  style={userSettings.chatBubbleStyle}
                  limit={userSettings.chatBubbleLimit}
                  onOpen={handleOpenChatBubbleInCenter}
                />
              </CornerStack>
            )}

          {workspaceId && (
            <CreateProjectDocModal
              open={createProjectDocModal.open}
              workspaceId={workspaceId}
              title={createProjectDocModal.title}
              defaultFolderPath={
                createProjectDocModal.defaultFolderPath ?? docsFolderPath
              }
              initialUploadFiles={createProjectDocModal.initialUploadFiles}
              initialMode={createProjectDocModal.initialMode}
              existingTranscripts={meetingDocs.transcripts}
              decisionDocuments={meetingDocs.decisions}
              onOpenDecisions={(doc) =>
                openTab({
                  type: "doc",
                  docId: doc.id,
                  name: doc.name || doc.id,
                })
              }
              onUseExisting={(doc) => {
                openTab({
                  type: "doc",
                  docId: doc.id,
                  name: doc.name || doc.id,
                });
                startDecisionExtraction(doc);
              }}
              onCreated={(doc) => {
                openTab({
                  type: "doc",
                  docId: doc.id,
                  name: doc.name || doc.id,
                });
                // A stored transcript starts this phase's extraction template
                // over it: upload plus analysis is one user action
                // (g5.workbench.meeting). A committed config without that
                // action leaves the transcript merely open.
                if (createProjectDocModal.initialMode === "transcript") {
                  startDecisionExtraction(doc);
                }
              }}
              onClose={() =>
                setCreateProjectDocModal({
                  open: false,
                  title: "Create Doc",
                })
              }
            />
          )}

          {workspaceId && (
            <BuildMockModal
              open={buildMockOpen}
              docsFolderPath={docsFolderPath ?? null}
              mocks={mockDocs}
              buildDisabledReason={buildMockDisabledReason}
              onOpenMock={(doc) =>
                openTab({
                  type: "doc",
                  docId: doc.id,
                  name: doc.name || doc.id,
                })
              }
              onBuild={startMockGeneration}
              onClose={() => setBuildMockOpen(false)}
            />
          )}

          {workspaceId && (
            <CreateExplainerModal
              key={workspaceId}
              open={createExplainerModal.open}
              workspaceId={workspaceId}
              branchId={docsBranchId}
              agentConfig={syncAgentConfig}
              agentConfigOptions={syncAgentConfigOptions}
              onCreated={({ sessionId }) => {
                handleViewSession(sessionId);
                setCreateExplainerModal({ open: false });
              }}
              onClose={() => setCreateExplainerModal({ open: false })}
            />
          )}

          {settingsOpen && (
            <UserSettingsPanel
              open={settingsOpen}
              onClose={closeSettings}
              workspaceId={workspaceId}
            />
          )}

          {notificationsOpen && (
            <NotificationPanel
              notifications={notifications.active}
              onAction={handleNotificationAction}
              onClearAll={notifications.clearAll}
              onClose={() => setNotificationsOpen(false)}
            />
          )}

          {projectDocDeleteRequest && (
            <DeleteProjectDocDialog
              name={projectDocDeleteRequest.name}
              onCancel={() => setProjectDocDeleteRequest(null)}
              onConfirm={() => {
                const { docId } = projectDocDeleteRequest;
                setProjectDocDeleteRequest(null);
                void handleConfirmDeleteProjectDoc(docId);
              }}
            />
          )}

          {importTaskWorkspaceId === workspaceId && (
            <ImportTaskPicker
              key={workspaceId}
              workspaceId={workspaceId}
              viewerOwnsWorkspace={canWriteIssueLinks}
              writeBlockedReason={issueLinkCreationBlockedReason}
              onClose={() => setImportTaskWorkspaceId(null)}
            />
          )}
          {detachIssue?.workspaceId === workspaceId && canWriteIssueLinks && (
            <Modal
              ariaLabel="Detach from workspace"
              onClose={() => setDetachIssue(null)}
              closeDisabled={unlinkWorkspaceIssue.isLoading}
            >
              <IssueUnlinkConfirmation
                link={detachIssue}
                warnPrimary
                unlinking={
                  unlinkWorkspaceIssue.isLoading || !!issueWriteBlockedReason
                }
                unlinkError={unlinkWorkspaceIssue.error}
                onCancel={() => setDetachIssue(null)}
                onConfirm={() => {
                  if (issueWriteBlockedReason) return;
                  void unlinkWorkspaceIssue
                    .mutateAsync(detachIssue.id)
                    .then(() => setDetachIssue(null))
                    .catch(() => {});
                }}
              />
              {issueWriteBlockedReason && (
                <p role="status">{issueWriteBlockedReason}</p>
              )}
            </Modal>
          )}
          {/* Create Workspace Dialog */}
          {showCloudWorkspaceWorkflows &&
            createWorkspaceDialogOpen &&
            resolvedOrgId && (
              <CreateWorkspaceDialog
                open
                onClose={() => setCreateWorkspaceDialogOpen(false)}
                initialIssueRef={createWorkspaceIssueRef ?? undefined}
                orgId={resolvedOrgId}
                onCreated={(wsName, landing) => {
                  setCreateWorkspaceDialogOpen(false);
                  resetTaskSelection();
                  navigate(
                    `/${org}/${encodeURIComponent(wsName)}${forkLandingQuery(landing)}`,
                    { state: forkLandingState(landing) },
                  );
                }}
              />
            )}

          {/* Clone Workspace Dialog — source is the current workspace (top fork
          button) or any workspace chosen from the switcher dropdown rows. */}
          {showCloudWorkspaceWorkflows && cloneSourceWorkspace && (
            <CloneDialog
              source={cloneSourceWorkspace}
              orgId={resolvedOrgId}
              onCloned={(workspace, landing) => {
                resetTaskSelection();
                navigate(
                  `/${org}/${encodeURIComponent(workspace.name)}${forkLandingQuery(landing)}`,
                  { state: forkLandingState(landing) },
                );
              }}
              onClose={() => setCloneSourceWorkspace(null)}
            />
          )}

          {removeWorkspaceRequest && (
            <RemoveWorkspaceDialog
              workspace={removeWorkspaceRequest}
              isRemoving={deleteWorkspace.isLoading}
              errorMessage={deleteWorkspace.error?.message ?? null}
              onClose={handleCloseRemoveWorkspaceDialog}
              onConfirm={handleConfirmRemoveWorkspace}
            />
          )}

          <WorkspaceMergeConfirmModal
            key={
              workspaceMergeConfirmOpen
                ? "workspace-merge-open"
                : "workspace-merge-closed"
            }
            open={workspaceMergeConfirmOpen}
            sourceWorkspaceId={workspaceId}
            sourceWorkspaceName={workspaceSlug ?? "workspace"}
            targets={workspaceMergeTargets}
            targetWorkspaceId={workspaceMergeTargetId}
            agentConfig={syncAgentConfig}
            agentConfigOptions={syncAgentConfigOptions}
            resolveAgentLaunch={resolveAgentLaunchPreferences}
            isSubmitting={
              launchSubmissionKeys.has("workspace-merge") ||
              preflightWorkspaceMerge.isLoading ||
              startWorkspaceMerge.isLoading
            }
            errorMessage={workspaceMergeError}
            onTargetWorkspaceChange={setWorkspaceMergeTargetId}
            onCancel={() => {
              if (
                launchSubmissionKeys.has("workspace-merge") ||
                preflightWorkspaceMerge.isLoading ||
                startWorkspaceMerge.isLoading
              )
                return;
              setWorkspaceMergeConfirmOpen(false);
              preflightWorkspaceMerge.reset();
              startWorkspaceMerge.reset();
            }}
            onConfirm={(agentConfig, agentLaunch) => {
              void handleConfirmWorkspaceMerge(agentConfig, agentLaunch);
            }}
          />

          {prepareToShipOpen && prepareToShipTarget ? (
            <PrepareToShipModal
              workspaceId={workspaceId}
              orgId={resolvedOrgId}
              dirtyCount={previewDirtyCount}
              sourceWorkspaceName={
                currentWorkspaceSummary?.name ?? workspaceSlug ?? "workspace"
              }
              targetWorkspaceId={prepareToShipTarget.id}
              targetWorkspaceName={prepareToShipTarget.name}
              destinationStatus={pullRequestDestinationStatus}
              repositoryComparison={repositoryComparison}
              shipApprovals={shipApprovals}
              existingPullRequest={prepareToShipExistingPullRequest}
              agentConfig={syncAgentConfig}
              agentConfigOptions={syncAgentConfigOptions}
              resolveAgentLaunch={resolveAgentLaunchPreferences}
              isRebaseSubmitting={
                launchSubmissionKeys.has("workspace-merge") ||
                preflightWorkspaceMerge.isLoading ||
                startWorkspaceMerge.isLoading
              }
              rebaseErrorMessage={workspaceMergeError}
              onStartRebase={(agentConfig, agentLaunch) => {
                void confirmWorkspaceMergeForTarget(
                  prepareToShipTarget.id,
                  prepareToShipTarget.name,
                  agentConfig,
                  agentLaunch,
                );
              }}
              onReviewApprovals={
                !reviewExcluded ? openWorkspaceApprovalsTab : undefined
              }
              onAgentStarted={(sessionId) => {
                setPrepareToShipOpen(false);
                handleViewSession(sessionId);
              }}
              startWaitReason={capacityWaitReason}
              onClose={() => setPrepareToShipOpen(false)}
            />
          ) : null}

          <DirectCleanMergeChoiceModal
            open={!!directCleanChoice}
            title={
              directCleanChoice
                ? workspaceMergeReviewTitle(directCleanChoice)
                : "Merge Review"
            }
            preflight={directCleanChoice?.preflight ?? null}
            operationKind="workspace-rebase"
            isSubmitting={
              launchSubmissionKeys.has("merge-review") ||
              launchSubmissionKeys.has("workspace-rebase") ||
              startWorkspaceMerge.isLoading
            }
            errorMessage={startWorkspaceMerge.error?.message ?? null}
            previewAction={
              directCleanChoice &&
              shouldShowDirectMergePreviewAction(
                directCleanChoice.preflight,
              ) ? (
                <PreviewControl
                  workspaceId={workspaceId}
                  isDirty={previewDirtyCount > 0}
                  mode="unmerged"
                  mergeResult={
                    directCleanChoice.preflight.decision ===
                    "cleanMergeNoGitConflicts"
                      ? {
                          preflightFingerprint:
                            directCleanChoice.preflight.preflightFingerprint,
                          targetWorkspaceId:
                            directCleanChoice.targetWorkspaceId,
                        }
                      : undefined
                  }
                  label="Preview rebased state"
                  title="Preview the current workspace after source rebase"
                  triggerClassName={classNames(
                    "inline-flex items-center gap-1.5 rounded-md border border-[var(--g5-stroke)] px-3 py-1.5 text-sm font-medium",
                    "text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]",
                  )}
                />
              ) : null
            }
            startWaitReason={capacityWaitReason}
            onCancel={handleDirectCleanChoiceCancel}
            onApprove={() => {
              void handleMergeReviewApproval();
            }}
            onFullWorkflow={(options) => {
              void handleDirectCleanFullWorkflow(options);
            }}
          />

          {switchPromptTaskId && (
            <TaskWorkspaceSwitchPrompt
              taskId={switchPromptTaskId}
              currentWorkspaceId={workspaceId}
              org={org ?? ""}
              onClose={() => setSwitchPromptTaskId(null)}
              onSwitch={(path) => {
                setSwitchPromptTaskId(null);
                navigate(path);
              }}
              onOpenHere={(taskId) => {
                setSwitchPromptTaskId(null);
                openTab({
                  type: "workspace_changes",
                  name: "Workspace changes",
                });
                handleSelectTaskAndOpenChat(taskId);
              }}
            />
          )}

          {/* Client-side ephemeral toast notifications */}
          {toasts.length > 0 && (
            <NotificationToastStack
              notifications={toasts}
              onDismiss={dismissToast}
              onAction={handleClientToastAction}
            />
          )}
        </ErrorBoundary>
      </WorkspacePageProviders>
    </ComposerOwner>
  );
}
