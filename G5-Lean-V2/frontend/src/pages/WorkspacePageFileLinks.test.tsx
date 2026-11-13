// Chat replies cite workspace files as `g5://file` links
// (g5.chat.file-citations). The workspace page's document-level delegate
// opens them in a tab, in the repository the link resolves to
// (g5.documents.file-links), at a cited `#L` line in Source view
// (g5.documents.file-links.line).
import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { seedUserSettings } from "../test/userSettings";
import {
  createMockChatMessage,
  createMockChatSession,
  createMockUser,
  createMockWorkspace,
} from "../test/fixtures";
import { WorkspacePage } from "./WorkspacePage";

const DOCS_REPO = "11111111-1111-4111-8111-111111111111";
const CLIENT_REPO = "22222222-2222-4222-8222-222222222222";
const API_REPO = "33333333-3333-4333-8333-333333333333";

function branch(repoId: string, repoName: string) {
  return {
    workspaceId: "links",
    repoId,
    repoName,
    branchName: "issue-875",
    headCommit: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
    trackingMode: "tracking",
    materializationStatus: "ready",
    hasDraft: false,
    dirtyFileCount: 0,
    dirtyFiles: [],
  };
}

/** Requests for file contents, as `<repoId>:<path>`. */
let fileReads: string[] = [];

function setup(codeRepos: Array<[string, string]>, reply: string) {
  const workspace = createMockWorkspace({ id: "links", name: "links" });
  const branches = [
    branch(DOCS_REPO, "docs"),
    ...codeRepos.map(([id, name]) => branch(id, name)),
  ];
  const session = createMockChatSession({
    id: "links-session",
    workspaceId: "links",
    scopeType: "workspace",
    scopeRef: "links",
    title: "File links",
    agentProvider: "claude_code",
    messageCount: 1,
  });
  const message = createMockChatMessage({
    id: "links-reply",
    sessionId: "links-session",
    content: reply,
    messageIndex: 0,
  });
  server.use(
    http.get("/api/v1/resolve", () =>
      HttpResponse.json({
        data: { orgId: "org-g5labs", workspaceId: "links" },
      }),
    ),
    http.get("/api/v1/orgs/:orgId/workspaces", () =>
      HttpResponse.json({ data: [workspace] }),
    ),
    http.get("/api/v1/workspaces/:id", () =>
      HttpResponse.json({
        data: {
          ...workspace,
          orgId: "org-g5labs",
          isBase: false,
          createdBy: "user-alex",
          updatedAt: "2026-10-27T08:00:00Z",
          docsRepoId: DOCS_REPO,
          docsFolderPath: "docs/links",
          branches,
        },
      }),
    ),
    http.get("/api/v1/workspaces/:id/branches", () =>
      HttpResponse.json({ data: branches }),
    ),
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: [session] }),
    ),
    http.get("/api/v1/sessions/:sessionId", () =>
      HttpResponse.json({ data: session }),
    ),
    http.get("/api/v1/sessions/:sessionId/messages", () =>
      HttpResponse.json({ data: [message] }),
    ),
    http.get("/api/v1/sessions/:sessionId/timeline", () =>
      HttpResponse.json({
        data: { entries: [], cursor: null, hasMore: false },
      }),
    ),
    http.get(
      "/api/v1/workspaces/:workspaceId/files/*",
      ({ params, request }) => {
        const url = new URL(request.url);
        const prefix = `/api/v1/workspaces/${params.workspaceId}/files/`;
        const path = decodeURIComponent(url.pathname.slice(prefix.length));
        fileReads.push(`${url.searchParams.get("repoId")}:${path}`);
        const content = Array.from(
          { length: 60 },
          (_, index) => `line ${index + 1}`,
        ).join("\n");
        return HttpResponse.json({
          data: {
            path,
            content,
            size: content.length,
            encoding: "utf-8",
            draftDirty: false,
          },
        });
      },
    ),
  );
  renderApp(
    <Routes>
      <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
    </Routes>,
    {
      route: "/org-g5labs/links?session=links-session",
      user: createMockUser(),
    },
  );
}

async function clickReplyLink(name: string) {
  const link = await screen.findByRole("link", { name });
  expect(link).toHaveAttribute("data-g5-file-link", "true");
  await userEvent.click(link);
}

function fileTab(name: string) {
  return screen.queryByRole("tab", { name: new RegExp(name) });
}

beforeEach(() => {
  fileReads = [];
  // Desktop layout: the workbench with its tab strip beside Chat.
  vi.mocked(window.matchMedia).mockImplementation((query) => ({
    matches: query.includes("min-width: 1024px"),
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
  seedUserSettings({ showTabBar: true, sidebarDisplayMode: "expanded" });
});

/** The mode a Markdown file shows, read off the switch that leads away from it. */
async function expectView(view: "preview" | "code") {
  await waitFor(() =>
    expect(screen.getByTestId("file-view-switch-mode")).toHaveTextContent(
      view === "preview" ? "Switch to plain text" : "Switch to rich text",
    ),
  );
}

describe("g5://file links in a chat reply", () => {
  it("opens a Project Docs link in the docs repository, in Preview", async () => {
    setup(
      [[CLIENT_REPO, "g5-client"]],
      "See [open-questions.md](g5://file/docs/links/open-questions.md).",
    );

    await clickReplyLink("open-questions.md");

    await waitFor(() =>
      expect(fileReads).toContain(`${DOCS_REPO}:docs/links/open-questions.md`),
    );
    await expectView("preview");
  });

  it("opens a cited line of the only code repository in Source", async () => {
    setup(
      [[CLIENT_REPO, "g5-client"]],
      "The rule is in [design.md](g5://file/notes/design.md#L44).",
    );

    await clickReplyLink("design.md");

    await waitFor(() =>
      expect(fileReads).toContain(`${CLIENT_REPO}:notes/design.md`),
    );
    // Line citations open Source (g5.documents.source-editing).
    await expectView("code");
  });

  it("opens the repository a ?repo= id names when there are several code repositories", async () => {
    setup(
      [
        [CLIENT_REPO, "g5-client"],
        [API_REPO, "g5-api"],
      ],
      `See [main.rs](g5://file/src/main.rs?repo=${API_REPO}#L12-L20).`,
    );

    await clickReplyLink("main.rs");

    await waitFor(() => expect(fileReads).toContain(`${API_REPO}:src/main.rs`));
    expect(fileReads).not.toContain(`${CLIENT_REPO}:src/main.rs`);
    expect(fileTab("main.rs")).not.toBeNull();
  });

  it("refuses an unqualified code link when there are several code repositories", async () => {
    setup(
      [
        [CLIENT_REPO, "g5-client"],
        [API_REPO, "g5-api"],
      ],
      "See [main.rs](g5://file/src/main.rs#L12).",
    );

    await clickReplyLink("main.rs");

    expect(
      await screen.findByText("Cannot open file link"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/g5-api \(33333333-3333-4333-8333-333333333333\)/),
    ).toBeInTheDocument();
    expect(fileReads).toEqual([]);
    expect(fileTab("main.rs")).toBeNull();
  });

  it("opens a link with a heading fragment without a line, in Preview", async () => {
    setup(
      [[CLIENT_REPO, "g5-client"]],
      "See [plan.md](g5://file/docs/links/plan.md#goals).",
    );

    await clickReplyLink("plan.md");

    await waitFor(() =>
      expect(fileReads).toContain(`${DOCS_REPO}:docs/links/plan.md`),
    );
    await expectView("preview");
  });
});
