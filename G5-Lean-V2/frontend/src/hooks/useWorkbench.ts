// =============================================================================
// G5 Client — Workbench configuration and templated chat starts
// =============================================================================
// Workbench g5.workbench.actions / g5.workbench.persona. The configuration
// (`GET /workspaces/:id/workbench`) is one committed snapshot: its `revision`
// is pinned on every templated start so the server resolves the template and
// persona against the same snapshot, refusing a stale one with
// `WORKBENCH_CONFIG_STALE` (409) before anything is created.
// =============================================================================

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef } from "react";
import { api, ApiError } from "../lib/api";
import type {
  AgentLaunchSelectionV1,
  AsyncState,
  MutationHook,
  StartChatSessionResponseDTO,
  WorkbenchConfigDTO,
  WorkbenchStartContext,
  WorkspacePhase,
  ChatMessageContext,
  ChatUploadAttachment,
} from "../lib/types";
import { randomUuid } from "../lib/randomUuid";
import { WORKSPACE_PHASES } from "../lib/workbenchDefaults";
import { toAsyncState, toMutationHook } from "./hookUtils";
import { seedStartedChatSession } from "./useChat";

export const workbenchConfigKey = (workspaceId: string) =>
  ["workbench", workspaceId] as const;

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

/** An action that names a kind; a menu's `items`, when present, are actions too. */
function isWorkbenchAction(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.kind === "string" &&
    (value.items === undefined ||
      (Array.isArray(value.items) && value.items.every(isWorkbenchAction)))
  );
}

/** An action group the ribbon can project: a named record with an action list. */
function isWorkbenchActionGroup(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.name === "string" &&
    Array.isArray(value.actions) &&
    value.actions.every(isWorkbenchAction)
  );
}

/**
 * `true` when a workbench payload has the shape the ribbon renders from:
 * for every phase a list of action groups whose actions name a kind, a
 * persona per phase and a configuration record. A body of another shape
 * (a proxy's empty list, a truncated body, a group without actions) reads
 * as the error state and leaves the ribbon on the complete built-in
 * fallback (ui g5.ui.workbench-config) instead of throwing mid-render.
 */
export function isWorkbenchConfigDTO(
  value: unknown,
): value is WorkbenchConfigDTO {
  if (!isRecord(value)) return false;
  const dto = value as Partial<WorkbenchConfigDTO>;
  if (typeof dto.revision !== "string") return false;
  if (!isRecord(dto.actions) || !isRecord(dto.personas)) return false;
  if (!isRecord(dto.config)) return false;
  return WORKSPACE_PHASES.every((phase) => {
    const groups: unknown = dto.actions?.[phase];
    return (
      Array.isArray(groups) &&
      groups.every(isWorkbenchActionGroup) &&
      typeof dto.personas?.[phase]?.text === "string"
    );
  });
}

/** The workspace's resolved ribbon actions and personas. */
export function useWorkbenchConfig(
  workspaceId: string,
  enabled: boolean,
): AsyncState<WorkbenchConfigDTO> & { refetch: () => void } {
  const query = useQuery<WorkbenchConfigDTO, Error>({
    queryKey: workbenchConfigKey(workspaceId),
    queryFn: async ({ signal }) => {
      const payload = await api.get<unknown>(
        `/api/v1/workspaces/${workspaceId}/workbench`,
        { signal },
      );
      if (!isWorkbenchConfigDTO(payload)) {
        throw new Error("Invalid workbench configuration payload");
      }
      return payload;
    },
    enabled: enabled && !!workspaceId,
    // The revision follows the committed docs head; a commit from an agent
    // turn lands within a refetch cycle without a page reload.
    refetchInterval: 15_000,
  });
  const state = toAsyncState(query);
  return { ...state, refetch: () => void query.refetch() };
}

/** `true` for the server's "configuration changed under you" refusal. */
export function isWorkbenchConfigStale(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.status === 409 &&
    error.details?.code === "WORKBENCH_CONFIG_STALE"
  );
}

export const WORKBENCH_CONFIG_STALE_MESSAGE =
  "The workbench configuration changed; try again";

export interface StartWorkbenchActionInput {
  workspaceId: string;
  persona: WorkspacePhase;
  /** Ribbon phase the action was clicked in (selects that phase's template). */
  phase: WorkspacePhase;
  /** Ribbon action id; omitted for a bare persona start with typed text. */
  actionId?: string;
  context?: WorkbenchStartContext;
  configRevision: string;
  /**
   * The message text: typed text appended after the template ("" when
   * none), or the complete request of a seeded composer's send (workbench
   * g5.workbench.chat-actions), which carries no `actionId`.
   */
  content: string;
  agentLaunch: AgentLaunchSelectionV1;
  /**
   * Start and send (g5.chat.start-and-send): record Start consent and queue
   * this first message bound to it, in the same request.
   */
  startCompute?: boolean;
  scopeType?: string;
  scopeRef?: string;
  /** The composer's references, for a seeded composer's send. */
  messageContext?: ChatMessageContext;
  /** The composer's attachments, for a seeded composer's send. */
  attachments?: ChatUploadAttachment[];
}

/**
 * One in-flight submission: the request key and the exact payload, frozen
 * when the submission is created and reused by every retry until it
 * resolves (`g5.conversation.inbox.reservation`: a lost response retried by
 * the browser replays the same session instead of starting another). Keyed
 * by the submission's own fields, like `useDeleteWorkspace` keeps its
 * `operationId` in a ref.
 */
interface FrozenSubmission {
  requestKey: string;
  body: Record<string, unknown>;
}

function frozenSubmission(
  inflight: Map<string, FrozenSubmission>,
  fingerprint: string,
  prefix: string,
  build: () => Record<string, unknown>,
): FrozenSubmission {
  const existing = inflight.get(fingerprint);
  if (existing) return existing;
  const created = { requestKey: `${prefix}-${randomUuid()}`, body: build() };
  inflight.set(fingerprint, created);
  return created;
}

/**
 * Start a templated chat under a persona (`POST /sessions/start` with the
 * workbench fields). The request key and payload are frozen per submission
 * and reused on retry until the server answers; a stale revision drops the
 * cached configuration (the caller refetches) and surfaces
 * `WORKBENCH_CONFIG_STALE_MESSAGE`.
 */
export function useStartWorkbenchAction(): MutationHook<
  StartWorkbenchActionInput,
  StartChatSessionResponseDTO
> {
  const queryClient = useQueryClient();
  const inflight = useRef(new Map<string, FrozenSubmission>());
  const mutation = useMutation<
    StartChatSessionResponseDTO,
    Error,
    StartWorkbenchActionInput
  >({
    mutationFn: async (input) => {
      const fingerprint = JSON.stringify([
        input.workspaceId,
        input.persona,
        input.phase,
        input.actionId ?? null,
        input.context ?? null,
        input.configRevision,
        input.content,
        input.scopeType ?? "workspace",
        input.scopeRef ?? null,
        input.messageContext ?? null,
        input.attachments?.map((attachment) => attachment.fileName) ?? null,
        // `startCompute` is frozen with the body, not part of the identity:
        // a retry after the VM came up is the same submission.
      ]);
      const submission = frozenSubmission(
        inflight.current,
        fingerprint,
        "workbench",
        () => {
          const body: Record<string, unknown> = {
            content: input.content,
            scopeType: input.scopeType ?? "workspace",
            agentLaunch: input.agentLaunch,
            persona: input.persona,
            phase: input.phase,
            configRevision: input.configRevision,
          };
          if (input.scopeRef) body.scopeRef = input.scopeRef;
          if (input.actionId) body.actionId = input.actionId;
          if (input.context) body.workbenchContext = input.context;
          if (input.messageContext) body.context = input.messageContext;
          if (input.attachments?.length) body.attachments = input.attachments;
          if (input.startCompute) body.startCompute = true;
          return body;
        },
      );
      try {
        const started = await api.post<StartChatSessionResponseDTO>(
          `/api/v1/workspaces/${input.workspaceId}/sessions/start`,
          { ...submission.body, requestKey: submission.requestKey },
        );
        inflight.current.delete(fingerprint);
        return started;
      } catch (error) {
        if (isWorkbenchConfigStale(error)) {
          // The payload is stale too: the next attempt pins a new revision.
          inflight.current.delete(fingerprint);
          await queryClient.invalidateQueries({
            queryKey: workbenchConfigKey(input.workspaceId),
          });
          throw new Error(WORKBENCH_CONFIG_STALE_MESSAGE, { cause: error });
        }
        if (error instanceof ApiError && error.isClientError) {
          // Refused outright; a retry with the same payload would be too.
          inflight.current.delete(fingerprint);
        }
        throw error;
      }
    },
    onSuccess: (data) => {
      seedStartedChatSession(queryClient, data);
      // A Start and send recorded a Start: re-read the VM state. The
      // start-session response carries the Start with its queued message.
      if (data.startCompute ?? data.message?.startCompute) {
        void queryClient.invalidateQueries({ queryKey: ["workspace-vm"] });
      }
    },
  });
  return toMutationHook(mutation);
}

export interface FollowViewInput {
  sessionId: string;
  workspaceId: string;
  /** Last message index of the source as the browser sees it. */
  atIndex: number;
  persona: WorkspacePhase;
  configRevision: string;
}

/**
 * "Follow view" (g5.workbench.persona): fork the open chat and continue it
 * under the current phase's persona. The fork, its persona and its first
 * input commit together server-side; an empty or queued-only source starts
 * a new session instead (the response still names the session to open).
 * The request key and payload are frozen per submission like the start.
 */
export function useFollowView(): MutationHook<
  FollowViewInput,
  { sessionId: string; messagesCopied: number }
> {
  const queryClient = useQueryClient();
  const inflight = useRef(new Map<string, FrozenSubmission>());
  const mutation = useMutation<
    { sessionId: string; messagesCopied: number },
    Error,
    FollowViewInput
  >({
    mutationFn: async (input) => {
      const fingerprint = JSON.stringify([
        input.sessionId,
        input.atIndex,
        input.persona,
        input.configRevision,
      ]);
      const submission = frozenSubmission(
        inflight.current,
        fingerprint,
        "follow-view",
        () => ({
          fromMessageIndex: Math.max(0, input.atIndex),
          persona: input.persona,
          configRevision: input.configRevision,
        }),
      );
      try {
        const forked = await api.post<{
          sessionId: string;
          messagesCopied: number;
        }>(`/api/v1/sessions/${input.sessionId}/fork`, {
          ...submission.body,
          requestKey: submission.requestKey,
        });
        inflight.current.delete(fingerprint);
        return forked;
      } catch (error) {
        if (isWorkbenchConfigStale(error)) {
          inflight.current.delete(fingerprint);
          await queryClient.invalidateQueries({
            queryKey: workbenchConfigKey(input.workspaceId),
          });
          throw new Error(WORKBENCH_CONFIG_STALE_MESSAGE, { cause: error });
        }
        if (error instanceof ApiError && error.isClientError) {
          inflight.current.delete(fingerprint);
        }
        throw error;
      }
    },
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: ["chat-sessions", variables.workspaceId],
      });
      queryClient.invalidateQueries({
        queryKey: ["workspace-tasks", variables.workspaceId],
      });
    },
  });
  return toMutationHook(mutation);
}
