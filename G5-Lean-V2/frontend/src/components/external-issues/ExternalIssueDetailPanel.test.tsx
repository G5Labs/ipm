/**
 * ExternalIssueDetailPanel — read-only external GitHub issue tab.
 *
 * Covered: issue fields, oldest-first comments and the truncation note,
 * linked workspaces (visible and inaccessible), typed read errors with
 * retry, body/comment rendering that follows the domain's `contentTrusted`
 * flag, and the tab staying read-only with a link to the issue's G5 page.
 */

import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ExternalIssueDetailDTO } from "../../lib/types";
import {
  issueErrorResponse,
  makeIssueDetail,
  serveExternalIssuesFeature,
  serveInstanceInfoFailingOnce,
  serveOrgExternalIssues,
} from "../../test/externalIssueFixtures";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import {
  ExternalIssueDetailPanel,
  type ExternalIssueDetailPanelProps,
} from "./ExternalIssueDetailPanel";

vi.mock("../shared/MermaidDiagram", () => ({
  MermaidDiagram: ({ code }: { code: string }) => (
    <div data-testid="mermaid-diagram">{code}</div>
  ),
}));

const DETAIL_URL = "/api/v1/orgs/org-g5labs/issues/github/11/12";

const RICH_BODY = [
  "Intro with [a spec](g5://ontology/core/search) link.",
  "",
  "```mermaid",
  "graph TD; A-->B",
  "```",
  "",
  "![diagram](https://example.com/diagram.png)",
].join("\n");

function serveDetail(detail: ExternalIssueDetailDTO) {
  server.use(http.get(DETAIL_URL, () => HttpResponse.json({ data: detail })));
}

function renderPanel(
  route = "/g5labs/current",
  controls: Pick<
    ExternalIssueDetailPanelProps,
    "actions" | "onRequestApproval"
  > = {},
) {
  return renderApp(
    <Routes>
      <Route
        path="/:org/:workspace"
        element={
          <ExternalIssueDetailPanel
            {...controls}
            repositoryId="11"
            number={12}
            name="acme/api#12"
          />
        }
      />
    </Routes>,
    { route, user: createMockUser() },
  );
}

describe("ExternalIssueDetailPanel", () => {
  beforeEach(() => {
    serveExternalIssuesFeature();
  });

  it("labels the assignees, also when nobody is assigned", async () => {
    serveDetail(makeIssueDetail({ assignees: [] }));
    renderPanel();

    const detail = await screen.findByTestId("external-issue-detail");
    expect(
      within(detail).getByTestId("external-issue-assignees"),
    ).toHaveTextContent("Assignees: None");
  });

  it("renders the issue fields, comments oldest first, and linked workspaces", async () => {
    serveDetail(
      makeIssueDetail({
        contentTrusted: true,
        links: [
          { workspaceId: "ws-1", workspaceName: "Search Fix", role: "primary" },
          // The API omits the id of a workspace the viewer cannot access.
          { workspaceName: null, role: "related" },
        ],
      }),
    );
    renderPanel();

    const detail = await screen.findByTestId("external-issue-detail");
    expect(
      screen.getByRole("heading", {
        level: 1,
        name: "Search returns stale results",
      }),
    ).toBeVisible();
    expect(
      within(detail).getByTestId("external-issue-display-key"),
    ).toHaveTextContent("acme/api#12");
    expect(
      within(detail).getByTestId("external-issue-state"),
    ).toHaveTextContent("Open");
    expect(
      within(detail).getByTestId("external-issue-assignees"),
    ).toHaveTextContent("Assignees: @octocat");
    expect(
      within(detail).getByTestId("external-issue-labels"),
    ).toHaveTextContent("bug");
    expect(detail).toHaveTextContent("@reporter opened");
    expect(
      within(detail).getByTestId("external-issue-open-github"),
    ).toHaveAttribute("href", "https://github.com/acme/api/issues/12");

    const body = screen.getByTestId("external-issue-body");
    expect(within(body).getByText("stale").tagName).toBe("STRONG");

    const comments = screen.getByTestId("external-issue-comments");
    expect(comments).toHaveTextContent("Comments (2)");
    const items = within(comments).getAllByRole("listitem");
    expect(items.map((item) => item.dataset.testid)).toEqual([
      "external-issue-comment-c1",
      "external-issue-comment-c2",
    ]);
    expect(items[0]).toHaveTextContent("@first");
    expect(items[0]).toHaveTextContent("First comment");
    expect(
      screen.queryByTestId("external-issue-comments-truncated"),
    ).not.toBeInTheDocument();

    const links = screen.getByTestId("external-issue-links");
    expect(
      within(links).getByRole("link", { name: "Search Fix" }),
    ).toHaveAttribute("href", "/g5labs/Search%20Fix?view=issues");
    expect(
      within(links).getByTestId("external-issue-link-hidden-1"),
    ).toHaveTextContent("A workspace you cannot access");
    expect(
      screen.queryByTestId("external-issue-restricted-notice"),
    ).not.toBeInTheDocument();
  });

  it("stays read-only in a workspace tab and links to the issue's G5 page", async () => {
    let targetReads = 0;
    server.use(
      http.get(`${DETAIL_URL}/write-target`, () => {
        targetReads += 1;
        return HttpResponse.json({ data: null });
      }),
    );
    serveDetail(makeIssueDetail());
    renderPanel();

    const page = await screen.findByTestId("external-issue-open-page");
    expect(page).toHaveAttribute("href", "/g5labs/issues/github/11/12");
    expect(page).toHaveTextContent("Open issue page");
    expect(screen.getByTestId("external-issue-open-github")).toBeVisible();
    expect(screen.queryByTestId("issue-page-edit-button")).toBeNull();
    expect(screen.queryByTestId("issue-page-state-button")).toBeNull();
    expect(screen.queryByTestId("activity-comment-composer")).toBeNull();
    expect(screen.queryByTestId(/^issue-page-comment-edit-/)).toBeNull();
    expect(targetReads).toBe(0);
  });

  it("notes when only the first comments were loaded", async () => {
    serveDetail(
      makeIssueDetail({ commentCount: 140, commentsTruncated: true }),
    );
    renderPanel();

    expect(
      await screen.findByTestId("external-issue-comments-truncated"),
    ).toHaveTextContent("Showing the first 2 of 140 comments.");
  });

  it("shows an explicit empty description", async () => {
    serveDetail(makeIssueDetail({ body: null }));
    renderPanel();

    expect(await screen.findByTestId("external-issue-body")).toHaveTextContent(
      "No description provided.",
    );
  });

  it("renders untrusted issue content as restricted Markdown", async () => {
    serveDetail(
      makeIssueDetail({
        contentTrusted: false,
        body: RICH_BODY,
        comments: [
          {
            id: "c1",
            author: { login: "first", avatarUrl: null },
            body: "```g5-js\nalert(1)\n```",
            createdAt: "2026-09-21T12:00:00Z",
            updatedAt: "2026-09-21T12:00:00Z",
            url: "https://github.com/acme/api/issues/12#issuecomment-1",
          },
        ],
        commentCount: 1,
      }),
    );
    renderPanel();

    expect(
      await screen.findByTestId("external-issue-restricted-notice"),
    ).toBeVisible();
    const body = screen.getByTestId("external-issue-body");
    expect(within(body).getByTestId("markdown-renderer")).toHaveAttribute(
      "data-content-trust",
      "restricted",
    );
    expect(within(body).queryByTestId("mermaid-diagram")).toBeNull();
    expect(body).toHaveTextContent("graph TD; A-->B");
    expect(within(body).queryByRole("link", { name: "a spec" })).toBeNull();
    expect(body).toHaveTextContent("g5://ontology/core/search");
    expect(body.querySelector("img")).toBeNull();
    expect(
      within(body).getByRole("link", { name: "Image: diagram" }),
    ).toHaveAttribute("href", "https://example.com/diagram.png");

    const comment = screen.getByTestId("external-issue-comment-c1");
    expect(comment.querySelector("iframe")).toBeNull();
    expect(comment).toHaveTextContent("alert(1)");
  });

  it("renders trusted issue content with the full Markdown renderer", async () => {
    serveDetail(makeIssueDetail({ contentTrusted: true, body: RICH_BODY }));
    renderPanel();

    const body = await screen.findByTestId("external-issue-body");
    expect(within(body).getByTestId("markdown-renderer")).toHaveAttribute(
      "data-content-trust",
      "trusted",
    );
    expect(within(body).getByTestId("mermaid-diagram")).toBeInTheDocument();
    expect(within(body).queryByRole("link", { name: "a spec" })).toBeNull();
    expect(body).toHaveTextContent("a spec");
    expect(body.querySelector("img")).toHaveAttribute(
      "src",
      "https://example.com/diagram.png",
    );
  });

  it("copies an issue's code blocks, trusted or restricted (g5.documents.markdown.code-copy)", async () => {
    const writeText = vi.fn(async (_text: string) => {});
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    try {
      serveDetail(
        makeIssueDetail({
          contentTrusted: false,
          body: "```sh\ncargo test -p g5-api\n```",
          comments: [
            {
              id: "c1",
              author: { login: "first", avatarUrl: null },
              body: "```mermaid\ngraph TD; A-->B\n```",
              createdAt: "2026-09-21T12:00:00Z",
              updatedAt: "2026-09-21T12:00:00Z",
              url: "https://github.com/acme/api/issues/12#issuecomment-1",
            },
          ],
          commentCount: 1,
        }),
      );
      renderPanel();

      const body = await screen.findByTestId("external-issue-body");
      fireEvent.click(within(body).getByRole("button", { name: "Copy code" }));
      expect(await within(body).findByText("Copied")).toBeInTheDocument();
      expect(writeText).toHaveBeenLastCalledWith("cargo test -p g5-api");

      // A restricted diagram fence is inert code, so it is copyable too.
      const comment = screen.getByTestId("external-issue-comment-c1");
      fireEvent.click(
        within(comment).getByRole("button", { name: "Copy code" }),
      );
      expect(await within(comment).findByText("Copied")).toBeInTheDocument();
      expect(writeText).toHaveBeenLastCalledWith("graph TD; A-->B");
    } finally {
      Reflect.deleteProperty(navigator, "clipboard");
    }
  });

  it("explains a typed read failure and retries", async () => {
    let calls = 0;
    server.use(
      http.get(DETAIL_URL, () => {
        calls += 1;
        return calls === 1
          ? issueErrorResponse(403, "access_denied", "GitHub returned 403")
          : HttpResponse.json({ data: makeIssueDetail() });
      }),
    );
    renderPanel();
    const user = userEvent.setup();

    const error = await screen.findByTestId("external-issue-error");
    expect(error).toHaveTextContent(
      "GitHub denied the G5 GitHub App access to this repository.",
    );
    await user.click(screen.getByTestId("external-issue-error-retry"));
    expect(
      await screen.findByTestId("external-issue-detail"),
    ).toBeInTheDocument();
  });

  it("reports a failed availability check with retry, not as unavailable", async () => {
    serveInstanceInfoFailingOnce();
    serveDetail(makeIssueDetail());
    renderPanel();
    const user = userEvent.setup();

    const error = await screen.findByTestId("external-issue-feature-error");
    expect(error).toHaveTextContent(
      "Could not check whether issues are available.",
    );
    expect(
      screen.queryByTestId("external-issue-unavailable"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("external-issue-feature-error-retry"));
    expect(
      await screen.findByTestId("external-issue-detail"),
    ).toBeInTheDocument();
  });

  it("tells a non-member they are not in the organization", async () => {
    renderPanel("/other-org/current");

    expect(
      await screen.findByTestId("external-issue-no-org"),
    ).toHaveTextContent("You are not a member of this organization.");
    expect(
      screen.queryByTestId("external-issue-unavailable"),
    ).not.toBeInTheDocument();
  });

  it("says the organization has external issues off", async () => {
    serveOrgExternalIssues(false);
    renderPanel();

    expect(
      await screen.findByTestId("external-issue-unavailable"),
    ).toHaveTextContent("Issues are off for this organization.");
  });
});

describe("issue page actions", () => {
  beforeEach(() => {
    serveExternalIssuesFeature();
    serveDetail(makeIssueDetail());
  });
  it("dispatches actions and approval, explains disabled items, and closes on outside click or Escape", async () => {
    const onSelect = vi.fn();
    const approval = vi.fn();
    renderPanel(undefined, {
      actions: [
        { id: "implement", label: "Implement in this workspace", onSelect },
        {
          id: "done",
          label: "Mark as done",
          onSelect,
          disabled: true,
          title: "Tracker writes unavailable",
        },
      ],
      onRequestApproval: approval,
    });
    const trigger = await screen.findByRole("button", { name: "Action ▾" });
    const user = userEvent.setup();
    await user.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    const done = screen.getByRole("menuitem", { name: "Mark as done" });
    expect(done).toBeDisabled();
    expect(done).toHaveAttribute("title", "Tracker writes unavailable");
    await user.click(done);
    expect(onSelect).not.toHaveBeenCalled();
    await user.click(
      screen.getByRole("menuitem", { name: "Implement in this workspace" }),
    );
    expect(onSelect).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).toBeNull();
    await user.click(trigger);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(trigger).toHaveFocus();
    await user.click(trigger);
    await user.click(screen.getByRole("heading", { level: 1 }));
    expect(screen.queryByRole("menu")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Request approval" }));
    expect(approval).toHaveBeenCalledOnce();
  });
  it("omits all actions when no controls are supplied", async () => {
    renderPanel();
    await screen.findByTestId("external-issue-detail");
    expect(screen.queryByRole("button", { name: "Action ▾" })).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Request approval" }),
    ).toBeNull();
  });
});
