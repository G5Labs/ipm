import type {
  ChatAgentProvider,
  ChatMessageDTO,
  TimelineEntry,
} from "../lib/types";
import { projectTaskTimelineEntry } from "../lib/taskTimeline";

export const chatProviders = ["claude_code", "codex"] as const;
export const chatFormats = ["native", "legacy"] as const;
export const chatHistories = ["rich", "sparse"] as const;
export const chatStates = [
  "pending",
  "streaming",
  "complete",
  "stopped",
  "error",
] as const;
export type ChatFixtureOptions = {
  provider: ChatAgentProvider;
  format: (typeof chatFormats)[number];
  history: (typeof chatHistories)[number];
  state: (typeof chatStates)[number];
};
export const fixtureNarration = "I will inspect the source.";
export const fixtureAnswer =
  'The shared renderer is ready.\n\n```ts\nconst answer = 42;\n```\n\n:::artifact{type="code" title="Inline example" language="ts"}const inline = true;:::';
export const fixtureDiagnostic = "Provider connection failed.";

export function chatRenderingFixture({
  provider,
  format,
  history,
  state,
}: ChatFixtureOptions) {
  const message: ChatMessageDTO & { toolCalls?: unknown[] } = {
    id: "assistant-fixture",
    sessionId: "session-fixture",
    role: "assistant",
    content: `${fixtureNarration}\n\n${fixtureAnswer}`,
    artifacts: [
      {
        type: "code",
        title: "Inline example",
        content: "const inline = true;",
        language: "ts",
      },
      {
        type: "code",
        title: "Saved example",
        content: "const saved = true;",
        language: "ts",
      },
    ],
    citations: [
      { label: "Source", filePath: "src/main.ts", lineRange: [1, 3] },
    ],
    inputTokens: null,
    outputTokens: null,
    status: state,
    errorMessage: state === "error" ? fixtureDiagnostic : undefined,
    messageIndex: 1,
    createdAt: "2026-10-25T00:00:01Z",
  };
  const user: ChatMessageDTO = {
    ...message,
    id: "user-fixture",
    role: "user",
    content: "Unify the chat rendering.",
    artifacts: null,
    citations: null,
    status: "complete",
    errorMessage: undefined,
    messageIndex: 0,
    createdAt: "2026-10-25T00:00:00Z",
  };
  const toolCalls = [
    {
      id: "call-fixture",
      name: "Bash",
      arguments: { command: "rg renderer src" },
      result: "src/main.ts",
      status: "complete",
    },
  ];
  if (history === "sparse")
    return {
      messages: [user, { ...message, toolCalls }],
      entries: [] as TimelineEntry[],
    };
  let sequence = 0;
  const event = (
    kind: string,
    payload: Record<string, unknown>,
    legacy: Record<string, unknown>,
    text: string,
  ): TimelineEntry => {
    sequence++;
    const row: TimelineEntry = {
      id: `fixture-${sequence}`,
      entryType: "checkpoint",
      content: text,
      sequence,
      createdAt: `2026-10-25T00:00:${String(sequence + 1).padStart(2, "0")}Z`,
      metadata: {
        sessionId: message.sessionId,
        messageId: message.id,
        turnId: message.id,
        taskId: "task-fixture",
        attempt: 1,
        source: "provided_agent",
        provider,
        ...legacy,
        ...(format === "native"
          ? { schema_version: "g5.task-event.v1", kind, payload }
          : {}),
      },
    };
    return format === "native" ? projectTaskTimelineEntry(row) : row;
  };
  const entries = [
    event(
      "thought.final",
      { content: "Compare both rendering paths." },
      { phase: "thought", providerEvent: "provided_agent.thought" },
      "Compare both rendering paths.",
    ),
    event(
      "message.final",
      { block_id: "narration", content: fixtureNarration },
      {
        phase: "output",
        providerEvent: "provided_agent.output.delta",
        blockId: "narration",
        streamPersistence: "semantic",
      },
      fixtureNarration,
    ),
    event(
      "tool.started",
      { call_id: "call-fixture", name: "Bash", input: toolCalls[0].arguments },
      {
        phase: "tool_call",
        providerEvent: "provided_agent.tool.started",
        toolCallId: "call-fixture",
        toolName: "Bash",
        args: toolCalls[0].arguments,
      },
      "Used Bash",
    ),
    event(
      "tool.completed",
      {
        call_id: "call-fixture",
        name: "Bash",
        status: "ok",
        output: "src/main.ts",
      },
      {
        phase: "tool_result",
        providerEvent: "provided_agent.tool.completed",
        toolCallId: "call-fixture",
        toolName: "Bash",
        contentSnippet: "src/main.ts",
      },
      "src/main.ts",
    ),
    event(
      "message.final",
      { block_id: "answer", content: fixtureAnswer },
      {
        phase: "output",
        providerEvent: "provided_agent.output.delta",
        blockId: "answer",
        streamPersistence: "semantic",
      },
      fixtureAnswer,
    ),
  ];
  if (format === "legacy")
    entries.push(
      event(
        "message.final",
        {},
        {
          phase: "output",
          providerEvent: "provided_agent.output.final",
          blockId: "aggregate",
        },
        message.content,
      ),
    );
  if (state === "error")
    entries.push(
      event(
        "notice",
        { level: "error", message: fixtureDiagnostic },
        {
          phase: "status",
          providerEvent: "provided_agent.error",
          message: fixtureDiagnostic,
        },
        fixtureDiagnostic,
      ),
    );
  return { messages: [user, message], entries };
}
