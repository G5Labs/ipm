// Leaving a document from its rendered view: a followed link offers the
// existing Back on the file it opened (g5.documents.ontology.links), and a
// finding's chat action seeds a fresh chat in the right-edge panel beside the
// document (g5.workbench.chat-actions).
import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { seedUserSettings } from "../test/userSettings";
import { createMockUser, createMockWorkspace } from "../test/fixtures";
import { serveExternalIssuesFeature } from "../test/externalIssueFixtures";
import { WorkspacePage } from "./WorkspacePage";

const DOCS_REPO = "11111111-1111-4111-8111-111111111111";
const CODE_REPO = "22222222-2222-4222-8222-222222222222";

const FILES: Record<string, string> = {
  ".g5/ontology/root.g5.md": [
    "# Example",
    "",
    "Pages: [Receiving](features/receiving.g5.md).",
    "",
  ].join("\n"),
  ".g5/ontology/features/receiving.g5.md": [
    "# Receiving",
    "",
    '<a id="rcv.ceiling"></a>',
    "[I · rcv.ceiling]",
    "",
    "Receipts never exceed the ordered quantity.",
    "",
  ].join("\n"),
  ".g5/TLA_verification/bugs_issues.md": [
    "# Bugs and issues",
    "",
    "## Actionable",
    "",
    '<a id="rcv-001"></a>',
    "### Duplicate receipt lines bypass the ceiling",
    "",
    "Severity: High · Decision: Open",
    "",
    "Two lines each pass alone.",
    "",
  ].join("\n"),
};

function branch(repoId: string, repoName: string) {
  return {
    workspaceId: "nav",
    repoId,
    repoName,
    branchName: "nav",
    headCommit: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
    trackingMode: "tracking",
    materializationStatus: "ready",
    hasDraft: false,
    dirtyFileCount: 0,
    dirtyFiles: [],
  };
}

/** Files read through the workspace read facade, in order. */
let fileReads: string[] = [];

function setup(path: string) {
  const workspace = createMockWorkspace({ id: "nav", name: "nav" });
  const branches = [branch(DOCS_REPO, "docs"), branch(CODE_REPO, "code")];
  server.use(
    http.get("/api/v1/resolve", () =>
      HttpResponse.json({ data: { orgId: "org-g5labs", workspaceId: "nav" } }),
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
          ownerUserId: "user-alex",
          updatedAt: "2026-11-02T08:00:00Z",
          docsRepoId: DOCS_REPO,
          docsFolderPath: "docs/nav",
          branches,
        },
      }),
    ),
    http.get("/api/v1/workspaces/:id/branches", () =>
      HttpResponse.json({ data: branches }),
    ),
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.get(
      "/api/v1/workspaces/:workspaceId/files/*",
      ({ params, request }) => {
        const url = new URL(request.url);
        const prefix = `/api/v1/workspaces/${params.workspaceId}/files/`;
        const file = decodeURIComponent(url.pathname.slice(prefix.length));
        fileReads.push(file);
        const content = FILES[file];
        if (content === undefined)
          return HttpResponse.json(
            { error: { code: "NOT_FOUND", message: "Not found" } },
            { status: 404 },
          );
        return HttpResponse.json({
          data: {
            path: file,
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
      route: `/org-g5labs/nav/file/${path}?branch=${CODE_REPO}`,
      user: createMockUser(),
    },
  );
}

function selectedTab() {
  return screen
    .getAllByRole("tab")
    .find((tab) => tab.getAttribute("aria-selected") === "true");
}

/** The Back control a person can see (the editor's copy stays hidden). */
function visibleBack() {
  return (
    screen
      .queryAllByTestId("navigate-back")
      .find((button) => button.closest(".hidden, [hidden]") === null) ?? null
  );
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

describe("a link followed from a rendered document", () => {
  it("offers Back on the file it opened, which returns to the document", async () => {
    setup(".g5/ontology/root.g5.md");
    const preview = await screen.findByTestId("file-view-preview");
    await userEvent.click(
      await within(preview).findByRole("link", { name: "Receiving" }),
    );
    await waitFor(() => expect(selectedTab()).toHaveTextContent(/receiving/i));
    await waitFor(() => expect(visibleBack()).not.toBeNull());
    // The read that found the target is the one the opened file shows.
    await screen.findByText("Receipts never exceed the ordered quantity.");
    expect(
      fileReads.filter((f) => f === ".g5/ontology/features/receiving.g5.md"),
    ).toHaveLength(1);
    expect(visibleBack()).toHaveAttribute("title", "Back to root.g5.md");

    await userEvent.click(visibleBack()!);
    await waitFor(() => expect(selectedTab()).toHaveTextContent(/root/i));
    // One step: the origin is used up.
    expect(visibleBack()).toBeNull();
  });
});

describe("a finding's chat action", () => {
  beforeEach(() => serveExternalIssuesFeature());

  it("seeds a fresh chat in the right-edge panel and keeps the findings in view", async () => {
    setup(".g5/TLA_verification/bugs_issues.md");
    const chat = await screen.findByTestId("finding-chat");
    await waitFor(() => expect(selectedTab()).toHaveTextContent("bugs_issues"));
    await userEvent.click(chat);

    expect(await screen.findByTestId("floating-panel-dock")).toBeVisible();
    await waitFor(() =>
      expect(screen.getByTestId("chat-input")).toHaveValue(
        'Let\'s discuss the bug "Duplicate receipt lines bypass the ceiling" (rcv-001, severity High, decision Open) in .g5/TLA_verification/bugs_issues.md.',
      ),
    );
    expect(selectedTab()).toHaveTextContent("bugs_issues");
    expect(screen.getByTestId("findings-document")).toBeVisible();
  });
});
