// Actual ChatPanel header and chat picker (chat g5.chat.ui.header-title,
// g5.chat.ui.picker, g5.chat.activity) at a chosen width and layout, with
// fixture sessions. No API stack: rename resolves or fails locally.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ChatPanel } from "../../src/components/chat/ChatPanel";
import { createMockChatSession } from "../../src/test/fixtures";
import type { ChatSessionDTO } from "../../src/lib/types";
import "../../src/index.css";

const query = new URLSearchParams(location.search);
const layout = query.get("layout") === "panel" ? "panel" : "center";
const width = Number(query.get("width") ?? "900");
const title = query.get("title") ?? "Short title";
const failRename = query.get("fail") === "1";
document.documentElement.dataset.theme = "alpine-graphite";
document.documentElement.dataset.mode = query.get("mode") ?? "light";

const now = Date.now();
const ago = (minutes: number) => new Date(now - minutes * 60_000).toISOString();
const base = (id: string, name: string, minutes: number): ChatSessionDTO =>
  createMockChatSession({
    id,
    workspaceId: "workspace-1",
    title: name,
    createdAt: ago(minutes),
    updatedAt: ago(1),
  });
const initialSessions: ChatSessionDTO[] = [
  {
    ...base("running", "Running chat", 180),
    activity: {
      activeTurnStartedAt: ago(1),
      lastFinishedAt: ago(40),
      latestResult: { taskId: "t1", attempt: 1, hasOutput: true },
      unseenResult: false,
    },
  },
  {
    ...base("current", title, 120),
    activity: {
      activeTurnStartedAt: null,
      lastFinishedAt: ago(5),
      latestResult: { taskId: "t2", attempt: 1, hasOutput: true },
      unseenResult: false,
    },
  },
  {
    ...base("unseen", "Finished while away", 300),
    activity: {
      activeTurnStartedAt: null,
      lastFinishedAt: ago(12),
      latestResult: { taskId: "t3", attempt: 1, hasOutput: true },
      unseenResult: true,
    },
  },
  {
    ...base("idle", "Never ran", 60 * 26),
    activity: {
      activeTurnStartedAt: null,
      lastFinishedAt: null,
      latestResult: null,
      unseenResult: false,
    },
  },
];

const client = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

function App() {
  const [sessions, setSessions] = useState(initialSessions);
  const [activeId, setActiveId] = useState("current");
  const session = sessions.find((item) => item.id === activeId) ?? null;
  return (
    <div style={{ width, height: 640 }} data-testid="frame">
      <ChatPanel
        layout={layout}
        session={session}
        messages={[]}
        isStreaming={false}
        timelineEntries={[]}
        scopeType="workspace"
        scopeRef="workspace-1"
        scopeLabel="Workspace"
        sessions={sessions}
        selectedAgentProvider="claude_code"
        availableAgentProviders={["claude_code"]}
        onSelectAgentProvider={() => {}}
        setupError={null}
        composerDisabledReason={null}
        onSendMessage={() => true}
        onStopStreaming={() => {}}
        onForkSession={() => {}}
        onSummarize={() => {}}
        onCreateSession={() => {}}
        onSelectSession={setActiveId}
        onDetach={layout === "center" ? () => {} : undefined}
        onClose={layout === "panel" ? () => {} : undefined}
        onRenameSession={async (id, next) => {
          await new Promise((resolve) => setTimeout(resolve, 150));
          if (failRename) throw new Error("Workspace is read-only");
          setSessions((items) =>
            items.map((item) =>
              item.id === id ? { ...item, title: next } : item,
            ),
          );
        }}
      />
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <QueryClientProvider client={client}>
    <App />
  </QueryClientProvider>,
);
