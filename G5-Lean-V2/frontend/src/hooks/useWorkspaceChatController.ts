// =============================================================================
// G5 Client — workspace Chat controller for surfaces outside the workbench
// =============================================================================
// Home's personal chat side panel (personal_workspace g5.personal.home) mounts
// the same `ChatPanel` the workbench does, for the workspace scope of one
// workspace. This hook assembles that panel's data and actions from the same
// chat hooks and rules the workbench uses: sessions and their messages with
// scrollback, the persisted plus live timeline, the open turn's task, Stop
// with its A→B handoff, background-wait End, approvals, renames and forks,
// the provider choice and launch confirmation, and Start and send. The
// caller supplies the live gate's verdicts (`writable`, `canControlLiveTurn`,
// `startAndSend`); this hook decides nothing about authority.
// =============================================================================

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  agentLaunchPreferencesForConfig,
  type ResolveAgentLaunchPreferences,
} from "../components/agent/AgentLaunchConfigControls";
import type {
  ChatPanelSendOptions,
  ChatSendResult,
} from "../components/chat/ChatPanel";
import {
  effectiveAssignment,
  useAssignmentConfigs,
} from "../components/settings/ProviderAccountAssignment";
import { classifyAgentApprovals } from "../lib/agentApprovals";
import { defaultAgentProviderFromConfig } from "../lib/agentLaunchDefaults";
import {
  getPreferredAvailableAgentProvider,
  hasLiveAssistantTurn,
  supportedCloudAgentProviders,
} from "../lib/chatTurns";
import { authenticatableAgentProviders } from "../lib/providerAccountSelection";
import { isProviderAccountsEnabled } from "../lib/providerFlags";
import {
  useChatStopController,
  type ChatStopState,
} from "./useChatStopController";
import { useLiveTail, type LiveTailState } from "./useLiveTail";
import type {
  AgentApprovalResolutionInput,
  ChatAgentProvider,
  ChatMessageDTO,
  ChatSessionDTO,
  ChatSessionResultDTO,
  PickerHarness,
  ProviderAccountDTO,
  SubscriptionUsage,
  TaskSummary,
  TimelineEntry,
} from "../lib/types";
import { useResolveAgentLaunch } from "./useAgentLaunch";
import {
  useChatMessages,
  useChatResultInspection,
  useChatSession,
  useChatSessions,
  useEndBackgroundWait,
  useForkSession,
  useRenameChatSession,
  useReplyToProviderRequest,
  useSendChatMessage,
  useSessionTask,
  useSessionTimeline,
  useStartChatMessage,
  type TimelineCoverage,
} from "./useChat";
import { useProviderAccounts } from "./useProviderAccounts";
import { useInstanceInfo, useWorkspaceScopedConfig } from "./useSettings";
import { useUserSettings } from "./useUserSettings";
import { useWSChannel } from "./useWebSocket";

export interface WorkspaceChatControllerInput {
  workspaceId: string;
  orgId: string;
  /** Chat reads run only once the workspace is known and readable. */
  enabled: boolean;
  /** The gate admits sending (or Start and send) and session writes. */
  writable: boolean;
  /** The gate admits Stop and End on a live reply. */
  canControlLiveTurn: boolean;
  /** A stopped personal composer sends with `startCompute`. */
  startAndSend: boolean;
  /** Reported once per failed send so the surface can toast it. */
  onError?: (title: string, message: string) => void;
}

export interface WorkspaceChatController {
  session: ChatSessionDTO | null;
  sessions: ChatSessionDTO[];
  messages: ChatMessageDTO[];
  isStreaming: boolean;
  stopControlSuppressed: boolean;
  stopState: ChatStopState | null;
  liveTailState: LiveTailState;
  timelineEntries: TimelineEntry[];
  hasEarlierHistory: boolean;
  isLoadingEarlierHistory: boolean;
  onLoadEarlierHistory: () => void;
  timelineCoverage: TimelineCoverage;
  earlierHistoryError: Error | null;
  hasOlderMessages: boolean;
  isLoadingOlderMessages: boolean;
  onLoadOlderMessages: () => void;
  activeTaskSummary: TaskSummary | null;
  pendingApprovalIds: string[];
  selectedAgentProvider: ChatAgentProvider;
  availableAgentProviders: ChatAgentProvider[];
  onSelectAgentProvider: (provider: ChatAgentProvider) => void;
  providerAccountsEnabled: boolean;
  providerAccounts: ProviderAccountDTO[] | undefined;
  providerAccountAssignments: Record<PickerHarness, string>;
  runtimeVariablesAvailable: Record<PickerHarness, boolean>;
  resolveAgentLaunch: ResolveAgentLaunchPreferences;
  defaultClaudeModel: string | undefined;
  workspaceSubscriptionUsage: SubscriptionUsage | null;
  setupError: string | null;
  onSendMessage: (
    content: string,
    options?: ChatPanelSendOptions,
  ) => Promise<ChatSendResult>;
  /** Record that this person saw a chat's latest finished result. */
  onAcknowledgeResult: (
    sessionId: string,
    result: ChatSessionResultDTO,
  ) => Promise<unknown> | void;
  onStopStreaming: (() => void) | undefined;
  onEndBackgroundWait:
    | ((
        sessionId: string,
        wait: { taskId: string; attempt: number },
      ) => Promise<void>)
    | undefined;
  onResolveAgentApproval:
    | ((input: AgentApprovalResolutionInput) => Promise<unknown>)
    | undefined;
  onForkSession: ((atMessageIndex: number) => void) | undefined;
  onCreateSession: (() => void) | undefined;
  onSelectSession: (sessionId: string) => void;
  onRenameSession:
    | ((sessionId: string, title: string | null) => Promise<void>)
    | undefined;
}

export function useWorkspaceChatController({
  workspaceId,
  orgId,
  enabled,
  writable,
  canControlLiveTurn,
  startAndSend,
  onError,
}: WorkspaceChatControllerInput): WorkspaceChatController {
  const chatWorkspaceId = enabled ? workspaceId : "";
  const chatSessions = useChatSessions(chatWorkspaceId, "workspace");
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  // A different workspace starts on the no-session landing.
  useEffect(() => setActiveSessionId(null), [workspaceId]);
  const querySessionId = enabled ? (activeSessionId ?? "") : "";
  const sessionState = useChatSession(querySessionId);
  const messagesState = useChatMessages(querySessionId);
  const taskState = useSessionTask(querySessionId);
  const activeTaskSummary = taskState.status === "ok" ? taskState.data : null;
  const timeline = useSessionTimeline(querySessionId);

  const sendChatMessage = useSendChatMessage();
  const startChatMessage = useStartChatMessage();
  const renameChatSession = useRenameChatSession();
  const replyToProviderRequest = useReplyToProviderRequest();
  const endBackgroundWait = useEndBackgroundWait();
  const forkSession = useForkSession();

  // The workbench subscribes to the same channels for chat events.
  useWSChannel(enabled ? `workspace:${workspaceId}` : "");
  useWSChannel(enabled && activeSessionId ? `chat:${activeSessionId}` : "");

  const sessions = useMemo(
    () =>
      (chatSessions.status === "ok" ? chatSessions.data : []).filter(
        (session) => session.scopeType === "workspace",
      ),
    [chatSessions],
  );
  const session = useMemo(() => {
    const detail = sessionState.status === "ok" ? sessionState.data : null;
    if (detail && detail.scopeType === "workspace") return detail;
    if (!activeSessionId) return null;
    return sessions.find((item) => item.id === activeSessionId) ?? null;
  }, [activeSessionId, sessionState, sessions]);
  const messages = useMemo(
    () => (messagesState.status === "ok" ? messagesState.data : []),
    [messagesState],
  );
  const onAcknowledgeResult = useChatResultInspection(
    session?.id,
    session?.activity ??
      sessions.find((item) => item.id === session?.id)?.activity,
    messages,
  );
  const isStreaming = useMemo(() => hasLiveAssistantTurn(messages), [messages]);

  // Persisted timeline plus the entries streamed for the open session.
  const apiTimelineEntries = useMemo(
    () => (timeline.status === "ok" ? timeline.data.items : []),
    [timeline],
  );
  // Live rows arrive through `chat.timeline` into the session-timeline cache
  // (chat g5.chat.live-delivery); the unsaved tail is read back after each
  // saved read while the turn is active (g5.chat.live-tail-recovery).
  const liveTailState = useLiveTail(
    activeSessionId,
    isStreaming,
    timeline.historyReads,
  );
  const timelineEntries = useMemo(() => {
    const seen = new Set<string>();
    const merged: TimelineEntry[] = [];
    for (const entry of apiTimelineEntries) {
      const entrySessionId = entry.metadata?.sessionId;
      const belongs =
        !activeSessionId ||
        typeof entrySessionId !== "string" ||
        entrySessionId === activeSessionId;
      if (!belongs || seen.has(entry.id)) continue;
      seen.add(entry.id);
      merged.push(entry);
    }
    return merged;
  }, [activeSessionId, apiTimelineEntries]);
  const pendingApprovalIds = useMemo(
    () =>
      session
        ? classifyAgentApprovals(timelineEntries, messages, activeTaskSummary)
            .pending
        : [],
    [activeTaskSummary, messages, session, timelineEntries],
  );

  // Scrollback loaders behind refs, so the memoized panel keeps stable props.
  const loadOlderTimelineRef = useRef(timeline.loadOlder);
  loadOlderTimelineRef.current = timeline.loadOlder;
  const onLoadEarlierHistory = useCallback(() => {
    void loadOlderTimelineRef.current();
  }, []);
  const loadOlderMessagesRef = useRef(messagesState.loadOlder);
  loadOlderMessagesRef.current = messagesState.loadOlder;
  const onLoadOlderMessages = useCallback(() => {
    void loadOlderMessagesRef.current();
  }, []);

  // One Stop state per turn, shared with the workbench (chat g5.chat.controls).
  const chatStop = useChatStopController({
    sessionId: activeSessionId,
    messages,
    messagesReady: messagesState.status === "ok",
  });
  const { stopControlSuppressed, stopState, onStop: handleStop } = chatStop;

  const endBackgroundWaitAsync = endBackgroundWait.mutateAsync;
  const handleEndBackgroundWait = useCallback(
    async (sessionId: string, wait: { taskId: string; attempt: number }) => {
      try {
        await endBackgroundWaitAsync({ sessionId, ...wait });
      } catch (error) {
        onError?.(
          "Could not end the wait",
          error instanceof Error ? error.message : String(error),
        );
        throw error;
      }
    },
    [endBackgroundWaitAsync, onError],
  );

  const replyAsync = replyToProviderRequest.mutateAsync;
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
      return replyAsync({
        sessionId: input.sessionId,
        taskId: input.taskId,
        attempt: input.attempt,
        providerRequestId: input.providerRequestId,
        validatedResponse,
        ...(input.startCompute ? { startCompute: true } : {}),
      });
    },
    [replyAsync],
  );

  // Provider choice and launch confirmation, as in the workbench.
  const { settings: userSettings } = useUserSettings();
  const scopedConfig = useWorkspaceScopedConfig(chatWorkspaceId, orgId);
  const defaultAgentProvider = useMemo(
    () =>
      defaultAgentProviderFromConfig(
        scopedConfig.status === "ok"
          ? scopedConfig.data.effectiveSettings
          : null,
      ),
    [scopedConfig],
  );
  const availableAgentProviders = useMemo(
    () => supportedCloudAgentProviders(),
    [],
  );
  const instanceInfo = useInstanceInfo();
  const providerAccountsEnabled = isProviderAccountsEnabled(
    instanceInfo.status === "ok" ? instanceInfo.data : null,
  );
  const providerAccountsOrgId = providerAccountsEnabled && enabled ? orgId : "";
  const providerAccountsState = useProviderAccounts(
    providerAccountsOrgId,
    workspaceId,
  );
  const assignmentConfigs = useAssignmentConfigs(
    providerAccountsOrgId,
    workspaceId,
  );
  const providerAccountAssignments = useMemo(
    () => ({
      claude_code: effectiveAssignment(assignmentConfigs.triples.claude_code)
        .value,
      codex: effectiveAssignment(assignmentConfigs.triples.codex).value,
    }),
    [assignmentConfigs.triples],
  );
  const runtimeVariablesAvailable = assignmentConfigs.runtimeVariablesAvailable;
  // Auto prefers an agent that can authenticate (chat
  // g5.chat.account-defaults).
  const authenticatableAgents = useMemo(
    () =>
      authenticatableAgentProviders(
        providerAccountAssignments,
        runtimeVariablesAvailable,
      ),
    [providerAccountAssignments, runtimeVariablesAvailable],
  );

  const preferredProvider = getPreferredAvailableAgentProvider(
    availableAgentProviders,
    defaultAgentProvider,
    authenticatableAgents,
  );
  const [selectedAgentProvider, setSelectedAgentProvider] =
    useState<ChatAgentProvider>("claude_code");
  const userChoseProviderRef = useRef(false);
  const onSelectAgentProvider = useCallback((provider: ChatAgentProvider) => {
    userChoseProviderRef.current = true;
    setSelectedAgentProvider(provider);
  }, []);
  useEffect(() => {
    if (!activeSessionId && !userChoseProviderRef.current) {
      setSelectedAgentProvider(preferredProvider);
    }
  }, [activeSessionId, preferredProvider]);
  // An open session keeps the provider it was started with.
  useEffect(() => {
    if (session?.agentProvider) setSelectedAgentProvider(session.agentProvider);
  }, [session?.agentProvider]);
  const resolveAgentLaunchMutation = useResolveAgentLaunch(chatWorkspaceId);
  const resolveAgentLaunch = resolveAgentLaunchMutation.mutateAsync;

  // Sending: an open session takes a follow-up; otherwise the first message
  // starts a new workspace-scope session after launch confirmation.
  const [setupError, setSetupError] = useState<string | null>(null);
  const startingRef = useRef(false);
  const sendAsync = sendChatMessage.mutateAsync;
  const startAsync = startChatMessage.mutateAsync;
  const onSendMessage = useCallback(
    async (
      content: string,
      options?: ChatPanelSendOptions,
    ): Promise<ChatSendResult> => {
      if (!enabled || !workspaceId) return false;
      setSetupError(null);
      const {
        agentLaunch: confirmedLaunch,
        launchSource,
        startCompute: requestedStart,
        attachments,
        // Home has no project documents to reference.
        projectReferences: _projectReferences,
        ...turnOptions
      } = options ?? {};
      void _projectReferences;
      const startCompute =
        requestedStart === true && startAndSend ? true : undefined;
      if (activeSessionId && session) {
        try {
          await sendAsync({
            sessionId: session.id,
            content,
            options: turnOptions,
            attachments,
            startCompute,
          });
          return true;
        } catch (error) {
          const message =
            error instanceof Error
              ? error.message
              : "Could not send chat message.";
          setSetupError(message);
          onError?.("Chat message failed", message);
          return false;
        }
      }
      // A selected session whose details have not loaded yet is still the
      // target: never start another one in its place.
      if (activeSessionId) {
        const message =
          "The conversation is still loading; send again in a moment.";
        setSetupError(message);
        onError?.("Chat message failed", message);
        return false;
      }
      if (startingRef.current) return false;
      startingRef.current = true;
      try {
        const agentLaunch =
          confirmedLaunch ??
          (await (async () => {
            const preferences = agentLaunchPreferencesForConfig({
              launchSource: launchSource ?? "inherit_configured",
              agentProvider: selectedAgentProvider,
              model: turnOptions.model,
              permissionMode: turnOptions.permissionMode,
              effort: turnOptions.effort,
            });
            const resolved = await resolveAgentLaunch(preferences);
            return {
              preferences,
              confirmedResolutionId: resolved.resolutionId,
            };
          })());
        const started = await startAsync({
          workspaceId,
          content,
          scopeType: "workspace",
          agentLaunch,
          attachments,
          startCompute,
        });
        setActiveSessionId(started.session.id);
        // Prompt history records against the chat this send created.
        return { sessionId: started.session.id };
      } catch (error) {
        const message =
          error instanceof Error
            ? error.message
            : "Could not start chat session.";
        setSetupError(message);
        onError?.("Chat session failed", message);
        return false;
      } finally {
        startingRef.current = false;
      }
    },
    [
      activeSessionId,
      enabled,
      onError,
      resolveAgentLaunch,
      selectedAgentProvider,
      sendAsync,
      session,
      startAndSend,
      startAsync,
      workspaceId,
    ],
  );

  const renameAsync = renameChatSession.mutateAsync;
  const onRenameSession = useCallback(
    async (sessionId: string, title: string | null) => {
      await renameAsync({ sessionId, title });
    },
    [renameAsync],
  );
  const forkAsync = forkSession.mutateAsync;
  const onForkSession = useCallback(
    (atIndex: number) => {
      if (!session) return;
      void forkAsync({ sessionId: session.id, workspaceId, atIndex })
        .then((result) => setActiveSessionId(result.sessionId))
        .catch((error: unknown) =>
          onError?.(
            "Could not fork the conversation",
            error instanceof Error ? error.message : String(error),
          ),
        );
    },
    [forkAsync, onError, session, workspaceId],
  );

  return {
    session,
    sessions,
    messages,
    isStreaming,
    stopControlSuppressed,
    stopState,
    liveTailState,
    timelineEntries,
    hasEarlierHistory: timeline.hasOlder,
    isLoadingEarlierHistory: timeline.isLoadingOlder,
    timelineCoverage: timeline.coverage,
    earlierHistoryError: timeline.appendError,
    onLoadEarlierHistory,
    hasOlderMessages: messagesState.hasOlder,
    isLoadingOlderMessages: messagesState.isLoadingOlder,
    onLoadOlderMessages,
    activeTaskSummary,
    pendingApprovalIds,
    selectedAgentProvider,
    availableAgentProviders,
    onSelectAgentProvider,
    providerAccountsEnabled,
    providerAccounts:
      providerAccountsState.status === "ok"
        ? providerAccountsState.data
        : undefined,
    providerAccountAssignments,
    runtimeVariablesAvailable,
    resolveAgentLaunch,
    defaultClaudeModel:
      userSettings.defaultAgentProviderChoices.claude_code.model,
    workspaceSubscriptionUsage: null,
    setupError,
    onSendMessage,
    onAcknowledgeResult,
    onStopStreaming: canControlLiveTurn ? handleStop : undefined,
    onEndBackgroundWait: canControlLiveTurn
      ? handleEndBackgroundWait
      : undefined,
    onResolveAgentApproval: writable ? handleResolveAgentApproval : undefined,
    onForkSession: writable ? onForkSession : undefined,
    onCreateSession: writable ? () => setActiveSessionId(null) : undefined,
    onSelectSession: setActiveSessionId,
    onRenameSession: writable ? onRenameSession : undefined,
  };
}
