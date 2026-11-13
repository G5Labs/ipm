// Actual ChatPanel with deterministic provider transport/persistence fixtures.
// Live provider execution and server persistence are not supplied by this page.
import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ChatPanel } from "../../src/components/chat/ChatPanel";
import { createMockChatSession } from "../../src/test/fixtures";
import {
  chatRenderingFixture,
  fixtureNarration,
} from "../../src/test/chatRenderingFixtures";
import type { ChatFixtureOptions } from "../../src/test/chatRenderingFixtures";
import type { ChatMessageDTO, TimelineEntry } from "../../src/lib/types";
import "../../src/index.css";

const query = new URLSearchParams(location.search);
const provider = query.get("provider") === "codex" ? "codex" : "claude_code";
const format = query.get("format") === "legacy" ? "legacy" : "native";
const layout = query.get("layout") === "panel" ? "panel" : "center";
document.documentElement.dataset.theme = "alpine-graphite";
document.documentElement.dataset.mode = query.get("mode") ?? "light";
const storageKey = `chat-browser-fixture:${location.search}`;
type State = {
  state: ChatFixtureOptions["state"] | "empty";
  text: string;
  chunks: boolean;
  sparse: boolean;
  older: boolean;
  question: boolean;
  answered: boolean;
};
const initial: State = {
  state: "empty",
  text: "",
  chunks: false,
  sparse: false,
  older: false,
  question: false,
  answered: false,
};
const client = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

function App() {
  const [view, setView] = useState<State>(
    () => JSON.parse(sessionStorage.getItem(storageKey) ?? "null") ?? initial,
  );
  useEffect(() => {
    sessionStorage.setItem(storageKey, JSON.stringify(view));
  }, [view]);
  const live = view.state === "streaming";
  const fixture = chatRenderingFixture({
    provider,
    format,
    history: view.sparse ? "sparse" : "rich",
    state: view.state === "empty" ? "pending" : view.state,
  });
  let messages: ChatMessageDTO[] =
    view.state === "empty"
      ? []
      : fixture.messages.map((message) =>
          message.role === "user"
            ? { ...message, content: view.text }
            : message,
        );
  let entries = view.state === "empty" ? [] : fixture.entries;
  if (live) {
    messages = messages.map((message) =>
      message.role === "assistant"
        ? { ...message, content: fixtureNarration, artifacts: null }
        : message,
    );
    // Before the next chunk only the saved prefix is held; the chunk then
    // delivers the rest as live rows ahead of saved history
    // (chat g5.chat.live-delivery), which Complete/Fail/Stop later save.
    entries = view.chunks
      ? entries.map((entry, index) =>
          index < 4
            ? entry
            : {
                ...entry,
                metadata: {
                  ...entry.metadata,
                  live: true,
                  seq: entry.sequence ?? index + 1,
                },
              },
        )
      : entries.slice(0, 4);
  }
  if (view.older)
    messages = [
      {
        ...fixture.messages[0],
        id: "older-user",
        content: "Earlier prompt",
        messageIndex: -2,
        createdAt: "2026-10-24T00:00:00Z",
      },
      {
        ...fixture.messages[1],
        id: "older-assistant",
        content: "Earlier saved reply",
        artifacts: null,
        citations: null,
        status: "stopped",
        messageIndex: -1,
        createdAt: "2026-10-24T00:00:01Z",
      },
      ...messages,
    ];
  if (view.question) {
    const question: TimelineEntry = {
      id: "question-fixture",
      entryType: "approval",
      content: "Choose a color",
      sequence: 100,
      createdAt: "2026-10-25T00:01:00Z",
      metadata: {
        provider: "provided_agent",
        providerEvent: "provided_agent.approval.requested",
        phase: "approval",
        messageId: "assistant-fixture",
        sessionId: "session-fixture",
        approvalId: "question-fixture",
        toolName: "AskUserQuestion",
        taskId: "task-fixture",
        attempt: 1,
        providerRequestId: "question-fixture",
        requestKind: "question",
        nativeResumeSessionId: "native-fixture",
        nativeRequestStatus: view.answered ? "responded" : "open",
        input: {
          questions: [
            {
              question: "Which color?",
              options: [{ label: "Blue" }, { label: "Green" }],
            },
          ],
        },
      },
    };
    entries = [...entries, question];
  }
  const session = createMockChatSession({
    id: "session-fixture",
    agentProvider: provider,
    model: null,
  });
  return (
    <main
      style={{
        height: "100vh",
        background: "var(--g5-bg0)",
        color: "var(--g5-text0)",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <nav
        aria-label="Fixture transport"
        style={{ display: "flex", gap: 12, padding: 8, flexWrap: "wrap" }}
      >
        <button
          onClick={() => setView((current) => ({ ...current, chunks: true }))}
        >
          Next stream chunk
        </button>
        <button
          onClick={() =>
            setView((current) => ({
              ...current,
              state: "complete",
              chunks: true,
            }))
          }
        >
          Complete reply
        </button>
        <button
          onClick={() =>
            setView((current) => ({ ...current, state: "error", chunks: true }))
          }
        >
          Fail reply
        </button>
        <button
          onClick={() => setView((current) => ({ ...current, sparse: true }))}
        >
          Saved messages only
        </button>
        {provider === "claude_code" && (
          <button
            onClick={() =>
              setView((current) => ({
                ...current,
                question: true,
                state: "streaming",
                chunks: true,
              }))
            }
          >
            Ask a native question
          </button>
        )}
      </nav>
      <div
        style={{
          flex: 1,
          minHeight: 0,
          width: layout === "panel" ? 420 : 1050,
          maxWidth: "100%",
          margin: "0 auto",
        }}
      >
        <ChatPanel
          layout={layout}
          session={session}
          messages={messages}
          timelineEntries={entries}
          isStreaming={live}
          scopeType="workspace"
          scopeRef="workspace-fixture"
          scopeLabel="Fixture"
          sessions={[session]}
          selectedAgentProvider={provider}
          availableAgentProviders={[provider]}
          setupError={null}
          composerDisabledReason={null}
          hasEarlierHistory={!view.older && view.state !== "empty"}
          onLoadEarlierHistory={() =>
            setView((current) => ({ ...current, older: true, sparse: false }))
          }
          onSendMessage={(text) => {
            setView({ ...initial, text, state: "streaming" });
          }}
          onStopStreaming={() =>
            setView((current) => ({
              ...current,
              state: "stopped",
              chunks: true,
            }))
          }
          onResolveAgentApproval={async () => {
            setView((current) => ({ ...current, answered: true }));
            return { delivered: true };
          }}
          onCreateSession={() => {}}
          onSummarize={() => {}}
          onSelectSession={() => {}}
          onOpenFile={(path) => {
            document.title = path;
          }}
        />
      </div>
    </main>
  );
}
createRoot(document.getElementById("root")!).render(
  <QueryClientProvider client={client}>
    <App />
  </QueryClientProvider>,
);
