/**
 * WorkspaceIssuesPanel — workspace external issue links (`?view=issues`).
 *
 * Covered: live summaries and per-link errors, opening the detail view,
 * finding issues with the organization search, linking a result (default
 * role, typed issue errors, and every 409 conflict code),
 * unlink with confirmation, write-scope gating, the feature gate, and
 * unlinking while the organization has external issues off.
 */

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes, useLocation, useParams } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  CreateWorkspaceIssueLinkInput,
  ExternalIssueSummaryDTO,
  WorkspaceIssueLinkDTO,
} from "../../lib/types";
import {
  issueErrorResponse,
  makeIssueLink,
  makeIssueSummary,
  makeNativeIssueSummary,
  serveExternalIssuesFeature,
  serveInstanceInfoFailingOnce,
  serveOrgExternalIssues,
} from "../../test/externalIssueFixtures";
import {
  createMockUser,
  MOCK_VIEWER_SCOPES,
  MOCK_WORKSPACE_ACTIVITY,
} from "../../test/fixtures";
import { ISSUE_COMMENT_DRAFT_PREFIX } from "../../lib/issueCommentDrafts";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { WorkspaceIssuesPanel } from "./WorkspaceIssuesPanel";

const WORKSPACE_ID = "ws-1";
const LINKS_URL = `/api/v1/workspaces/${WORKSPACE_ID}/issue-links`;

function WorkspaceEcho() {
  const { workspace } = useParams();
  const location = useLocation();
  return (
    <div data-testid="other-workspace-route">
      {workspace}
      {location.search}
    </div>
  );
}

function renderPanel(
  options: {
    scopes?: string[];
    onOpenIssue?: () => void;
    viewerOwnsWorkspace?: boolean;
    writeBlockedReason?: string;
  } = {},
) {
  const onOpenIssue = options.onOpenIssue ?? vi.fn();
  const result = renderApp(
    <Routes>
      <Route
        path="/:org/current"
        element={
          <WorkspaceIssuesPanel
            workspaceId={WORKSPACE_ID}
            viewerOwnsWorkspace={options.viewerOwnsWorkspace ?? true}
            writeBlockedReason={options.writeBlockedReason ?? null}
            onOpenIssue={onOpenIssue}
          />
        }
      />
      <Route path="/:org/:workspace" element={<WorkspaceEcho />} />
    </Routes>,
    {
      route: "/g5labs/current",
      user: createMockUser(options.scopes ? { scopes: options.scopes } : {}),
    },
  );
  return { ...result, onOpenIssue };
}

/** Serve a mutable link list; returns a setter for later responses. */
function serveLinks(initial: WorkspaceIssueLinkDTO[]) {
  let links = initial;
  server.use(http.get(LINKS_URL, () => HttpResponse.json({ data: { links } })));
  return (next: WorkspaceIssueLinkDTO[]) => {
    links = next;
  };
}

function conflict(details: Record<string, unknown>) {
  return HttpResponse.json(
    {
      error: {
        code: "CONFLICT",
        message: "Conflict",
        details,
        requestId: "",
      },
    },
    { status: 409 },
  );
}

const SEARCH_URL = "*/api/v1/orgs/:orgId/issues/search";

/** Serve `results` for every search; returns the searches' parameters. */
function serveSearch(
  results: ExternalIssueSummaryDTO[] = [makeIssueSummary()],
) {
  const searches: Array<Record<string, string>> = [];
  server.use(
    http.get(SEARCH_URL, ({ request }) => {
      searches.push(
        Object.fromEntries(new URL(request.url).searchParams.entries()),
      );
      return HttpResponse.json({
        data: { results, domainCount: 1, domainErrors: [], truncated: false },
      });
    }),
  );
  return searches;
}

/** Search for `text` (Enter) and link the result keyed `displayKey`. */
async function linkIssue(text: string, displayKey = "acme/api#12") {
  const user = userEvent.setup();
  await user.type(
    await screen.findByRole("searchbox", { name: "Find an issue" }),
    `${text}{Enter}`,
  );
  const row = await screen.findByTestId(`workspace-issue-result-${displayKey}`);
  await user.click(within(row).getByTestId("workspace-issue-result-link"));
  return user;
}

describe("WorkspaceIssuesPanel", () => {
  beforeEach(() => {
    serveExternalIssuesFeature();
    serveSearch();
  });

  it("continues the existing issue-comment draft at its one creation destination and never posts workspace feedback", async () => {
    serveLinks([makeIssueLink()]);
    const draftKey = `${ISSUE_COMMENT_DRAFT_PREFIX}user-alex:${WORKSPACE_ID}:tracker-1:9001`;
    const operation = "retained-operation";
    localStorage.setItem(
      draftKey,
      JSON.stringify({
        body: "Retained GitHub comment",
        key: operation,
        savedAt: Date.now(),
      }),
    );
    const posted: unknown[] = [];
    let workspaceComments = 0;
    server.use(
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/activity`, () =>
        HttpResponse.json({
          data: {
            ...MOCK_WORKSPACE_ACTIVITY,
            composer: {
              state: "ready",
              githubLogin: "ada-gh",
              destination: { trackerId: "tracker-1", externalIssueId: "9001" },
              displayKey: "acme/api#12",
            },
          },
        }),
      ),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/activity/comments`,
        async ({ request }) => {
          posted.push(await request.json());
          return HttpResponse.json(
            {
              data: {
                id: "publication",
                state: "published",
                kind: "comment",
                origin: "person",
                displayKey: "acme/api#12",
                githubLogin: "ada-gh",
                body: "Retained GitHub comment",
                payload: "Retained GitHub comment",
                externalCommentUrl:
                  "https://github.com/acme/api/issues/12#issuecomment-1",
                error: null,
                createdAt: "2026-10-23T00:00:00Z",
                publishedAt: "2026-10-23T00:00:00Z",
              },
            },
            { status: 201 },
          );
        },
      ),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/comments`, () => {
        workspaceComments++;
        return HttpResponse.json({ data: {} });
      }),
    );
    renderPanel({ viewerOwnsWorkspace: false });
    const composer = await screen.findByTestId("activity-comment-composer");
    expect(screen.getByTestId("workspace-issues-comments")).toContainElement(
      composer,
    );
    expect(screen.getAllByTestId("activity-comment-composer")).toHaveLength(1);
    expect(screen.getByTestId("activity-comment-input")).toHaveValue(
      "Retained GitHub comment",
    );
    const user = userEvent.setup();
    await user.click(screen.getByTestId("activity-comment-review-button"));
    expect(screen.getByTestId("activity-comment-review")).toHaveTextContent(
      "acme/api#12",
    );
    await user.click(screen.getByTestId("activity-comment-post"));
    await screen.findByTestId("activity-comment-posted");
    expect(posted).toEqual([
      {
        body: "Retained GitHub comment",
        idempotencyKey: operation,
        destination: { trackerId: "tracker-1", externalIssueId: "9001" },
      },
    ]);
    expect(workspaceComments).toBe(0);
    expect(localStorage.getItem(draftKey)).toBeNull();
  });

  it("refreshes comment admission after unlinking the primary issue while preserving its private draft", async () => {
    const setLinks = serveLinks([makeIssueLink()]);
    let linked = true;
    server.use(
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/activity`, () =>
        HttpResponse.json({
          data: {
            ...MOCK_WORKSPACE_ACTIVITY,
            composer: linked
              ? {
                  state: "ready",
                  githubLogin: "ada-gh",
                  destination: {
                    trackerId: "tracker-1",
                    externalIssueId: "9001",
                  },
                  displayKey: "acme/api#12",
                }
              : null,
          },
        }),
      ),
      http.post(`${LINKS_URL}/:linkId/unlink`, () => {
        linked = false;
        setLinks([]);
        return HttpResponse.json({ data: { unlinked: true } });
      }),
    );
    renderPanel();
    const input = await screen.findByTestId("activity-comment-input");
    const user = userEvent.setup();
    await user.type(input, "Keep this draft for its original issue");
    const row = screen.getByTestId("workspace-issue-link-link-1");
    await user.click(within(row).getByTestId("workspace-issue-unlink"));
    await user.click(
      within(row).getByTestId("workspace-issue-unlink-confirm-button"),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("activity-comment-composer")).toBeNull(),
    );
    const key = `${ISSUE_COMMENT_DRAFT_PREFIX}user-alex:${WORKSPACE_ID}:tracker-1:9001`;
    expect(JSON.parse(localStorage.getItem(key)!)).toMatchObject({
      body: "Keep this draft for its original issue",
    });
    localStorage.removeItem(key);
  });

  it("lists links with their live summary or typed read error and opens the detail view", async () => {
    const primary = makeIssueLink();
    const broken = makeIssueLink({
      id: "link-2",
      role: "related",
      locator: { repositoryId: "11", number: 40 },
      displayKey: "acme/api#40",
      canonicalUrl: "https://github.com/acme/api/issues/40",
      issue: null,
      issueError: { issueError: "not_found", message: "Issue was deleted" },
    });
    serveLinks([primary, broken]);
    const { onOpenIssue } = renderPanel();
    const user = userEvent.setup();

    const primaryRow = await screen.findByTestId("workspace-issue-link-link-1");
    expect(
      within(primaryRow).getByTestId("workspace-issue-link-role"),
    ).toHaveTextContent("Primary");
    expect(primaryRow).toHaveTextContent("acme/api#12");
    expect(primaryRow).toHaveTextContent("Search returns stale results");
    expect(
      within(primaryRow).getByTestId("external-issue-state"),
    ).toHaveTextContent("Open");
    expect(primaryRow).toHaveTextContent("@octocat");
    expect(primaryRow).toHaveTextContent("bug");

    const brokenRow = screen.getByTestId("workspace-issue-link-link-2");
    expect(
      within(brokenRow).getByTestId("workspace-issue-link-role"),
    ).toHaveTextContent("Related");
    const error = within(brokenRow).getByTestId(
      "workspace-issue-link-error-link-2",
    );
    expect(error).toHaveTextContent(
      "The issue or repository could not be found.",
    );
    expect(error).toHaveTextContent("Issue was deleted");
    expect(
      within(brokenRow).getByTestId("external-issue-open-github"),
    ).toHaveAttribute("href", "https://github.com/acme/api/issues/40");

    await user.click(
      within(primaryRow).getByRole("button", { name: "Open acme/api#12" }),
    );
    expect(onOpenIssue).toHaveBeenCalledWith(primary);
  });

  it("keeps Link and Unlink visible but disabled with the reason on a review copy (g5.governance.review-workspace.exclusions)", async () => {
    const reason = "A review copy follows its source.";
    serveLinks([makeIssueLink()]);
    renderPanel({ writeBlockedReason: reason });
    await screen.findByTestId("workspace-issue-link-link-1");
    expect(
      screen.getByTestId("workspace-issue-link-blocked"),
    ).toHaveTextContent(reason);
    expect(screen.getByTestId("workspace-issue-link-form")).toBeInTheDocument();
    const input = screen.getByRole("searchbox", { name: "Find an issue" });
    expect(input).toBeDisabled();
    expect(input).toHaveAttribute("title", reason);
    const unlink = screen.getAllByTestId("workspace-issue-unlink")[0];
    expect(unlink).toBeDisabled();
    expect(unlink).toHaveAttribute("title", reason);
  });

  it("shows an explicit empty state when nothing is linked", async () => {
    serveLinks([]);
    renderPanel();
    expect(
      await screen.findByTestId("workspace-issues-empty"),
    ).toHaveTextContent("No issues are linked to this workspace.");
  });

  it("renders a failed link list as an error, not as no links", async () => {
    server.use(
      http.get(LINKS_URL, () =>
        HttpResponse.json(
          { error: { code: "INTERNAL", message: "boom", requestId: "" } },
          { status: 500 },
        ),
      ),
    );
    renderPanel();
    expect(
      await screen.findByTestId("workspace-issues-error"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-issues-empty"),
    ).not.toBeInTheDocument();
  });

  it("links an issue as primary by default and refreshes the list", async () => {
    const setLinks = serveLinks([]);
    const posted: CreateWorkspaceIssueLinkInput[] = [];
    server.use(
      http.post(LINKS_URL, async ({ request }) => {
        const body = (await request.json()) as CreateWorkspaceIssueLinkInput;
        posted.push(body);
        const link = makeIssueLink();
        setLinks([link]);
        return HttpResponse.json({ data: link }, { status: 201 });
      }),
    );
    renderPanel();

    await screen.findByTestId("workspace-issues-empty");
    expect(screen.getByTestId("workspace-issue-role-primary")).toBeChecked();
    await linkIssue("stale results");

    expect(
      await screen.findByTestId("workspace-issue-link-link-1"),
    ).toBeInTheDocument();
    expect(posted).toEqual([{ ref: "acme/api#12", role: "primary" }]);
    expect(
      screen.getByTestId("workspace-issue-result-acme/api#12"),
    ).toHaveTextContent("Linked");
    // The workspace now has a primary, so the next link defaults to related.
    expect(screen.getByTestId("workspace-issue-role-related")).toBeChecked();
  });

  it("explains an already-linked conflict without offering a related link", async () => {
    serveLinks([]);
    server.use(
      http.post(LINKS_URL, () => conflict({ code: "already_linked" })),
    );
    renderPanel();
    await screen.findByTestId("workspace-issues-empty");
    await linkIssue("acme/api#12");

    expect(
      await screen.findByTestId("workspace-issue-conflict-already_linked"),
    ).toHaveTextContent("already linked to this workspace");
    expect(
      screen.queryByTestId("workspace-issue-link-as-related"),
    ).not.toBeInTheDocument();
  });

  it("offers a related link when the workspace already has a primary", async () => {
    const setLinks = serveLinks([]);
    const posted: CreateWorkspaceIssueLinkInput[] = [];
    server.use(
      http.post(LINKS_URL, async ({ request }) => {
        const body = (await request.json()) as CreateWorkspaceIssueLinkInput;
        posted.push(body);
        if (body.role === "primary") {
          return conflict({ code: "workspace_has_primary" });
        }
        const link = makeIssueLink({ role: "related" });
        setLinks([link]);
        return HttpResponse.json({ data: link }, { status: 201 });
      }),
    );
    renderPanel();
    await screen.findByTestId("workspace-issues-empty");
    const user = await linkIssue("acme/api#12");

    expect(
      await screen.findByTestId(
        "workspace-issue-conflict-workspace_has_primary",
      ),
    ).toHaveTextContent("This workspace already has a primary issue.");
    await user.click(screen.getByTestId("workspace-issue-link-as-related"));

    await waitFor(() =>
      expect(posted).toEqual([
        { ref: "acme/api#12", role: "primary" },
        { ref: "acme/api#12", role: "related" },
      ]),
    );
    expect(
      await screen.findByTestId("workspace-issue-link-link-1"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-issue-conflict-workspace_has_primary"),
    ).not.toBeInTheDocument();
  });

  it("names a visible owning workspace for an issue that is primary elsewhere", async () => {
    serveLinks([]);
    server.use(
      http.post(LINKS_URL, () =>
        conflict({
          code: "issue_has_primary",
          ownerWorkspace: {
            visible: true,
            workspaceId: "ws-9",
            name: "Other WS",
          },
        }),
      ),
    );
    renderPanel();
    await screen.findByTestId("workspace-issues-empty");
    const user = await linkIssue("acme/api#12");

    const notice = await screen.findByTestId(
      "workspace-issue-conflict-issue_has_primary",
    );
    expect(notice).toHaveTextContent(
      "This issue is already the primary issue of workspace Other WS.",
    );
    expect(
      within(notice).getByTestId("workspace-issue-link-as-related"),
    ).toBeInTheDocument();
    await user.click(within(notice).getByRole("link", { name: "Other WS" }));
    expect(
      await screen.findByTestId("other-workspace-route"),
    ).toHaveTextContent("Other WS?view=issues");
  });

  it("does not reveal a hidden owning workspace", async () => {
    serveLinks([]);
    server.use(
      http.post(LINKS_URL, () =>
        conflict({
          code: "issue_has_primary",
          ownerWorkspace: { visible: false },
        }),
      ),
    );
    renderPanel();
    await screen.findByTestId("workspace-issues-empty");
    await linkIssue("acme/api#12");

    const notice = await screen.findByTestId(
      "workspace-issue-conflict-issue_has_primary",
    );
    expect(notice).toHaveTextContent("another workspace you cannot access");
    expect(within(notice).queryByRole("link")).not.toBeInTheDocument();
  });

  it("explains a typed issue error from the link request", async () => {
    serveLinks([]);
    server.use(
      http.post(LINKS_URL, () =>
        issueErrorResponse(422, "outside_domain", "acme/other is not a domain"),
      ),
    );
    renderPanel();
    await screen.findByTestId("workspace-issues-empty");
    serveSearch([
      makeIssueSummary({
        displayKey: "acme/other#1",
        externalIssueId: "777",
      }),
    ]);
    await linkIssue("acme/other#1", "acme/other#1");

    const error = await screen.findByTestId("workspace-issue-link-error");
    expect(error).toHaveTextContent(
      "This repository is not configured as an issue domain for the organization.",
    );
    expect(error).toHaveTextContent("acme/other is not a domain");
  });

  it("finds issues to link with the organization's issue search (g5.issue.search.one-path)", async () => {
    serveLinks([]);
    const searches = serveSearch([
      makeNativeIssueSummary(),
      makeIssueSummary({ state: "closed", stateReason: "completed" }),
    ]);
    renderPanel();
    await screen.findByTestId("workspace-issues-empty");
    const user = userEvent.setup();
    const input = screen.getByRole("searchbox", { name: "Find an issue" });
    await user.type(input, "jane crash");
    // Typing searches after the pause, every state, any assignee.
    await waitFor(
      () => expect(searches).toEqual([{ q: "jane crash", state: "all" }]),
      { timeout: 2500 },
    );
    const results = await screen.findByTestId("workspace-issue-search-results");
    expect(within(results).getAllByRole("listitem")).toHaveLength(2);
    expect(
      within(results).getByTestId("workspace-issue-result-acme/api#12"),
    ).toHaveTextContent("Closed");
  });

  it("shows a domain's search failure beside the other matches", async () => {
    serveLinks([]);
    server.use(
      http.get(SEARCH_URL, () =>
        HttpResponse.json({
          data: {
            results: [],
            domainCount: 1,
            domainErrors: [
              {
                domainId: "d1",
                displayKey: "ACME",
                issueError: "invalid_query",
                message: "G5 issues cannot be searched by label:",
              },
            ],
            truncated: false,
          },
        }),
      ),
    );
    renderPanel();
    await screen.findByTestId("workspace-issues-empty");
    await userEvent
      .setup()
      .type(
        screen.getByRole("searchbox", { name: "Find an issue" }),
        "label:bug{Enter}",
      );
    expect(
      await screen.findByTestId("workspace-issue-domain-error-d1"),
    ).toHaveTextContent("G5 issues cannot be searched by label:");
    expect(
      screen.getByText("Issue search is unavailable."),
    ).toBeInTheDocument();
  });

  it("unlinks only after confirmation", async () => {
    const setLinks = serveLinks([makeIssueLink()]);
    const unlinked: string[] = [];
    server.use(
      http.post(`${LINKS_URL}/:linkId/unlink`, ({ params }) => {
        unlinked.push(String(params.linkId));
        setLinks([]);
        return HttpResponse.json({ data: { unlinked: true } });
      }),
    );
    renderPanel();
    const user = userEvent.setup();

    const row = await screen.findByTestId("workspace-issue-link-link-1");
    await user.click(
      within(row).getByRole("button", { name: "Unlink acme/api#12" }),
    );
    expect(
      within(row).getByTestId("workspace-issue-unlink-confirm"),
    ).toHaveTextContent("The issue itself is not changed.");
    await user.click(within(row).getByTestId("workspace-issue-unlink-cancel"));
    expect(unlinked).toEqual([]);
    expect(
      within(row).queryByTestId("workspace-issue-unlink-confirm"),
    ).not.toBeInTheDocument();

    await user.click(
      within(row).getByRole("button", { name: "Unlink acme/api#12" }),
    );
    await user.click(
      within(row).getByTestId("workspace-issue-unlink-confirm-button"),
    );
    expect(
      await screen.findByTestId("workspace-issues-empty"),
    ).toBeInTheDocument();
    expect(unlinked).toEqual(["link-1"]);
  });

  it("hides link and unlink controls without workspace write access", async () => {
    serveLinks([makeIssueLink()]);
    renderPanel({ scopes: MOCK_VIEWER_SCOPES });

    expect(
      await screen.findByTestId("workspace-issue-link-link-1"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-issue-link-form"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-issue-unlink"),
    ).not.toBeInTheDocument();
  });

  it("hides link and unlink controls from a writer who does not own the workspace", async () => {
    serveLinks([makeIssueLink()]);
    renderPanel({ viewerOwnsWorkspace: false });

    expect(
      await screen.findByTestId("workspace-issue-link-link-1"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-issue-link-form"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-issue-unlink"),
    ).not.toBeInTheDocument();
  });

  it("renders nothing and requests no links while the feature is off", async () => {
    let requested = false;
    let instanceInfoServed = false;
    server.use(
      http.get("/api/v1/instance/info", () => {
        instanceInfoServed = true;
        return HttpResponse.json({
          data: { version: "0.1.0", features: ["chat"], authProviders: [] },
        });
      }),
      http.get(LINKS_URL, () => {
        requested = true;
        return HttpResponse.json({ data: { links: [] } });
      }),
    );
    renderPanel();

    await waitFor(() => expect(instanceInfoServed).toBe(true));
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(
      screen.queryByTestId("workspace-issues-panel"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Issues" }),
    ).not.toBeInTheDocument();
    expect(requested).toBe(false);
  });

  describe("while the organization has external issues off", () => {
    beforeEach(() => {
      serveOrgExternalIssues(false);
    });

    it("lets the owner unlink existing links without showing issue content", async () => {
      serveLinks([makeIssueLink({ issue: null, issueError: null })]);
      const unlinked: string[] = [];
      server.use(
        http.post(`${LINKS_URL}/:linkId/unlink`, ({ params }) => {
          unlinked.push(String(params.linkId));
          return HttpResponse.json({ data: { unlinked: true } });
        }),
      );
      renderPanel();
      const user = userEvent.setup();

      const section = await screen.findByTestId("workspace-issues-cleanup");
      expect(section).toHaveTextContent(
        "Issues are off for this organization.",
      );
      const row = within(section).getByTestId("workspace-issue-link-link-1");
      expect(row).toHaveTextContent("acme/api#12");
      expect(
        within(row).queryByTestId("workspace-issue-open"),
      ).not.toBeInTheDocument();
      expect(
        within(row).queryByTestId("workspace-issue-link-error-link-1"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("workspace-issue-link-form"),
      ).not.toBeInTheDocument();

      await user.click(within(row).getByTestId("workspace-issue-unlink"));
      await user.click(
        within(row).getByTestId("workspace-issue-unlink-confirm-button"),
      );
      await waitFor(() => expect(unlinked).toEqual(["link-1"]));
    });

    it("shows nothing when no link is left", async () => {
      let requested = false;
      server.use(
        http.get(LINKS_URL, () => {
          requested = true;
          return HttpResponse.json({ data: { links: [] } });
        }),
      );
      renderPanel();

      await waitFor(() => expect(requested).toBe(true));
      await new Promise((resolve) => setTimeout(resolve, 50));
      expect(
        screen.queryByRole("heading", { name: "Issues" }),
      ).not.toBeInTheDocument();
    });

    it("shows nothing and requests no links for a viewer who cannot unlink", async () => {
      let requested = false;
      server.use(
        http.get(LINKS_URL, () => {
          requested = true;
          return HttpResponse.json({ data: { links: [makeIssueLink()] } });
        }),
      );
      renderPanel({ viewerOwnsWorkspace: false });

      await new Promise((resolve) => setTimeout(resolve, 100));
      expect(
        screen.queryByRole("heading", { name: "Issues" }),
      ).not.toBeInTheDocument();
      expect(requested).toBe(false);
    });
  });

  it("reports a failed availability check with retry, not as unavailable", async () => {
    serveInstanceInfoFailingOnce();
    serveLinks([]);
    renderPanel();
    const user = userEvent.setup();

    const error = await screen.findByTestId("workspace-issues-feature-error");
    expect(error).toHaveTextContent(
      "Could not check whether issues are available.",
    );
    expect(screen.getByRole("heading", { name: "Issues" })).toBeVisible();
    await user.click(
      screen.getByTestId("workspace-issues-feature-error-retry"),
    );
    expect(
      await screen.findByTestId("workspace-issues-empty"),
    ).toBeInTheDocument();
  });

  it("keeps a link without a live summary openable and explains why", async () => {
    serveLinks([
      makeIssueLink({
        issue: null,
        issueError: {
          issueError: "unavailable",
          message: "GitHub timed out",
        },
      }),
    ]);
    renderPanel();

    const row = await screen.findByTestId("workspace-issue-link-link-1");
    expect(
      within(row).getByTestId("workspace-issue-link-error-link-1"),
    ).toHaveTextContent("The issue tracker did not respond in time");
    expect(
      within(row).getByRole("button", { name: "Open acme/api#12" }),
    ).toBeEnabled();
  });
});
