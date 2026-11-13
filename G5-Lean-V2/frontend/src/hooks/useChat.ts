// =============================================================================
// G5 Client — Chat Session & Message Hooks
// =============================================================================

import { useCallback, useEffect, useRef, useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import { api } from "../lib/api";
import { invalidateAndReadAgain } from "../lib/readAgain";
import { SessionInputDTOSchema } from "../lib/api-schemas";
import type {
  AsyncState,
  AgentLaunchSelectionV1,
  ChatAgentProvider,
  ChatMessageContext,
  ChatProviderTurnOptions,
  ChatUploadAttachment,
  ChatMessageDTO,
  ChatSessionActivityDTO,
  ChatSessionDTO,
  ChatSessionResultDTO,
  SendMessageResponseDTO,
  SessionInputDTO,
  StartChatSessionResponseDTO,
  StopGenerationResponseDTO,
  NativeProviderRequestReplyResponseDTO,
  MutationHook,
  Page,
  TimelineEntry,
  LiveSubscriptionUsage,
} from "../lib/types";
import { isLiveTimelineRow, retainLiveRows } from "../lib/liveTimeline";
import { readSessionTimelinePage } from "../lib/sessionTimeline";
import { requestTimelineHoleFill } from "../lib/timelineHoles";
import type { TaskSummary } from "../lib/types";
import type { TimelineCoverage } from "../lib/historyCoverage";
import { randomUuid } from "../lib/randomUuid";
import { reconcileChatQueue, type ChatQueueInput } from "../lib/chatQueue";
import { toAsyncState, toMutationHook } from "./hookUtils";
import { ACTIVE_TASK_POLL_INTERVAL } from "./useTasks";

// ---------------------------------------------------------------------------
// Query Hooks
// ---------------------------------------------------------------------------

/** While a listed chat has an active turn, the list is re-read this often in
 * case a `chat.activity` hint was missed (chat g5.chat.activity.refresh). */
export const CHAT_ACTIVITY_REFRESH_INTERVAL = 15_000;

export function useChatSessions(
  workspaceId: string,
  scopeType?: string,
): AsyncState<ChatSessionDTO[]> {
  const query = useQuery<ChatSessionDTO[], Error>({
    queryKey: ["chat-sessions", workspaceId, scopeType],
    queryFn: ({ signal }) =>
      api.get<ChatSessionDTO[]>(`/api/v1/workspaces/${workspaceId}/sessions`, {
        signal,
        params: { scopeType },
      }),
    enabled: !!workspaceId,
    // The server orders by turn activity and reports each chat's activity
    // and this person's inspection state; converge after focus returns and
    // while any turn is active.
    refetchOnWindowFocus: true,
    refetchInterval: (query) =>
      query.state.data?.some((session) => session.activity?.activeTurnStartedAt)
        ? CHAT_ACTIVITY_REFRESH_INTERVAL
        : false,
  });
  return toAsyncState(query);
}

/** Server clamp for the sessions list `perPage` (PaginationParams). */
const SESSIONS_PAGE_SIZE = 100;

async function fetchAllChatSessionPages(
  workspaceId: string,
  signal?: AbortSignal,
): Promise<ChatSessionDTO[]> {
  const sessions: ChatSessionDTO[] = [];
  let page = 1;
  for (;;) {
    const response = await api.getRaw<ChatSessionDTO[]>(
      `/api/v1/workspaces/${workspaceId}/sessions`,
      { signal, params: { page, perPage: SESSIONS_PAGE_SIZE } },
    );
    sessions.push(...response.data);
    const total = response.meta?.total;
    if (
      response.data.length < SESSIONS_PAGE_SIZE ||
      (typeof total === "number" && sessions.length >= total)
    ) {
      return sessions;
    }
    page += 1;
  }
}

/**
 * Load EVERY chat session of a workspace by draining the paginated list
 * (top-bar D29): the server defaults to page 1 / perPage 50 and clamps
 * perPage to 100, so a single default read silently bounds pickers that must
 * cover the full set (CloneDialog fork-chat carry). Exposes `refetch` so a
 * failed load can be retried in place.
 */
export function useAllChatSessions(
  workspaceId: string,
  options?: { enabled?: boolean },
): {
  state: AsyncState<ChatSessionDTO[]>;
  refetch: () => void;
} {
  const query = useQuery<ChatSessionDTO[], Error>({
    queryKey: ["chat-sessions-all", workspaceId],
    queryFn: ({ signal }) => fetchAllChatSessionPages(workspaceId, signal),
    enabled: !!workspaceId && (options?.enabled ?? true),
  });
  return { state: toAsyncState(query), refetch: () => void query.refetch() };
}

export function useChatSession(sessionId: string): AsyncState<ChatSessionDTO> {
  const query = useQuery<ChatSessionDTO, Error>({
    queryKey: ["chat-session", sessionId],
    queryFn: ({ signal }) =>
      api.get<ChatSessionDTO>(`/api/v1/sessions/${sessionId}`, { signal }),
    enabled: !!sessionId,
  });
  const live = useQuery<LiveSubscriptionUsage | null>({
    queryKey: ["subscription-usage", sessionId],
    queryFn: () => null,
    enabled: false,
    initialData: null,
    gcTime: 0,
  }).data;
  const session = query.data;
  const usage =
    live &&
    session &&
    live.authMode === session.authMode &&
    live.accountId === (session.account?.id ?? null)
      ? live.usage
      : null;
  const state = toAsyncState(query);
  return state.status === "ok"
    ? { ...state, data: { ...state.data, subscriptionUsage: usage } }
    : state;
}

export function useSessionTask(
  sessionId: string,
): AsyncState<TaskSummary | null> {
  const query = useQuery<TaskSummary | null, Error>({
    queryKey: ["session-task", sessionId],
    queryFn: ({ signal }) =>
      api.get<TaskSummary | null>(`/api/v1/sessions/${sessionId}/task`, {
        signal,
      }),
    enabled: !!sessionId,
    refetchInterval: ACTIVE_TASK_POLL_INTERVAL,
  });
  return toAsyncState(query);
}

/**
 * Single-flight authoritative refresh of the chat transcript
 * (chat_panel_runtime D21): marks the messages query stale WITHOUT canceling
 * an in-flight window read. The D21 recovery poll calls this on a repeating
 * interval; with TanStack's default `cancelRefetch: true` (and the query
 * consuming the abort signal), any read slower than the poll interval would be
 * aborted and restarted by every tick, so the persisted terminal row could
 * never reach the cache and the recovery loop would stay gated on a stale
 * pending placeholder.
 */
export function refreshChatMessages(
  queryClient: QueryClient,
  sessionId: string,
): Promise<void> {
  return queryClient.invalidateQueries(
    { queryKey: ["chat-messages", sessionId] },
    { cancelRefetch: false },
  );
}

type MessagePage = {
  messages: ChatMessageDTO[];
  cursor?: number | null;
  hasMore?: boolean;
};

/** The tail-window size shared by the D39 refresh and older-page reads. */
const MESSAGES_WINDOW_LIMIT = 500;

/**
 * `useChatMessages` result: the window-merged transcript plus the messages
 * scrollback surface (job_chat_draft_hooks D39): `hasOlder` while persisted
 * rows remain below the loaded window, and a single-flight `loadOlder()` that
 * prepends the page strictly older than the cached minimum `messageIndex`.
 */
export type ChatMessagesState = AsyncState<ChatMessageDTO[]> & {
  hasOlder: boolean;
  isLoadingOlder: boolean;
  loadOlder: () => Promise<void>;
};

export function useChatMessages(sessionId: string): ChatMessagesState {
  const queryClient = useQueryClient();
  // Scrollback state (job_chat_draft_hooks D39). `olderExhausted` clears
  // `hasOlder` once an older-page read returns no strictly-older rows (a
  // defensive bound: `message_index` is dense from zero, so a minimum above
  // zero normally implies older rows exist). The VISIBLE state is per-session
  // and resets on switch; in-flight OWNERSHIP is keyed by session plus
  // request token (PR #2435 review) so a previous session's pending read
  // neither blocks the new session's first load nor, on late completion,
  // clears the new session's loading state, releases its lock, or marks it
  // exhausted.
  const [isLoadingOlder, setIsLoadingOlder] = useState(false);
  const [olderExhausted, setOlderExhausted] = useState(false);
  const olderReadRef = useRef<{ sessionId: string; token: number } | null>(
    null,
  );
  const olderTokenRef = useRef(0);
  const currentSessionIdRef = useRef(sessionId);
  currentSessionIdRef.current = sessionId;
  useEffect(() => {
    setIsLoadingOlder(false);
    setOlderExhausted(false);
  }, [sessionId]);

  const query = useQuery<ChatMessageDTO[], Error>({
    queryKey: ["chat-messages", sessionId],
    queryFn: async ({ signal }) => {
      // One latest-window read per refresh (job_chat_draft_hooks D39): the
      // server's tail mode returns the newest `limit` rows in ascending order,
      // so the current persisted turn is by definition in the response — a
      // missed terminal chat.done recovers over REST at any transcript length
      // (ws_event_envelope D5), the D21 poll's cancelRefetch cannot starve a
      // single request the way it starved a sequential multi-page drain, and
      // per-poll cost is bounded by the window, not the transcript.
      const result: MessagePage | ChatMessageDTO[] = await api.get<
        MessagePage | ChatMessageDTO[]
      >(`/api/v1/sessions/${sessionId}/messages`, {
        signal,
        params: { tail: true, limit: MESSAGES_WINDOW_LIMIT },
      });
      const fetched: ChatMessageDTO[] = Array.isArray(result)
        ? result
        : Array.isArray(result?.messages)
          ? result.messages
          : [];

      // Window merge (D39): fetched rows win for the index range they cover;
      // cached rows strictly older than the window are preserved before it
      // (previously loaded history), cached rows strictly newer than the
      // window and absent from it are preserved after it (in-flight optimistic
      // rows). An empty fetch leaves the cached list unchanged.
      const cached = queryClient.getQueryData<ChatMessageDTO[]>([
        "chat-messages",
        sessionId,
      ]);
      if (!cached?.length) return fetched;
      if (!fetched.length) return cached;
      // Queue-to-history recovery may read while the existing assistant is
      // still streaming. A saved partial prefix cannot erase a live suffix;
      // a terminal row, or a newer saved prefix, still wins.
      const cachedById = new Map(
        cached.map((message) => [message.id, message]),
      );
      for (let index = 0; index < fetched.length; index += 1) {
        const saved = fetched[index];
        const live = cachedById.get(saved.id);
        if (
          live?.role === "assistant" &&
          live.status === "streaming" &&
          ["pending", "streaming"].includes(saved.status) &&
          live.content.startsWith(saved.content)
        ) {
          fetched[index] = {
            ...saved,
            content: live.content,
            status: live.status,
          };
        }
      }
      const minFetchedIndex = fetched[0].messageIndex;
      const maxFetchedIndex = fetched[fetched.length - 1].messageIndex;
      const fetchedIds = new Set(fetched.map((message) => message.id));
      const olderHistory = cached.filter(
        (message) =>
          message.messageIndex < minFetchedIndex && !fetchedIds.has(message.id),
      );
      const newerTail = cached.filter(
        (message) =>
          message.messageIndex > maxFetchedIndex && !fetchedIds.has(message.id),
      );
      return olderHistory.length || newerTail.length
        ? [...olderHistory, ...fetched, ...newerTail]
        : fetched;
    },
    enabled: !!sessionId,
  });

  // Single-flight older-page read (job_chat_draft_hooks D39 scrollback): fetch
  // the page strictly older than the cached minimum `messageIndex` through the
  // endpoint's forward cursor — rows with `messageIndex > cursor` ascending
  // (routes/chat I6), so `cursor = min − limit − 1` yields [min−limit, min−1];
  // when `min ≤ limit` read from the start (`message_index` is dense from zero
  // in every session, including forks — routes/chat D12/D13) and let the merge
  // below drop overlap. The prepend goes through the setQueryData updater so
  // it composes with a concurrent tail refetch, whose D39 window merge then
  // preserves the prepended history on every later refresh.
  const loadOlder = useCallback(async () => {
    if (!sessionId) return;
    if (olderReadRef.current?.sessionId === sessionId) return;
    const cached = queryClient.getQueryData<ChatMessageDTO[]>([
      "chat-messages",
      sessionId,
    ]);
    const min = cached?.[0]?.messageIndex;
    if (min == null || min <= 0) return;
    const token = ++olderTokenRef.current;
    olderReadRef.current = { sessionId, token };
    setIsLoadingOlder(true);
    let prepended = 0;
    let succeeded = false;
    try {
      const params: Record<string, string | number | boolean> = {
        limit: MESSAGES_WINDOW_LIMIT,
      };
      if (min > MESSAGES_WINDOW_LIMIT) {
        params.cursor = min - MESSAGES_WINDOW_LIMIT - 1;
      }
      const result: MessagePage | ChatMessageDTO[] = await api.get<
        MessagePage | ChatMessageDTO[]
      >(`/api/v1/sessions/${sessionId}/messages`, { params });
      const fetched: ChatMessageDTO[] = Array.isArray(result)
        ? result
        : Array.isArray(result?.messages)
          ? result.messages
          : [];
      // Serialize with the tail reader (job_chat_draft_hooks D39, PR #2435
      // review): a tail read in flight right now snapshotted the cache BEFORE
      // this prepend; if its response committed afterwards it would overwrite
      // the prepended rows. Cancel and await it on this exact key, then apply
      // the atomic updater. One-shot per user action — the D21 poll re-issues
      // the tail read on its next tick, so terminal recovery is not starved.
      await queryClient.cancelQueries({
        queryKey: ["chat-messages", sessionId],
        exact: true,
      });
      queryClient.setQueryData<ChatMessageDTO[]>(
        ["chat-messages", sessionId],
        (current) => {
          if (!current?.length) return current;
          const currentMin = current[0].messageIndex;
          const currentIds = new Set(current.map((message) => message.id));
          const older = fetched.filter(
            (message) =>
              message.messageIndex < currentMin && !currentIds.has(message.id),
          );
          prepended = older.length;
          return older.length ? [...older, ...current] : current;
        },
      );
      succeeded = true;
    } catch {
      // A failed older-page read must not blank or destabilize the loaded
      // transcript (same posture as the timeline appendError channel,
      // job_chat_draft_hooks D41): keep `hasOlder` true so the top-of-stream
      // sentinel/button can simply retry the idempotent read.
    } finally {
      // Release the lock only if this request still owns it, and touch the
      // visible state only if the user is still on the requesting session —
      // a late completion for a previous session must not clear the new
      // session's loading state or mark it exhausted (PR #2435 review).
      if (olderReadRef.current?.token === token) {
        olderReadRef.current = null;
      }
      if (currentSessionIdRef.current === sessionId) {
        setIsLoadingOlder(false);
        if (succeeded && prepended === 0) setOlderExhausted(true);
      }
    }
  }, [queryClient, sessionId]);

  const oldestLoadedIndex = query.data?.[0]?.messageIndex;
  const hasOlder =
    !olderExhausted && oldestLoadedIndex != null && oldestLoadedIndex > 0;
  // A failed reconciliation read must not hide already visible history or
  // the live suffix while queue-to-history reconciliation is pending.
  const state: AsyncState<ChatMessageDTO[]> = query.data
    ? { status: "ok", data: query.data }
    : toAsyncState(query);
  return { ...state, hasOlder, isLoadingOlder, loadOlder };
}

/// Pre-#629 API pods serialized `SessionInputView` in snake_case; a pod
/// still draining during a rolling deploy may answer with that shape.
const LEGACY_SESSION_INPUT_KEYS: Record<string, string> = {
  session_id: "sessionId",
  input_index: "inputIndex",
  input_origin: "inputOrigin",
  delivery_generation: "deliveryGeneration",
  task_id: "taskId",
  created_at: "createdAt",
  delivered_at: "deliveredAt",
  withdrawn_at: "withdrawnAt",
};

/// The inputs API serializes `SessionInputView`; parse instead of casting so
/// a wire-contract drift fails loudly here rather than silently hiding the
/// queued-input controls (issue #629). A draining older pod's snake_case
/// response is normalized first so the tray keeps working mid-deploy.
function parseSessionInput(raw: unknown): SessionInputDTO {
  let candidate = raw;
  if (raw !== null && typeof raw === "object" && "input_origin" in raw) {
    candidate = Object.fromEntries(
      Object.entries(raw).map(([key, value]) => [
        LEGACY_SESSION_INPUT_KEYS[key] ?? key,
        value,
      ]),
    );
  }
  return SessionInputDTOSchema.parse(candidate) as SessionInputDTO;
}

export function useSessionInputs(
  sessionId: string,
  visibleMessages?: ChatMessageDTO[],
): AsyncState<ChatQueueInput[]> {
  const queryClient = useQueryClient();
  const query = useQuery<ChatQueueInput[], Error>({
    queryKey: ["session-inputs", sessionId],
    queryFn: async ({ signal }) => {
      const fetched = (
        await api.get<unknown[]>(`/api/v1/sessions/${sessionId}/inputs`, {
          signal,
        })
      ).map(parseSessionInput);
      const reconciled = reconcileChatQueue(
        queryClient.getQueryData<ChatQueueInput[]>([
          "session-inputs",
          sessionId,
        ]) ?? [],
        fetched,
      );
      const messages =
        queryClient.getQueryData<ChatMessageDTO[]>([
          "chat-messages",
          sessionId,
        ]) ?? [];
      if (
        reconciled.some(
          (input) =>
            input.awaitingHistory &&
            !messages.some((message) => message.id === input.id),
        )
      ) {
        // Recover the handoff even if its WebSocket hint was lost. Keep the
        // queue entry until the transcript actually renders that identity.
        void refreshChatMessages(queryClient, sessionId);
      }
      return reconciled;
    },
    enabled: !!sessionId,
    refetchInterval: 2_000,
  });
  useEffect(() => {
    if (!visibleMessages?.length) return;
    const ids = new Set(visibleMessages.map((message) => message.id));
    queryClient.setQueryData<ChatQueueInput[]>(
      ["session-inputs", sessionId],
      (current) => {
        if (!current?.some((input) => ids.has(input.id))) return current;
        return current.filter((input) => !ids.has(input.id));
      },
    );
  }, [queryClient, sessionId, visibleMessages, query.data]);
  // An unavailable refresh must not hide a submission already on screen.
  return query.data ? { status: "ok", data: query.data } : toAsyncState(query);
}

function useSessionInputMutation(
  action: "edit" | "withdraw" | "redeliver" | "cancel",
): MutationHook<
  {
    sessionId: string;
    inputId: string;
    expectedRevision?: number;
    content?: string;
  },
  SessionInputDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    SessionInputDTO,
    Error,
    {
      sessionId: string;
      inputId: string;
      expectedRevision?: number;
      content?: string;
    }
  >({
    mutationFn: async ({ sessionId, inputId, expectedRevision, content }) => {
      const base = `/api/v1/sessions/${sessionId}/inputs/${inputId}`;
      if (action === "edit") {
        return parseSessionInput(
          await api.patch<unknown>(base, { expectedRevision, content }),
        );
      }
      if (action === "withdraw") {
        return parseSessionInput(
          await api.post<unknown>(`${base}/withdraw`, { expectedRevision }),
        );
      }
      if (action === "cancel") {
        return parseSessionInput(await api.post<unknown>(`${base}/cancel`, {}));
      }
      return parseSessionInput(
        await api.post<unknown>(`${base}/redeliver`, {}),
      );
    },
    onSuccess: async (input, { sessionId }) => {
      await queryClient.cancelQueries(
        {
          queryKey: ["session-inputs", sessionId],
        },
        { revert: false },
      );
      queryClient.setQueryData<SessionInputDTO[]>(
        ["session-inputs", sessionId],
        (current) =>
          (current ?? []).map((item) => (item.id === input.id ? input : item)),
      );
      queryClient.invalidateQueries({ queryKey: ["session-task", sessionId] });
    },
  });
  return toMutationHook(mutation);
}

export function useEditSessionInput() {
  return useSessionInputMutation("edit");
}

export function useWithdrawSessionInput() {
  return useSessionInputMutation("withdraw");
}

export function useRedeliverSessionInput() {
  return useSessionInputMutation("redeliver");
}

export function useCancelUnknownSessionInput() {
  return useSessionInputMutation("cancel");
}

// ---------------------------------------------------------------------------
// Mutation Hooks
// ---------------------------------------------------------------------------

/** Re-read VM state after a Start and send recorded a Start. */
export function invalidateWorkspaceVmQueries(queryClient: QueryClient): void {
  void queryClient.invalidateQueries({ queryKey: ["workspace-vm"] });
}

export function useSendChatMessage(): MutationHook<
  {
    sessionId: string;
    content: string;
    agentProvider?: ChatAgentProvider;
    context?: ChatMessageContext;
    options?: ChatProviderTurnOptions;
    attachments?: ChatUploadAttachment[];
    /**
     * Start and send (g5.personal.start-and-send): record Start consent and
     * queue this input bound to it, in one request.
     */
    startCompute?: boolean;
  },
  SendMessageResponseDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    SendMessageResponseDTO,
    Error,
    {
      sessionId: string;
      content: string;
      agentProvider?: ChatAgentProvider;
      context?: ChatMessageContext;
      options?: ChatProviderTurnOptions;
      attachments?: ChatUploadAttachment[];
      startCompute?: boolean;
    },
    ChatQueueInput
  >({
    mutationFn: ({
      sessionId,
      content,
      agentProvider,
      context,
      options,
      attachments,
      startCompute,
    }) =>
      api.post<SendMessageResponseDTO>(
        `/api/v1/sessions/${sessionId}/messages`,
        {
          content,
          context,
          agentProvider,
          model: options?.model,
          permissionMode: options?.permissionMode,
          effort: options?.effort,
          account: options?.account,
          attachments: attachments?.length ? attachments : undefined,
          ...(startCompute ? { startCompute: true } : {}),
        },
      ),
    onMutate: async ({ sessionId, content }) => {
      // A query cancelled on chat switch restores its fetch-start snapshot.
      // Retire any earlier queue read before adding this submission, without
      // reverting sibling submissions or touching the transcript stream.
      await queryClient.cancelQueries(
        { queryKey: ["session-inputs", sessionId] },
        { revert: false },
      );
      const pending: ChatQueueInput = {
        id: `optimistic-${randomUuid()}`,
        sessionId,
        content,
        inputIndex: Number.MAX_SAFE_INTEGER,
        artifacts: [],
        revision: 0,
        inputOrigin: "human",
        delivery: "queued",
        deliveryGeneration: 0,
        createdAt: new Date().toISOString(),
        localSubmission: "submitting",
        editable: false,
        withdrawable: false,
      };
      queryClient.setQueryData<ChatQueueInput[]>(
        ["session-inputs", sessionId],
        (current) => [...(current ?? []), pending],
      );
      return pending;
    },
    onError: async (_err, { sessionId }, pending) => {
      // Retire the read's rollback snapshot before removing this submission.
      await queryClient.cancelQueries(
        { queryKey: ["session-inputs", sessionId] },
        { revert: false },
      );
      queryClient.setQueryData<ChatQueueInput[]>(
        ["session-inputs", sessionId],
        (current) =>
          (current ?? []).filter((input) => input.id !== pending?.id),
      );
    },
    onSuccess: async (data, { sessionId }, pending) => {
      await queryClient.cancelQueries(
        { queryKey: ["session-inputs", sessionId] },
        { revert: false },
      );
      queryClient.setQueryData<ChatQueueInput[]>(
        ["session-inputs", sessionId],
        (current = []) =>
          current.flatMap((input) => {
            if (input.id !== pending?.id) return [input];
            // The response is the first reliable link to the server identity.
            // A read may already have supplied its newer state; keep that row.
            if (
              !data.inputId ||
              current.some((item) => item.id === data.inputId)
            )
              return [];
            return [
              {
                ...input,
                id: data.inputId,
                localSubmission: "accepted" as const,
              },
            ];
          }),
      );
      // Compatibility with old servers that directly accepted a message.
      // Current inputId responses never write speculative transcript rows.
      if (!data.inputId && pending) {
        queryClient.setQueryData<ChatMessageDTO[]>(
          ["chat-messages", sessionId],
          (current) => {
            const list = current ?? [];
            if (list.some((message) => message.id === data.messageId))
              return list;
            const index = (list[list.length - 1]?.messageIndex ?? -1) + 1;
            const fields = {
              sessionId,
              artifacts: null,
              citations: null,
              inputTokens: null,
              outputTokens: null,
            };
            return [
              ...list,
              {
                ...fields,
                id: pending.id,
                role: "user",
                content: pending.content,
                status: "complete",
                messageIndex: index,
                createdAt: pending.createdAt,
              },
              {
                ...fields,
                id: data.messageId,
                role: "assistant",
                content: "",
                status: data.status || "pending",
                messageIndex: index + 1,
                createdAt: data.createdAt ?? pending.createdAt,
              },
            ];
          },
        );
      }
      // The accepted turn may create, reuse, or reactivate this session's
      // task. Refresh that authoritative projection now so a follow-up's empty
      // timeline cannot inherit the preceding turn's lifecycle label until
      // the periodic poll runs (chat_panel_runtime D24, issue #1050).
      queryClient.invalidateQueries({
        queryKey: ["session-task", sessionId],
      });
      queryClient.invalidateQueries({
        queryKey: ["session-inputs", sessionId],
      });
      // A Start and send moved the VM out of stopped: show its progress.
      if (data.startCompute) invalidateWorkspaceVmQueries(queryClient);
    },
    onSettled: (_data, _err, { sessionId }) => {
      // Mark chat-messages stale WITHOUT an immediate refetch (refetchType
      // "none"). The POST settles mid-stream, so an active refetch here can
      // resolve to server content that is behind the client's accumulated
      // chat.chunk deltas and clobber the live turn. The terminal WS events
      // (chat.done / chat.error / chat.message_updated) invalidate and refetch
      // chat-messages at turn end, which is the correct reconvergence point.
      queryClient.invalidateQueries({
        queryKey: ["chat-messages", sessionId],
        refetchType: "none",
      });
      queryClient.invalidateQueries({ queryKey: ["chat-session", sessionId] });
      queryClient.invalidateQueries({ queryKey: ["chat-sessions"] });
      queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-activity-current"],
      });
    },
  });
  return toMutationHook(mutation);
}

export function useStartChatMessage(): MutationHook<
  {
    workspaceId: string;
    content: string;
    title?: string;
    scopeType?: string;
    scopeRef?: string;
    agentLaunch: AgentLaunchSelectionV1;
    context?: ChatMessageContext;
    attachments?: ChatUploadAttachment[];
    /** Start and send (g5.personal.start-and-send). */
    startCompute?: boolean;
  },
  StartChatSessionResponseDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    StartChatSessionResponseDTO,
    Error,
    {
      workspaceId: string;
      content: string;
      title?: string;
      scopeType?: string;
      scopeRef?: string;
      agentLaunch: AgentLaunchSelectionV1;
      context?: ChatMessageContext;
      attachments?: ChatUploadAttachment[];
      startCompute?: boolean;
    }
  >({
    mutationFn: ({
      workspaceId,
      content,
      title,
      scopeType,
      scopeRef,
      agentLaunch,
      context,
      attachments,
      startCompute,
    }) => {
      const body: Record<string, unknown> = {
        content,
        context,
        scopeType: scopeType ?? "workspace",
      };
      body.agentLaunch = agentLaunch;
      if (title) body.title = title;
      if (scopeRef) body.scopeRef = scopeRef;
      if (attachments?.length) body.attachments = attachments;
      if (startCompute) body.startCompute = true;
      return api.post<StartChatSessionResponseDTO>(
        `/api/v1/workspaces/${workspaceId}/sessions/start`,
        body,
      );
    },
    onSuccess: (data) => {
      seedStartedChatSession(queryClient, data);
      if (data.startCompute || data.message?.startCompute) {
        invalidateWorkspaceVmQueries(queryClient);
      }
    },
  });
  return toMutationHook(mutation);
}

/**
 * Cache handling shared by every session-starting mutation (typed start,
 * workbench templated start): seed the exact cache variants WorkspacePage
 * reads and invalidate the same scopes, so the first-turn UI never briefly
 * selects the new session, sees an unchanged scoped session list and clears
 * the selection back to Start Chat before the session-detail query wins.
 */
export function seedStartedChatSession(
  queryClient: QueryClient,
  data: StartChatSessionResponseDTO,
): void {
  const insertSession = (old: ChatSessionDTO[] | undefined) => {
    if (!old) return [data.session];
    if (old.some((s) => s.id === data.session.id)) return old;
    return [data.session, ...old];
  };
  queryClient.setQueryData<ChatSessionDTO[]>(
    ["chat-sessions", data.session.workspaceId, data.session.scopeType],
    insertSession,
  );
  queryClient.setQueryData<ChatSessionDTO[]>(
    ["chat-sessions", data.session.workspaceId, undefined],
    insertSession,
  );
  queryClient.setQueryData(["chat-session", data.session.id], data.session);

  queryClient.invalidateQueries({
    queryKey: [
      "chat-sessions",
      data.session.workspaceId,
      data.session.scopeType,
    ],
    exact: true,
  });
  queryClient.invalidateQueries({
    queryKey: ["chat-sessions", data.session.workspaceId, undefined],
    exact: true,
  });
  queryClient.invalidateQueries({
    queryKey: ["chat-messages", data.session.id],
  });
  queryClient.invalidateQueries({
    queryKey: ["workspace-tasks", data.session.workspaceId],
  });
  queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
  queryClient.invalidateQueries({ queryKey: ["workspace-activity-current"] });
}

export function useRenameChatSession(): MutationHook<
  { sessionId: string; title: string | null },
  ChatSessionDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    ChatSessionDTO,
    Error,
    { sessionId: string; title: string | null }
  >({
    mutationFn: ({ sessionId, title }) =>
      api.patch<ChatSessionDTO>(`/api/v1/sessions/${sessionId}`, { title }),
    onSuccess: (data) => {
      // A rename is not turn activity: keep the activity the read reported
      // (the mutation response carries none).
      const renamed = (
        session: ChatSessionDTO | undefined,
      ): ChatSessionDTO => ({
        ...data,
        activity: data.activity ?? session?.activity,
      });
      const replaceSession = (old: ChatSessionDTO[] | undefined) =>
        old?.map((session) =>
          session.id === data.id ? renamed(session) : session,
        );

      queryClient.setQueryData<ChatSessionDTO>(
        ["chat-session", data.id],
        (old) => renamed(old),
      );
      queryClient.setQueriesData<ChatSessionDTO[]>(
        { queryKey: ["chat-sessions"] },
        replaceSession,
      );
      queryClient.invalidateQueries({ queryKey: ["chat-session", data.id] });
      queryClient.invalidateQueries({ queryKey: ["chat-sessions"] });
    },
  });
  return toMutationHook(mutation);
}

/**
 * Record that this person saw one finished result of a chat (chat
 * g5.chat.inspection.acknowledge). The server only moves the watermark
 * forward and answers with the chat's activity, which replaces the cached
 * activity without reordering anything.
 */
export function useRecordChatInspection(): MutationHook<
  { sessionId: string; result: ChatSessionResultDTO },
  ChatSessionActivityDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    ChatSessionActivityDTO,
    Error,
    { sessionId: string; result: ChatSessionResultDTO }
  >({
    mutationFn: ({ sessionId, result }) =>
      api.put<ChatSessionActivityDTO>(
        `/api/v1/sessions/${sessionId}/inspection`,
        {
          taskId: result.taskId,
          attempt: result.attempt,
        },
      ),
    onSuccess: (activity, { sessionId }) => {
      queryClient.setQueryData<ChatSessionDTO>(
        ["chat-session", sessionId],
        (old) => (old ? { ...old, activity } : old),
      );
      queryClient.setQueriesData<ChatSessionDTO[]>(
        { queryKey: ["chat-sessions"] },
        (old) =>
          old?.map((session) =>
            session.id === sessionId ? { ...session, activity } : session,
          ),
      );
    },
  });
  return toMutationHook(mutation);
}

/**
 * The owner's side of result acknowledgement (chat
 * g5.chat.inspection.acknowledge): returns the callback ChatPanel calls once a
 * result is displayed, and re-reads a cached transcript that cannot show the
 * chat's unseen result yet, so a stale cache never counts as seeing it and
 * never blocks it for good.
 */
export function useChatResultInspection(
  sessionId: string | null | undefined,
  activity: ChatSessionActivityDTO | null | undefined,
  messages: readonly ChatMessageDTO[],
): (sessionId: string, result: ChatSessionResultDTO) => Promise<unknown> {
  const queryClient = useQueryClient();
  const { mutateAsync } = useRecordChatInspection();
  const refreshedRef = useRef<string | null>(null);
  const result = activity?.unseenResult ? activity.latestResult : null;
  const missing =
    !!sessionId &&
    !!result?.hasOutput &&
    result.attempt != null &&
    !messages.some(
      (message) =>
        message.role === "assistant" &&
        message.taskId === result.taskId &&
        message.attempt === result.attempt,
    );
  const resultKey = result
    ? `${sessionId}:${result.taskId}:${result.attempt ?? "-"}`
    : null;
  useEffect(() => {
    if (!missing || !sessionId || !resultKey) return;
    if (refreshedRef.current === resultKey) return;
    refreshedRef.current = resultKey;
    // A first read still out would answer from before the result.
    void invalidateAndReadAgain(queryClient, ["chat-messages", sessionId]);
  }, [missing, queryClient, resultKey, sessionId]);
  return useCallback(
    (id: string, acknowledged: ChatSessionResultDTO) =>
      mutateAsync({ sessionId: id, result: acknowledged }),
    [mutateAsync],
  );
}

export function useReplyToProviderRequest(): MutationHook<
  {
    sessionId: string;
    taskId: string;
    attempt: number;
    providerRequestId: string;
    validatedResponse: Record<string, unknown>;
    /** Start and answer (g5.chat.start-and-send). */
    startCompute?: boolean;
  },
  NativeProviderRequestReplyResponseDTO
> {
  const queryClient = useQueryClient();
  const requestIds = useRef(new Map<string, string>());
  const mutation = useMutation<
    NativeProviderRequestReplyResponseDTO,
    Error,
    {
      sessionId: string;
      taskId: string;
      attempt: number;
      providerRequestId: string;
      validatedResponse: Record<string, unknown>;
      startCompute?: boolean;
    }
  >({
    mutationFn: ({
      taskId,
      attempt,
      providerRequestId,
      validatedResponse,
      startCompute,
    }) => {
      const key = `${taskId}:${attempt}:${providerRequestId}:${JSON.stringify(validatedResponse)}`;
      let requestId = requestIds.current.get(key);
      if (!requestId) {
        requestId = randomUuid();
        requestIds.current.set(key, requestId);
      }
      return api.post<NativeProviderRequestReplyResponseDTO>(
        `/api/v1/tasks/${taskId}/provider-requests/${encodeURIComponent(providerRequestId)}/reply`,
        {
          attempt,
          requestId,
          validatedResponse,
          ...(startCompute ? { startCompute: true } : {}),
        },
      );
    },
    onSuccess: (data, vars) => {
      if (data.startCompute) invalidateWorkspaceVmQueries(queryClient);
      requestIds.current.delete(
        `${vars.taskId}:${vars.attempt}:${vars.providerRequestId}:${JSON.stringify(vars.validatedResponse)}`,
      );
      queryClient.invalidateQueries({
        queryKey: ["session-timeline", vars.sessionId],
      });
      queryClient.invalidateQueries({
        queryKey: ["chat-messages", vars.sessionId],
      });
    },
  });
  return toMutationHook(mutation);
}

export function useStopStreaming(): MutationHook<
  { sessionId: string; messageId: string },
  StopGenerationResponseDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    StopGenerationResponseDTO,
    Error,
    { sessionId: string; messageId: string }
  >({
    mutationFn: async ({ sessionId, messageId }) => {
      const result = await api.post<StopGenerationResponseDTO | null>(
        `/api/v1/sessions/${sessionId}/messages/${messageId}/stop`,
      );
      // An older server answers without a body or a disposition.
      return result ?? { stopped: true };
    },
    onSuccess: (_data, { sessionId }) => {
      queryClient.invalidateQueries({
        queryKey: ["chat-messages", sessionId],
      });
      // The stop's saved rows (interrupted marker, outcome) and the task's
      // state land with the history batch (chat g5.chat.history-batches).
      queryClient.invalidateQueries({
        queryKey: ["session-timeline", sessionId],
      });
      queryClient.invalidateQueries({ queryKey: ["session-task", sessionId] });
      queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-activity-current"],
      });
      // Stop ends the turn (`g5.agents.lifecycle.chat-stop`): a queued or held
      // turn ends at once and a running one once its attempt stops, so the
      // workspace Activity counts (active/stopped) change. Refresh the stats
      // so "Active now" does not linger on a stale running count.
      queryClient.invalidateQueries({ queryKey: ["workspace-stats"] });
    },
  });
  return toMutationHook(mutation);
}

export function useEndBackgroundWait(): MutationHook<
  { sessionId: string; taskId: string; attempt: number },
  { delivered: boolean }
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    { delivered: boolean },
    Error,
    { sessionId: string; taskId: string; attempt: number }
  >({
    // The request names the attempt whose wait the owner saw, so a delayed or
    // retried End never reaches the session's next attempt.
    mutationFn: ({ sessionId, taskId, attempt }) =>
      api.post<{ delivered: boolean }>(
        `/api/v1/sessions/${sessionId}/background/end`,
        { taskId, attempt },
      ),
    onSuccess: (_data, { sessionId }) => {
      queryClient.invalidateQueries({
        queryKey: ["chat-messages", sessionId],
      });
      queryClient.invalidateQueries({ queryKey: ["workspace-tasks"] });
      queryClient.invalidateQueries({
        queryKey: ["workspace-activity-current"],
      });
      queryClient.invalidateQueries({ queryKey: ["workspace-stats"] });
      queryClient.invalidateQueries({
        queryKey: ["session-timeline", sessionId],
      });
    },
  });
  return toMutationHook(mutation);
}

export function useForkSession(): MutationHook<
  { sessionId: string; workspaceId: string; atIndex: number },
  { sessionId: string; messagesCopied: number }
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    { sessionId: string; messagesCopied: number },
    Error,
    { sessionId: string; workspaceId: string; atIndex: number }
  >({
    mutationFn: ({ sessionId, atIndex }) =>
      api.post<{ sessionId: string; messagesCopied: number }>(
        `/api/v1/sessions/${sessionId}/fork`,
        {
          fromMessageIndex: atIndex,
        },
      ),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: ["chat-sessions", variables.workspaceId],
      });
    },
  });
  return toMutationHook(mutation);
}

export function useCreateChatSession(): MutationHook<
  {
    workspaceId: string;
    title?: string;
    scopeType?: string;
    scopeRef?: string;
  },
  ChatSessionDTO
> {
  const queryClient = useQueryClient();
  const mutation = useMutation<
    ChatSessionDTO,
    Error,
    {
      workspaceId: string;
      title?: string;
      scopeType?: string;
      scopeRef?: string;
    }
  >({
    mutationFn: ({ workspaceId, title, scopeType, scopeRef }) => {
      const body: {
        title?: string;
        scopeType: string;
        scopeRef?: string;
      } = {
        scopeType: scopeType ?? "workspace",
      };
      if (title) body.title = title;
      if (scopeRef) body.scopeRef = scopeRef;
      return api.post<ChatSessionDTO>(
        `/api/v1/workspaces/${workspaceId}/sessions`,
        body,
      );
    },
    onSuccess: (data) => {
      const insertSession = (old: ChatSessionDTO[] | undefined) => {
        if (!old) return [data];
        if (old.some((s) => s.id === data.id)) return old;
        return [data, ...old];
      };

      // Only update the exact cache variants the new session belongs to.
      queryClient.setQueryData<ChatSessionDTO[]>(
        ["chat-sessions", data.workspaceId, data.scopeType],
        insertSession,
      );
      queryClient.setQueryData<ChatSessionDTO[]>(
        ["chat-sessions", data.workspaceId, undefined],
        insertSession,
      );

      // Background-refetch for eventual consistency.
      queryClient.invalidateQueries({
        queryKey: ["chat-sessions", data.workspaceId, data.scopeType],
        exact: true,
      });
      queryClient.invalidateQueries({
        queryKey: ["chat-sessions", data.workspaceId, undefined],
        exact: true,
      });
      queryClient.invalidateQueries({
        queryKey: ["chat-session", data.id],
      });
    },
  });
  return toMutationHook(mutation);
}

// ---------------------------------------------------------------------------
// Session Timeline Hook — fetches timeline across all tasks for a session
// ---------------------------------------------------------------------------

export type { TimelineCoverage };

/**
 * `useSessionTimeline` result: the held chronological rows plus the
 * load-older surface. `loadOlder` is single-flight per session and never
 * re-reads held pages; a failed older read surfaces on `appendError` without
 * collapsing the loaded rows.
 */
export type PaginatedTimelineState = AsyncState<Page<TimelineEntry>> & {
  hasOlder: boolean;
  isLoadingOlder: boolean;
  loadOlder: () => Promise<void>;
  appendError: Error | null;
  coverage: TimelineCoverage;
  /**
   * Completed reads of saved history from the server for this session (0
   * before the first). A live row placed into the held window is a cache
   * write, not a read, and does not count; the live tail is re-read after
   * each read (chat g5.chat.live-tail-recovery).
   */
  historyReads: number;
};

type TimelineWindow = Page<TimelineEntry>;

function sharesEntry(window: TimelineWindow, ids: Set<string>): boolean {
  return window.items.some((entry) => ids.has(entry.id));
}

/**
 * Merge a newer window over held rows that it overlaps: the newer window wins
 * for the range it covers, held rows strictly older than the overlap stay in
 * front, and the floor cursor describes the oldest row actually kept.
 */
function mergeTimelineWindow(
  held: TimelineWindow,
  newer: TimelineWindow,
): TimelineWindow {
  const newerIds = new Set(newer.items.map((entry) => entry.id));
  const overlap = held.items.findIndex((entry) => newerIds.has(entry.id));
  const preserved = held.items
    .slice(0, overlap === -1 ? held.items.length : overlap)
    .filter((entry) => !newerIds.has(entry.id));
  // Live rows the saved window does not cover stay ahead of it; the ones it
  // covers are replaced by their saved rows (chat g5.chat.replay).
  if (!preserved.length) return retainLiveRows(held, newer);
  return retainLiveRows(held, {
    items: [...preserved, ...newer.items],
    cursor: held.cursor,
    hasMore: held.hasMore,
  });
}

/**
 * The oldest held native request whose status can still change without a new
 * row. A request's status is joined onto its opening row when the row is
 * read, so a refresh must reach back to that row while the request is open
 * (g5.chat.history-refresh). A live copy of the opening row carries no
 * status at all, and nothing but a read brings its saved copy (a live row is
 * kept until that copy is held, chat g5.chat.live-tail-recovery), so it
 * counts as open until then: the question stays provisional and gates the
 * composer otherwise. A reply or invalidation row, or the attempt's outcome,
 * ends that: later changes either arrive as rows or no longer occur.
 */
function oldestOpenNativeRequest(
  rows: TimelineEntry[],
): TimelineEntry | undefined {
  const settledRequests = new Set<string>();
  const settledAttempts = new Set<string>();
  for (const entry of rows) {
    const kind = entry.metadata?.kind;
    if (
      (kind === "provider_request.replied" ||
        kind === "provider_request.invalidated") &&
      typeof entry.metadata?.approvalId === "string"
    )
      settledRequests.add(entry.metadata.approvalId);
    if (kind === "invocation.outcome")
      settledAttempts.add(
        `${entry.metadata?.taskId}:${entry.metadata?.attempt}`,
      );
  }
  return rows.find(
    (entry) =>
      entry.metadata?.kind === "provider_request.opened" &&
      (isLiveTimelineRow(entry) ||
        entry.metadata?.nativeRequestStatus === "pending" ||
        entry.metadata?.nativeRequestStatus === "reply_pending") &&
      typeof entry.metadata?.approvalId === "string" &&
      !settledRequests.has(entry.metadata.approvalId) &&
      !settledAttempts.has(
        `${entry.metadata?.taskId}:${entry.metadata?.attempt}`,
      ),
  );
}

export function useSessionTimeline(sessionId: string): PaginatedTimelineState {
  const queryClient = useQueryClient();
  const queryKey = ["session-timeline", sessionId];
  // Read-back progress toward the held rows (g5.chat.history-refresh), kept
  // outside the query so a refresh cancelled by the next poll resumes instead
  // of restarting.
  const bridgeRef = useRef<{ sessionId: string; run: TimelineWindow } | null>(
    null,
  );
  // Single-flight ownership for older reads is keyed by session plus token, so
  // a previous session's pending read neither blocks the new session's first
  // load nor, on late completion, touches its state.
  const olderReadRef = useRef<{ sessionId: string; token: number } | null>(
    null,
  );
  const olderTokenRef = useRef(0);
  const currentSessionIdRef = useRef(sessionId);
  currentSessionIdRef.current = sessionId;
  const [isLoadingOlder, setIsLoadingOlder] = useState(false);
  const [appendError, setAppendError] = useState<Error | null>(null);
  useEffect(() => {
    setIsLoadingOlder(false);
    setAppendError(null);
  }, [sessionId]);
  // Completed server reads, told apart from cache writes: a placed live row
  // is a query update too, so the query's `dataUpdatedAt` cannot tell a read
  // from a placement, while the cache reports a fetch's success without
  // `manual`. Counted, not timed: two reads can complete in one millisecond.
  const [reads, setReads] = useState<{
    sessionId: string;
    count: number;
  } | null>(null);
  useEffect(() => {
    if (!sessionId) return;
    return queryClient.getQueryCache().subscribe((event) => {
      if (
        event.type !== "updated" ||
        event.action.type !== "success" ||
        event.action.manual ||
        event.query.queryKey.length !== 2 ||
        event.query.queryKey[0] !== "session-timeline" ||
        event.query.queryKey[1] !== sessionId
      )
        return;
      setReads((prev) => ({
        sessionId,
        count: prev?.sessionId === sessionId ? prev.count + 1 : 1,
      }));
      // A read that met held rows above a hole persistence already covers
      // leaves that hole; it is closed by sequence from saved history.
      void requestTimelineHoleFill(queryClient, sessionId);
    });
  }, [queryClient, sessionId]);
  const historyReads = reads?.sessionId === sessionId ? reads.count : 0;

  const fetchTimelinePage = useCallback(
    (cursor: string | null | undefined, signal?: AbortSignal) =>
      readSessionTimelinePage(sessionId, cursor, signal),
    [sessionId],
  );

  const query = useQuery<TimelineWindow, Error>({
    queryKey,
    queryFn: async ({ signal }) => {
      // A refresh is one newest-window read merged into the held rows
      // (g5.chat.history-refresh). Older pages are read again only to close
      // a gap or to reach a held request that is still open.
      const held = () =>
        queryClient.getQueryData<TimelineWindow>([
          "session-timeline",
          sessionId,
        ]);
      let run = await fetchTimelinePage(undefined, signal);
      if (!held()?.items.length) return retainLiveRows(held(), run);
      if (!run.items.length) return held() as TimelineWindow;

      // A window that no longer overlaps the held saved rows reads back
      // until it does (a held live row carries the id its saved row will
      // have, so meeting one proves nothing about the saved prefix), and
      // further while a held request that is still open lies below it, so
      // that request's status is read again; a request held only as a live
      // row is open until its saved copy, the one that carries a status,
      // is read. Resume progress an interrupted read saved, when this
      // window still meets it.
      const heldIds = () => new Set(held()?.items.map((entry) => entry.id));
      const heldSavedIds = () =>
        new Set(
          (held()?.items ?? [])
            .filter((entry) => !isLiveTimelineRow(entry))
            .map((entry) => entry.id),
        );
      const needsOlder = () => {
        const saved = heldSavedIds();
        if (saved.size && !sharesEntry(run, saved)) return true;
        const runIds = new Set(run.items.map((entry) => entry.id));
        const open = oldestOpenNativeRequest([
          ...(held()?.items ?? []).filter((entry) => !runIds.has(entry.id)),
          ...run.items,
        ]);
        return !!open && !runIds.has(open.id);
      };
      const saved = bridgeRef.current;
      if (
        saved?.sessionId === sessionId &&
        sharesEntry(saved.run, new Set(run.items.map((entry) => entry.id)))
      ) {
        run = mergeTimelineWindow(saved.run, run);
      }
      while (needsOlder() && run.hasMore && run.cursor) {
        bridgeRef.current = { sessionId, run };
        const older = await fetchTimelinePage(run.cursor, signal);
        const runIds = new Set(run.items.map((entry) => entry.id));
        run = {
          items: [
            ...older.items.filter((entry) => !runIds.has(entry.id)),
            ...run.items,
          ],
          cursor: older.cursor,
          hasMore: older.hasMore,
        };
        if (!older.items.length) break;
      }
      if (bridgeRef.current?.sessionId === sessionId) bridgeRef.current = null;
      const current = held();
      // History ended without meeting the held rows: they no longer exist
      // server-side, so the run's own floor is the true floor. Live rows of
      // the running turn are not server history yet and stay.
      if (!current?.items.length || !sharesEntry(run, heldIds()))
        return retainLiveRows(current, run);
      return mergeTimelineWindow(current, run);
    },
    enabled: !!sessionId,
  });

  const loadOlder = useCallback(async () => {
    if (!sessionId) return;
    if (olderReadRef.current?.sessionId === sessionId) return;
    const key = ["session-timeline", sessionId];
    const cached = queryClient.getQueryData<TimelineWindow>(key);
    if (!cached) {
      // No rows yet (the first read failed): loading older starts with it.
      await queryClient.refetchQueries({ queryKey: key, exact: true });
      return;
    }
    if (!cached.hasMore || !cached.cursor) return;
    const floor = cached.cursor;
    const token = ++olderTokenRef.current;
    olderReadRef.current = { sessionId, token };
    setIsLoadingOlder(true);
    setAppendError(null);
    try {
      const older = await fetchTimelinePage(floor);
      // A refresh in flight snapshotted the held rows before this prepend and
      // would overwrite it on commit; cancel it, prepend, then read again so a
      // cancelled refresh is never lost.
      const refreshing = queryClient.isFetching({ queryKey: key, exact: true });
      await queryClient.cancelQueries({ queryKey: key, exact: true });
      queryClient.setQueryData<TimelineWindow>(key, (current) => {
        // The floor moved meanwhile (another load or a refresh that found the
        // held rows gone); this page no longer adjoins it.
        if (!current || current.cursor !== floor) return current;
        const ids = new Set(current.items.map((entry) => entry.id));
        return {
          items: [
            ...older.items.filter((entry) => !ids.has(entry.id)),
            ...current.items,
          ],
          cursor: older.cursor,
          hasMore: older.hasMore,
        };
      });
      if (refreshing) {
        void queryClient.invalidateQueries({ queryKey: key, exact: true });
      }
    } catch (error) {
      if (currentSessionIdRef.current === sessionId) {
        setAppendError(
          error instanceof Error ? error : new Error(String(error)),
        );
      }
    } finally {
      if (olderReadRef.current?.token === token) olderReadRef.current = null;
      if (currentSessionIdRef.current === sessionId) setIsLoadingOlder(false);
    }
  }, [fetchTimelinePage, queryClient, sessionId]);

  const data = query.data;
  const hasOlder = !!data?.hasMore && !!data.cursor;
  const coverage: TimelineCoverage = data
    ? {
        status: "ready",
        floorCreatedAt: data.items[0]?.createdAt ?? null,
        hasOlder,
      }
    : {
        status: query.isError ? "error" : "loading",
        floorCreatedAt: null,
        hasOlder: false,
      };
  const surface = {
    hasOlder,
    isLoadingOlder,
    loadOlder,
    appendError,
    coverage,
    historyReads,
  };
  // A failed refresh keeps the held rows; only a failed first read is an error.
  if (data) return { status: "ok", data, ...surface };
  if (query.isError) return { status: "error", error: query.error, ...surface };
  return { status: "loading", ...surface };
}
