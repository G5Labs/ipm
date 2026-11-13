import { compareTimelineEntries } from "./providerOutput";
import type { TimelineEntry } from "./types";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function numberValue(value: unknown): number | undefined {
  return typeof value === "number" && Number.isInteger(value)
    ? value
    : undefined;
}

/** Projected event for Claude's whole reported background set. */
export const BACKGROUND_TASKS_EVENT = "provided_agent.background.tasks";

export type BackgroundTaskKind = "shell" | "agent" | "other";

/**
 * One entry of the background set Claude reports while an attempt stays live
 * (conversations `g5.conversation.providers.claude-background.visibility`).
 * `subtaskId` is Claude's own task id; it is never the G5 task id that the
 * row's `metadata.taskId` names.
 */
export interface BackgroundTaskSummary {
  subtaskId: string;
  kind: BackgroundTaskKind;
  /** Bounded, redacted description; empty when Claude gave none. */
  description: string;
  /** When the adapter first saw the task, in Unix milliseconds. */
  startedAtUnixMs: number | null;
}

/** Claude's whole live background set, replaced by every report. */
export interface BackgroundTaskSet {
  /** The listed tasks; the adapter lists a bounded prefix of the set. */
  tasks: BackgroundTaskSummary[];
  /** Full size of the set, never smaller than `tasks.length`. */
  count: number;
  /** True when the set has more tasks than are listed. */
  truncated: boolean;
  /** When the current wait began (the set last became non-empty), in Unix
   * milliseconds; null when the adapter did not report it. */
  waitStartedAtUnixMs: number | null;
  /** The G5 task and attempt that reported the set; End names this attempt
   * so a stale request never reaches the session's next attempt. */
  taskId: string | null;
  attempt: number | null;
}

function backgroundTaskKind(value: unknown): BackgroundTaskKind {
  return value === "shell" || value === "agent" ? value : "other";
}

function backgroundTaskSummaries(value: unknown): BackgroundTaskSummary[] {
  if (!Array.isArray(value)) return [];
  return value.filter(isRecord).map((task, index) => {
    const startedAt = task.started_at_unix_ms;
    return {
      subtaskId: stringValue(task.task_id) ?? `background-${index}`,
      kind: backgroundTaskKind(task.kind),
      description:
        typeof task.description === "string" ? task.description.trim() : "",
      startedAtUnixMs:
        typeof startedAt === "number" && Number.isFinite(startedAt)
          ? startedAt
          : null,
    };
  });
}

function isBackgroundTaskSummary(
  value: unknown,
): value is BackgroundTaskSummary {
  return (
    isRecord(value) &&
    typeof value.subtaskId === "string" &&
    (value.kind === "shell" ||
      value.kind === "agent" ||
      value.kind === "other") &&
    typeof value.description === "string" &&
    (value.startedAtUnixMs === null ||
      typeof value.startedAtUnixMs === "number")
  );
}

/**
 * The background set of a raw `background_tasks.replaced` payload, as the
 * session's live task carries it (`backgroundWait`); null when absent.
 */
export function backgroundTaskSetFromPayload(
  payload: unknown,
  identity: { taskId: string | null; attempt: number | null } = {
    taskId: null,
    attempt: null,
  },
): BackgroundTaskSet | null {
  if (!isRecord(payload)) return null;
  const tasks = backgroundTaskSummaries(payload.tasks);
  const count = Math.max(numberValue(payload.count) ?? 0, tasks.length);
  return {
    tasks,
    count,
    truncated: payload.truncated === true || count > tasks.length,
    waitStartedAtUnixMs: numberValue(payload.wait_started_at_unix_ms) ?? null,
    taskId: identity.taskId,
    attempt: identity.attempt,
  };
}

/** The background set a projected `background_tasks.replaced` row carries. */
export function backgroundTaskSetFromEntry(
  entry: TimelineEntry,
): BackgroundTaskSet | null {
  const metadata = entry.metadata ?? {};
  if (metadata.providerEvent !== BACKGROUND_TASKS_EVENT) return null;
  const tasks = Array.isArray(metadata.backgroundTasks)
    ? metadata.backgroundTasks.filter(isBackgroundTaskSummary)
    : [];
  const reported = numberValue(metadata.backgroundCount);
  const count = Math.max(reported ?? 0, tasks.length);
  return {
    tasks,
    count,
    truncated: metadata.backgroundTruncated === true || count > tasks.length,
    waitStartedAtUnixMs:
      numberValue(metadata.backgroundWaitStartedAtUnixMs) ?? null,
    taskId: stringValue(metadata.taskId) ?? null,
    attempt: numberValue(metadata.attempt) ?? null,
  };
}

/**
 * The newest background set among `entries` in journal order, or null when
 * none was reported. Each report replaces the whole set, so an empty newest
 * report means no background work remains.
 */
export function latestBackgroundTaskSet(
  entries: readonly TimelineEntry[],
): BackgroundTaskSet | null {
  const reports = entries.filter(
    (entry) => entry.metadata?.providerEvent === BACKGROUND_TASKS_EVENT,
  );
  if (reports.length === 0) return null;
  const latest = [...reports].sort(compareTimelineEntries).at(-1);
  return latest ? backgroundTaskSetFromEntry(latest) : null;
}

export interface NativeProviderRequestIdentity {
  taskId: string;
  attempt: number;
  providerRequestId: string;
  requestKind: "question" | "permission";
}

/**
 * Return the execution-fenced identity carried by a native provider request.
 * The browser must echo all four fields to the task-scoped reply route; an
 * approval id or session id alone cannot identify an attempt-bound request.
 */
export function nativeProviderRequestIdentity(
  entry: TimelineEntry,
): NativeProviderRequestIdentity | null {
  const metadata = entry.metadata ?? {};
  const taskId = stringValue(metadata.taskId);
  const attempt = numberValue(metadata.attempt);
  const providerRequestId = stringValue(metadata.providerRequestId);
  const requestKind = metadata.requestKind;
  if (
    !taskId ||
    attempt === undefined ||
    !providerRequestId ||
    (requestKind !== "question" && requestKind !== "permission")
  ) {
    return null;
  }
  return { taskId, attempt, providerRequestId, requestKind };
}

/**
 * Adapt the durable provider-neutral task journal to the established chat
 * presentation fields. The durable schema remains available under
 * `kind`/`payload`; these additive fields are a browser projection only.
 */
export function projectTaskTimelineEntry(entry: TimelineEntry): TimelineEntry {
  const metadata = entry.metadata ?? {};
  if (metadata.schema_version !== "g5.task-event.v1") return entry;
  const kind = stringValue(metadata.kind);
  const payload = isRecord(metadata.payload) ? metadata.payload : {};
  if (!kind) return entry;

  // A rejected journal fact is durable error evidence, never an actionable
  // event: interpreting its original kind would render a phantom approval
  // card or mark a live request resolved (issue #628).
  const projectionError = stringValue(metadata.projection_error);
  if (projectionError) {
    return {
      ...entry,
      metadata: {
        ...metadata,
        source: "provided_agent",
        provider: "provided_agent",
        providerEvent: "provided_agent.error",
        phase: "status",
        level: "error",
        message: projectionError,
      },
    };
  }

  const projected: Record<string, unknown> = {
    ...metadata,
    source: "provided_agent",
    provider: "provided_agent",
  };
  // The journal sequence orders rows that share a timestamp, live or saved
  // (chat g5.chat.replay).
  const sequence =
    entry.sequence ??
    (typeof metadata.seq === "number" ? metadata.seq : undefined);
  const payloadMessageId = stringValue(payload.message_id);
  if (!stringValue(projected.messageId) && payloadMessageId) {
    projected.messageId = payloadMessageId;
  }

  switch (kind) {
    case "input.accepted":
    case "input.shipped":
    case "input.steer_reserved":
    case "input.steer_pending":
    case "input.steer_rejected":
      projected.providerEvent = `provided_agent.${kind}`;
      projected.phase = "status";
      break;
    case "provider.session":
      projected.providerEvent = "provided_agent.started";
      projected.phase = stringValue(payload.phase) ?? "started";
      projected.model = payload.model_id;
      break;
    case "message.delta":
      projected.providerEvent = "provided_agent.output.delta";
      projected.phase = "output";
      projected.blockId = stringValue(payload.block_id) ?? "native-output";
      projected.text = payload.text_delta;
      projected.streamPersistence = "durable";
      break;
    case "message.final":
      projected.providerEvent = "provided_agent.output.final";
      projected.phase = "output";
      projected.blockId = stringValue(payload.block_id) ?? "native-output";
      projected.text = payload.content;
      break;
    case "thought.final":
      projected.providerEvent = "provided_agent.thought";
      projected.phase = "thought";
      projected.text = payload.content;
      break;
    // Tool events carry the digest of the full input/output as `*_ref` and a
    // bounded inline copy the card renders (`input` as the tool's JSON,
    // `output` as text). Older journals carry only the digest, which is not
    // renderable: the card then shows the tool name and status alone.
    case "tool.started":
      projected.providerEvent = "provided_agent.tool.started";
      projected.phase = "tool_call";
      projected.toolCallId = payload.call_id;
      projected.toolName = payload.name;
      projected.inputRef = payload.input_ref;
      projected.input = payload.input ?? null;
      projected.inputTruncated = payload.input_truncated === true;
      break;
    case "tool.completed": {
      projected.providerEvent = "provided_agent.tool.completed";
      projected.phase = "tool_result";
      projected.toolCallId = payload.call_id;
      if (stringValue(payload.name)) projected.toolName = payload.name;
      projected.outputRef = payload.output_ref;
      const output = typeof payload.output === "string" ? payload.output : null;
      projected.output = output;
      if (output !== null) projected.contentSnippet = output;
      projected.outputTruncated = payload.output_truncated === true;
      projected.isError = payload.status === "error";
      break;
    }
    case "usage.updated":
      projected.providerEvent = "provided_agent.usage.updated";
      projected.phase = "usage";
      projected.usage = payload.usage;
      break;
    // Claude's background work. `payload.task_id` is Claude's task id, so it
    // is projected as `subtaskId`; `taskId` stays the G5 task and `kind` the
    // durable event kind.
    case "background_tasks.replaced": {
      const tasks = backgroundTaskSummaries(payload.tasks);
      const reported = numberValue(payload.count);
      const count = Math.max(reported ?? 0, tasks.length);
      projected.providerEvent = BACKGROUND_TASKS_EVENT;
      projected.phase = "background";
      projected.backgroundTasks = tasks;
      projected.backgroundCount = count;
      projected.backgroundTruncated =
        payload.truncated === true || count > tasks.length;
      projected.backgroundWaitStartedAtUnixMs =
        numberValue(payload.wait_started_at_unix_ms) ?? null;
      break;
    }
    case "subtask.started":
      projected.providerEvent = "provided_agent.subtask.started";
      projected.phase = "background";
      projected.subtaskId = stringValue(payload.task_id);
      projected.subtaskKind = backgroundTaskKind(payload.kind);
      projected.subtaskDescription = stringValue(payload.description);
      break;
    case "subtask.progress":
      projected.providerEvent = "provided_agent.subtask.progress";
      projected.phase = "background";
      projected.subtaskId = stringValue(payload.task_id);
      projected.subtaskDescription = stringValue(payload.description);
      break;
    case "subtask.completed":
      projected.providerEvent = "provided_agent.subtask.completed";
      projected.phase = "background";
      projected.subtaskId = stringValue(payload.task_id);
      projected.subtaskStatus =
        payload.status === "ok" ||
        payload.status === "stopped" ||
        payload.status === "error"
          ? payload.status
          : undefined;
      projected.subtaskSummary = stringValue(payload.summary);
      break;
    case "notice":
      // Info notices (for example the owner ending a background wait) are
      // neutral facts, not warnings.
      projected.providerEvent =
        payload.level === "error"
          ? "provided_agent.error"
          : payload.level === "info"
            ? "provided_agent.info"
            : "provided_agent.warning";
      projected.phase = "status";
      projected.level = payload.level;
      projected.code = payload.code;
      projected.message = payload.message;
      projected.retryable = payload.retryable;
      break;
    case "invocation.outcome": {
      // Module success means it returned an agent result, not that the agent
      // succeeded. Older failures may have no notice or provider frames at all.
      const result = isRecord(payload.result) ? payload.result : {};
      const status =
        result.schema_version === "g5.agent.turn.outcome/v1"
          ? result.status
          : payload.status;
      if (
        status === "failed" ||
        status === "timed_out" ||
        status === "limit_exceeded"
      ) {
        projected.providerEvent = "provided_agent.error";
        projected.phase = "status";
        projected.level = "error";
        projected.message =
          status === "timed_out"
            ? "The agent timed out before completing its response."
            : status === "limit_exceeded"
              ? "The agent exceeded its output or log limit before completing its response."
              : "Error generating response";
        projected.outcomeFallback = true;
      }
      break;
    }
    case "provider_request.opened": {
      const providerRequestId = stringValue(payload.provider_request_id);
      const requestKind = payload.request_kind;
      if (
        providerRequestId &&
        (requestKind === "question" || requestKind === "permission")
      ) {
        const display = isRecord(payload.display) ? payload.display : {};
        projected.providerEvent = "provided_agent.approval.requested";
        projected.phase = "approval";
        projected.approvalId = providerRequestId;
        projected.providerRequestId = providerRequestId;
        projected.requestKind = requestKind;
        projected.toolName = payload.tool_name;
        projected.input =
          requestKind === "question"
            ? { questions: display.questions ?? [] }
            : (display.input ?? payload.payload ?? null);
        projected.requestedAction =
          requestKind === "question"
            ? "Answer the provider question"
            : "Allow the requested tool use";
        projected.deadlineUnixMs = payload.deadline_unix_ms;
        projected.nativeResumeSessionId = payload.resume_session_id;
      }
      break;
    }
    case "provider_request.replied":
      projected.providerEvent = "provided_agent.approval.resolved";
      projected.phase = "approval";
      projected.approvalId = payload.provider_request_id;
      projected.decision =
        payload.disposition === "denied" ? "rejected" : "approved";
      break;
    case "provider_request.invalidated":
      projected.providerEvent = "provided_agent.approval.failed";
      projected.phase = "approval";
      projected.approvalId = payload.provider_request_id;
      projected.errorCode = payload.reason;
      break;
  }

  return {
    ...entry,
    ...(sequence !== undefined ? { sequence } : {}),
    metadata: projected,
  };
}

export function projectTaskTimelineEntries(
  entries: TimelineEntry[],
): TimelineEntry[] {
  return entries.map(projectTaskTimelineEntry);
}
