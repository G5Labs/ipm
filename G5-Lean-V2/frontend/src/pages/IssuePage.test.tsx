/**
 * IssuePage — one issue outside any workspace
 * (`/:org/issues/github/:repositoryId/:number`, g5.workbench.issues.create,
 * and the native `/:org/issues/g5/:number`, g5.issue.native.page).
 *
 * Covered: the GitHub issue view with a way back to Home and to the visible
 * workspace holding it as primary, Start workspace
 * only while no workspace owns the issue, and the fork dialog it opens with
 * the issue preselected as primary. A GitHub issue's page, as the viewer's
 * connected account: the edit form sends only changed fields with what it
 * started from as `seen` (without G5's creation marker), reviews the exact
 * change first on an untrusted domain, asks for a reload when the issue
 * changed, and reports partial and unconfirmed outcomes, never as saved;
 * Close/Reopen, including an unconfirmed outcome; comments reviewed first,
 * a draft and key kept when GitHub does not confirm; the viewer's own
 * comments edited by login; connection, blocked and viewer states. A native
 * issue's page, on a deployment without the GitHub App: members comment
 * (with an idempotency key), edit their own comments and the issue at the
 * version they saw, close and reopen it; a changed issue asks for a reload;
 * viewers read. Labels on either page: the picker's changes sent as deltas
 * from what the editor started from (a label someone else added meanwhile
 * is left alone), the labels as re-read shown at once, an untrusted GitHub
 * domain's exact additions and removals reviewed first, partial and unknown
 * outcomes listed per label, a refusal explained, a blocked write target
 * disabling the editor, and viewers reading only.
 */

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it } from "vitest";

import type { IssueWriteTargetDTO } from "../lib/types";

import {
  issueConflictResponse,
  makeGithubCommentResult,
  makeIssueDetail,
  makeIssueEdit,
  makeIssueWriteTarget,
  makeIssueSummary,
  makeNativeCommentResult,
  makeNativeIssueDetail,
  makeNativeIssueSummary,
  makeIssueStateChange,
  makeIssueLabelChange,
  makeLabelPage,
  labelValidationResponse,
  serveLabelCatalog,
  NATIVE_ALEX,
  NATIVE_MAYA,
  serveExternalIssuesFeature,
  serveNativeIssuesFeature,
} from "../test/externalIssueFixtures";
import {
  createMockUser,
  createMockWorkspace,
  mockOrgMembers,
} from "../test/fixtures";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { RequireAuth } from "../hooks/useAuth";
import { IssuePage } from "./IssuePage";

const DETAIL_URL = "/api/v1/orgs/org-g5labs/issues/github/11/12";

function serveWriteTarget(overrides: Partial<IssueWriteTargetDTO> = {}) {
  let reads = 0;
  server.use(
    http.get(`${DETAIL_URL}/write-target`, () => {
      reads += 1;
      return HttpResponse.json({ data: makeIssueWriteTarget(overrides) });
    }),
  );
  return () => reads;
}

function renderPage(user = createMockUser()) {
  return renderApp(
    <Routes>
      <Route
        path="/:org/issues/github/:repositoryId/:number"
        element={
          <RequireAuth>
            <IssuePage />
          </RequireAuth>
        }
      />
      <Route path="/:org/home" element={<div data-testid="home-route" />} />
    </Routes>,
    { route: "/g5labs/issues/github/11/12", user },
  );
}

const VIEWER = createMockUser({
  orgs: [
    {
      orgId: "org-g5labs",
      orgName: "G5 Labs",
      orgSlug: "g5labs",
      role: "viewer",
    },
  ],
});

beforeEach(() => {
  serveExternalIssuesFeature();
  serveWriteTarget();
  window.localStorage.clear();
  server.use(
    http.get("/api/v1/orgs/:orgId/workspaces", () =>
      HttpResponse.json({
        data: [createMockWorkspace({ id: "base", name: "base", isBase: true })],
      }),
    ),
    http.get("/api/v1/workspaces/:workspaceId/fork_advisories", () =>
      HttpResponse.json({ data: [] }),
    ),
    // The fork dialog's "start work" launch is not under test here.
    http.post(
      "/api/v1/workspaces/:workspaceId/config/agent-launch/resolve",
      () =>
        HttpResponse.json(
          { error: { code: "NOT_FOUND", message: "no launch configured" } },
          { status: 404 },
        ),
    ),
  );
});

describe("IssuePage", () => {
  it("shows the issue with a way back to Home and starts a workspace for an unowned issue", async () => {
    const user = userEvent.setup();
    server.use(
      http.get(DETAIL_URL, () =>
        HttpResponse.json({
          data: makeIssueDetail({ primaryWorkspace: null }),
        }),
      ),
      http.get("/api/v1/orgs/org-g5labs/issues/resolve", () =>
        HttpResponse.json({ data: makeIssueSummary() }),
      ),
    );
    renderPage();

    expect(await screen.findByTestId("external-issue-detail")).toBeVisible();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      "Search returns stale results",
    );
    expect(screen.getByTestId("issue-page-back")).toHaveAttribute(
      "href",
      "/g5labs/home",
    );

    await user.click(await screen.findByTestId("issue-page-start-workspace"));
    expect(
      await screen.findByRole("heading", { name: "Fork base" }),
    ).toBeVisible();
    expect(await screen.findByTestId("clone-issue-ref")).toHaveValue(
      "acme/api#12",
    );
    expect(screen.getByTestId("clone-issue-role")).toHaveValue("primary");
  });

  it("offers no Start workspace once a workspace owns the issue", async () => {
    server.use(
      http.get(DETAIL_URL, () =>
        HttpResponse.json({
          data: makeIssueDetail({
            primaryWorkspace: {
              visible: true,
              workspaceId: "ws-1",
              name: "Alpha",
            },
            links: [
              { workspaceId: "ws-1", workspaceName: "Alpha", role: "primary" },
            ],
          }),
        }),
      ),
    );
    renderPage();

    expect(await screen.findByTestId("external-issue-detail")).toBeVisible();
    await waitFor(() =>
      expect(screen.getByTestId("external-issue-link-ws-1")).toHaveTextContent(
        "Alpha",
      ),
    );
    expect(
      screen.queryByTestId("issue-page-start-workspace"),
    ).not.toBeInTheDocument();
    // The workspace that holds it as primary is one step back.
    const back = screen.getByTestId("issue-page-back-to-workspace");
    expect(back).toHaveAttribute("href", "/g5labs/Alpha");
    expect(back).toHaveTextContent("Back to Alpha");
  });

  it("offers no way back to a primary workspace the viewer cannot see, or to none", async () => {
    for (const primaryWorkspace of [{ visible: false } as const, null]) {
      server.use(
        http.get(DETAIL_URL, () =>
          HttpResponse.json({ data: makeIssueDetail({ primaryWorkspace }) }),
        ),
      );
      const { unmount } = renderPage();
      expect(await screen.findByTestId("external-issue-detail")).toBeVisible();
      expect(screen.getByTestId("issue-page-back")).toBeVisible();
      expect(
        screen.queryByTestId("issue-page-back-to-workspace"),
      ).not.toBeInTheDocument();
      unmount();
    }
  });
});

describe("IssuePage — GitHub issue writes", () => {
  function serveIssue(overrides: Parameters<typeof makeIssueDetail>[0] = {}) {
    server.use(
      http.get(DETAIL_URL, () =>
        HttpResponse.json({ data: makeIssueDetail(overrides) }),
      ),
    );
  }

  it("edits only the changed fields with what the form showed, and asks for a reload when GitHub's issue changed", async () => {
    const user = userEvent.setup();
    const edits: unknown[] = [];
    serveIssue();
    serveWriteTarget({ contentTrusted: true });
    server.use(
      http.patch(DETAIL_URL, async ({ request }) => {
        edits.push(await request.json());
        return issueConflictResponse("issue_changed");
      }),
    );
    renderPage();

    expect(await screen.findByText(/made on GitHub as @alex-gh/)).toBeVisible();
    await user.click(await screen.findByTestId("issue-page-edit-button"));
    const title = screen.getByTestId("issue-page-edit-title");
    await user.clear(title);
    await user.type(title, "Search serves stale results");
    // The repository's assignable accounts are offered.
    await user.click(screen.getByTestId("issue-page-edit-assignee-hubot"));
    // A trusted domain saves without a review step.
    await user.click(screen.getByTestId("issue-page-edit-save"));

    await waitFor(() =>
      expect(edits).toEqual([
        {
          title: "Search serves stale results",
          assignees: ["octocat", "hubot"],
          seen: {
            title: "Search returns stale results",
            assignees: ["octocat"],
          },
        },
      ]),
    );
    expect(
      await screen.findByTestId("issue-page-edit-error"),
    ).toHaveTextContent("This issue changed since you opened it.");
    expect(
      screen.getByTestId("issue-page-edit-error-reload"),
    ).toBeInTheDocument();
  });

  it("reviews an untrusted edit as the connected account, keeps the creation marker out, and reports a partial outcome", async () => {
    const user = userEvent.setup();
    const edits: unknown[] = [];
    serveIssue({
      body: "The search index is stale.\n\n<!-- g5-issue 3f2c -->",
    });
    server.use(
      http.patch(DETAIL_URL, async ({ request }) => {
        edits.push(await request.json());
        return HttpResponse.json({
          data: makeIssueEdit({
            outcome: "partial",
            notUnassigned: ["octocat"],
          }),
        });
      }),
    );
    renderPage();

    await user.click(await screen.findByTestId("issue-page-edit-button"));
    const body = screen.getByTestId("issue-page-edit-body");
    expect(body).toHaveValue("The search index is stale.");
    await user.type(body, " Rebuild it.");
    await user.click(screen.getByTestId("issue-page-edit-assignee-octocat"));
    expect(screen.queryByTestId("issue-page-edit-save")).toBeNull();
    await user.click(screen.getByTestId("issue-page-edit-review-button"));

    const review = screen.getByTestId("issue-page-edit-review");
    expect(review).toHaveTextContent(
      "saved to acme/api#12 on GitHub as @alex-gh",
    );
    expect(review).toHaveTextContent("The search index is stale. Rebuild it.");
    expect(review).toHaveTextContent("keeps its hidden reference line");
    expect(review).toHaveTextContent("(none)");
    await user.click(screen.getByTestId("issue-page-edit-save"));

    await waitFor(() =>
      expect(edits).toEqual([
        {
          body: "The search index is stale. Rebuild it.",
          assignees: [],
          seen: {
            body: "The search index is stale.",
            assignees: ["octocat"],
          },
        },
      ]),
    );
    expect(
      await screen.findByTestId("issue-page-edit-partial"),
    ).toHaveTextContent("Still assigned: @octocat.");
    expect(screen.queryByTestId("issue-page-edit-form")).toBeNull();
  });

  it("keeps the form when GitHub does not confirm an edit, and saves the same change again", async () => {
    const user = userEvent.setup();
    const edits: unknown[] = [];
    serveIssue();
    serveWriteTarget({ contentTrusted: true });
    server.use(
      http.patch(DETAIL_URL, async ({ request }) => {
        edits.push(await request.json());
        return HttpResponse.json({
          data:
            edits.length === 1
              ? makeIssueEdit({ outcome: "unknown", issue: null })
              : makeIssueEdit(),
        });
      }),
    );
    renderPage();

    await user.click(await screen.findByTestId("issue-page-edit-button"));
    await user.type(screen.getByTestId("issue-page-edit-title"), "!");
    await user.click(screen.getByTestId("issue-page-edit-save"));
    expect(
      await screen.findByTestId("issue-page-edit-unconfirmed"),
    ).toHaveTextContent("saving again is safe");
    expect(screen.getByTestId("issue-page-edit-form")).toBeVisible();

    await user.click(screen.getByTestId("issue-page-edit-save"));
    await waitFor(() =>
      expect(screen.queryByTestId("issue-page-edit-form")).toBeNull(),
    );
    expect(edits).toHaveLength(2);
    expect(edits[1]).toEqual(edits[0]);
  });

  it("closes the issue on GitHub and says when GitHub did not confirm", async () => {
    const user = userEvent.setup();
    const states: unknown[] = [];
    serveIssue();
    server.use(
      http.post(`${DETAIL_URL}/state`, async ({ request }) => {
        states.push(await request.json());
        return HttpResponse.json({
          data: {
            requested: "closed",
            outcome: "unknown",
            issue: null,
            githubLogin: "alex-gh",
            message: "GitHub did not respond",
          },
        });
      }),
    );
    renderPage();

    const button = await screen.findByTestId("issue-page-state-button");
    await waitFor(() => expect(button).toBeEnabled());
    expect(button).toHaveTextContent("Close issue");
    await user.click(button);
    await waitFor(() => expect(states).toEqual([{ state: "closed" }]));
    expect(
      await screen.findByTestId("issue-page-state-unconfirmed"),
    ).toHaveTextContent("The change was not confirmed.");
  });

  it("reviews a comment first and keeps its draft and key when GitHub does not confirm", async () => {
    const user = userEvent.setup();
    const posts: Array<Record<string, unknown>> = [];
    serveIssue();
    server.use(
      http.post(`${DETAIL_URL}/comments`, async ({ request }) => {
        posts.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json({
          data: makeGithubCommentResult({
            state: "outcome_unknown",
            externalCommentUrl: null,
            publishedAt: null,
          }),
        });
      }),
    );
    renderPage();

    await user.type(
      await screen.findByTestId("activity-comment-input"),
      "Fixed by the rebuild.",
    );
    await user.click(screen.getByTestId("activity-comment-review-button"));
    expect(screen.getByTestId("activity-comment-review")).toHaveTextContent(
      "posted to acme/api#12 as @alex-gh",
    );
    await user.click(screen.getByTestId("activity-comment-post"));
    expect(
      await screen.findByTestId("activity-comment-unconfirmed"),
    ).toHaveTextContent("posting again is safe");
    await user.click(screen.getByTestId("activity-comment-post"));
    await waitFor(() => expect(posts).toHaveLength(2));
    expect(posts[0]).toEqual({
      body: "Fixed by the rebuild.",
      idempotencyKey: expect.any(String),
    });
    expect(posts[1]).toEqual(posts[0]);
  });

  it("edits only the viewer's own comments, by GitHub login, and handles refusals", async () => {
    const user = userEvent.setup();
    const edits: Array<Record<string, unknown>> = [];
    const detail = makeIssueDetail();
    detail.comments[1] = {
      ...detail.comments[1],
      author: { login: "Alex-GH", avatarUrl: null },
      body: "Second comment\n\n<!-- g5-comment cbc40edc-c5ed-4f83-ba43-85069252d6b6 -->",
    };
    server.use(
      http.get(DETAIL_URL, () => HttpResponse.json({ data: detail })),
      http.patch(
        `${DETAIL_URL}/comments/:commentId`,
        async ({ request, params }) => {
          edits.push({
            commentId: params.commentId,
            ...((await request.json()) as Record<string, unknown>),
          });
          return edits.length === 1
            ? issueConflictResponse("comment_changed")
            : issueConflictResponse("not_your_comment");
        },
      ),
    );
    renderPage();

    await user.click(await screen.findByTestId("issue-page-comment-edit-c2"));
    expect(screen.queryByTestId("issue-page-comment-edit-c1")).toBeNull();
    const input = screen.getByTestId("issue-page-comment-edit-input");
    expect(input).toHaveValue("Second comment");
    await user.type(input, " (edited)");
    await user.click(
      screen.getByTestId("issue-page-comment-edit-review-button"),
    );
    expect(
      screen.getByTestId("issue-page-comment-edit-review"),
    ).toHaveTextContent("as @alex-gh. G5 keeps its hidden reference line");
    await user.click(screen.getByTestId("issue-page-comment-edit-save"));
    expect(
      await screen.findByTestId("issue-page-comment-edit-error"),
    ).toHaveTextContent("This comment changed since you opened the editor.");
    expect(
      screen.getByTestId("issue-page-comment-edit-error-reload"),
    ).toBeInTheDocument();
    expect(edits[0]).toEqual({
      commentId: "c2",
      body: "Second comment (edited)",
      expectedUpdatedAt: "2026-09-22T12:00:00Z",
    });

    await user.click(screen.getByTestId("issue-page-comment-edit-save"));
    expect(
      await screen.findByText("Only your own comments can be edited."),
    ).toBeVisible();
  });

  it("offers the GitHub connection and disables writes until connected", async () => {
    serveIssue();
    serveWriteTarget({ state: "connection_required", githubLogin: null });
    renderPage();

    const connect = await screen.findByTestId("issue-page-connect");
    expect(connect).toHaveAttribute(
      "href",
      `/account/github?returnTo=${encodeURIComponent(window.location.pathname + window.location.search)}`,
    );
    expect(screen.getByTestId("issue-page-edit-button")).toBeDisabled();
    expect(screen.getByTestId("issue-page-state-button")).toBeDisabled();
    expect(screen.getByTestId("activity-comment-connect")).toBeVisible();
    // No own comment can be recognized without a connected account.
    expect(screen.queryByTestId(/^issue-page-comment-edit-/)).toBeNull();
  });

  it("explains a blocked repository and keeps writes disabled", async () => {
    serveIssue();
    serveWriteTarget({ state: "public_destination" });
    renderPage();

    await waitFor(() =>
      expect(screen.getByTestId("issue-page-write-target")).toHaveTextContent(
        "G5 does not change issues in public repositories.",
      ),
    );
    expect(screen.getByTestId("issue-page-edit-button")).toBeDisabled();
    expect(screen.getByTestId("issue-page-state-button")).toBeDisabled();
    expect(screen.getByTestId("activity-comment-blocked")).toHaveTextContent(
      "Read-only integration: public destination",
    );
  });

  it("shows viewers the issue without write controls or a write-target read", async () => {
    serveIssue();
    const reads = serveWriteTarget();
    renderPage(VIEWER);

    expect(await screen.findByTestId("external-issue-detail")).toBeVisible();
    expect(screen.queryByTestId("issue-page-edit-button")).toBeNull();
    expect(screen.queryByTestId("issue-page-state-button")).toBeNull();
    expect(screen.queryByTestId("activity-comment-composer")).toBeNull();
    expect(screen.queryByTestId("issue-page-write-target")).toBeNull();
    expect(reads()).toBe(0);
  });
});

describe("IssuePage — native G5 issue", () => {
  const NATIVE_URL = "/api/v1/orgs/org-g5labs/issues/g5/12";

  function renderNativePage(user = createMockUser()) {
    return renderApp(
      <Routes>
        <Route
          path="/:org/issues/g5/:number"
          element={
            <RequireAuth>
              <IssuePage />
            </RequireAuth>
          }
        />
      </Routes>,
      { route: "/g5labs/issues/g5/12", user },
    );
  }

  beforeEach(() => {
    // No GitHub App: only the always-on `issues` feature.
    serveNativeIssuesFeature();
    server.use(
      http.get(NATIVE_URL, () =>
        HttpResponse.json({ data: makeNativeIssueDetail() }),
      ),
      http.get("/api/v1/orgs/org-g5labs/members", () =>
        HttpResponse.json({
          data: [
            {
              ...mockOrgMembers[0],
              userId: "user-alex",
              email: NATIVE_ALEX.login,
              displayName: "Alex Chen",
              role: "admin",
              deactivatedAt: null,
            },
            {
              ...mockOrgMembers[0],
              userId: "user-maya",
              email: NATIVE_MAYA.login,
              displayName: "Maya Patel",
              role: "member",
              deactivatedAt: null,
            },
            {
              ...mockOrgMembers[0],
              userId: "user-vic",
              email: "vic@g5labs.dev",
              displayName: "Vic Viewer",
              role: "viewer",
              deactivatedAt: null,
            },
          ],
        }),
      ),
    );
  });

  it("shows the native issue by member names, with no GitHub link", async () => {
    renderNativePage();

    const detail = await screen.findByTestId("external-issue-detail");
    expect(screen.getByTestId("external-issue-display-key")).toHaveTextContent(
      "ACME-12",
    );
    expect(screen.getByTestId("external-issue-assignees")).toHaveTextContent(
      "Maya Patel",
    );
    expect(detail).toHaveTextContent("Maya Patel opened");
    expect(screen.getByTestId("external-issue-comment-41")).toHaveTextContent(
      "Alex Chen",
    );
    // The page is the issue's own; nothing links to GitHub or to itself.
    expect(
      screen.queryByTestId("external-issue-open-github"),
    ).not.toBeInTheDocument();
    expect(detail).not.toHaveTextContent("GitHub");
  });

  it("comments with an idempotency key and edits only the viewer's own comment", async () => {
    const user = userEvent.setup();
    const posted: Record<string, unknown>[] = [];
    const edited: Record<string, unknown>[] = [];
    server.use(
      http.post(`${NATIVE_URL}/comments`, async ({ request }) => {
        posted.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json(
          {
            data: makeNativeCommentResult(makeNativeIssueDetail().comments[0]),
          },
          { status: 201 },
        );
      }),
      http.patch(
        `${NATIVE_URL}/comments/:commentId`,
        async ({ request, params }) => {
          edited.push({
            commentId: params.commentId,
            ...((await request.json()) as Record<string, unknown>),
          });
          return HttpResponse.json({
            data: makeNativeCommentResult(
              makeNativeIssueDetail().comments[0],
              "edit",
            ),
          });
        },
      ),
    );
    renderNativePage();

    await user.type(
      await screen.findByTestId("activity-comment-input"),
      "Fixed in the index rebuild.",
    );
    await user.click(screen.getByTestId("activity-comment-post"));
    await waitFor(() => expect(posted).toHaveLength(1));
    expect(posted[0]).toEqual({
      body: "Fixed in the index rebuild.",
      idempotencyKey: expect.any(String),
    });
    await waitFor(() =>
      expect(screen.getByTestId("activity-comment-input")).toHaveValue(""),
    );

    // Alex wrote comment 41; Maya's 42 offers no Edit.
    expect(
      screen.queryByTestId("issue-page-comment-edit-42"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("issue-page-comment-edit-41"));
    const input = screen.getByTestId("issue-page-comment-edit-input");
    await user.clear(input);
    await user.type(input, "I can reproduce it on main.");
    await user.click(screen.getByTestId("issue-page-comment-edit-save"));
    await waitFor(() =>
      expect(edited).toEqual([
        {
          commentId: "41",
          body: "I can reproduce it on main.",
          expectedUpdatedAt: "2026-10-19T13:00:00.123456Z",
        },
      ]),
    );
  });

  it("closes the issue in G5 without a GitHub connection", async () => {
    const user = userEvent.setup();
    const states: unknown[] = [];
    server.use(
      http.post(`${NATIVE_URL}/state`, async ({ request }) => {
        states.push(await request.json());
        return HttpResponse.json({
          data: {
            ...makeIssueStateChange("closed", makeNativeIssueSummary()),
            githubLogin: null,
          },
        });
      }),
    );
    renderNativePage();

    await user.click(await screen.findByTestId("issue-page-state-button"));
    await waitFor(() => expect(states).toEqual([{ state: "closed" }]));
  });

  it("edits title and assignees at the version read, and asks for a reload when the issue changed", async () => {
    const user = userEvent.setup();
    const edits: unknown[] = [];
    server.use(
      http.patch(NATIVE_URL, async ({ request }) => {
        edits.push(await request.json());
        return issueConflictResponse("issue_changed");
      }),
    );
    renderNativePage();

    await user.click(await screen.findByTestId("issue-page-edit-button"));
    const title = screen.getByTestId("issue-page-edit-title");
    await user.clear(title);
    await user.type(title, "Native search is slow on large orgs");
    // Members who can write are offered; viewers are not.
    await user.click(
      await screen.findByTestId("issue-page-edit-assignee-alex@g5labs.dev"),
    );
    expect(
      screen.queryByTestId("issue-page-edit-assignee-vic@g5labs.dev"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("issue-page-edit-save"));

    await waitFor(() =>
      expect(edits).toEqual([
        {
          title: "Native search is slow on large orgs",
          assignees: ["maya@g5labs.dev", "alex@g5labs.dev"],
          expectedUpdatedAt: "2026-10-20T12:00:00.123456Z",
        },
      ]),
    );
    expect(
      await screen.findByTestId("issue-page-edit-error"),
    ).toHaveTextContent("This issue changed since you opened it.");
    expect(
      screen.getByTestId("issue-page-edit-error-reload"),
    ).toBeInTheDocument();
  });

  it("shows viewers the issue without write controls", async () => {
    renderNativePage(VIEWER);

    expect(await screen.findByTestId("external-issue-detail")).toBeVisible();
    expect(
      screen.queryByTestId("issue-page-edit-button"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("issue-page-state-button"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("activity-comment-composer"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("issue-page-comment-edit-41"),
    ).not.toBeInTheDocument();
  });
});

describe("IssuePage — labels", () => {
  const NATIVE_URL = "/api/v1/orgs/org-g5labs/issues/g5/12";

  function renderNativePage(user = createMockUser()) {
    return renderApp(
      <Routes>
        <Route
          path="/:org/issues/g5/:number"
          element={
            <RequireAuth>
              <IssuePage />
            </RequireAuth>
          }
        />
      </Routes>,
      { route: "/g5labs/issues/g5/12", user },
    );
  }

  type Labels = { name: string; color: string | null }[];

  /** The issue's labels as the server reads them; a change updates them. */
  const served = { labels: [] as Labels };

  function serveNativeDetail(labels: Labels) {
    served.labels = labels;
    server.use(
      http.get(NATIVE_URL, () =>
        HttpResponse.json({
          data: makeNativeIssueDetail({ labels: served.labels }),
        }),
      ),
    );
  }

  function serveGithubDetail(labels: Labels) {
    served.labels = labels;
    server.use(
      http.get(DETAIL_URL, () =>
        HttpResponse.json({ data: makeIssueDetail({ labels: served.labels }) }),
      ),
    );
  }

  function serveLabelChange(
    url: string,
    answer: (body: { addLabels: string[]; removeLabels: string[] }) => Response,
  ) {
    const bodies: unknown[] = [];
    server.use(
      http.post(`${url}/labels`, async ({ request }) => {
        const body = (await request.json()) as {
          addLabels: string[];
          removeLabels: string[];
        };
        bodies.push(body);
        const response = answer(body);
        const { data } = (await response.clone().json()) as {
          data?: { issue?: { labels: Labels } | null };
        };
        if (data?.issue) served.labels = data.issue.labels;
        return response;
      }),
    );
    return bodies;
  }

  function shownLabels() {
    return Array.from(
      screen.getByTestId("external-issue-labels").querySelectorAll("li"),
    ).map((item) => item.textContent);
  }

  it("sends a native issue's label changes as deltas from what the editor started from", async () => {
    serveNativeIssuesFeature();
    serveLabelCatalog(
      "domain-g5",
      makeLabelPage({ domainId: "domain-g5", displayKey: "ACME" }),
    );
    serveNativeDetail([
      { name: "bug", color: "d73a4a" },
      { name: "wip", color: null },
    ]);
    const bodies = serveLabelChange(NATIVE_URL, () =>
      HttpResponse.json({
        data: makeIssueLabelChange({
          githubLogin: null,
          changes: [
            { label: "needs triage", change: "add", outcome: "applied" },
            { label: "wip", change: "remove", outcome: "applied" },
          ],
          issue: makeNativeIssueSummary({
            labels: [
              { name: "bug", color: "d73a4a" },
              { name: "docs", color: null },
              { name: "needs triage", color: "fbca04" },
            ],
          }),
        }),
      }),
    );
    const user = userEvent.setup();
    const { queryClient } = renderNativePage();

    expect(
      await screen.findByTestId("issue-page-labels-edit"),
    ).toHaveTextContent("Edit labels");
    expect(shownLabels()).toEqual(["bug", "wip"]);
    await user.click(screen.getByTestId("issue-page-labels-edit"));
    expect(await screen.findByTestId("label-picker-option-wip")).toBeChecked();

    // Someone else adds `docs` while the editor is open; the next read of
    // the issue must not turn that into a removal.
    serveNativeDetail([
      { name: "bug", color: "d73a4a" },
      { name: "wip", color: null },
      { name: "docs", color: null },
    ]);
    await queryClient.refetchQueries({ queryKey: ["external-issue"] });

    await user.click(screen.getByTestId("label-picker-option-wip"));
    await user.click(screen.getByTestId("label-picker-option-needs triage"));
    await user.click(screen.getByTestId("issue-page-labels-save"));

    await waitFor(() =>
      expect(bodies).toEqual([
        { addLabels: ["needs triage"], removeLabels: ["wip"] },
      ]),
    );
    await waitFor(() =>
      expect(shownLabels()).toEqual(["bug", "docs", "needs triage"]),
    );
    expect(
      screen.queryByTestId("issue-page-labels-outcome"),
    ).not.toBeInTheDocument();
  });

  it("explains a refused change and keeps the selection", async () => {
    serveNativeIssuesFeature();
    serveLabelCatalog(
      "domain-g5",
      makeLabelPage({ domainId: "domain-g5", displayKey: "ACME" }),
    );
    serveNativeDetail([]);
    serveLabelChange(NATIVE_URL, () =>
      labelValidationResponse(
        "unknown_label",
        "wip is not in ACME's labels; create it with `g5d issue label create` or in the label catalog first",
      ),
    );
    const user = userEvent.setup();
    renderNativePage();

    await user.click(await screen.findByTestId("issue-page-labels-edit"));
    await user.click(await screen.findByTestId("label-picker-option-wip"));
    await user.click(screen.getByTestId("issue-page-labels-save"));

    expect(
      await screen.findByTestId("issue-page-labels-error"),
    ).toHaveTextContent(
      "Wip is not in ACME's labels. Only existing labels can be added; create labels in Manage labels.",
    );
    expect(screen.getByTestId("label-picker-option-wip")).toBeChecked();
  });

  it("reviews an untrusted GitHub domain's exact changes and lists a partial result", async () => {
    serveGithubDetail([
      { name: "bug", color: "d73a4a" },
      { name: "wip", color: null },
    ]);
    serveLabelCatalog();
    const bodies = serveLabelChange(DETAIL_URL, () =>
      HttpResponse.json({
        data: makeIssueLabelChange({
          outcome: "partial",
          changes: [
            { label: "needs triage", change: "add", outcome: "applied" },
            {
              label: "bug",
              change: "remove",
              outcome: "refused",
              message: "Must have triage access",
            },
            { label: "wip", change: "remove", outcome: "not_attempted" },
          ],
          issue: makeIssueSummary({
            labels: [
              { name: "bug", color: "d73a4a" },
              { name: "needs triage", color: "fbca04" },
              { name: "wip", color: null },
            ],
          }),
        }),
      }),
    );
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByTestId("issue-page-labels-edit"));
    await user.click(await screen.findByTestId("label-picker-option-bug"));
    await user.click(screen.getByTestId("label-picker-option-wip"));
    await user.click(screen.getByTestId("label-picker-option-needs triage"));
    await user.click(screen.getByTestId("issue-page-labels-review-button"));

    const review = screen.getByTestId("issue-page-labels-review");
    expect(review).toHaveTextContent(
      "These label changes will be saved to acme/api#12 on GitHub as @alex-gh.",
    );
    expect(
      screen.getByTestId("issue-page-labels-review-add"),
    ).toHaveTextContent("needs triage");
    expect(
      screen.getByTestId("issue-page-labels-review-remove").textContent,
    ).toBe("bug\nwip");
    expect(bodies).toEqual([]);
    await user.click(screen.getByTestId("issue-page-labels-save"));

    const outcome = await screen.findByTestId("issue-page-labels-outcome");
    expect(bodies).toEqual([
      { addLabels: ["needs triage"], removeLabels: ["bug", "wip"] },
    ]);
    expect(outcome).toHaveAttribute("data-outcome", "partial");
    expect(outcome).toHaveTextContent("Only some label changes were applied.");
    expect(
      screen.getByTestId("issue-page-labels-outcome-add-needs triage"),
    ).toHaveTextContent("Add “needs triage”: applied");
    expect(
      screen.getByTestId("issue-page-labels-outcome-remove-bug"),
    ).toHaveTextContent("Remove “bug”: refused (Must have triage access)");
    expect(
      screen.getByTestId("issue-page-labels-outcome-remove-wip"),
    ).toHaveTextContent("Remove “wip”: not attempted");
    await waitFor(() =>
      expect(shownLabels()).toEqual(["bug", "needs triage", "wip"]),
    );
  });

  it("says when GitHub did not confirm a change", async () => {
    serveWriteTarget({ contentTrusted: true });
    serveGithubDetail([]);
    serveLabelCatalog();
    serveLabelChange(DETAIL_URL, () =>
      HttpResponse.json({
        data: makeIssueLabelChange({
          outcome: "unknown",
          changes: [{ label: "bug", change: "add", outcome: "unknown" }],
          issue: null,
          message: "GitHub timed out",
        }),
      }),
    );
    const user = userEvent.setup();
    renderPage();

    expect(
      await screen.findByTestId("issue-page-labels-edit"),
    ).toHaveTextContent("Add labels");
    await user.click(screen.getByTestId("issue-page-labels-edit"));
    await user.click(await screen.findByTestId("label-picker-option-bug"));
    // A trusted domain saves straight from the picker.
    await user.click(screen.getByTestId("issue-page-labels-save"));

    const outcome = await screen.findByTestId("issue-page-labels-outcome");
    expect(outcome).toHaveAttribute("data-outcome", "unknown");
    expect(outcome).toHaveTextContent(
      "GitHub did not confirm every label change, so some may or may not have applied.",
    );
    expect(outcome).toHaveTextContent(
      "The issue could not be read again; reload to see its labels. GitHub timed out",
    );
    expect(
      screen.getByTestId("issue-page-labels-outcome-add-bug"),
    ).toHaveTextContent("Add “bug”: not confirmed");
  });

  it("disables label changes while the write target blocks them", async () => {
    serveWriteTarget({ state: "comments_disabled", githubLogin: null });
    serveGithubDetail([{ name: "bug", color: "d73a4a" }]);
    renderPage();

    expect(await screen.findByTestId("issue-page-labels-edit")).toBeDisabled();
    expect(screen.getByTestId("issue-page-write-target")).toHaveTextContent(
      "G5 comments and changes are not enabled for this issue's repository.",
    );
  });

  it("shows viewers the labels without a way to change them", async () => {
    serveGithubDetail([{ name: "bug", color: "d73a4a" }]);
    renderPage(VIEWER);

    expect(
      await screen.findByTestId("external-issue-labels"),
    ).toHaveTextContent("bug");
    expect(
      screen.queryByTestId("issue-page-labels-edit"),
    ).not.toBeInTheDocument();
  });
});
