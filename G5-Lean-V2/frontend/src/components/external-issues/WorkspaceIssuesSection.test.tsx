import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  issueConflictResponse,
  makeIssueCreateTarget,
  makeIssueCreation,
  makeIssueLink,
  makeIssueStateChange,
  makeIssueSummary,
  makeNativeIssueLink,
  makeNativeIssueSummary,
  serveExternalIssuesFeature,
  serveGithubConnection,
  serveInstanceInfoFailingOnce,
  serveNativeIssuesFeature,
} from "../../test/externalIssueFixtures";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import type { WorkspaceIssueLinkDTO } from "../../lib/types";
import {
  ISSUE_QUICK_ADD_LINKS_LOADING_REASON,
  ISSUE_QUICK_ADD_OWNER_REASON,
  WorkspaceIssuesSection,
} from "./WorkspaceIssuesSection";
import {
  ISSUE_STATE_CONNECT_REASON,
  ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE,
  ISSUE_STATE_UNREADABLE_REASON,
} from "../../lib/issueState";

function renderSection(
  props: { viewerOwnsWorkspace?: boolean; writeBlockedReason?: string } = {},
) {
  const onOpenIssue = vi.fn();
  const rendered = renderApp(
    <Routes>
      <Route
        path="/:org/:workspace"
        element={
          <WorkspaceIssuesSection
            workspaceId="ws-1"
            onOpenIssue={onOpenIssue}
            {...props}
          />
        }
      />
    </Routes>,
    { route: "/g5labs/current", user: createMockUser() },
  );
  return { ...rendered, onOpenIssue };
}
const url = "/api/v1/workspaces/ws-1/issue-links";
const STATE_URL = "/api/v1/workspaces/ws-1/issue-links/:linkId/state";

const TARGET_URL = "/api/v1/orgs/:orgId/issues/create-target";
const CREATE_URL = "/api/v1/orgs/:orgId/issues";
const RESOLVE_URL = "/api/v1/orgs/:orgId/issues/resolve";

function serveCreation(
  linkStatus: number,
  links: WorkspaceIssueLinkDTO[] = [],
) {
  const linkBodies: unknown[] = [];
  const createBodies: Array<Record<string, unknown>> = [];
  server.use(
    http.get(url, () => HttpResponse.json({ data: { links } })),
    http.get(RESOLVE_URL, () =>
      HttpResponse.json({ data: makeIssueSummary({ title: "Release 2.0" }) }),
    ),
    http.get(TARGET_URL, () =>
      HttpResponse.json({ data: makeIssueCreateTarget() }),
    ),
    http.post(CREATE_URL, async ({ request }) => {
      createBodies.push((await request.json()) as Record<string, unknown>);
      return HttpResponse.json(
        {
          data: makeIssueCreation({
            parent: createBodies.at(-1)?.parent
              ? {
                  externalIssueId: "900012",
                  number: 12,
                  displayKey: "acme/api#12",
                  title: "Release 2.0",
                  url: "https://github.com/acme/api/issues/12",
                  state: "attached",
                  message: null,
                }
              : null,
          }),
        },
        { status: 201 },
      );
    }),
    http.post(url, async ({ request }) => {
      linkBodies.push(await request.json());
      return linkStatus === 201
        ? HttpResponse.json({ data: { id: "link-77" } }, { status: 201 })
        : HttpResponse.json(
            { error: { code: "INTERNAL", message: "link failed" } },
            { status: linkStatus },
          );
    }),
  );
  return { linkBodies, createBodies };
}
beforeEach(() => {
  serveExternalIssuesFeature();
  serveGithubConnection(true);
});

describe("WorkspaceIssuesSection", () => {
  it("sorts primary first, labels related as Subtask, opens rows, and offers Done as the viewer's own write", async () => {
    const primary = makeIssueLink();
    const closed = makeIssueLink({
      id: "closed",
      role: "related",
      issue: makeIssueSummary({ state: "closed" }),
    });
    server.use(
      http.get(url, () =>
        HttpResponse.json({ data: { links: [closed, primary] } }),
      ),
    );
    const { onOpenIssue } = renderSection();
    await screen.findByTestId("workspace-issues-row-link-1");
    const rows = within(
      screen.getByRole("list", { name: "Linked issues" }),
    ).getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("Primary");
    expect(rows[1]).toHaveTextContent("Subtask");
    expect(
      screen.getByTestId("workspace-issues-done-link-1"),
    ).not.toBeChecked();
    const done = screen.getByTestId("workspace-issues-done-closed");
    expect(done).toBeChecked();
    await waitFor(() => expect(done).toBeEnabled());
    expect(done).toHaveAttribute(
      "title",
      "Reopen acme/api#12 on GitHub as your account",
    );
    expect(screen.getByTestId("workspace-issues-done-link-1")).toHaveAttribute(
      "title",
      "Mark acme/api#12 as done on GitHub as your account",
    );
    const quick = screen.getByTestId("workspace-issues-quick-add");
    expect(quick).toBeDisabled();
    expect(quick).toHaveAttribute("title", ISSUE_QUICK_ADD_OWNER_REASON);
    await userEvent.click(screen.getByTestId("workspace-issues-row-link-1"));
    expect(onOpenIssue).toHaveBeenCalledWith(primary);
  });
  it("disables Done with the reason on a review copy, without GitHub, or for an unreadable issue", async () => {
    const unreadable = makeIssueLink({
      id: "broken",
      role: "related",
      issue: null,
      issueError: { issueError: "unavailable", message: "GitHub timed out" },
    });
    server.use(
      http.get(url, () =>
        HttpResponse.json({ data: { links: [makeIssueLink(), unreadable] } }),
      ),
    );
    const copy = renderSection({
      writeBlockedReason: "A review copy follows its source",
    });
    const onCopy = await screen.findByTestId("workspace-issues-done-link-1");
    expect(onCopy).toBeDisabled();
    expect(onCopy).toHaveAttribute("title", "A review copy follows its source");
    copy.unmount();

    serveGithubConnection(false);
    renderSection();
    const done = await screen.findByTestId("workspace-issues-done-link-1");
    await waitFor(() =>
      expect(done).toHaveAttribute("title", ISSUE_STATE_CONNECT_REASON),
    );
    expect(done).toBeDisabled();
    const broken = screen.getByTestId("workspace-issues-done-broken");
    expect(broken).toBeDisabled();
    expect(broken).toHaveAttribute("title", ISSUE_STATE_UNREADABLE_REASON);
  });

  it("Done closes the issue as the viewer, Reopen reopens it, and the row follows GitHub's answer", async () => {
    const link = makeIssueLink();
    const bodies: Array<{ linkId: string; state: string }> = [];
    server.use(
      http.get(url, () => HttpResponse.json({ data: { links: [link] } })),
      http.post(STATE_URL, async ({ request, params }) => {
        const body = (await request.json()) as { state: "closed" | "open" };
        bodies.push({ linkId: String(params.linkId), state: body.state });
        return HttpResponse.json({ data: makeIssueStateChange(body.state) });
      }),
    );
    renderSection();
    const done = await screen.findByTestId("workspace-issues-done-link-1");
    await waitFor(() => expect(done).toBeEnabled());
    await userEvent.click(done);
    await waitFor(() =>
      expect(screen.getByTestId("workspace-issues-done-link-1")).toBeChecked(),
    );
    expect(bodies).toEqual([{ linkId: "link-1", state: "closed" }]);
    expect(screen.getByTestId("workspace-issues-row-link-1")).toHaveTextContent(
      /closed/i,
    );
    await userEvent.click(screen.getByTestId("workspace-issues-done-link-1"));
    await waitFor(() =>
      expect(
        screen.getByTestId("workspace-issues-done-link-1"),
      ).not.toBeChecked(),
    );
    expect(bodies[1]).toEqual({ linkId: "link-1", state: "open" });
    expect(
      screen.queryByTestId("workspace-issues-state-notice-link-1"),
    ).not.toBeInTheDocument();
  });

  it("shows a refusal under the row and keeps the box, and re-reads after an unknown outcome", async () => {
    let links = [makeIssueLink()];
    let listReads = 0;
    let answer: "refuse" | "unknown" = "refuse";
    server.use(
      http.get(url, () => {
        listReads += 1;
        return HttpResponse.json({ data: { links } });
      }),
      http.post(STATE_URL, () =>
        answer === "refuse"
          ? issueConflictResponse("public_destination")
          : HttpResponse.json({
              data: {
                requested: "closed",
                outcome: "unknown",
                issue: null,
                githubLogin: "alex-gh",
                message: "GitHub timed out",
              },
            }),
      ),
    );
    renderSection();
    const done = await screen.findByTestId("workspace-issues-done-link-1");
    await waitFor(() => expect(done).toBeEnabled());
    await userEvent.click(done);
    const notice = await screen.findByTestId(
      "workspace-issues-state-notice-link-1",
    );
    expect(notice).toHaveTextContent("public destination");
    // A refusal re-reads the row: the issue or its domain may have changed.
    await waitFor(() => expect(listReads).toBeGreaterThan(1));
    expect(
      screen.getByTestId("workspace-issues-done-link-1"),
    ).not.toBeChecked();
    expect(screen.getByTestId("workspace-issues-done-link-1")).toBeEnabled();
    await userEvent.click(
      screen.getByTestId("workspace-issues-state-notice-dismiss-link-1"),
    );
    expect(
      screen.queryByTestId("workspace-issues-state-notice-link-1"),
    ).not.toBeInTheDocument();

    answer = "unknown";
    const readsBefore = listReads;
    links = [makeIssueLink({ issue: makeIssueSummary({ state: "closed" }) })];
    await userEvent.click(screen.getByTestId("workspace-issues-done-link-1"));
    expect(
      await screen.findByTestId("workspace-issues-state-notice-link-1"),
    ).toHaveTextContent(ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE);
    await waitFor(() => expect(listReads).toBeGreaterThan(readsBefore));
    await waitFor(() =>
      expect(screen.getByTestId("workspace-issues-done-link-1")).toBeChecked(),
    );
  });

  it("disables quick-add with the reason when creation is impossible or linking is blocked", async () => {
    server.use(http.get(url, () => HttpResponse.json({ data: { links: [] } })));
    // The default target answer: no repository for new issues.
    const first = renderSection({ viewerOwnsWorkspace: true });
    const quick = await screen.findByTestId("workspace-issues-quick-add");
    await waitFor(() =>
      expect(quick).toHaveAttribute(
        "title",
        "No repository is set up for new issues",
      ),
    );
    expect(quick).toBeDisabled();
    first.unmount();

    const targetReads = vi.fn();
    server.use(
      http.get(TARGET_URL, () => {
        targetReads();
        return HttpResponse.json({ data: makeIssueCreateTarget() });
      }),
    );
    renderSection({
      viewerOwnsWorkspace: true,
      writeBlockedReason: "A review copy follows its source",
    });
    const blocked = await screen.findByTestId("workspace-issues-quick-add");
    expect(blocked).toBeDisabled();
    expect(blocked).toHaveAttribute(
      "title",
      "A review copy follows its source",
    );
    expect(targetReads).not.toHaveBeenCalled();
  });

  it("quick-add creates an issue without Create & Fork and links it as related", async () => {
    const user = userEvent.setup();
    const { linkBodies, createBodies } = serveCreation(201);
    renderSection({ viewerOwnsWorkspace: true });
    const quick = await screen.findByTestId("workspace-issues-quick-add");
    await waitFor(() => expect(quick).toBeEnabled());
    await user.click(quick);

    const dialog = await screen.findByTestId("create-issue-dialog");
    expect(
      within(dialog).queryByTestId("create-issue-submit-fork"),
    ).not.toBeInTheDocument();
    await user.type(
      within(dialog).getByTestId("create-issue-title"),
      "Write the release notes",
    );
    await user.click(within(dialog).getByTestId("create-issue-submit"));

    await waitFor(() =>
      expect(linkBodies).toEqual([{ ref: "acme/api#77", role: "related" }]),
    );
    expect(createBodies).toHaveLength(1);
    expect(screen.queryByTestId("create-issue-dialog")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-issues-quick-add-link-failed"),
    ).not.toBeInTheDocument();
  });

  it("quick-add makes the task a sub-issue of the primary issue in the destination", async () => {
    const user = userEvent.setup();
    const { linkBodies, createBodies } = serveCreation(201, [makeIssueLink()]);
    renderSection({ viewerOwnsWorkspace: true });
    const quick = await screen.findByTestId("workspace-issues-quick-add");
    await waitFor(() =>
      expect(quick).toHaveAttribute(
        "title",
        "Create a sub-issue of acme/api#12 and link it to this workspace",
      ),
    );
    await user.click(quick);

    const dialog = await screen.findByTestId("create-issue-dialog");
    expect(within(dialog).getByTestId("create-issue-parent")).toHaveValue(
      "acme/api#12",
    );
    expect(await within(dialog).findByText(/Release 2\.0/)).toBeInTheDocument();
    await user.type(
      within(dialog).getByTestId("create-issue-title"),
      "Write the release notes",
    );
    await waitFor(() =>
      expect(within(dialog).getByTestId("create-issue-submit")).toBeEnabled(),
    );
    await user.click(within(dialog).getByTestId("create-issue-submit"));

    await waitFor(() =>
      expect(linkBodies).toEqual([{ ref: "acme/api#77", role: "related" }]),
    );
    expect(createBodies).toHaveLength(1);
    expect(createBodies[0].parent).toBe("acme/api#12");
  });

  it("keeps quick-add disabled until the links load, so a late primary issue still becomes the parent", async () => {
    const user = userEvent.setup();
    serveCreation(201);
    let releaseLinks!: () => void;
    const linksHeld = new Promise<void>((resolve) => {
      releaseLinks = resolve;
    });
    server.use(
      http.get(url, async () => {
        await linksHeld;
        return HttpResponse.json({ data: { links: [makeIssueLink()] } });
      }),
    );
    renderSection({ viewerOwnsWorkspace: true });
    const quick = await screen.findByTestId("workspace-issues-quick-add");
    await waitFor(() =>
      expect(quick).toHaveAttribute(
        "title",
        ISSUE_QUICK_ADD_LINKS_LOADING_REASON,
      ),
    );
    expect(quick).toBeDisabled();

    releaseLinks();
    await waitFor(() => expect(quick).toBeEnabled());
    await user.click(quick);
    const dialog = await screen.findByTestId("create-issue-dialog");
    expect(within(dialog).getByTestId("create-issue-parent")).toHaveValue(
      "acme/api#12",
    );
  });

  it("quick-add starts without a parent when the primary issue is in another repository, or cleared", async () => {
    const user = userEvent.setup();
    const elsewhere = makeIssueLink({
      locator: { repositoryId: "99", number: 5 },
    });
    serveCreation(201, [elsewhere]);
    const first = renderSection({ viewerOwnsWorkspace: true });
    let quick = await screen.findByTestId("workspace-issues-quick-add");
    await waitFor(() =>
      expect(quick).toHaveAttribute(
        "title",
        "Create an issue and link it to this workspace",
      ),
    );
    await user.click(quick);
    let dialog = await screen.findByTestId("create-issue-dialog");
    expect(within(dialog).getByTestId("create-issue-parent")).toHaveValue("");
    first.unmount();

    // The suggested parent can be cleared: the task is then top-level.
    const { createBodies } = serveCreation(201, [makeIssueLink()]);
    renderSection({ viewerOwnsWorkspace: true });
    quick = await screen.findByTestId("workspace-issues-quick-add");
    await waitFor(() => expect(quick).toBeEnabled());
    await user.click(quick);
    dialog = await screen.findByTestId("create-issue-dialog");
    await user.clear(within(dialog).getByTestId("create-issue-parent"));
    await user.type(within(dialog).getByTestId("create-issue-title"), "Docs");
    await user.click(within(dialog).getByTestId("create-issue-submit"));
    await waitFor(() => expect(createBodies).toHaveLength(1));
    expect(createBodies[0]).not.toHaveProperty("parent");
  });

  it("keeps a created issue it could not link and points to Import task", async () => {
    const user = userEvent.setup();
    const { linkBodies } = serveCreation(500);
    renderSection({ viewerOwnsWorkspace: true });
    const quick = await screen.findByTestId("workspace-issues-quick-add");
    await waitFor(() => expect(quick).toBeEnabled());
    await user.click(quick);
    await user.type(
      await screen.findByTestId("create-issue-title"),
      "Write the release notes",
    );
    await user.click(screen.getByTestId("create-issue-submit"));

    const notice = await screen.findByTestId(
      "workspace-issues-quick-add-link-failed",
    );
    expect(notice).toHaveTextContent(
      "Created acme/api#77, but it could not be linked to this workspace. Import task can link it.",
    );
    expect(
      within(notice).getByRole("link", { name: "acme/api#77" }),
    ).toHaveAttribute("href", "https://github.com/acme/api/issues/77");
    expect(linkBodies).toHaveLength(1);
    await user.click(
      within(notice).getByTestId(
        "workspace-issues-quick-add-link-failed-dismiss",
      ),
    );
    expect(
      screen.queryByTestId("workspace-issues-quick-add-link-failed"),
    ).not.toBeInTheDocument();
  });

  it("keeps unreadable links openable with the shared typed error", async () => {
    server.use(
      http.get(url, () =>
        HttpResponse.json({
          data: {
            links: [
              makeIssueLink({
                issue: null,
                issueError: {
                  issueError: "not_found",
                  message: "Issue was deleted",
                },
              }),
            ],
          },
        }),
      ),
    );
    renderSection();
    expect(
      await screen.findByTestId("workspace-issue-link-error-link-1"),
    ).toHaveTextContent("Issue was deleted");
    expect(screen.getByTestId("workspace-issues-row-link-1")).toBeEnabled();
  });
  it("reports a failed list and allows retry", async () => {
    let calls = 0;
    server.use(
      http.get(url, () =>
        ++calls === 1
          ? HttpResponse.json(
              { error: { code: "INTERNAL", message: "boom" } },
              { status: 500 },
            )
          : HttpResponse.json({ data: { links: [] } }),
      ),
    );
    renderSection();
    await screen.findByTestId("workspace-issues-links-error");
    expect(
      screen.queryByText("No issues are linked to this workspace."),
    ).toBeNull();
    await userEvent.click(
      screen.getByTestId("workspace-issues-links-error-retry"),
    );
    await screen.findByText("No issues are linked to this workspace.");
  });
  it("retries a failed feature check", async () => {
    serveInstanceInfoFailingOnce();
    server.use(http.get(url, () => HttpResponse.json({ data: { links: [] } })));
    renderSection();
    await userEvent.click(
      await screen.findByTestId("workspace-issues-feature-error-retry"),
    );
    await screen.findByText("No issues are linked to this workspace.");
  });
  it("does not read links without the feature", async () => {
    let checked = false;
    const read = vi.fn();
    server.use(
      http.get("/api/v1/instance/info", () => {
        checked = true;
        return HttpResponse.json({ data: { features: [] } });
      }),
      http.get(url, () => {
        read();
        return HttpResponse.json({ data: { links: [] } });
      }),
    );
    renderSection();
    await waitFor(() => expect(checked).toBe(true));
    expect(screen.queryByTestId("workspace-issues-section")).toBeNull();
    expect(read).not.toHaveBeenCalled();
  });
});

// Issues g5.workbench.issues.arrival: a link that arrives after the first
// read is spotlighted once; the first read and a refused link never are.
describe("WorkspaceIssuesSection — arrival spotlight", () => {
  it("spotlights only links that arrive after the list first loaded", async () => {
    const user = userEvent.setup();
    const primary = makeIssueLink();
    const { linkBodies } = serveCreation(201, [primary]);
    renderSection({ viewerOwnsWorkspace: true });
    const first = await screen.findByTestId(
      `workspace-issues-item-${primary.id}`,
    );
    expect(first).not.toHaveAttribute("data-arrived");
    expect(first.className).not.toContain("g5-row-spotlight");

    // Quick-add creates and links; the refetch brings the new row.
    const arrived = makeIssueLink({
      id: "link-77",
      role: "related",
      displayKey: "acme/api#77",
      locator: { repositoryId: "11", number: 77 },
      issue: makeIssueSummary({ title: "Write the release notes" }),
    });
    const quick = await screen.findByTestId("workspace-issues-quick-add");
    await waitFor(() => expect(quick).toBeEnabled());
    await user.click(quick);
    const dialog = await screen.findByTestId("create-issue-dialog");
    await user.type(
      within(dialog).getByTestId("create-issue-title"),
      "Write the release notes",
    );
    server.use(
      http.get(url, () =>
        HttpResponse.json({ data: { links: [primary, arrived] } }),
      ),
    );
    await user.click(within(dialog).getByTestId("create-issue-submit"));
    await waitFor(() => expect(linkBodies).toHaveLength(1));

    const row = await screen.findByTestId("workspace-issues-item-link-77");
    expect(row).toHaveAttribute("data-arrived", "true");
    expect(row.className).toContain("g5-row-spotlight");
    expect(
      screen.getByTestId(`workspace-issues-item-${primary.id}`),
    ).not.toHaveAttribute("data-arrived");
  });
});

describe("WorkspaceIssuesSection — native G5 issues", () => {
  it("marks a native issue done and reopens it without a GitHub connection", async () => {
    // No GitHub App and no GitHub account: only the native tracker.
    serveNativeIssuesFeature();
    server.use(
      http.get("/api/v1/github-app/user-connection", () =>
        HttpResponse.json(
          { error: { code: "NOT_FOUND", message: "no GitHub App" } },
          { status: 404 },
        ),
      ),
    );
    const link = makeNativeIssueLink();
    const bodies: string[] = [];
    server.use(
      http.get(url, () => HttpResponse.json({ data: { links: [link] } })),
      http.post(STATE_URL, async ({ request }) => {
        const body = (await request.json()) as { state: "closed" | "open" };
        bodies.push(body.state);
        return HttpResponse.json({
          data: {
            requested: body.state,
            outcome: "applied",
            issue: makeNativeIssueSummary({ state: body.state }),
            githubLogin: null,
            message: null,
          },
        });
      }),
    );
    renderSection();

    const done = await screen.findByTestId("workspace-issues-done-link-1");
    await waitFor(() => expect(done).toBeEnabled());
    expect(done).toHaveAttribute("title", "Mark ACME-12 as done");
    expect(screen.getByTestId("workspace-issues-row-link-1")).toHaveTextContent(
      "ACME-12",
    );
    await userEvent.click(done);
    await waitFor(() =>
      expect(screen.getByTestId("workspace-issues-done-link-1")).toBeChecked(),
    );
    expect(screen.getByTestId("workspace-issues-done-link-1")).toHaveAttribute(
      "title",
      "Reopen ACME-12",
    );
    await userEvent.click(screen.getByTestId("workspace-issues-done-link-1"));
    await waitFor(() => expect(bodies).toEqual(["closed", "open"]));
  });
});
