// =============================================================================
// G5 Workspace Page — CloneDialog
// =============================================================================
// Modal dialog for cloning a workspace. Opened from the WorkspaceTable's clone
// action or from the WorkspacePage toolbar menu.
//
// Layout (top-bar D4): simple by default — the name input, the optional issue
// and "Start work" fields (first message plus Agent, Account, Model, Effort
// and Mode), and the dirty-state warning render at top level.
// The fork keeps the source's description. Other retained options live in a
// single collapsed "Advanced" section: the per-repository branch names, target
// workspace (searchable combobox, D31), the read-only approvals-required
// summary grouped by phase (g5.workbench.gates), and the read-only toggle. The collapsed toggle
// carries an "N changed" badge whenever an advanced option differs from its
// default.
// =============================================================================

import React, { useEffect, useState, useMemo, useRef } from "react";
import {
  X,
  Bot,
  ChevronDown,
  ChevronRight,
  AlertTriangle,
  Loader2,
  Search,
} from "lucide-react";
import { classNames, Button, Modal } from "../ui/UI";
import {
  useCloneWorkspace,
  useForkSourceAdvisories,
  useWorkspaces,
} from "../../hooks/useWorkspace";
import { ForkSourceAdvisoryNotice } from "./ForkSourceAdvisoryNotice";
import {
  useExternalIssuesFeature,
  useResolveExternalIssue,
} from "../../hooks/useExternalIssues";
import { workspaceNameFromIssue } from "../../lib/workspaceNameFromIssue";
import { startMessageFromIssue } from "../../lib/startMessageFromIssue";
import type { ForkLanding } from "../../lib/workspaceLanding";
import {
  defaultAgentProviderFromConfig,
  getAvailableSyncAgentConfigs,
  getPreferredSyncAgentConfig,
} from "../../lib/agentLaunchDefaults";
import { useResolveAgentLaunch } from "../../hooks/useAgentLaunch";
import { useUserSettings } from "../../hooks/useUserSettings";
import { useProviderAccounts } from "../../hooks/useProviderAccounts";
import { useAssignmentConfigs } from "../settings/ProviderAccountAssignment";
import type { ScopeTriple } from "../settings/ProviderAccountAssignment";
import {
  providerAccountKindLabel,
  providerAccountScopeLabel,
} from "../../lib/agentLaunchPresentation";
import {
  RUNTIME_CONFIG_ACCOUNT_VALUE,
  accountMatchesHarness,
  accountSelectionValue,
  authenticatableAgentProviders,
  runtimeVariablesAvailableFrom,
  unassignedAccountOption,
} from "../../lib/providerAccountSelection";
import {
  AgentLaunchConfigControls,
  dedupeAgentConfigs,
  initialAgentConfig,
  reconcileAgentConfig,
} from "../agent/AgentLaunchConfigControls";
import { describeForkIssueError } from "../../lib/externalIssueErrors";
import { ForkIssueField } from "../external-issues/ForkIssueField";
import {
  useProviderAccountsEnabled,
  useWorkspaceScopedConfig,
} from "../../hooks/useSettings";
import { useWorkspaceApprovals } from "../../hooks/useWorkspaceApprovals";
import {
  APPROVAL_PHASES,
  approvalPhaseLabel,
  definitionPhases,
} from "../../lib/approvalPhases";
import { WORKBENCH_PHASES } from "../../lib/workbenchDefaults";
import { randomUuid } from "../../lib/randomUuid";
import { buildWorkspaceTreeRows } from "../../lib/workspaceTree";
import { ApiError } from "../../lib/api";
import type {
  AgentLaunchSelectionV1,
  CloneWorkspaceInput,
  ForkIssueLinkOutcomeDTO,
  OrgApprovalDefinitionDTO,
  PickerHarness,
  ProviderAccountDTO,
  WorkspaceIssueLinkRole,
  WorkspacePhase,
  WorkspaceRequiredIntoDTO,
  WorkspaceSummary,
} from "../../lib/types";

// ---------------------------------------------------------------------------
// Approver avatars — same visual language as the workspace Approvals tab
// (WorkspaceApprovalsPageContent); module-local per the established idiom.
// ---------------------------------------------------------------------------

const AVATAR_PALETTE = [
  "rgb(103 164 255)",
  "rgb(113 201 136)",
  "rgb(242 162 73)",
  "rgb(192 132 252)",
  "rgb(103 232 249)",
];

function initialsOf(displayName: string): string {
  const parts = displayName.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  const first = parts[0][0] ?? "";
  const last = parts.length > 1 ? (parts[parts.length - 1][0] ?? "") : "";
  return (first + last).toUpperCase() || "?";
}

function avatarColor(userId: string): string {
  let hash = 0;
  for (const ch of userId) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return AVATAR_PALETTE[hash % AVATAR_PALETTE.length];
}

/**
 * The minimal per-definition approver stack for the approvals-required
 * summary (top-bar D32): one initials avatar per human, one bot avatar when
 * the definition carries an agent check. No per-approver state.
 */
function ApproverStack({
  definition,
}: {
  definition: OrgApprovalDefinitionDTO;
}) {
  return (
    <span className="flex items-center">
      {definition.humans.map((human, index) => (
        <span
          key={human.userId}
          className={classNames(
            "inline-flex h-[22px] w-[22px] items-center justify-center rounded-full",
            "text-[9.5px] font-bold text-[rgb(8,12,24)]",
            "ring-2 ring-[var(--g5-bg1)]",
            index > 0 && "-ml-1.5",
          )}
          style={{ backgroundColor: avatarColor(human.userId) }}
          title={human.displayName}
        >
          {initialsOf(human.displayName)}
        </span>
      ))}
      {definition.agentType !== null && (
        <span
          className={classNames(
            "inline-flex h-[22px] w-[22px] items-center justify-center rounded-full",
            "bg-[var(--g5-active)]/20 text-[var(--g5-active)]",
            "ring-2 ring-[var(--g5-bg1)]",
            definition.humans.length > 0 && "-ml-1.5",
          )}
          title="Agent check"
        >
          <Bot size={12} />
        </span>
      )}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Approvals-required grouping by phase (g5.workbench.gates)
// ---------------------------------------------------------------------------

/** One grouped approval row with every phase the definition binds. */
interface ForkApprovalRow {
  definition: OrgApprovalDefinitionDTO;
  phases: WorkspacePhase[];
}

/** One phase group in the approvals-required summary. */
interface ForkApprovalGroup {
  key: WorkspacePhase;
  label: string;
  rows: ForkApprovalRow[];
}

/**
 * Group the target's incoming requirements by the phases each one binds, in
 * workbench order (BE · SE · IE). A definition bound to several phases
 * appears under each with its phase chips; an inert definition (no phase)
 * is omitted — no chevron waits for it. Groups with no rows are dropped, so
 * the summary never shows an empty header.
 */
export function groupForkApprovalsByPhase(
  requiredInto: WorkspaceRequiredIntoDTO[],
): ForkApprovalGroup[] {
  return APPROVAL_PHASES.flatMap((phase) => {
    const rows = requiredInto
      .map(({ definition }) => ({
        definition,
        phases: definitionPhases(definition),
      }))
      .filter((row) => row.phases.includes(phase));
    return rows.length
      ? [
          {
            key: phase,
            label: `▲ ${WORKBENCH_PHASES[phase].name}`,
            rows,
          },
        ]
      : [];
  });
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface CloneDialogProps {
  /** Source workspace to clone */
  source: WorkspaceSummary;
  /** Org owning the source workspace. */
  orgId: string;
  /**
   * Called after a successful fork with how the new workspace lands (ui
   * g5.workspace.ui.fork-landing): on the chat the fork started, else on a
   * fresh composer carrying the issue's suggested first message, or the
   * message a chat that failed to start did not send.
   */
  onCloned: (workspace: WorkspaceSummary, landing: ForkLanding) => void;
  /** Called to close the dialog */
  onClose: () => void;
  /** Issue reference to preselect (e.g. starting work from an issue). */
  initialIssueRef?: string;
}

interface ForkRetryDraft {
  key: string;
  fingerprint: string;
  request: Omit<CloneWorkspaceInput, "idempotencyKey">;
  name: string;
  description: string;
  readOnly: boolean;
  mergeTargetId: string | null;
  issueRef?: string;
  issueRole?: WorkspaceIssueLinkRole;
  startWork?: boolean;
  startMessage?: string;
}

const ISSUE_LOOKUP_DEBOUNCE_MS = 400;

/**
 * The account the fork's first chat starts under unless the person picks
 * another: the source's effective assignment for the harness (the same
 * precedence the chat composer shows, user over workspace over organization),
 * skipping any assignment the new workspace cannot use, else Runtime
 * variables. The fork has no workspace settings of its own yet, so an
 * assignment at the source's workspace scope counts only when it names an
 * account the fork can still see.
 */
export function forkDefaultAccountValue(
  triple: ScopeTriple,
  visible: readonly ProviderAccountDTO[],
): string {
  for (const scope of ["user", "workspace", "org"] as const) {
    const accountId = triple[scope];
    if (accountId && visible.some((account) => account.id === accountId)) {
      return accountId;
    }
  }
  return RUNTIME_CONFIG_ACCOUNT_VALUE;
}

/**
 * The account a saved retry attempted, keyed by its harness, so an unchanged
 * retry reproduces the same launch and keeps its request identity.
 */
function restoredAccountPicks(
  draft: ForkRetryDraft | null,
): Partial<Record<PickerHarness, string>> {
  const preferences = draft?.request.startChat?.launchPreferences;
  if (
    !preferences ||
    preferences.provider.source !== "explicit" ||
    preferences.account?.source !== "explicit"
  ) {
    return {};
  }
  const value = accountSelectionValue(preferences.account.value);
  if (!value) return {};
  const harness: PickerHarness =
    preferences.provider.value === "codex" ? "codex" : "claude_code";
  return { [harness]: value };
}

function forkRetryStorageKey(sourceId: string): string {
  return `g5:workspace-fork-retry:${sourceId}`;
}

function loadForkRetryDraft(sourceId: string): ForkRetryDraft | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.sessionStorage.getItem(forkRetryStorageKey(sourceId));
    if (!raw) return null;
    const value = JSON.parse(raw) as Partial<ForkRetryDraft>;
    if (
      typeof value.key !== "string" ||
      typeof value.fingerprint !== "string" ||
      typeof value.request !== "object" ||
      value.request === null ||
      typeof value.name !== "string" ||
      typeof value.description !== "string" ||
      typeof value.readOnly !== "boolean" ||
      (value.mergeTargetId !== null &&
        typeof value.mergeTargetId !== "string") ||
      (value.issueRef !== undefined && typeof value.issueRef !== "string") ||
      (value.issueRole !== undefined &&
        value.issueRole !== "primary" &&
        value.issueRole !== "related") ||
      (value.startWork !== undefined && typeof value.startWork !== "boolean") ||
      (value.startMessage !== undefined &&
        typeof value.startMessage !== "string")
    ) {
      return null;
    }
    return value as ForkRetryDraft;
  } catch {
    return null;
  }
}

function saveForkRetryDraft(sourceId: string, draft: ForkRetryDraft | null) {
  if (typeof window === "undefined") return;
  try {
    if (draft) {
      window.sessionStorage.setItem(
        forkRetryStorageKey(sourceId),
        JSON.stringify(draft),
      );
    } else {
      window.sessionStorage.removeItem(forkRetryStorageKey(sourceId));
    }
  } catch {
    // Retry still works within this mounted dialog when browser storage is
    // unavailable.
  }
}

// ---------------------------------------------------------------------------
// CloneDialog Component
// ---------------------------------------------------------------------------

/**
 * Modal dialog for cloning a workspace.
 *
 * Internal state:
 * - name: Exact shared destination branch name, entered by the user
 * - description: the source's, carried to the fork (not edited here)
 * - showAdvanced: Advanced section toggle, collapsed by default (top-bar D4)
 * - isCloning: Loading state during mutation
 *
 * Uses `useCloneWorkspace(source.id)` mutation hook.
 * On submit → calls mutation → on success → `onCloned(newWorkspaceId)`.
 */
export const CloneDialog: React.FC<CloneDialogProps> = ({
  source,
  orgId,
  onCloned,
  onClose,
  initialIssueRef,
}) => {
  // -----------------------------------------------------------------------
  // Internal state
  // -----------------------------------------------------------------------

  const retryDraft = useRef(loadForkRetryDraft(source.id));
  // What the user typed. Until they edit the field, an issue's suggested
  // name (below) stands in for it; a restored draft counts as edited.
  const [typedName, setTypedName] = useState(retryDraft.current?.name ?? "");
  const [nameEdited, setNameEdited] = useState(
    Boolean(retryDraft.current?.name),
  );
  const forkIdempotencyKey = useRef(retryDraft.current?.key ?? randomUuid());
  const attemptedFingerprint = useRef(retryDraft.current?.fingerprint ?? null);
  const attemptedRequest = useRef(retryDraft.current?.request ?? null);
  // The fork keeps the source's description.
  const [description] = useState(
    retryDraft.current?.description ?? source.description ?? "",
  );
  // Optional existing issue, linked when the fork completes. Shown only
  // while the deployment advertises external issues.
  const externalIssues = useExternalIssuesFeature(orgId).status === "enabled";
  const [issueRef, setIssueRef] = useState(
    retryDraft.current?.issueRef ?? initialIssueRef ?? "",
  );
  const [issueRole, setIssueRole] = useState<WorkspaceIssueLinkRole>(
    retryDraft.current?.issueRole ?? "primary",
  );
  // Look the entered reference up (debounced) to confirm it and suggest a
  // workspace name from the issue.
  const trimmedIssueRef = issueRef.trim();
  const [lookupRef, setLookupRef] = useState(trimmedIssueRef);
  useEffect(() => {
    const timer = window.setTimeout(
      () => setLookupRef(trimmedIssueRef),
      ISSUE_LOOKUP_DEBOUNCE_MS,
    );
    return () => window.clearTimeout(timer);
  }, [trimmedIssueRef]);
  const lookupCurrent =
    externalIssues && trimmedIssueRef !== "" && lookupRef === trimmedIssueRef;
  const issueLookup = useResolveExternalIssue(
    orgId,
    lookupCurrent ? lookupRef : "",
  );
  const suggestedName =
    lookupCurrent && issueLookup.status === "ok"
      ? workspaceNameFromIssue(issueLookup.data)
      : null;
  const name = nameEdited ? typedName : (suggestedName ?? typedName);
  // A suggested name the user kept may take a free "-2", "-3", … suffix on
  // the server; a name the user typed is exact.
  const nameIsSuggestion = !nameEdited && suggestedName !== null;
  // Start work: the fork starts a chat in the new workspace with this first
  // message. On by default when the dialog opens from an issue's "Start
  // workspace"; the message is suggested from the issue until edited.
  const [startWork, setStartWork] = useState(
    retryDraft.current?.startWork ?? initialIssueRef !== undefined,
  );
  const [typedStartMessage, setTypedStartMessage] = useState(
    retryDraft.current?.startMessage ?? "",
  );
  const [startMessageEdited, setStartMessageEdited] = useState(
    retryDraft.current?.startMessage !== undefined,
  );
  const suggestedStartMessage =
    lookupCurrent && issueLookup.status === "ok"
      ? startMessageFromIssue(issueLookup.data)
      : null;
  const startMessage = startMessageEdited
    ? typedStartMessage
    : (suggestedStartMessage ?? typedStartMessage);
  // The agent the chat runs with, chosen and confirmed as a chat start is.
  // Defaults come from the person's choices and the source workspace's
  // configured provider; the fork resolves them again for the new workspace.
  const { settings: userSettings } = useUserSettings();
  const launchChoices = userSettings.defaultAgentProviderChoices;
  const scopedConfig = useWorkspaceScopedConfig(
    startWork ? source.id : "",
    orgId,
  );
  const agentConfigOptions = useMemo(
    () => dedupeAgentConfigs(getAvailableSyncAgentConfigs(launchChoices)),
    [launchChoices],
  );
  // The account the chat runs under (g5.chat.configuration; #987), chosen
  // like Agent, Model and Effort. Shown only while the provider-accounts
  // switch is on; its reads stay dormant until the fork starts a chat.
  const providerAccountsEnabled = useProviderAccountsEnabled();
  const accountsOrgId = providerAccountsEnabled && startWork ? orgId : "";
  const providerAccountsState = useProviderAccounts(accountsOrgId, source.id);
  const accountAssignments = useAssignmentConfigs(accountsOrgId, source.id);
  // Raw keys the fork's chat could run on: the organization's and the
  // person's, never the source workspace's (chat g5.chat.account-defaults).
  const forkRuntimeVariablesAvailable = useMemo(
    () =>
      runtimeVariablesAvailableFrom([
        accountAssignments.org.status === "ok"
          ? accountAssignments.org.data
          : null,
        accountAssignments.user.status === "ok"
          ? accountAssignments.user.data
          : null,
      ]),
    [accountAssignments.org, accountAssignments.user],
  );
  // The agents the new workspace could authenticate as configured (chat
  // g5.chat.account-defaults): the person's or the organization's
  // assignment, or raw keys in those scopes; the source's workspace scope
  // does not travel. Auto prefers one of them.
  const forkAuthenticatableAgents = useMemo(
    () =>
      authenticatableAgentProviders(
        {
          claude_code:
            accountAssignments.triples.claude_code.user ||
            accountAssignments.triples.claude_code.org,
          codex:
            accountAssignments.triples.codex.user ||
            accountAssignments.triples.codex.org,
        },
        forkRuntimeVariablesAvailable,
      ),
    [accountAssignments.triples, forkRuntimeVariablesAvailable],
  );
  const defaultAgentConfig = useMemo(
    () =>
      getPreferredSyncAgentConfig(
        defaultAgentProviderFromConfig(
          scopedConfig.status === "ok"
            ? scopedConfig.data.effectiveSettings
            : null,
        ),
        launchChoices,
        forkAuthenticatableAgents,
      ),
    [scopedConfig, launchChoices, forkAuthenticatableAgents],
  );
  const [agentConfig, setAgentConfig] = useState(() =>
    initialAgentConfig(defaultAgentConfig, agentConfigOptions),
  );
  const userPickedAgentConfig = useRef(false);
  useEffect(() => {
    setAgentConfig((previous) =>
      reconcileAgentConfig(
        previous,
        userPickedAgentConfig.current,
        defaultAgentConfig,
        agentConfigOptions,
      ),
    );
  }, [defaultAgentConfig, agentConfigOptions]);
  const launchHarness: PickerHarness =
    agentConfig.agentProvider === "codex" ? "codex" : "claude_code";
  // Only accounts the new workspace will see are offered: the organization's
  // and the person's own. The source's workspace accounts belong to the
  // source alone, so the fork's chat would refuse them (account_not_visible)
  // even though resolving them against the source would succeed.
  const forkVisibleAccounts = useMemo(
    () =>
      (providerAccountsState.status === "ok"
        ? providerAccountsState.data
        : []
      ).filter(
        (account) =>
          account.scope !== "workspace" &&
          accountMatchesHarness(account, agentConfig.agentProvider),
      ),
    [providerAccountsState, agentConfig.agentProvider],
  );
  // A pick is kept per harness; an untouched field follows the default. A
  // saved retry restores the account it attempted.
  const [pickedAccounts, setPickedAccounts] = useState(() =>
    restoredAccountPicks(retryDraft.current),
  );
  // The default is known only once the accounts and every assignment scope
  // have loaded; until then the field is a placeholder and the fork waits,
  // so a configured account is never silently replaced by Runtime variables.
  // A failed load is shown, and the person may still fork with what is
  // offered.
  const accountsLoadError =
    providerAccountsState.status === "error"
      ? providerAccountsState.error
      : accountAssignments.loadError;
  const accountsLoaded =
    providerAccountsState.status === "ok" && accountAssignments.ready;
  const accountsPending =
    accountsOrgId !== "" && !accountsLoaded && accountsLoadError === null;
  const accountValue =
    pickedAccounts[launchHarness] ??
    forkDefaultAccountValue(
      accountAssignments.triples[launchHarness],
      forkVisibleAccounts,
    );
  const accountOptions = useMemo(() => {
    if (!providerAccountsEnabled) return undefined;
    if (accountsPending) {
      return [{ value: accountValue, label: "Loading accounts…" }];
    }
    const unassigned = unassignedAccountOption(
      forkRuntimeVariablesAvailable[launchHarness],
      accountValue,
    );
    const options = [
      ...(unassigned ? [unassigned] : []),
      ...forkVisibleAccounts.map((account) => ({
        value: account.id,
        label: account.name,
        assistiveDescription: `${providerAccountKindLabel(account.kind)} · ${providerAccountScopeLabel(account.scope)}`,
      })),
    ];
    if (
      accountValue !== RUNTIME_CONFIG_ACCOUNT_VALUE &&
      !forkVisibleAccounts.some((account) => account.id === accountValue)
    ) {
      options.push({
        value: accountValue,
        label: `Unavailable (${accountValue.slice(0, 8)})`,
      });
    }
    return options;
  }, [
    providerAccountsEnabled,
    accountsPending,
    forkVisibleAccounts,
    accountValue,
    forkRuntimeVariablesAvailable,
    launchHarness,
  ]);
  const resolveAgentLaunch = useResolveAgentLaunch(source.id);
  const [confirmedLaunch, setConfirmedLaunch] =
    useState<AgentLaunchSelectionV1 | null>(null);
  const startsChat = startWork && startMessage.trim() !== "";
  // The chat starts only with a confirmed launch that names the account on
  // screen: never a confirmation from before the accounts loaded or from a
  // value the person has since changed.
  const launchReady =
    confirmedLaunch !== null &&
    !accountsPending &&
    (accountOptions === undefined ||
      (confirmedLaunch.preferences.account?.source === "explicit" &&
        accountSelectionValue(confirmedLaunch.preferences.account.value) ===
          accountValue));
  // The fork completed but not everything it asked for happened: its primary
  // claim went to another workspace meanwhile (the issue was linked as
  // related), or its chat did not start. Explained before leaving.
  const [readyNotice, setReadyNotice] = useState<{
    workspace: WorkspaceSummary;
    landing: ForkLanding;
    issueLink: ForkIssueLinkOutcomeDTO | null;
    chatProblem: string | null;
  } | null>(null);
  // Advanced section (top-bar D4): collapsed by default, no contents hint.
  const [showAdvanced, setShowAdvanced] = useState(false);
  // Read-only creation toggle — default off even when the source is itself
  // read-only; a clone never inherits the source's mutation_policy
  // (clone_service D30).
  const [readOnly, setReadOnly] = useState(
    retryDraft.current?.readOnly ?? false,
  );
  // -----------------------------------------------------------------------
  // Target workspace (top-bar D31): the field is omitted for the default
  // parent (the server defaults the target to the parent).
  // -----------------------------------------------------------------------
  // null = the default parent (the source workspace being forked), which is
  // represented on the wire by omitting mergeTargetWorkspaceId (D31).
  const [mergeTargetId, setMergeTargetId] = useState<string | null>(
    retryDraft.current?.mergeTargetId ?? null,
  );
  const [targetPickerOpen, setTargetPickerOpen] = useState(false);
  const [targetSearch, setTargetSearch] = useState("");
  // Org workspace list for the target tree.
  // Loading or failing never blocks Fork submit — the picker falls back to
  // the default parent selection (D31).
  const workspacesState = useWorkspaces(orgId);
  const targetTreeRows = useMemo(
    () =>
      workspacesState.status === "ok"
        ? buildWorkspaceTreeRows(workspacesState.data)
        : [],
    [workspacesState],
  );
  // Search filter (D31): case-insensitive substring on the workspace name;
  // while filtering, the surviving rows render flattened (no indentation).
  const targetFilter = targetSearch.trim().toLowerCase();
  const filteredTargetRows = useMemo(
    () =>
      targetFilter
        ? targetTreeRows.filter(({ workspace }) =>
            workspace.name.toLowerCase().includes(targetFilter),
          )
        : targetTreeRows,
    [targetTreeRows, targetFilter],
  );
  const mergeTargetName = useMemo(() => {
    if (!mergeTargetId) return null;
    return (
      targetTreeRows.find((r) => r.workspace.id === mergeTargetId)?.workspace
        .name ?? null
    );
  }, [mergeTargetId, targetTreeRows]);

  // -----------------------------------------------------------------------
  // Approvals-required summary (top-bar D32): the fork's outgoing merge must
  // satisfy the requirements attached to its merge target, so the summary
  // queries the currently selected target (default: the parent = source).
  // Read-only and fail-open — no state of this query ever gates the Fork.
  // -----------------------------------------------------------------------
  const approvalsTargetId = mergeTargetId ?? source.id;
  const approvalsState = useWorkspaceApprovals(approvalsTargetId);
  // Grouped by the phase each requirement binds (g5.workbench.gates).
  const approvalGroups = useMemo(
    () =>
      approvalsState.status === "ok"
        ? groupForkApprovalsByPhase(approvalsState.data.requiredInto)
        : [],
    [approvalsState],
  );
  const approvalGroupRowCount = approvalGroups.reduce(
    (total, group) => total + group.rows.length,
    0,
  );

  // -----------------------------------------------------------------------
  // Changed-count badge (top-bar D4): how many retained advanced options
  // differ from their defaults.
  // -----------------------------------------------------------------------
  const advancedChangedCount =
    (mergeTargetId !== null ? 1 : 0) + (readOnly ? 1 : 0);

  // -----------------------------------------------------------------------
  // Mutation hook
  // -----------------------------------------------------------------------

  const cloneMutation = useCloneWorkspace(source.id);
  const isCloning = cloneMutation.isLoading;
  // Advisory only: a failed read shows nothing and never blocks the fork.
  const sourceAdvisories = useForkSourceAdvisories(source.id);

  // -----------------------------------------------------------------------
  // Dirty state detection
  // -----------------------------------------------------------------------

  const dirtyRepoCount = useMemo(() => {
    return source.branches.filter((b) => b.dirtyFileCount > 0).length;
  }, [source.branches]);

  const hasDirtyState = dirtyRepoCount > 0;

  // -----------------------------------------------------------------------
  // Submit handler
  // -----------------------------------------------------------------------

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    // A chat start waits for its agent launch to resolve.
    if (!name || isCloning || (startsChat && !launchReady)) return;
    // The account an attempt uses is pinned from then on: an untouched field
    // no longer follows assignment refreshes, so an unchanged retry keeps its
    // request identity.
    if (startsChat && accountOptions !== undefined) {
      setPickedAccounts((previous) =>
        previous[launchHarness] === undefined
          ? { ...previous, [launchHarness]: accountValue }
          : previous,
      );
    }
    const nextRequest: Omit<CloneWorkspaceInput, "idempotencyKey"> = {
      name,
      description: description.trim() || undefined,
      readOnly: readOnly || undefined,
      mergeTargetWorkspaceId:
        mergeTargetId !== null &&
        targetTreeRows.some((row) => row.workspace.id === mergeTargetId)
          ? mergeTargetId
          : undefined,
      issue:
        externalIssues && issueRef.trim()
          ? { ref: issueRef.trim(), role: issueRole }
          : undefined,
      uniquifyName: nameIsSuggestion || undefined,
      startChat: startsChat
        ? {
            content: startMessage.trim(),
            launchPreferences: confirmedLaunch?.preferences,
          }
        : undefined,
    };
    // Only user-controlled inputs define request identity. Feature-gate and
    // workspace-list loading may alter the first wire projection, so retries
    // reuse the exact saved request until the user changes an input.
    const fingerprint = JSON.stringify({
      name,
      nameIsSuggestion,
      description,
      readOnly,
      mergeTargetId,
      issueRef: issueRef.trim(),
      issueRole,
      startWork,
      startMessage: startWork ? startMessage.trim() : "",
      launch: startsChat ? (confirmedLaunch?.preferences ?? null) : null,
    });
    if (
      attemptedFingerprint.current !== null &&
      attemptedFingerprint.current !== fingerprint
    ) {
      forkIdempotencyKey.current = randomUuid();
    }
    const request =
      attemptedFingerprint.current === fingerprint && attemptedRequest.current
        ? attemptedRequest.current
        : nextRequest;
    attemptedFingerprint.current = fingerprint;
    attemptedRequest.current = request;
    const retryDraftToSave: ForkRetryDraft = {
      key: forkIdempotencyKey.current,
      fingerprint,
      request,
      name,
      description,
      readOnly,
      mergeTargetId,
      issueRef,
      issueRole,
      startWork,
      startMessage: startMessageEdited ? typedStartMessage : undefined,
    };
    saveForkRetryDraft(source.id, retryDraftToSave);
    try {
      const result = await cloneMutation.mutateAsync({
        ...request,
        idempotencyKey: forkIdempotencyKey.current,
      });
      saveForkRetryDraft(source.id, null);
      const issueLink = result.issueLink ?? null;
      const relinked =
        issueLink && issueLink.role !== issueLink.requestedRole
          ? issueLink
          : null;
      const chat = result.startedChat ?? null;
      const landing: ForkLanding = chat?.sessionId
        ? { chatSessionId: chat.sessionId, draft: null }
        : {
            chatSessionId: null,
            draft: startsChat
              ? startMessage.trim()
              : (suggestedStartMessage ?? null),
          };
      const chatProblem = chat?.error
        ? `Its chat did not start: ${chat.error}`
        : chat?.workspaceStartError
          ? "Its first message is queued; start the workspace to run it."
          : null;
      if (relinked || chatProblem) {
        setReadyNotice({
          workspace: result.workspace,
          landing,
          issueLink: relinked,
          chatProblem,
        });
        return;
      }
      onClose();
      onCloned(result.workspace, landing);
    } catch (error) {
      // Error is available via cloneMutation.error. A refusal because the
      // organization turned external issues off happens before any fork is
      // admitted (an admitted fork resumes without that check), so nothing
      // exists under this key: the next attempt starts over from the form,
      // which no longer carries the issue, instead of replaying it.
      if (
        error instanceof ApiError &&
        error.details.code === "external_issues_disabled"
      ) {
        attemptedFingerprint.current = null;
        attemptedRequest.current = null;
        forkIdempotencyKey.current = randomUuid();
        saveForkRetryDraft(source.id, {
          ...retryDraftToSave,
          key: forkIdempotencyKey.current,
          fingerprint: "",
        });
      }
    }
  };

  // -----------------------------------------------------------------------
  // Clone button click handler (used instead of form submit since Button
  // renders type="button")
  // -----------------------------------------------------------------------

  const handleCloneClick = () => {
    if (!name || isCloning) return;
    // Trigger the form submit programmatically
    const form = document.getElementById(
      "clone-form",
    ) as HTMLFormElement | null;
    if (form) {
      form.requestSubmit();
    }
  };

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------

  return (
    <Modal
      onClose={onClose}
      labelledBy="clone-dialog-title"
      size="md"
      closeDisabled={isCloning}
      className="p-0 flex flex-col"
    >
      <>
        {/* Header */}
        <div
          className={classNames(
            "flex items-center justify-between",
            "px-5 py-4",
            "border-b border-[var(--g5-stroke)]",
          )}
        >
          <h2
            id="clone-dialog-title"
            className="text-base font-semibold text-[var(--g5-text0)]"
          >
            Fork{" "}
            <code className="text-sm font-[var(--g5-font-mono)] text-[var(--g5-info)]">
              {source.name}
            </code>
          </h2>
          <button
            type="button"
            data-testid="clone-dialog-close"
            className={classNames(
              "inline-flex items-center justify-center",
              "w-7 h-7",
              "rounded-[var(--g5-radius-sm)]",
              "text-[var(--g5-text2)]",
              "hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
              "transition-colors duration-150",
              "cursor-pointer",
            )}
            onClick={onClose}
            disabled={isCloning}
            title="Close"
            aria-label="Close"
          >
            <X size={16} />
          </button>
        </div>

        {/* Body */}
        <form
          id="clone-form"
          onSubmit={handleSubmit}
          className="flex-1 min-h-0 overflow-y-auto"
        >
          <div className="px-5 py-4 space-y-4">
            {/* Dirty state warning */}
            {hasDirtyState && (
              <div
                className={classNames(
                  "flex items-start gap-2.5",
                  "px-3 py-2.5",
                  "rounded-[var(--g5-radius-sm)]",
                  "bg-[var(--g5-warn)]/10",
                  "border border-[var(--g5-warn)]/30",
                )}
              >
                <AlertTriangle
                  size={16}
                  className="text-[var(--g5-warn)] shrink-0 mt-0.5"
                />
                <p className="text-sm text-[var(--g5-text1)]">
                  This workspace has uncommitted changes in{" "}
                  <strong>{dirtyRepoCount}</strong> repo
                  {dirtyRepoCount !== 1 ? "s" : ""}. The fork will not include
                  those changes.
                </p>
              </div>
            )}

            {/* Name input */}
            <div>
              <label
                htmlFor="clone-name"
                className="block text-sm font-medium text-[var(--g5-text1)] mb-1"
              >
                Workspace name (also its branch name)
              </label>
              <input
                id="clone-name"
                type="text"
                className={classNames(
                  "w-full",
                  "bg-[var(--g5-bg0)]",
                  "border border-[var(--g5-stroke)]",
                  "rounded-[var(--g5-radius-sm)]",
                  "text-sm text-[var(--g5-text0)]",
                  "placeholder:text-[var(--g5-text2)]",
                  "px-3 py-2",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
                )}
                value={name}
                onChange={(e) => {
                  setTypedName(e.target.value);
                  setNameEdited(true);
                }}
                placeholder="feature/exact-name"
                required
                autoFocus
              />
            </div>

            {externalIssues && (
              <ForkIssueField
                issueRef={issueRef}
                lookup={lookupCurrent ? issueLookup : null}
                role={issueRole}
                onIssueRefChange={setIssueRef}
                onRoleChange={setIssueRole}
              />
            )}

            <div className="space-y-2">
              <label className="flex items-center gap-2 text-sm text-[var(--g5-text1)] cursor-pointer select-none">
                <input
                  type="checkbox"
                  data-testid="clone-start-work-checkbox"
                  checked={startWork}
                  onChange={(e) => setStartWork(e.target.checked)}
                  disabled={isCloning}
                  className="w-3.5 h-3.5 accent-[var(--g5-info)] cursor-pointer disabled:opacity-50"
                />
                Start work in the new workspace
              </label>
              {startWork && (
                <div>
                  <label
                    htmlFor="clone-start-message"
                    className="block text-sm font-medium text-[var(--g5-text1)] mb-1"
                  >
                    First message for the agent
                  </label>
                  <textarea
                    id="clone-start-message"
                    data-testid="clone-start-message"
                    className={classNames(
                      "w-full",
                      "bg-[var(--g5-bg0)]",
                      "border border-[var(--g5-stroke)]",
                      "rounded-[var(--g5-radius-sm)]",
                      "text-sm text-[var(--g5-text0)]",
                      "placeholder:text-[var(--g5-text2)]",
                      "px-3 py-2",
                      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
                      "resize-y min-h-[80px]",
                    )}
                    value={startMessage}
                    onChange={(e) => {
                      setTypedStartMessage(e.target.value);
                      setStartMessageEdited(true);
                    }}
                    placeholder="What should the agent do?"
                    rows={4}
                  />
                  <p className="mt-1 text-xs text-[var(--g5-text2)]">
                    A new chat starts with this message as soon as the workspace
                    is ready.
                  </p>
                  <AgentLaunchConfigControls
                    value={agentConfig}
                    options={agentConfigOptions}
                    onChange={(config) => {
                      userPickedAgentConfig.current = true;
                      setAgentConfig(config);
                    }}
                    resolvePreferences={resolveAgentLaunch.mutateAsync}
                    onResolutionSelectionChange={setConfirmedLaunch}
                    accountOptions={accountOptions}
                    account={accountValue}
                    onAccountChange={(value) =>
                      setPickedAccounts((previous) => ({
                        ...previous,
                        [launchHarness]: value,
                      }))
                    }
                    accountDisabled={accountsPending}
                    disabled={isCloning}
                    testIdPrefix="clone-start"
                    selectVariant="themed"
                    className="mt-2"
                  />
                  {accountsLoadError && (
                    <p
                      role="alert"
                      data-testid="clone-start-accounts-error"
                      className="mt-1 text-[10px] leading-snug text-[var(--g5-blocked)]"
                    >
                      Accounts could not be loaded: {accountsLoadError.message}
                    </p>
                  )}
                </div>
              )}
            </div>

            {/* Advanced section (top-bar D4): every non-essential retained
                option lives behind this collapsed divider toggle. */}
            <div>
              <button
                type="button"
                data-testid="clone-advanced-toggle"
                aria-expanded={showAdvanced}
                className={classNames(
                  "flex items-center gap-2 w-full",
                  "text-sm font-semibold text-[var(--g5-text1)]",
                  "hover:text-[var(--g5-text0)]",
                  "transition-colors duration-150",
                  "cursor-pointer",
                )}
                onClick={() => setShowAdvanced(!showAdvanced)}
              >
                {showAdvanced ? (
                  <ChevronDown size={14} />
                ) : (
                  <ChevronRight size={14} />
                )}
                Advanced
                {advancedChangedCount > 0 && (
                  <span
                    data-testid="clone-advanced-badge"
                    className={classNames(
                      "inline-flex items-center h-[18px] px-2 rounded-full",
                      "text-[10.5px] font-semibold",
                      "bg-[var(--g5-info)]/10 text-[var(--g5-info)]",
                    )}
                  >
                    {advancedChangedCount} changed
                  </span>
                )}
                <span
                  className="flex-1 h-px bg-[var(--g5-stroke)]"
                  aria-hidden="true"
                />
              </button>

              {showAdvanced && (
                <div className="mt-4 space-y-4">
                  <div
                    data-testid="clone-member-mapping"
                    className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)]"
                  >
                    <div className="border-b border-[var(--g5-stroke)] px-3 py-2 text-xs font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
                      Repository branches
                    </div>
                    <div className="divide-y divide-[var(--g5-stroke)]">
                      {source.branches.map((branch) => (
                        <div
                          key={branch.repoId}
                          data-testid="clone-member-mapping-row"
                          className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-2 px-3 py-2 text-sm"
                        >
                          <span className="min-w-0">
                            <span className="block truncate font-medium text-[var(--g5-text1)]">
                              {branch.repoName}
                            </span>
                            <span className="block truncate font-[var(--g5-font-mono)] text-xs text-[var(--g5-text2)]">
                              {branch.remoteBranch ?? branch.branchName}
                            </span>
                          </span>
                          <span
                            aria-hidden="true"
                            className="text-[var(--g5-text2)]"
                          >
                            →
                          </span>
                          <span className="truncate font-[var(--g5-font-mono)] text-xs text-[var(--g5-info)]">
                            {name || "destination name"}
                          </span>
                        </div>
                      ))}
                    </div>
                    <p className="px-3 py-2 text-xs text-[var(--g5-text2)]">
                      Every repository uses the same destination name. Source
                      branches remain unchanged.
                    </p>
                  </div>

                  {/* Target workspace — UI v3 gate only (top-bar D31).
                      Combobox-style trigger; the expanded panel is IN-FLOW
                      (never an absolute overlay — the modal body's scroll
                      container would clip it) with a search input filtering
                      the lineage tree. */}
                  <div>
                    <label className="block text-sm font-medium text-[var(--g5-text1)] mb-1">
                      Target workspace
                    </label>
                    <button
                      type="button"
                      data-testid="clone-target-workspace-toggle"
                      aria-expanded={targetPickerOpen}
                      className={classNames(
                        "w-full flex items-center gap-2",
                        "bg-[var(--g5-bg0)]",
                        "border border-[var(--g5-stroke)]",
                        "rounded-[var(--g5-radius-sm)]",
                        "text-sm text-[var(--g5-text0)] text-left",
                        "px-3 py-2",
                        "hover:border-[var(--g5-text2)]",
                        "transition-colors duration-150",
                        "cursor-pointer",
                      )}
                      onClick={() => {
                        setTargetPickerOpen(!targetPickerOpen);
                        setTargetSearch("");
                      }}
                      disabled={isCloning}
                    >
                      <span className="flex-1 truncate">
                        {mergeTargetName ?? `${source.name} (parent)`}
                      </span>
                      <ChevronDown
                        size={14}
                        className="text-[var(--g5-text2)] shrink-0"
                      />
                    </button>

                    {targetPickerOpen && (
                      <div className="mt-1 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] overflow-hidden">
                        <div className="flex items-center gap-2 px-3 py-2 border-b border-[var(--g5-stroke)]">
                          <Search
                            size={12}
                            className="text-[var(--g5-text2)] shrink-0"
                          />
                          <input
                            type="text"
                            data-testid="clone-target-workspace-search"
                            className="flex-1 bg-transparent text-sm text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)] rounded-[var(--g5-radius-sm)]"
                            placeholder="Search workspaces…"
                            value={targetSearch}
                            onChange={(e) => setTargetSearch(e.target.value)}
                            autoFocus
                          />
                        </div>
                        {workspacesState.status === "loading" ? (
                          <p className="px-3 py-2 text-xs text-[var(--g5-text2)] italic">
                            Loading workspaces…
                          </p>
                        ) : workspacesState.status === "error" ? (
                          <p className="px-3 py-2 text-xs text-[var(--g5-text2)] italic">
                            Failed to load workspaces — the fork will merge to
                            its parent.
                          </p>
                        ) : filteredTargetRows.length === 0 ? (
                          <p className="px-3 py-2 text-xs text-[var(--g5-text2)] italic">
                            No workspaces match &ldquo;{targetSearch}&rdquo;
                          </p>
                        ) : (
                          <div className="max-h-52 overflow-y-auto">
                            {filteredTargetRows.map(
                              ({ workspace, depth }) => {
                                const isDefaultParent =
                                  workspace.id === source.id;
                                const selected = mergeTargetId
                                  ? workspace.id === mergeTargetId
                                  : isDefaultParent;
                                return (
                                  <button
                                    key={workspace.id}
                                    type="button"
                                    data-testid={`clone-target-workspace-option-${workspace.id}`}
                                    className={classNames(
                                      "w-full flex items-center gap-2 px-3 py-1.5 text-left",
                                      "text-sm cursor-pointer",
                                      "hover:bg-[var(--g5-bg2)]",
                                      selected
                                        ? "text-[var(--g5-info)] font-medium bg-[var(--g5-info)]/5"
                                        : "text-[var(--g5-text0)]",
                                    )}
                                    style={{
                                      // Search results render flattened
                                      // (D31) — indentation only shows the
                                      // full tree.
                                      paddingLeft: `${12 + (targetFilter ? 0 : depth * 16)}px`,
                                    }}
                                    onClick={() => {
                                      // Picking the source again returns to
                                      // the default parent selection
                                      // (omitted on the wire, D31).
                                      setMergeTargetId(
                                        isDefaultParent ? null : workspace.id,
                                      );
                                      setTargetPickerOpen(false);
                                    }}
                                  >
                                    <span
                                      className={classNames(
                                        "w-1.5 h-1.5 rounded-full shrink-0",
                                        selected
                                          ? "bg-[var(--g5-info)]"
                                          : "bg-[var(--g5-stroke)]",
                                      )}
                                      aria-hidden="true"
                                    />
                                    <span className="truncate">
                                      {workspace.name}
                                      {isDefaultParent && (
                                        <span className="text-[var(--g5-text2)] font-normal">
                                          {" "}
                                          (parent)
                                        </span>
                                      )}
                                    </span>
                                  </button>
                                );
                              },
                            )}
                          </div>
                        )}
                      </div>
                    )}
                    <p className="mt-1 text-xs text-[var(--g5-text2)]">
                      The workspace this fork will merge into. Defaults to the
                      parent.
                    </p>
                  </div>

                  {/* Approvals required (top-bar D32).
                      Read-only summary of the selected target's incoming
                      requirements; informational, never gates the Fork. */}
                  <div>
                    <label className="block text-sm font-medium text-[var(--g5-text1)] mb-1">
                      Approvals required
                    </label>
                    {/* Flat informational panel — deliberately NOT the
                        input-style bordered bg0 box, so the read-only list
                        never reads as an editable field (chat b76ae3b5). */}
                    <div
                      data-testid="clone-approvals-required"
                      className="rounded-[var(--g5-radius-md)] bg-[var(--g5-bg1)] px-3"
                    >
                      {approvalsState.status === "loading" ? (
                        <p className="py-2 text-xs text-[var(--g5-text2)] italic">
                          Loading approvals…
                        </p>
                      ) : approvalsState.status === "error" ||
                        approvalsState.refreshError ? (
                        // A load failure must never masquerade as "no
                        // approvals" (D32) — distinct muted note; the Fork
                        // submit is unaffected either way.
                        <p className="py-2 text-xs text-[var(--g5-text2)] italic">
                          Approvals could not be loaded.
                        </p>
                      ) : approvalGroupRowCount === 0 ? (
                        <p className="py-2 text-xs text-[var(--g5-text2)] italic">
                          No approvals required.
                        </p>
                      ) : (
                        <div className="flex flex-col gap-2 py-1.5">
                          {approvalGroups.map((group) => (
                            <div
                              key={group.key}
                              data-testid="clone-approval-group"
                            >
                              <div className="mb-0.5 text-[10px] font-bold uppercase tracking-[0.06em] text-[var(--g5-active)]">
                                {group.label}
                              </div>
                              <div className="divide-y divide-dashed divide-[var(--g5-stroke)]">
                                {group.rows.map(({ definition, phases }) => (
                                  <div
                                    key={`${group.key}:${definition.id}`}
                                    data-testid="clone-approval-row"
                                    data-phase={group.key}
                                    className="flex items-center gap-2 py-1.5"
                                  >
                                    <span className="min-w-0 flex-1 truncate text-sm text-[var(--g5-text0)]">
                                      {definition.name}
                                    </span>
                                    {phases.map((phase) => (
                                      <span
                                        key={phase}
                                        data-testid={`clone-approval-phase-chip-${phase}`}
                                        className="shrink-0 whitespace-nowrap rounded-full border border-dashed px-1.5 py-0.5 text-[9.5px] font-bold"
                                        style={{
                                          color: `var(${WORKBENCH_PHASES[phase].token})`,
                                          borderColor: `var(${WORKBENCH_PHASES[phase].token})`,
                                        }}
                                      >
                                        {approvalPhaseLabel(phase)}
                                      </span>
                                    ))}
                                    <ApproverStack definition={definition} />
                                  </div>
                                ))}
                              </div>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                    <p className="mt-1 text-xs text-[var(--g5-text2)]">
                      Inherited from the target workspace&apos;s approval
                      configurations — manage them in the workspace&apos;s
                      Approvals tab after forking.
                    </p>
                  </div>

                  {/* Read-only workspace toggle — top-bar.semlift.md D4.
                      Default off; a clone never inherits the source policy
                      (clone_service D30). */}
                  <div className="space-y-1">
                    <label className="flex items-center gap-2 text-sm text-[var(--g5-text1)] cursor-pointer select-none">
                      <input
                        type="checkbox"
                        data-testid="clone-readonly-checkbox"
                        checked={readOnly}
                        onChange={(e) => setReadOnly(e.target.checked)}
                        disabled={isCloning}
                        aria-label="Make the new workspace read-only"
                        className="w-3.5 h-3.5 accent-[var(--g5-info)] cursor-pointer disabled:opacity-50"
                      />
                      Make the new workspace read-only
                    </label>
                  </div>
                </div>
              )}
            </div>

            {!readyNotice && sourceAdvisories.status === "ok" && (
              <ForkSourceAdvisoryNotice advisories={sourceAdvisories.data} />
            )}

            {/* Error display */}
            {cloneMutation.error && !readyNotice && (
              <div
                data-testid="clone-error"
                className={classNames(
                  "px-3 py-2",
                  "rounded-[var(--g5-radius-sm)]",
                  "bg-[var(--g5-blocked)]/10",
                  "border border-[var(--g5-blocked)]/30",
                  "text-sm text-[var(--g5-blocked)]",
                )}
              >
                {describeForkIssueError(cloneMutation.error) ??
                  (cloneMutation.error.message || "Failed to fork workspace.")}
              </div>
            )}

            {readyNotice && (
              <div
                role="status"
                data-testid="clone-ready-notice"
                className={classNames(
                  "space-y-1 px-3 py-2",
                  "rounded-[var(--g5-radius-sm)]",
                  "bg-[var(--g5-info)]/10",
                  "border border-[var(--g5-info)]/30",
                  "text-sm text-[var(--g5-text1)]",
                )}
              >
                <p>Workspace {readyNotice.workspace.name} is ready.</p>
                {readyNotice.issueLink && (
                  <p data-testid="clone-issue-linked-as-related">
                    {readyNotice.issueLink.displayKey} became another
                    workspace&apos;s primary issue while this one was being
                    created, so it was linked as related.
                  </p>
                )}
                {readyNotice.chatProblem && (
                  <p data-testid="clone-chat-not-started">
                    {readyNotice.chatProblem}
                  </p>
                )}
              </div>
            )}
          </div>

          {/* Footer */}
          <div
            className={classNames(
              "flex items-center justify-end gap-2",
              "px-5 py-4",
              "border-t border-[var(--g5-stroke)]",
            )}
          >
            {readyNotice ? (
              <Button
                kind="primary"
                data-testid="clone-open-workspace-button"
                onClick={() => {
                  onClose();
                  onCloned(readyNotice.workspace, readyNotice.landing);
                }}
              >
                Open workspace
              </Button>
            ) : (
              <>
                <Button
                  kind="secondary"
                  data-testid="clone-cancel-button"
                  onClick={onClose}
                  disabled={isCloning}
                >
                  Cancel
                </Button>
                <Button
                  kind="primary"
                  data-testid="clone-submit-button"
                  disabled={isCloning || !name || (startsChat && !launchReady)}
                  onClick={handleCloneClick}
                >
                  {isCloning ? (
                    <span className="inline-flex items-center gap-1.5">
                      <Loader2 size={14} className="animate-spin" />
                      Forking...
                    </span>
                  ) : startWork && startMessage.trim() ? (
                    "Fork & start work"
                  ) : (
                    "Fork"
                  )}
                </Button>
              </>
            )}
          </div>
        </form>
      </>
    </Modal>
  );
};
