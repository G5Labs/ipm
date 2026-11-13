import { describe, expect, it } from "vitest";
import { classifyAgentApprovals } from "./agentApprovals";
import {
  backgroundTaskSetFromEntry,
  latestBackgroundTaskSet,
  nativeProviderRequestIdentity,
  projectTaskTimelineEntries,
  backgroundTaskSetFromPayload,
} from "./taskTimeline";
import type { TimelineEntry } from "./types";

function taskEvent(
  id: string,
  kind: string,
  payload: Record<string, unknown>,
): TimelineEntry {
  return {
    id,
    entryType: "checkpoint",
    content: kind,
    createdAt: `2026-09-21T12:00:0${id}.000Z`,
    metadata: {
      schema_version: "g5.task-event.v1",
      kind,
      payload,
      taskId: "task-1",
      attempt: 3,
      messageId: "assistant-3",
      sessionId: "session-1",
    },
  };
}

describe("task timeline presentation", () => {
  it.each(["failed", "timed_out"])(
    "projects the nested agent %s outcome despite module success",
    (status) => {
      const [entry] = projectTaskTimelineEntries([
        taskEvent("1", "invocation.outcome", {
          status: "succeeded",
          exit_code: 0,
          result: {
            schema_version: "g5.agent.turn.outcome/v1",
            status,
            code: "provider_error",
          },
        }),
      ]);
      expect(entry.metadata).toMatchObject({
        providerEvent: "provided_agent.error",
        level: "error",
        outcomeFallback: true,
        taskId: "task-1",
        attempt: 3,
        messageId: "assistant-3",
      });
      expect(entry.metadata?.message).toBe(
        status === "failed"
          ? "Error generating response"
          : "The agent timed out before completing its response.",
      );
    },
  );

  it.each(["succeeded", "cancelled", "suspended"])(
    "does not turn an agent %s outcome into a failure",
    (status) => {
      const [entry] = projectTaskTimelineEntries([
        taskEvent("1", "invocation.outcome", {
          status: "succeeded",
          result: { schema_version: "g5.agent.turn.outcome/v1", status },
        }),
      ]);
      expect(entry.metadata?.providerEvent).toBeUndefined();
    },
  );

  it.each(["failed", "timed_out", "limit_exceeded"])(
    "projects a module %s outcome before the provider returns a result",
    (status) => {
      const [entry] = projectTaskTimelineEntries([
        taskEvent("1", "invocation.outcome", { status }),
      ]);
      expect(entry.metadata?.providerEvent).toBe("provided_agent.error");
    },
  );

  it("preserves a provider error diagnostic and its severity", () => {
    const [entry] = projectTaskTimelineEntries([
      taskEvent("1", "notice", {
        code: "provider_start_failed",
        level: "error",
        message: "Codex could not load its configuration.",
        retryable: true,
      }),
    ]);
    expect(entry.metadata).toMatchObject({
      providerEvent: "provided_agent.error",
      message: "Codex could not load its configuration.",
      retryable: true,
    });
  });

  it("projects the inline tool input and output beside their references", () => {
    const [started, completed] = projectTaskTimelineEntries([
      taskEvent("1", "tool.started", {
        call_id: "toolu_1",
        name: "Bash",
        input_ref: "sha256:aaaa",
        input: { command: "just dev-up", run_in_background: true },
        input_truncated: false,
        content: "Used Bash",
      }),
      taskEvent("2", "tool.completed", {
        call_id: "toolu_1",
        name: "Bash",
        status: "ok",
        output_ref: "sha256:bbbb",
        output: "Command running in background",
        output_truncated: false,
        content: "Command running in background",
      }),
    ]);

    expect(started.metadata).toMatchObject({
      providerEvent: "provided_agent.tool.started",
      phase: "tool_call",
      toolCallId: "toolu_1",
      toolName: "Bash",
      inputRef: "sha256:aaaa",
      input: { command: "just dev-up", run_in_background: true },
      inputTruncated: false,
    });
    expect(completed.metadata).toMatchObject({
      providerEvent: "provided_agent.tool.completed",
      phase: "tool_result",
      toolCallId: "toolu_1",
      toolName: "Bash",
      outputRef: "sha256:bbbb",
      output: "Command running in background",
      contentSnippet: "Command running in background",
      outputTruncated: false,
      isError: false,
    });
  });

  it("does not present a bare digest as a tool's input or output", () => {
    const [started, completed] = projectTaskTimelineEntries([
      taskEvent("1", "tool.started", {
        call_id: "toolu_1",
        name: "Bash",
        input_ref: "sha256:aaaa",
      }),
      taskEvent("2", "tool.completed", {
        call_id: "toolu_1",
        status: "error",
        output_ref: "sha256:bbbb",
      }),
    ]);

    expect(started.metadata?.input).toBeNull();
    expect(started.metadata?.inputRef).toBe("sha256:aaaa");
    expect(completed.metadata?.output).toBeNull();
    expect(completed.metadata?.contentSnippet).toBeUndefined();
    expect(completed.metadata?.toolName).toBeUndefined();
    expect(completed.metadata?.isError).toBe(true);
  });

  it("projects a native question with its execution-fenced reply identity", () => {
    const [entry] = projectTaskTimelineEntries([
      taskEvent("1", "provider_request.opened", {
        provider_request_id: "provider-request-1",
        request_kind: "question",
        tool_name: "AskUserQuestion",
        display: {
          questions: [
            {
              question: "Choose a color",
              options: [{ label: "Blue" }],
              multiSelect: false,
            },
          ],
        },
        deadline_unix_ms: 1_800_000_000_000,
      }),
    ]);

    expect(entry.metadata).toMatchObject({
      providerEvent: "provided_agent.approval.requested",
      phase: "approval",
      approvalId: "provider-request-1",
      toolName: "AskUserQuestion",
      messageId: "assistant-3",
      sessionId: "session-1",
    });
    expect(nativeProviderRequestIdentity(entry)).toEqual({
      taskId: "task-1",
      attempt: 3,
      providerRequestId: "provider-request-1",
      requestKind: "question",
    });
    expect(
      classifyAgentApprovals([entry], [], {
        id: "task-1",
        status: "running",
        latestAttempt: { attemptNumber: 3 },
      }).pending,
    ).toEqual(["provider-request-1"]);
    expect(
      classifyAgentApprovals([entry], [], {
        id: "task-1",
        status: "succeeded",
        latestAttempt: { attemptNumber: 3 },
      }).dead,
    ).toContain("provider-request-1");
  });

  it("settles the matching request after durable transport evidence", () => {
    const entries = projectTaskTimelineEntries([
      taskEvent("1", "provider_request.opened", {
        provider_request_id: "provider-request-1",
        request_kind: "permission",
        tool_name: "Bash",
        display: { input: { command: "make test" } },
        deadline_unix_ms: 1_800_000_000_000,
      }),
      taskEvent("2", "provider_request.replied", {
        provider_request_id: "provider-request-1",
        disposition: "allowed",
        evidence: "transport_write",
      }),
    ]);

    const classification = classifyAgentApprovals(entries, [
      { id: "assistant-3", role: "assistant", status: "streaming" },
    ]);
    expect(classification.pending).toEqual([]);
    expect(classification.settledOrDead).toContain("provider-request-1");
  });

  it("projects Claude's whole background set without touching the G5 task identity", () => {
    const [entry] = projectTaskTimelineEntries([
      taskEvent("1", "background_tasks.replaced", {
        tasks_ref: "sha256:cccc",
        tasks: [
          {
            task_id: "b1nohmzbo",
            kind: "shell",
            description: "python3 -c 'import time; time.sleep(600)'",
            started_at_unix_ms: 1_790_822_464_142,
          },
          {
            task_id: "a8bce50d36d7e4d1c",
            kind: "agent",
            description: "Background sleep task",
            started_at_unix_ms: 1_790_822_470_000,
          },
          { task_id: "r1", kind: "remote_agent", description: "  Review  " },
        ],
        count: 23,
        truncated: true,
        content: "Waiting on 23 background tasks",
      }),
    ]);

    expect(entry.metadata).toMatchObject({
      providerEvent: "provided_agent.background.tasks",
      phase: "background",
      kind: "background_tasks.replaced",
      taskId: "task-1",
      attempt: 3,
      messageId: "assistant-3",
      backgroundCount: 23,
      backgroundTruncated: true,
    });
    expect(backgroundTaskSetFromEntry(entry)).toEqual({
      tasks: [
        {
          subtaskId: "b1nohmzbo",
          kind: "shell",
          description: "python3 -c 'import time; time.sleep(600)'",
          startedAtUnixMs: 1_790_822_464_142,
        },
        {
          subtaskId: "a8bce50d36d7e4d1c",
          kind: "agent",
          description: "Background sleep task",
          startedAtUnixMs: 1_790_822_470_000,
        },
        {
          subtaskId: "r1",
          kind: "other",
          description: "Review",
          startedAtUnixMs: null,
        },
      ],
      count: 23,
      truncated: true,
      waitStartedAtUnixMs: null,
      // The G5 attempt that reported the set, which End names.
      taskId: "task-1",
      attempt: 3,
    });
  });

  it("projects when the background wait began", () => {
    const [entry] = projectTaskTimelineEntries([
      taskEvent("1", "background_tasks.replaced", {
        tasks_ref: "sha256:started",
        tasks: [
          {
            task_id: "bg1",
            kind: "shell",
            description: "docker-ci",
            started_at_unix_ms: 9,
          },
        ],
        count: 1,
        truncated: false,
        wait_started_at_unix_ms: 7,
        content: "Waiting on 1 background task",
      }),
    ]);
    expect(backgroundTaskSetFromEntry(entry)?.waitStartedAtUnixMs).toBe(7);
  });

  it("projects an empty background set as no remaining work", () => {
    const [entry] = projectTaskTimelineEntries([
      taskEvent("1", "background_tasks.replaced", {
        tasks_ref: "sha256:empty",
        tasks: [],
        count: 0,
        truncated: false,
        content: "No background work remains",
      }),
    ]);
    expect(backgroundTaskSetFromEntry(entry)).toEqual({
      tasks: [],
      count: 0,
      truncated: false,
      waitStartedAtUnixMs: null,
      taskId: "task-1",
      attempt: 3,
    });
  });

  it("projects background sub-task steps under subtaskId, keeping the G5 task id", () => {
    const [started, progress, completed] = projectTaskTimelineEntries([
      taskEvent("1", "subtask.started", {
        task_id: "b1nohmzbo",
        kind: "shell",
        description: "just docker-ci",
        content: "Started background command: just docker-ci",
      }),
      taskEvent("2", "subtask.progress", {
        task_id: "a8bce50d36d7e4d1c",
        progress_ref: "sha256:dddd",
        description: "Reading 12 files",
        content: "Reading 12 files",
      }),
      taskEvent("3", "subtask.completed", {
        task_id: "b1nohmzbo",
        status: "stopped",
        summary: "Background command stopped",
        result_ref: "sha256:eeee",
        content: "Background task stopped: Background command stopped",
      }),
    ]);

    expect(started.metadata).toMatchObject({
      providerEvent: "provided_agent.subtask.started",
      phase: "background",
      kind: "subtask.started",
      taskId: "task-1",
      subtaskId: "b1nohmzbo",
      subtaskKind: "shell",
      subtaskDescription: "just docker-ci",
    });
    expect(progress.metadata).toMatchObject({
      providerEvent: "provided_agent.subtask.progress",
      phase: "background",
      taskId: "task-1",
      subtaskId: "a8bce50d36d7e4d1c",
      subtaskDescription: "Reading 12 files",
    });
    expect(completed.metadata).toMatchObject({
      providerEvent: "provided_agent.subtask.completed",
      phase: "background",
      kind: "subtask.completed",
      taskId: "task-1",
      subtaskId: "b1nohmzbo",
      subtaskStatus: "stopped",
      subtaskSummary: "Background command stopped",
    });
  });

  it("projects info notices as neutral facts and keeps warnings and errors", () => {
    const [info, warning, unlevelled, error] = projectTaskTimelineEntries([
      taskEvent("1", "notice", {
        code: "background_wait_ended",
        level: "info",
        message:
          "The owner ended the wait. Claude's background work was stopped.",
        retryable: false,
      }),
      taskEvent("2", "notice", {
        code: "provider_warning",
        level: "warning",
        message: "Recoverable problem",
      }),
      taskEvent("3", "notice", { code: "legacy", message: "No level" }),
      taskEvent("4", "notice", {
        code: "provider_error",
        level: "error",
        message: "Failed",
      }),
    ]);

    expect(info.metadata).toMatchObject({
      providerEvent: "provided_agent.info",
      phase: "status",
      level: "info",
      code: "background_wait_ended",
      message:
        "The owner ended the wait. Claude's background work was stopped.",
      retryable: false,
    });
    expect(warning.metadata?.providerEvent).toBe("provided_agent.warning");
    expect(unlevelled.metadata?.providerEvent).toBe("provided_agent.warning");
    expect(error.metadata?.providerEvent).toBe("provided_agent.error");
  });

  it("finds the newest background report in journal order", () => {
    const [nonEmpty, other, empty] = projectTaskTimelineEntries([
      taskEvent("1", "background_tasks.replaced", {
        tasks: [{ task_id: "b1", kind: "shell", description: "sleep 60" }],
        count: 1,
        truncated: false,
      }),
      taskEvent("2", "usage.updated", { usage: { input_tokens: 1 } }),
      taskEvent("3", "background_tasks.replaced", {
        tasks: [],
        count: 0,
        truncated: false,
      }),
    ]);

    expect(latestBackgroundTaskSet([empty, other, nonEmpty])?.count).toBe(0);
    expect(latestBackgroundTaskSet([nonEmpty, other])?.tasks).toHaveLength(1);
    expect(latestBackgroundTaskSet([other])).toBeNull();
  });

  it("projects rejected provider facts as errors, never approval events", () => {
    const opened = taskEvent("1", "provider_request.opened", {
      provider_request_id: "provider-request-1",
      request_kind: "permission",
      tool_name: "Bash",
      display: { tool_name: "Bash" },
      deadline_unix_ms: 1_800_000_000_000,
    });
    const rejectedOpen = {
      ...opened,
      id: "2",
      entryType: "error",
      metadata: {
        ...opened.metadata,
        projection_error:
          "provider request event rejected: provider_request_id was already used in this attempt",
      },
    };
    const replied = taskEvent("3", "provider_request.replied", {
      provider_request_id: "provider-request-1",
      disposition: "allowed",
    });
    const rejectedReply = {
      ...replied,
      entryType: "error",
      metadata: {
        ...replied.metadata,
        projection_error: "provider request event rejected: digest mismatch",
      },
    };
    const [live, phantomOpen, phantomReply] = projectTaskTimelineEntries([
      opened,
      rejectedOpen,
      rejectedReply,
    ]);

    expect(live.metadata?.providerEvent).toBe(
      "provided_agent.approval.requested",
    );
    for (const rejected of [phantomOpen, phantomReply]) {
      expect(rejected.metadata).toMatchObject({
        providerEvent: "provided_agent.error",
        phase: "status",
        level: "error",
      });
      expect(rejected.metadata?.approvalId).toBeUndefined();
    }
    expect(
      classifyAgentApprovals([live, phantomOpen, phantomReply], [], {
        id: "task-1",
        status: "running",
        latestAttempt: { attemptNumber: 3 },
      }).pending,
    ).toEqual(["provider-request-1"]);
  });
});

describe("background sets from the session's live task", () => {
  it("normalizes the raw payload the live task carries", () => {
    expect(
      backgroundTaskSetFromPayload({
        tasks_ref: "sha256:00",
        tasks: [
          {
            task_id: "bg1",
            kind: "shell",
            description: " docker-ci ",
            started_at_unix_ms: 5,
          },
          { kind: "mystery" },
        ],
        count: 3,
        truncated: false,
        wait_started_at_unix_ms: 4,
      }),
    ).toEqual({
      tasks: [
        {
          subtaskId: "bg1",
          kind: "shell",
          description: "docker-ci",
          startedAtUnixMs: 5,
        },
        {
          subtaskId: "background-1",
          kind: "other",
          description: "",
          startedAtUnixMs: null,
        },
      ],
      count: 3,
      truncated: true,
      waitStartedAtUnixMs: 4,
      // The raw payload names no attempt; the caller supplies it.
      taskId: null,
      attempt: null,
    });
    expect(
      backgroundTaskSetFromPayload(
        { tasks: [], count: 1, truncated: false },
        { taskId: "task-7", attempt: 2 },
      ),
    ).toMatchObject({ count: 1, taskId: "task-7", attempt: 2 });
    expect(backgroundTaskSetFromPayload(undefined)).toBeNull();
    expect(backgroundTaskSetFromPayload("nope")).toBeNull();
  });
});
