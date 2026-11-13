import React from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { ChatPanel } from "../../src/components/chat/ChatPanel";
import { ExternalIssueDetailPanel } from "../../src/components/external-issues/ExternalIssueDetailPanel";
import { WorkspaceActivityPanel } from "../../src/components/external-issues/WorkspaceActivityPanel";
import { MarkdownPreview } from "../../src/components/files/MarkdownPreview";
import { MarkdownRenderer } from "../../src/components/shared/MarkdownRenderer";
import { JsonlTranscriptView } from "../../src/components/shared/JsonlTranscriptView";
import { OntologyStatementMarkdown } from "../../src/components/editor/OntologyStatementMarkdown";
import { AuthProvider, RequireAuth } from "../../src/hooks/useAuth";
import { ApprovalReportPage } from "../../src/pages/ApprovalReportPage";
import { IssuePage } from "../../src/pages/IssuePage";
import {
  createMockChatMessage,
  createMockChatSession,
} from "../../src/test/fixtures";
import "../../src/index.css";

const content = [
  "Ordinary words ".repeat(10).trim(),
  "",
  "Soft one\nsoft two.",
  "",
  "Hard one  \nhard two.",
  "",
  "Slash one\\\nslash two.",
  "",
  "```text",
  "line one",
  "  line two",
  "```",
].join("\n");

const surfaces = {
  shared: <MarkdownPreview content={content} />,
  chat: <MarkdownRenderer content={content} />,
  restricted: <MarkdownRenderer content={content} trustedContent={false} />,
  statement: <OntologyStatementMarkdown content={content} />,
  historical: <OntologyStatementMarkdown content={content} historical />,
};

const query = new URLSearchParams(location.search);
const pane = query.get("pane");
const role =
  query.get("role") === "user"
    ? "user"
    : query.get("role") === "system"
      ? "system"
      : "assistant";
const reading = ["issue", "issue-page", "activity", "report"].includes(
  pane ?? "",
);
const session = createMockChatSession();
const sidebar = Number(query.get("sidebar") ?? 0);
const client = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

createRoot(document.getElementById("root")!).render(
  pane ? (
    <QueryClientProvider client={client}>
      <main style={{ display: "flex", height: "100vh", padding: 16, gap: 16 }}>
        {sidebar > 0 && (
          <aside style={{ width: sidebar, flexShrink: 0 }}>Sidebar</aside>
        )}
        <section data-testid="content-pane" style={{ flex: 1, minWidth: 0 }}>
          {reading ? (
            <MemoryRouter
              initialEntries={[
                pane === "report"
                  ? "/g5labs/review-reports/report/ws/review"
                  : pane === "issue-page"
                    ? "/g5labs/issues/github/11/12"
                    : "/g5labs/workspace",
              ]}
            >
              <AuthProvider>
                <RequireAuth>
                  <Routes>
                    <Route
                      path="/:org/review-reports/report/:workspaceId/:reviewId"
                      element={<ApprovalReportPage />}
                    />
                    <Route
                      path="/:org/issues/github/:repositoryId/:number"
                      element={<IssuePage />}
                    />
                    <Route
                      path="/:org/:workspace"
                      element={
                        pane === "activity" ? (
                          <WorkspaceActivityPanel
                            workspaceId="ws"
                            onOpenCommit={() => {}}
                            header={<MarkdownRenderer content={content} />}
                          />
                        ) : (
                          <ExternalIssueDetailPanel
                            repositoryId="11"
                            number={12}
                            name="acme/api#12"
                          />
                        )
                      }
                    />
                  </Routes>
                </RequireAuth>
              </AuthProvider>
            </MemoryRouter>
          ) : pane === "transcript" ? (
            <JsonlTranscriptView
              displayPath="chat.jsonl"
              content={JSON.stringify({
                type: "assistant",
                sessionId: session.id,
                message: { role: "assistant", content },
              })}
            />
          ) : (
            <ChatPanel
              layout={pane === "panel" ? "panel" : "center"}
              session={session}
              messages={[
                createMockChatMessage({
                  role,
                  content:
                    role === "assistant"
                      ? content
                      : "Available content edge. ".repeat(64).trim(),
                }),
              ]}
              isStreaming={false}
              scopeType="workspace"
              scopeRef="workspace-fixture"
              scopeLabel="Fixture"
              sessions={[session]}
              onSendMessage={() => {}}
              onStopStreaming={() => {}}
              onCreateSession={() => {}}
              onSummarize={() => {}}
              onSelectSession={() => {}}
            />
          )}
        </section>
      </main>
    </QueryClientProvider>
  ) : (
    <main style={{ padding: 16 }}>
      {Object.entries(surfaces).map(([name, surface]) => (
        <section key={name} data-testid={name}>
          {surface}
        </section>
      ))}
    </main>
  ),
);
