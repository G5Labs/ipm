/**
 * WorkspaceActivityPanel — the primary issue's discussion with workspace
 * context.
 *
 * Covered: entries in order with their source labels and deep links, issue
 * comments rendered as (restricted) Markdown, an issue read failure shown
 * separately while workspace context stays, the no-primary prompt, the
 * truncated-history note, "Load earlier" requesting the previous page and
 * appending it below, ongoing work as badged Current rows on top (ui
 * g5.ui.activity-current), links to the canonical comment entries, and the filter row
 * above the feed (every group listed, client-side hiding, per-person
 * persistence). A native G5
 * primary issue's discussion is labelled G5, links to its issue page, names
 * members by name and offers Edit on the viewer's own comments by G5 user.
 */

import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes, useLocation, useParams } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

import type { ActivityEntryDTO, WorkspaceActivityDTO } from "../../lib/types";
import { activityFiltersKey } from "../../lib/activityFilters";
import { formatDateTime } from "../../lib/dateTime";
import {
  MOCK_WORKSPACE_ACTIVITY_CURRENT,
  createMockUser,
} from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import {
  WorkspaceActivityPanel,
  pullRequestEventLabel,
} from "./WorkspaceActivityPanel";

const WORKSPACE_ID = "ws-1";
const ACTIVITY_URL = `/api/v1/workspaces/${WORKSPACE_ID}/activity`;
const CURRENT_URL = `/api/v1/workspaces/${WORKSPACE_ID}/activity/current`;

const PRIMARY: WorkspaceActivityDTO["primaryIssue"] = {
  linkId: "link-1",
  displayKey: "acme/api#12",
  canonicalUrl: "https://github.com/acme/api/issues/12",
  locator: { repositoryId: "11", number: 12 },
  contentTrusted: false,
};

const ENTRIES: ActivityEntryDTO[] = [
  {
    key: "github-issue:900",
    at: "2025-12-31T10:00:00Z",
    source: "github",
    kind: "issue_opened",
    author: { login: "reporter", avatarUrl: null },
    title: "Session refresh fails",
    body: "Steps: **reconnect** twice.",
    url: "https://github.com/acme/api/issues/12",
  },
  {
    key: "chat:c1",
    at: "2026-01-01T10:00:00Z",
    source: "workspace",
    kind: "chat",
    sessionId: "c1",
    title: "Implementation chat",
    messageCount: 4,
    lastActivityAt: "2026-01-01T11:00:00Z",
  },
  {
    key: "github-comment:7001",
    at: "2026-01-02T10:00:00Z",
    source: "github",
    kind: "issue_comment",
    commentId: "7001",
    updatedAt: "2026-01-02T10:00:00Z",
    author: { login: "reviewer", avatarUrl: null },
    body: "Please **also** cover two disconnects.\n\n![shot](https://example.com/a.png)\n\n<!-- g5-comment cbc40edc-c5ed-4f83-ba43-85069252d6b6 -->",
    url: "https://github.com/acme/api/issues/12#issuecomment-7001",
  },
  {
    key: "github-event:42",
    at: "2026-01-03T10:00:00Z",
    source: "github",
    kind: "issue_event",
    actor: { login: "reviewer", avatarUrl: null },
    event: "labeled",
    detail: "bug",
    stateReason: null,
  },
  {
    key: "task:t3",
    at: "2026-01-04T10:00:00Z",
    source: "workspace",
    kind: "task",
    name: "Validation",
    taskId: "t3",
    taskKind: "command",
    reason: null,
    status: "succeeded",
  },
  {
    key: "task:t1",
    at: "2026-01-05T10:00:00Z",
    source: "workspace",
    kind: "task",
    taskId: "t1",
    name: "Preview",
    taskKind: "preview",
    status: "succeeded",
    reason: null,
  },
  {
    key: "task:t2",
    at: "2026-01-05T10:30:00Z",
    source: "workspace",
    kind: "task",
    taskId: "t2",
    name: "Preview",
    taskKind: "preview",
    status: "cancelled",
    reason: "idle_stop",
  },
  {
    key: "commit:r1:abcdef1234567890",
    at: "2026-01-05T11:00:00Z",
    source: "workspace",
    kind: "commit",
    repoId: "r1",
    repoName: "web",
    branchName: "fix/reconnect",
    sha: "abcdef1234567890",
    shortSha: "abcdef1",
    summary: "Fix reconnect",
    authorName: "Ada Dev",
  },
  {
    key: "approval:a1",
    at: "2026-01-06T10:00:00Z",
    source: "workspace",
    kind: "approval_decision",
    definitionName: "Final review",
    outcome: "approved",
    phase: "ie",
    actorType: "human",
    actorName: "Maya Chen",
    comment: "Looks right",
  },
];

function page(
  overrides: Partial<WorkspaceActivityDTO> = {},
): WorkspaceActivityDTO {
  return {
    primaryIssue: PRIMARY,
    issueError: null,
    issueHistoryTruncated: false,
    entries: ENTRIES,
    composer: null,
    earlierCursor: null,
    ...overrides,
  };
}

function RouteEcho() {
  const { workspace } = useParams();
  const location = useLocation();
  return (
    <div data-testid="route-echo">
      {workspace}
      {location.pathname}
      {location.search}
    </div>
  );
}

function renderPanel(onOpenCommit = vi.fn()) {
  const view = renderApp(
    <Routes>
      <Route
        path="/:org/:workspace"
        element={
          <WorkspaceActivityPanel
            workspaceId={WORKSPACE_ID}
            onOpenCommit={onOpenCommit}
          />
        }
      />
      <Route path="/:org/:workspace/task/:taskId" element={<RouteEcho />} />
    </Routes>,
    { route: "/g5labs/current", user: createMockUser() },
  );
  return { ...view, onOpenCommit };
}

describe("WorkspaceActivityPanel", () => {
  const messageThread = {
    id: "message-thread",
    workspaceId: WORKSPACE_ID,
    authorId: "reviewer",
    authorName: "Maya Chen",
    scope: "workspace",
    content: "Please preserve this context.",
    createdAt: "2026-01-06T10:00:00Z",
    resolved: false,
  };
  const approval = ENTRIES.find((entry) => entry.kind === "approval_decision")!;
  const issueComment = ENTRIES.find((entry) => entry.kind === "issue_comment")!;
  const cardCases: {
    entry: ActivityEntryDTO;
    boxed: boolean;
    label: string;
  }[] = [
    {
      label: "workspace comment",
      entry: {
        key: "comment:message-thread",
        kind: "workspace_comment",
        source: "workspace",
        at: messageThread.createdAt,
        comment: messageThread,
      },
      boxed: true,
    },
    {
      label: "workspace reply",
      entry: {
        key: "reply:message-thread",
        kind: "workspace_reply",
        source: "workspace",
        at: "2026-01-06T11:00:00Z",
        commentId: messageThread.id,
        actorName: "Owner",
        content: "Thanks for reviewing.",
        phase: "ie",
      },
      boxed: true,
    },
    ...(["github", "g5"] as const).map((source) => ({
      label: `${source} issue comment`,
      entry: { ...issueComment, source },
      boxed: true,
    })),
    {
      label: "approval decision with a comment",
      entry: approval,
      boxed: true,
    },
    {
      label: "approval decision with a thread",
      entry: { ...approval, comment: null, thread: messageThread },
      boxed: true,
    },
    ...([null, "", " \n\t "] as const).map((comment) => ({
      label: `approval decision without written content (${JSON.stringify(comment)})`,
      entry: { ...approval, comment },
      boxed: false,
    })),
    ...ENTRIES.filter((entry) =>
      ["issue_opened", "commit", "task"].includes(entry.kind),
    ).map((entry) => ({
      label: `${entry.kind} event (${entry.key})`,
      entry,
      boxed: false,
    })),
  ];

  it.each(cardCases)(
    "boxes only person-written messages: $label",
    async ({ entry, boxed }) => {
      server.use(
        http.get(ACTIVITY_URL, () =>
          HttpResponse.json({ data: page({ entries: [entry] }) }),
        ),
      );
      renderPanel();
      const row = await screen.findByTestId(`activity-entry-${entry.key}`);
      const article = within(row).getByRole("article");
      expect(within(row).getAllByRole("article")).toHaveLength(1);
      const card = article.querySelector('[data-card="true"]');
      if (boxed) expect(card).toBeVisible();
      else expect(card).toBeNull();
    },
  );

  it("refreshes an open timeline for a changed review snapshot, but not unchanged polls", async () => {
    let reads = 0;
    let decided = false;
    server.use(
      http.get(ACTIVITY_URL, () => {
        reads += 1;
        return HttpResponse.json({
          data: page({ entries: decided ? ENTRIES : [] }),
        });
      }),
    );
    const { queryClient } = renderPanel();
    await screen.findByTestId("workspace-activity-empty");
    await waitFor(() =>
      expect(
        queryClient.getQueryData(["workspace-review-flow", WORKSPACE_ID]),
      ).toBeDefined(),
    );
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["workspace-review-flow", WORKSPACE_ID],
      });
    });
    const unchangedReads = reads;
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["workspace-review-flow", WORKSPACE_ID],
      });
    });
    expect(reads).toBe(unchangedReads);
    decided = true;
    await act(async () => {
      queryClient.setQueryData(
        ["workspace-review-flow", WORKSPACE_ID],
        (previous: import("../../lib/types").ReviewOverview) => ({
          ...previous,
          decisions: [
            {
              id: "new-decision",
              requestId: "request-1",
              definitionId: "definition-1",
              name: "Final review",
              phase: "ie",
              actorUserId: "reviewer",
              outcome: "approved",
              comment: "Looks right",
              reviewedGitHeads: [],
              createdAt: "2026-01-06T10:00:00Z",
            },
          ],
        }),
      );
    });
    await screen.findByText("Looks right");
    expect(reads).toBeGreaterThan(unchangedReads);
  });

  it.each([null, "submitted-pr"])(
    "preserves approval evidence and identifies only unlinked legacy decisions (%s)",
    async (pullRequestId) => {
      const decision = ENTRIES.find(
        (entry) => entry.kind === "approval_decision",
      )!;
      server.use(
        http.get(ACTIVITY_URL, () =>
          HttpResponse.json({
            data: page({
              entries: [
                {
                  ...decision,
                  kind: "approval_decision",
                  actorType: "human",
                  pullRequestId,
                },
              ],
            }),
          }),
        ),
      );
      renderPanel();
      await screen.findByTestId(`activity-entry-${decision.key}`);
      const annotation = screen.queryByText(
        "Historical approval · not linked to a PR",
      );
      if (pullRequestId === null) expect(annotation).toBeVisible();
      else expect(annotation).toBeNull();
    },
  );

  it("puts the decision's summary under its headline, never a report clip, and labels only historical decisions", async () => {
    const decision = ENTRIES.find(
      (entry) => entry.kind === "approval_decision",
    )!;
    const base = { ...decision, kind: "approval_decision" as const };
    const reportClip = "## Verdict\n\nRejected. ".repeat(40);
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({
          data: page({
            entries: [
              {
                ...base,
                key: "approval:agent-new",
                outcome: "rejected",
                definitionName: "UI changes",
                actorType: "agent",
                actorName: null,
                comment: reportClip,
                summary: "The new button needs a human UI approval.",
                historical: false,
                pullRequestId: null,
              },
              {
                ...base,
                key: "approval:agent-legacy",
                outcome: "approved",
                definitionName: "Architecture",
                actorType: "agent",
                actorName: "Architecture agent",
                comment: reportClip,
                summary: null,
                pullRequestId: null,
              },
              {
                ...base,
                key: "approval:human-no-pr",
                outcome: "deferred",
                definitionName: "Documentation",
                actorName: "Ben",
                comment: null,
                pullRequestId: null,
                historical: false,
              },
              {
                ...base,
                key: "approval:human-legacy",
                outcome: "approved",
                definitionName: "Security review",
                actorName: "Kaihui Zheng",
                comment: "Checked the session flow.",
                pullRequestId: "pr-1",
                historical: true,
              },
            ],
          }),
        }),
      ),
    );
    renderPanel();
    const agent = await screen.findByTestId(
      "activity-entry-approval:agent-new",
    );
    expect(
      within(agent).getByRole("link", {
        name: "Agent reviewer rejected UI changes",
      }),
    ).toHaveAttribute("href", expect.stringContaining("view=approvals"));
    expect(
      within(agent).getByTestId("activity-approval-summary-approval:agent-new"),
    ).toHaveTextContent(/^The new button needs a human UI approval\.$/);
    expect(agent).not.toHaveTextContent("Verdict");
    // A new no-PR decision is not historical.
    expect(agent).not.toHaveTextContent("Historical approval");

    // A legacy agent decision has no summary: no excerpt of its report.
    const legacyAgent = screen.getByTestId(
      "activity-entry-approval:agent-legacy",
    );
    expect(legacyAgent).toHaveTextContent(
      "Architecture agent approved Architecture",
    );
    expect(legacyAgent).not.toHaveTextContent("Verdict");
    expect(legacyAgent.querySelector("[data-card]")).toBeNull();

    const withdrawn = screen.getByTestId("activity-entry-approval:human-no-pr");
    expect(
      within(withdrawn).getByRole("link", {
        name: "Ben withdrew response for Documentation",
      }),
    ).toBeVisible();
    expect(withdrawn).not.toHaveTextContent("Historical approval");

    const legacyHuman = screen.getByTestId(
      "activity-entry-approval:human-legacy",
    );
    expect(legacyHuman).toHaveTextContent(
      "Historical approval · not linked to a PR",
    );
    expect(
      within(legacyHuman).getByTestId(
        "activity-approval-summary-approval:human-legacy",
      ),
    ).toHaveTextContent("Checked the session flow.");
  });

  it("links a decision reply back to the canonical review thread", async () => {
    const thread = {
      id: "decision-thread",
      workspaceId: WORKSPACE_ID,
      authorId: "reviewer",
      authorName: "Maya Chen",
      scope: "workspace",
      content: "Review reasoning",
      createdAt: "2026-01-06T10:00:00Z",
      resolved: false,
    };
    const decision = ENTRIES.find((e) => e.kind === "approval_decision")!;
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({
          data: page({
            entries: [
              {
                ...decision,
                kind: "approval_decision",
                outcome: "approved",
                phase: "ie",
                actorType: "human",
                thread,
              },
              {
                key: "reply:1",
                kind: "workspace_reply",
                at: "2026-01-06T11:00:00Z",
                source: "workspace",
                commentId: thread.id,
                content: "Thanks for reviewing",
                actorName: "Owner",
                phase: "ie",
              },
            ],
          }),
        }),
      ),
      http.get(`/api/v1/workspaces/${WORKSPACE_ID}/comments`, () =>
        HttpResponse.json({ data: [thread] }),
      ),
      http.get(`/api/v1/comments/${thread.id}/replies`, () =>
        HttpResponse.json({
          data: [
            {
              id: "reply-1",
              authorName: "Owner",
              content: "Thanks for reviewing",
            },
          ],
        }),
      ),
    );
    const { queryClient } = renderPanel();
    queryClient.setQueryData(
      ["workspace-review-replies", thread.id],
      [{ id: "reply-1", authorName: "Owner", content: "Thanks for reviewing" }],
    );
    const reply = await screen.findByRole("button", { name: "Owner replied" });
    expect(screen.getAllByText("Thanks for reviewing")).toHaveLength(1);
    await userEvent.click(reply);
    const reader = await screen.findByTestId("activity-thread-reader");
    expect(reader).toHaveTextContent("Review reasoning");
    await userEvent.click(
      within(reader).getByRole("button", { name: "Reply" }),
    );
    await waitFor(() =>
      expect(reader).toHaveTextContent("Thanks for reviewing"),
    );
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    await userEvent.click(
      within(screen.getByTestId("activity-entry-reply:1")).getByRole("button", {
        name: "Reply",
      }),
    );
    const input = await screen.findByRole("textbox", { name: "Write a reply" });
    await waitFor(() => expect(input).toHaveFocus());
    expect(screen.getByTestId("activity-thread-reader")).toHaveTextContent(
      "Review reasoning",
    );
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    await userEvent.click(
      within(screen.getByTestId(`activity-entry-${decision.key}`)).getByRole(
        "button",
        { name: "Reply" },
      ),
    );
    await waitFor(() =>
      expect(
        screen.getByRole("textbox", { name: "Write a reply" }),
      ).toHaveFocus(),
    );
  });

  it("interleaves compact issue and workspace events without redundant navigation", async () => {
    server.use(
      http.get(ACTIVITY_URL, () => HttpResponse.json({ data: page() })),
    );
    const { onOpenCommit } = renderPanel();

    const list = await screen.findByTestId("workspace-activity-entries");
    const rows = within(list).getAllByRole("listitem");
    expect(rows.map((row) => row.getAttribute("data-testid"))).toEqual(
      ENTRIES.map((entry) => `activity-entry-${entry.key}`),
    );
    const panel = screen.getByTestId("workspace-activity-panel");
    expect(panel).not.toHaveTextContent(
      /Shared discussion|Reviews and comments|Workspace context|Selected:|Proposed interaction:|Comment on .* in Issues/,
    );
    expect(screen.queryByTestId("workspace-activity-open-issue")).toBeNull();
    const opened = screen.getByTestId("activity-entry-github-issue:900");
    expect(
      within(opened).getByRole("link", { name: "reporter opened acme/api#12" }),
    ).toHaveAttribute("href", PRIMARY!.canonicalUrl);
    expect(within(opened).getByRole("link")).toHaveAttribute(
      "target",
      "_blank",
    );
    expect(opened).not.toHaveTextContent("Steps:");
    expect(opened).not.toHaveTextContent("Session refresh fails");

    const chat = screen.getByTestId("activity-entry-chat:c1");
    expect(chat).toHaveTextContent("Implementation chat");
    expect(chat).toHaveTextContent("4 messages");
    expect(chat).not.toHaveTextContent("last active");
    expect(
      within(chat).getByRole("link", { name: "Implementation chat" }),
    ).toHaveAttribute("href", "/g5labs/current?session=c1");

    const comment = screen.getByTestId("activity-entry-github-comment:7001");
    expect(comment).toHaveTextContent("reviewer commented");
    expect(within(comment).getByText("also").tagName).toBe("STRONG");
    // Untrusted domain content: images are links, never loaded.
    expect(
      within(comment).queryByRole("img", { name: "shot" }),
    ).not.toBeInTheDocument();
    expect(comment).not.toHaveTextContent("GitHub comment");
    expect(
      within(comment).getByRole("img", { name: "Source: GitHub" }),
    ).toBeVisible();
    expect(
      within(comment).getByRole("link", {
        name: formatDateTime(ENTRIES[2].at),
      }),
    ).toHaveAttribute(
      "href",
      ENTRIES[2].kind === "issue_comment" ? ENTRIES[2].url : "",
    );
    expect(
      within(comment).getByRole("link", {
        name: formatDateTime(ENTRIES[2].at),
      }),
    ).toHaveAttribute("target", "_blank");
    // GitHub hides HTML comments, including G5's reference line.
    expect(comment).not.toHaveTextContent("g5-comment");

    expect(
      screen.getByTestId("activity-entry-github-event:42"),
    ).toHaveTextContent("reviewer added the label bug");
    expect(screen.getByTestId("activity-entry-task:t1")).toHaveTextContent(
      "Preview succeeded",
    );
    expect(screen.getByTestId("activity-entry-task:t1")).not.toHaveTextContent(
      "Stopped",
    );
    const idleStopped = screen.getByTestId("activity-entry-task:t2");
    expect(idleStopped).toHaveTextContent("Preview cancelled");
    expect(idleStopped).toHaveTextContent(
      "Stopped because the workspace went idle",
    );
    const commit = screen.getByTestId(
      "activity-entry-commit:r1:abcdef1234567890",
    );
    expect(commit).toHaveTextContent("Fix reconnect");
    expect(commit).toHaveTextContent("abcdef1 · Ada Dev · web/fix/reconnect");
    await userEvent
      .setup()
      .click(within(commit).getByRole("button", { name: "Fix reconnect" }));
    expect(onOpenCommit).toHaveBeenCalledWith(
      expect.objectContaining({ sha: "abcdef1234567890", repoId: "r1" }),
    );

    const approval = screen.getByTestId("activity-entry-approval:a1");
    expect(approval).toHaveTextContent("Maya Chen approved Final review");
    expect(approval).toHaveTextContent("Looks right");
    expect(
      within(approval).getByRole("link", {
        name: "Maya Chen approved Final review",
      }),
    ).toHaveAttribute("href", "/g5labs/current?view=approvals");

    const user = userEvent.setup();
    await user.click(
      within(screen.getByTestId("activity-entry-task:t3")).getByRole("link", {
        name: "Validation succeeded",
      }),
    );
    expect(await screen.findByTestId("route-echo")).toHaveTextContent(
      "/g5labs/current/task/t3",
    );
  });

  it("reports an issue read failure separately and keeps workspace context", async () => {
    let calls = 0;
    server.use(
      http.get(ACTIVITY_URL, () => {
        calls += 1;
        return HttpResponse.json({
          data:
            calls === 1
              ? page({
                  issueError: {
                    issueError: "unavailable",
                    message: "GitHub timed out",
                  },
                  entries: ENTRIES.filter(
                    (entry) => entry.source === "workspace",
                  ),
                })
              : page(),
        });
      }),
    );
    renderPanel();
    const user = userEvent.setup();

    const error = await screen.findByTestId("workspace-activity-issue-error");
    expect(error).toHaveTextContent(
      "The issue tracker did not respond in time",
    );
    expect(error).toHaveTextContent("Workspace context is still shown below.");
    expect(screen.getByTestId("activity-entry-chat:c1")).toBeVisible();
    expect(
      screen.queryByTestId("activity-entry-github-comment:7001"),
    ).not.toBeInTheDocument();

    await user.click(
      screen.getByTestId("workspace-activity-issue-error-retry"),
    );
    expect(
      await screen.findByTestId("activity-entry-github-comment:7001"),
    ).toBeVisible();
    expect(
      screen.queryByTestId("workspace-activity-issue-error"),
    ).not.toBeInTheDocument();
  });

  it("keeps native workspace activity without a redundant issue-link prompt", async () => {
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({
          data: page({
            primaryIssue: null,
            entries: ENTRIES.filter((e) => e.source === "workspace"),
          }),
        }),
      ),
    );
    renderPanel();
    await screen.findByTestId("workspace-activity-entries");
    expect(screen.queryByTestId("workspace-activity-no-primary")).toBeNull();
    expect(screen.queryByTestId("workspace-activity-link-issue")).toBeNull();
  });

  it("offers Edit only on the connected person's own comments", async () => {
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({
          data: page({
            composer: {
              state: "ready",
              githubLogin: "Reviewer",
              destination: { trackerId: "tracker-1", externalIssueId: "9001" },
              displayKey: "acme/api#12",
            },
          }),
        }),
      ),
    );
    renderPanel();
    const user = userEvent.setup();

    const edit = await screen.findByTestId("activity-comment-edit-7001");
    // Only the comment authored by the connected account (login compared
    // case-insensitively) is editable; the issue description is not.
    expect(
      screen
        .getAllByTestId(/^activity-comment-edit-\d+$/)
        .map((button) => button.getAttribute("data-testid")),
    ).toEqual(["activity-comment-edit-7001"]);
    await user.click(edit);
    expect(
      screen.getByTestId("activity-comment-editor-7001"),
    ).toBeInTheDocument();
  });

  it("offers no Edit before GitHub is connected", async () => {
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({
          data: page({
            composer: {
              state: "connection_required",
              githubLogin: null,
              destination: { trackerId: "tracker-1", externalIssueId: "9001" },
              displayKey: "acme/api#12",
            },
          }),
        }),
      ),
    );
    renderPanel();

    await screen.findByTestId("activity-entry-github-comment:7001");
    expect(
      screen.queryByTestId("activity-comment-edit-7001"),
    ).not.toBeInTheDocument();
    await userEvent.click(screen.getByTestId("activity-comment-reply-7001"));
    expect(screen.getByTestId("activity-comment-connect")).toBeEnabled();
    expect(screen.queryByTestId("activity-comment-post")).toBeNull();
    expect(
      (screen.getByTestId("activity-comment-input") as HTMLTextAreaElement)
        .value,
    ).not.toContain("<!-- g5-comment");
  });

  it("notes a truncated issue history", async () => {
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({ data: page({ issueHistoryTruncated: true }) }),
      ),
    );
    renderPanel();

    expect(
      await screen.findByTestId("workspace-activity-truncated"),
    ).toHaveTextContent("only the first 100 issue events");
  });

  it("loads earlier activity below the loaded entries (newest first)", async () => {
    // The API serves pages newest first; the panel renders them as given.
    const newestFirst = [...ENTRIES].reverse();
    const cursors: Array<string | null> = [];
    server.use(
      http.get(ACTIVITY_URL, ({ request }) => {
        const before = new URL(request.url).searchParams.get("before");
        cursors.push(before);
        return HttpResponse.json({
          data: before
            ? page({ entries: newestFirst.slice(6), earlierCursor: null })
            : page({
                entries: newestFirst.slice(0, 6),
                earlierCursor: "2026-01-03T10:00:00.000000Z|github-event:42",
              }),
        });
      }),
    );
    renderPanel();
    const user = userEvent.setup();

    await screen.findByTestId("activity-entry-github-event:42");
    expect(screen.queryByText("Newest first")).toBeNull();
    expect(
      screen.queryByTestId("activity-entry-chat:c1"),
    ).not.toBeInTheDocument();
    // "Load earlier" sits under the list.
    const list = screen.getByTestId("workspace-activity-entries");
    const loadEarlier = screen.getByTestId("workspace-activity-load-earlier");
    expect(
      list.compareDocumentPosition(loadEarlier) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    await user.click(loadEarlier);

    await screen.findByTestId("activity-entry-chat:c1");
    expect(cursors).toEqual([
      null,
      "2026-01-03T10:00:00.000000Z|github-event:42",
    ]);
    const rows = within(screen.getByTestId("workspace-activity-entries"))
      .getAllByRole("listitem")
      .map((row) => row.getAttribute("data-testid"));
    expect(rows).toEqual(
      newestFirst.map((entry) => `activity-entry-${entry.key}`),
    );
    await waitFor(() =>
      expect(
        screen.queryByTestId("workspace-activity-load-earlier"),
      ).not.toBeInTheDocument(),
    );
  });

  describe("ongoing work (g5.ui.activity-current)", () => {
    const CURRENT = MOCK_WORKSPACE_ACTIVITY_CURRENT;

    it("leads the list with badged Current rows, shown once and outside the filters", async () => {
      server.use(
        http.get(ACTIVITY_URL, () => HttpResponse.json({ data: page() })),
        http.get(CURRENT_URL, () =>
          HttpResponse.json({ data: { entries: CURRENT } }),
        ),
      );
      renderPanel();
      const user = userEvent.setup();
      const running = await screen.findByTestId("activity-entry-chat:c9");
      expect(running).toHaveAttribute("data-current", "true");
      expect(
        screen.getByTestId("activity-current-badge-chat:c9"),
      ).toHaveTextContent("Current · Running");
      expect(running).toHaveTextContent("14 messages");
      expect(running.textContent!.match(/running/gi)).toHaveLength(1);
      expect(
        screen.getByTestId("activity-current-badge-chat:c8"),
      ).toHaveTextContent("Current · Waiting");
      expect(
        screen.getByTestId("activity-current-badge-task:t1"),
      ).toHaveTextContent("Current · Queued");
      const rows = within(screen.getByTestId("workspace-activity-entries"))
        .getAllByRole("listitem")
        .map((row) => row.getAttribute("data-testid"));
      // Current first; the record's copy of task:t1 is not repeated.
      expect(rows.slice(0, 3)).toEqual([
        "activity-entry-chat:c9",
        "activity-entry-chat:c8",
        "activity-entry-task:t1",
      ]);
      expect(rows.filter((id) => id === "activity-entry-task:t1")).toHaveLength(
        1,
      );
      expect(rows).toHaveLength(ENTRIES.length + 2);
      // A record chat carries no badge.
      expect(screen.queryByTestId("activity-current-badge-chat:c1")).toBeNull();

      // Isolating Commits still leaves Current conversations/tasks pinned.
      await user.click(screen.getByTestId("activity-filter-commits"));
      expect(screen.getByTestId("activity-entry-chat:c9")).toBeInTheDocument();
      expect(screen.getByTestId("activity-entry-task:t1")).toBeInTheDocument();
      expect(screen.queryByTestId("activity-entry-chat:c1")).toBeNull();
      expect(screen.queryByTestId("activity-entry-task:t3")).toBeNull();
    });

    it("shows the Current rows while the record is still loading", async () => {
      let release: () => void = () => {};
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      server.use(
        http.get(ACTIVITY_URL, async () => {
          await gate;
          return HttpResponse.json({ data: page() });
        }),
        http.get(CURRENT_URL, () =>
          HttpResponse.json({ data: { entries: CURRENT } }),
        ),
      );
      renderPanel();
      await screen.findByTestId("workspace-activity-loading");
      expect(
        await screen.findByTestId("activity-current-badge-chat:c9"),
      ).toHaveTextContent("Current · Running");
      release();
      await screen.findByTestId("activity-entry-chat:c1");
      expect(screen.getAllByTestId("activity-entry-chat:c9")).toHaveLength(1);
    });

    it("keeps the Current rows above a failed record read", async () => {
      server.use(
        http.get(ACTIVITY_URL, () =>
          HttpResponse.json(
            { error: { code: "INTERNAL", message: "down", details: {} } },
            { status: 503 },
          ),
        ),
        http.get(CURRENT_URL, () =>
          HttpResponse.json({ data: { entries: CURRENT } }),
        ),
      );
      renderPanel();
      await screen.findByTestId("workspace-activity-error");
      expect(
        await screen.findByTestId("activity-current-badge-chat:c9"),
      ).toHaveTextContent("Current · Running");
      expect(screen.queryByTestId("activity-entry-chat:c1")).toBeNull();
    });

    it("reports a failed Current read with Retry and keeps the record", async () => {
      let calls = 0;
      server.use(
        http.get(ACTIVITY_URL, () => HttpResponse.json({ data: page() })),
        http.get(CURRENT_URL, () => {
          calls += 1;
          return calls === 1
            ? HttpResponse.json(
                { error: { code: "INTERNAL", message: "down", details: {} } },
                { status: 503 },
              )
            : HttpResponse.json({ data: { entries: CURRENT.slice(0, 1) } });
        }),
      );
      renderPanel();
      const error = await screen.findByTestId(
        "workspace-activity-current-error",
      );
      expect(error).toHaveTextContent("Ongoing work could not be loaded.");
      expect(screen.getByTestId("activity-entry-chat:c1")).toBeInTheDocument();
      await userEvent.click(
        within(error).getByRole("button", { name: "Retry" }),
      );
      await screen.findByTestId("activity-entry-chat:c9");
      expect(
        screen.queryByTestId("workspace-activity-current-error"),
      ).toBeNull();
    });
  });

  it("shows an entry that moved between page loads once, from the newest page", async () => {
    const running = {
      ...ENTRIES[4],
      at: "2025-12-01T10:00:00Z",
      status: "running",
    } as ActivityEntryDTO;
    server.use(
      http.get(ACTIVITY_URL, ({ request }) => {
        const before = new URL(request.url).searchParams.get("before");
        return HttpResponse.json({
          data: before
            ? page({ entries: [running], earlierCursor: null })
            : page({
                entries: [ENTRIES[4]],
                earlierCursor: "2026-01-04T10:00:00.000000Z|task:t3",
              }),
        });
      }),
    );
    renderPanel();
    const user = userEvent.setup();

    await screen.findByTestId("activity-entry-task:t3");
    await user.click(screen.getByTestId("workspace-activity-load-earlier"));
    await waitFor(() =>
      expect(
        screen.queryByTestId("workspace-activity-load-earlier"),
      ).not.toBeInTheDocument(),
    );
    const tasks = screen.getAllByTestId("activity-entry-task:t3");
    expect(tasks).toHaveLength(1);
    expect(tasks[0]).toHaveTextContent("Validation succeeded");
  });

  it("renders a failed activity request as an error with retry", async () => {
    let calls = 0;
    server.use(
      http.get(ACTIVITY_URL, () => {
        calls += 1;
        return calls === 1
          ? HttpResponse.json(
              { error: { code: "INTERNAL", message: "boom", requestId: "" } },
              { status: 500 },
            )
          : HttpResponse.json({ data: page() });
      }),
    );
    renderPanel();
    const user = userEvent.setup();

    await user.click(
      await screen.findByTestId("workspace-activity-error-retry"),
    );
    expect(
      await screen.findByTestId("workspace-activity-entries"),
    ).toBeVisible();
  });

  // Workbench timeline (g5.workbench.landing): pull-request events are rows
  // like any other, each from a durable record; a phase move is served but
  // never shown (2026-10-15).
  it("renders pull-request events with their links and no phase move", async () => {
    const WORKBENCH_ENTRIES: ActivityEntryDTO[] = [
      {
        key: "phase:evt-1",
        at: "2026-02-01T10:00:00Z",
        source: "workspace",
        kind: "phase_changed",
        fromPhase: "be",
        toPhase: "se",
        actor: { userId: "u1", displayName: "Maya Chen" },
      },
      {
        key: "pull-request:evt-2",
        at: "2026-02-02T10:00:00Z",
        source: "workspace",
        kind: "pull_request",
        pullRequestId: "pr-1",
        eventType: "provider_created",
        title: "Ship the fix",
        targetWorkspaceName: "base",
        message: null,
        repositoryCount: 1,
        providerRepoFullName: "acme/web",
        providerPrNumber: 77,
        providerPrUrl: "https://github.com/acme/web/pull/77",
        actor: { userId: "u1", displayName: "Maya Chen" },
        failureCount: null,
        firstFailedAt: null,
      },
      {
        key: "pull-request:evt-3",
        at: "2026-02-03T10:00:00Z",
        source: "workspace",
        kind: "pull_request",
        pullRequestId: "pr-1",
        eventType: "merge_blocked",
        title: "Ship the fix",
        targetWorkspaceName: "base",
        message: "conflict in README.md",
        repositoryCount: 1,
        providerRepoFullName: null,
        providerPrNumber: null,
        providerPrUrl: null,
        actor: null,
        failureCount: null,
        firstFailedAt: null,
      },
      {
        // A multi-repository PR: provider events name the aggregate PR, so
        // no single provider PR is attributed.
        key: "pull-request:evt-4",
        at: "2026-02-04T10:00:00Z",
        source: "workspace",
        kind: "pull_request",
        pullRequestId: "pr-2",
        eventType: "provider_failed",
        title: "Two repos",
        targetWorkspaceName: "base",
        message: "push rejected",
        repositoryCount: 2,
        providerRepoFullName: null,
        providerPrNumber: null,
        providerPrUrl: null,
        actor: null,
        failureCount: 3,
        firstFailedAt: "2026-02-04T09:30:00Z",
      },
    ];
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({
          data: page({ primaryIssue: null, entries: WORKBENCH_ENTRIES }),
        }),
      ),
    );
    const onOpenPullRequests = vi.fn();
    renderApp(
      <Routes>
        <Route
          path="/:org/:workspace"
          element={
            <WorkspaceActivityPanel
              workspaceId={WORKSPACE_ID}
              onOpenCommit={vi.fn()}
              onOpenPullRequests={onOpenPullRequests}
              header={<div data-testid="timeline-header-slot">Header</div>}
            />
          }
        />
      </Routes>,
      { route: "/g5labs/current", user: createMockUser() },
    );

    const published = await screen.findByTestId(
      "activity-entry-pull-request:evt-2",
    );
    expect(screen.queryByTestId("activity-entry-phase:evt-1")).toBeNull();
    expect(published).toHaveTextContent("Published to GitHub: Ship the fix");
    expect(published).toHaveTextContent("into base · acme/web#77 · Maya Chen");
    expect(
      within(published).getByRole("link", {
        name: "Published to GitHub: Ship the fix",
      }),
    ).toHaveAttribute("href", "https://github.com/acme/web/pull/77");
    expect(within(published).getByRole("link")).toHaveAttribute(
      "target",
      "_blank",
    );

    const blocked = screen.getByTestId("activity-entry-pull-request:evt-3");
    expect(blocked).toHaveTextContent("Merge blocked: Ship the fix");
    expect(blocked).toHaveTextContent("conflict in README.md");
    await userEvent.setup().click(
      within(blocked).getByRole("button", {
        name: "Merge blocked: Ship the fix",
      }),
    );
    expect(onOpenPullRequests).toHaveBeenCalledTimes(1);

    const multi = screen.getByTestId("activity-entry-pull-request:evt-4");
    expect(multi).toHaveTextContent("GitHub publication failed: Two repos");
    expect(multi).toHaveTextContent("into base · 2 repositories");
    expect(multi).toHaveTextContent("push rejected");
    // Repeated failures arrive as their newest one, with the run's count.
    expect(multi).toHaveTextContent(
      `Failed 3 times since ${formatDateTime("2026-02-04T09:30:00Z")}`,
    );
    expect(within(multi).queryByRole("link")).toBeNull();
    expect(
      within(multi).getByRole("button", {
        name: "GitHub publication failed: Two repos",
      }),
    ).toBeInTheDocument();

    // One page scroll keeps both the full summary and timeline reachable.
    const panel = screen.getByTestId("workspace-activity-panel");
    const slot = within(panel).getByTestId("timeline-header-slot");
    const scroller = within(panel).getByTestId("workspace-activity-scroller");
    expect(scroller.contains(slot)).toBe(true);
    expect(
      scroller.contains(screen.getByTestId("workspace-activity-entries")),
    ).toBe(true);
  });

  // Merge with override (g5.workspace.merge.admin-override.formal) and a
  // merge on GitHub outside G5 (g5.git.provider-merged-outside) name their
  // actor, reason or repository in the label.
  it("labels a merge with override and a merge on GitHub outside G5", async () => {
    const base = {
      at: "2026-02-03T10:00:00Z",
      source: "workspace" as const,
      kind: "pull_request" as const,
      pullRequestId: "pr-1",
      title: "Ship the fix",
      targetWorkspaceName: "base",
      repositoryCount: 1,
      providerRepoFullName: null,
      providerPrNumber: null,
      providerPrUrl: null,
      failureCount: null,
      firstFailedAt: null,
    };
    const entries: ActivityEntryDTO[] = [
      {
        ...base,
        key: "pull-request:evt-override",
        eventType: "merge_overridden",
        message: "Approver on leave",
        actor: { userId: "u-admin", displayName: "Ada Admin" },
      },
      {
        ...base,
        key: "pull-request:evt-outside",
        at: "2026-02-04T10:00:00Z",
        eventType: "provider_merged_outside",
        message: "acme/web",
        actor: null,
      },
    ];
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({ data: page({ primaryIssue: null, entries }) }),
      ),
    );
    renderPanel();
    const override = await screen.findByTestId(
      "activity-entry-pull-request:evt-override",
    );
    expect(override).toHaveTextContent(
      "Merge with override by Ada Admin: Approver on leave · Ship the fix",
    );
    const outside = screen.getByTestId(
      "activity-entry-pull-request:evt-outside",
    );
    expect(outside).toHaveTextContent(
      "Merged on GitHub outside G5: acme/web · Ship the fix",
    );
  });

  it("names where an incoming merge came from on the target's timeline", async () => {
    const entries: ActivityEntryDTO[] = [
      {
        at: "2026-02-03T10:00:00Z",
        source: "workspace",
        kind: "pull_request",
        key: "pull-request:evt-incoming",
        pullRequestId: "pr-1",
        eventType: "merge_overridden",
        title: "Ship the fix",
        targetWorkspaceName: "base",
        sourceWorkspaceName: "feature",
        incoming: true,
        message: "Approver on leave",
        repositoryCount: 0,
        providerRepoFullName: null,
        providerPrNumber: null,
        providerPrUrl: null,
        actor: { userId: "u-admin", displayName: "Ada Admin" },
        failureCount: null,
        firstFailedAt: null,
      },
    ];
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({ data: page({ primaryIssue: null, entries }) }),
      ),
    );
    renderPanel();
    const row = await screen.findByTestId(
      "activity-entry-pull-request:evt-incoming",
    );
    expect(row).toHaveTextContent("from feature");
    expect(row).not.toHaveTextContent("into base");
    expect(row).toHaveTextContent("Merge with override by Ada Admin");
  });

  it("maps pull-request event labels", () => {
    expect(pullRequestEventLabel("merged")).toBe("Merged");
    expect(
      pullRequestEventLabel("merge_overridden", {
        actorName: "Ada Admin",
        message: "Approver on leave",
      }),
    ).toBe("Merge with override by Ada Admin: Approver on leave");
    expect(
      pullRequestEventLabel("provider_merged_outside", { message: "acme/web" }),
    ).toBe("Merged on GitHub outside G5: acme/web");
  });

  it("renders no header block when none is given", async () => {
    server.use(
      http.get(ACTIVITY_URL, () => HttpResponse.json({ data: page() })),
    );
    renderPanel();
    await screen.findByTestId("workspace-activity-entries");
    expect(screen.queryByTestId("workspace-activity-header")).toBeNull();
  });

  describe("filter row (g5.workbench.landing)", () => {
    const WORKBENCH_ENTRIES: ActivityEntryDTO[] = [
      {
        key: "phase:evt-1",
        at: "2026-02-01T10:00:00Z",
        source: "workspace",
        kind: "phase_changed",
        fromPhase: "be",
        toPhase: "se",
        actor: { userId: "u1", displayName: "Maya Chen" },
      },
      {
        key: "pull-request:evt-2",
        at: "2026-02-02T10:00:00Z",
        source: "workspace",
        kind: "pull_request",
        pullRequestId: "pr-1",
        eventType: "created",
        title: "Ship the fix",
        targetWorkspaceName: "base",
        message: null,
        repositoryCount: 0,
        providerRepoFullName: null,
        providerPrNumber: null,
        providerPrUrl: null,
        actor: null,
        failureCount: null,
        firstFailedAt: null,
      },
    ];
    const GROUPS = [
      "issue_discussion",
      "conversations",
      "tasks",
      "commits",
      "approvals",
      "pull_requests",
      "guided_steps",
    ];
    const rowIds = () =>
      within(screen.getByTestId("workspace-activity-entries"))
        .getAllByRole("listitem")
        .map((row) => row.getAttribute("data-testid"));

    it("isolates one category from All, combines categories and resets the last selection", async () => {
      server.use(
        http.get(ACTIVITY_URL, () =>
          HttpResponse.json({
            data: page({ entries: [...ENTRIES, ...WORKBENCH_ENTRIES] }),
          }),
        ),
      );
      renderPanel();
      const user = userEvent.setup();
      await screen.findByTestId("workspace-activity-entries");
      for (const group of GROUPS)
        expect(screen.getByTestId(`activity-filter-${group}`)).toHaveAttribute(
          "aria-pressed",
          "false",
        );
      expect(screen.getByTestId("activity-filter-all")).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      expect(
        screen.getByTestId("activity-filter-count-issue_discussion"),
      ).toHaveTextContent("3");
      expect(
        screen.getByTestId("activity-filter-count-tasks"),
      ).toHaveTextContent("3");
      await user.click(screen.getByTestId("activity-filter-tasks"));
      expect(screen.getByTestId("activity-filter-tasks")).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      expect(screen.getByTestId("activity-filter-tasks")).toHaveFocus();
      expect(rowIds()).toEqual([
        "activity-entry-task:t3",
        "activity-entry-task:t1",
        "activity-entry-task:t2",
      ]);
      await user.click(screen.getByTestId("activity-filter-approvals"));
      expect(rowIds()).toEqual([
        "activity-entry-task:t3",
        "activity-entry-task:t1",
        "activity-entry-task:t2",
        "activity-entry-approval:a1",
      ]);
      await user.click(screen.getByTestId("activity-filter-tasks"));
      expect(rowIds()).toEqual(["activity-entry-approval:a1"]);
      await user.click(screen.getByTestId("activity-filter-approvals"));
      expect(screen.getByTestId("activity-filter-all")).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      expect(rowIds()).toHaveLength(ENTRIES.length + 1);
      expect(screen.queryByTestId("activity-filter-phase_changes")).toBeNull();
      await user.click(screen.getByTestId("activity-filter-tasks"));
      await user.keyboard("[Space]");
      expect(screen.getByTestId("activity-filter-all")).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      expect(screen.getByTestId("activity-filter-tasks")).toHaveFocus();
      await user.keyboard("{Enter}");
      expect(rowIds()).toHaveLength(3);
      await user.click(screen.getByTestId("activity-filter-all"));
      for (const group of GROUPS)
        await user.click(screen.getByTestId(`activity-filter-${group}`));
      expect(screen.getByTestId("activity-filter-all")).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      for (const group of GROUPS)
        expect(screen.getByTestId(`activity-filter-${group}`)).toHaveAttribute(
          "aria-pressed",
          "false",
        );
    });

    it("keeps zero-count categories selectable and retains Load earlier for no matching history", async () => {
      server.use(
        http.get(ACTIVITY_URL, () =>
          HttpResponse.json({
            data: page({ entries: [ENTRIES[1]], earlierCursor: "older" }),
          }),
        ),
      );
      renderPanel();
      await screen.findByTestId("activity-entry-chat:c1");
      await userEvent.click(screen.getByTestId("activity-filter-commits"));
      expect(
        screen.getByTestId("activity-filter-count-commits"),
      ).toHaveTextContent("0");
      expect(
        screen.getByTestId("workspace-activity-all-filtered"),
      ).toHaveTextContent("No matching history for these filters.");
      expect(
        screen.getByTestId("workspace-activity-load-earlier"),
      ).toBeEnabled();
      expect(screen.queryByTestId("workspace-activity-entries")).toBeNull();
    });

    it("restores the person's selection without changing another person's default", async () => {
      server.use(
        http.get(ACTIVITY_URL, () => HttpResponse.json({ data: page() })),
      );
      renderPanel();
      await screen.findByTestId("workspace-activity-entries");
      await userEvent.click(screen.getByTestId("activity-filter-commits"));
      expect(rowIds()).toEqual(["activity-entry-commit:r1:abcdef1234567890"]);
      expect(
        JSON.parse(localStorage.getItem(activityFiltersKey("user-alex"))!),
      ).toEqual(GROUPS.filter((g) => g !== "commits"));
      cleanup();
      renderPanel();
      await screen.findByTestId("workspace-activity-entries");
      expect(screen.getByTestId("activity-filter-commits")).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      expect(rowIds()).toEqual(["activity-entry-commit:r1:abcdef1234567890"]);
      cleanup();
      renderApp(
        <Routes>
          <Route
            path="/:org/:workspace"
            element={
              <WorkspaceActivityPanel
                workspaceId={WORKSPACE_ID}
                onOpenCommit={vi.fn()}
              />
            }
          />
        </Routes>,
        {
          route: "/g5labs/current",
          user: createMockUser({ id: "user-other" }),
        },
      );
      await screen.findByTestId("workspace-activity-entries");
      expect(screen.getByTestId("activity-filter-all")).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      expect(screen.getByTestId("activity-filter-commits")).toHaveAttribute(
        "aria-pressed",
        "false",
      );
      expect(rowIds()).toHaveLength(ENTRIES.length);
    });

    it("preserves an old proper hidden subset", async () => {
      localStorage.setItem(
        activityFiltersKey("user-alex"),
        JSON.stringify(["commits"]),
      );
      server.use(
        http.get(ACTIVITY_URL, () => HttpResponse.json({ data: page() })),
      );
      renderPanel();
      await screen.findByTestId("workspace-activity-entries");
      expect(screen.getByTestId("activity-filter-all")).toHaveAttribute(
        "aria-pressed",
        "false",
      );
      expect(screen.getByTestId("activity-filter-tasks")).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      expect(rowIds()).not.toContain(
        "activity-entry-commit:r1:abcdef1234567890",
      );
    });

    it.each(["{not json", JSON.stringify(GROUPS)])(
      "restores All for invalid or old hide-everything state: %s",
      async (stored) => {
        localStorage.setItem(activityFiltersKey("user-alex"), stored);
        server.use(
          http.get(ACTIVITY_URL, () => HttpResponse.json({ data: page() })),
        );
        renderPanel();
        await screen.findByTestId("workspace-activity-entries");
        expect(screen.getByTestId("activity-filter-all")).toHaveAttribute(
          "aria-pressed",
          "true",
        );
        for (const group of GROUPS)
          expect(
            screen.getByTestId(`activity-filter-${group}`),
          ).toHaveAttribute("aria-pressed", "false");
      },
    );

    it("keeps phase independent when All resets category selection", async () => {
      server.use(
        http.get(ACTIVITY_URL, () => HttpResponse.json({ data: page() })),
      );
      renderPanel();
      await screen.findByTestId("workspace-activity-entries");
      const user = userEvent.setup();
      await user.selectOptions(
        screen.getByLabelText("Timeline phase filter"),
        "ie",
      );
      expect(rowIds()).toEqual(["activity-entry-approval:a1"]);
      await user.click(screen.getByTestId("activity-filter-tasks"));
      expect(
        screen.getByTestId("workspace-activity-all-filtered"),
      ).toBeVisible();
      await user.click(screen.getByTestId("activity-filter-all"));
      expect(screen.getByLabelText("Timeline phase filter")).toHaveValue("ie");
      expect(rowIds()).toEqual(["activity-entry-approval:a1"]);
    });

    it("renders between the summary and feed within the shared page scroller", async () => {
      server.use(
        http.get(ACTIVITY_URL, () =>
          HttpResponse.json({
            data: page({
              earlierCursor: "2025-12-31T10:00:00.000000Z|github-issue:900",
            }),
          }),
        ),
      );
      renderApp(
        <Routes>
          <Route
            path="/:org/:workspace"
            element={
              <WorkspaceActivityPanel
                workspaceId={WORKSPACE_ID}
                onOpenCommit={vi.fn()}
                header={<div data-testid="timeline-header-slot">Header</div>}
              />
            }
          />
        </Routes>,
        { route: "/g5labs/current", user: createMockUser() },
      );
      await screen.findByTestId("workspace-activity-entries");
      const slot = screen.getByTestId("timeline-header-slot");
      const filters = screen.getByTestId("activity-filters");
      const scroller = screen.getByTestId("workspace-activity-scroller");
      expect(
        slot.compareDocumentPosition(filters) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      expect(scroller.contains(filters)).toBe(true);
      // "Load earlier" stays with the feed, below the filters.
      const loadEarlier = screen.getByTestId("workspace-activity-load-earlier");
      expect(scroller.contains(loadEarlier)).toBe(true);
      expect(
        filters.compareDocumentPosition(loadEarlier) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    });
  });
});

describe("pullRequestEventLabel (git g5.git.pr-no-github)", () => {
  it("names GitHub publication only for a PR with GitHub records", () => {
    expect(
      pullRequestEventLabel("provider_failed", { repositoryCount: 1 }),
    ).toBe("GitHub publication failed");
    expect(
      pullRequestEventLabel("provider_failed", { repositoryCount: 0 }),
    ).toBe("PR update failed");
    expect(pullRequestEventLabel("merged", { repositoryCount: 0 })).toBe(
      "Merged",
    );
  });
});

describe("guided-journey marks (g5.workbench.journey.marks)", () => {
  it("renders a mark as a Guided step entry named by the catalog, under its own filter group", async () => {
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({
          data: page({
            entries: [
              {
                key: "journey:evt-1",
                at: "2026-02-04T10:00:00Z",
                source: "workspace",
                kind: "journey_step",
                stepId: "se-diff-review",
                done: true,
                actor: { userId: "u1", displayName: "Maya Chen" },
              },
              {
                key: "journey:evt-2",
                at: "2026-02-04T11:00:00Z",
                source: "workspace",
                kind: "journey_step",
                stepId: "not-in-catalog",
                done: false,
                actor: null,
              },
              {
                key: "journey:evt-3",
                at: "2026-02-04T12:00:00Z",
                source: "workspace",
                kind: "journey_step",
                stepId: "se-explainer",
                done: false,
                skipped: true,
                actor: { userId: "u1", displayName: "Maya Chen" },
              },
            ],
          }),
        }),
      ),
    );
    renderPanel();
    const done = await screen.findByTestId("activity-entry-journey:evt-1");
    expect(done).toHaveTextContent("Marked done: Check what changed");
    expect(done).toHaveTextContent("SE · Maya Chen");
    const undone = screen.getByTestId("activity-entry-journey:evt-2");
    expect(undone).toHaveTextContent("Marked not done: not-in-catalog");
    expect(undone).toHaveTextContent("Guided · Unknown user");
    // A skip (g5.ui.guided-panel) is its own wording.
    expect(
      screen.getByTestId("activity-entry-journey:evt-3"),
    ).toHaveTextContent("Skipped: Explain the design");
    expect(
      screen.getByTestId("activity-filter-count-guided_steps"),
    ).toHaveTextContent("3");
  });
});

describe("WorkspaceActivityPanel — native G5 primary issue", () => {
  const NATIVE_PAGE = "http://localhost:3000/g5labs/issues/g5/12";
  const nativeEntries: ActivityEntryDTO[] = [
    {
      key: "g5-issue:712",
      at: "2026-10-19T12:00:00Z",
      source: "g5",
      kind: "issue_opened",
      author: {
        login: "maya@g5labs.dev",
        avatarUrl: null,
        userId: "user-maya",
        displayName: "Maya Patel",
      },
      title: "Native search is slow",
      body: "Takes seconds.",
      url: NATIVE_PAGE,
    },
    {
      key: "g5-comment:41",
      at: "2026-10-19T13:00:00Z",
      source: "g5",
      kind: "issue_comment",
      commentId: "41",
      updatedAt: "2026-10-19T13:00:00.123456Z",
      author: {
        login: "alex@g5labs.dev",
        avatarUrl: null,
        userId: "user-alex",
        displayName: "Alex Chen",
      },
      body: "I can reproduce it.",
      url: `${NATIVE_PAGE}#comment-41`,
    },
    {
      key: "g5-comment:42",
      at: "2026-10-19T14:00:00Z",
      source: "g5",
      kind: "issue_comment",
      commentId: "42",
      updatedAt: "2026-10-19T14:00:00.123456Z",
      author: {
        login: "maya@g5labs.dev",
        avatarUrl: null,
        userId: "user-maya",
        displayName: "Maya Patel",
      },
      body: "Looking into it.",
      url: `${NATIVE_PAGE}#comment-42`,
    },
    {
      key: "g5-event:7",
      at: "2026-10-19T15:00:00Z",
      source: "g5",
      kind: "issue_event",
      actor: {
        login: "maya@g5labs.dev",
        avatarUrl: null,
        userId: "user-maya",
        displayName: "Maya Patel",
      },
      event: "closed",
      detail: null,
      stateReason: "completed",
    },
  ];

  it("replies to either author's issue comment in its original issue and preserves retry identity", async () => {
    const requests: Record<string, unknown>[] = [];
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({
          data: page({
            primaryIssue: {
              provider: "g5",
              linkId: "link-g5",
              displayKey: "ACME-12",
              canonicalUrl: NATIVE_PAGE,
              locator: { repositoryId: "g5", number: 12 },
              contentTrusted: true,
            },
            entries:
              requests.length >= 2
                ? [
                    {
                      ...(nativeEntries[1] as Extract<
                        ActivityEntryDTO,
                        { kind: "issue_comment" }
                      >),
                      key: "g5-comment:43",
                      commentId: "43",
                      body: String(requests[1].body),
                      url: `${NATIVE_PAGE}#comment-43`,
                    },
                    ...nativeEntries,
                  ]
                : nativeEntries,
            composer: {
              provider: "g5",
              state: "ready",
              githubLogin: null,
              destination: { trackerId: "tracker-g5", externalIssueId: "712" },
              displayKey: "ACME-12",
            },
          }),
        }),
      ),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/activity/comments`,
        async ({ request }) => {
          requests.push((await request.json()) as Record<string, unknown>);
          if (requests.length === 1)
            return HttpResponse.json(
              { error: { code: "UNAVAILABLE", message: "Try again" } },
              { status: 503 },
            );
          return HttpResponse.json(
            {
              data: {
                state: "published",
                externalCommentUrl: `${NATIVE_PAGE}#comment-43`,
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    renderPanel();
    const user = userEvent.setup();
    const own = await screen.findByTestId("activity-comment-reply-41");
    const other = screen.getByTestId("activity-comment-reply-42");
    expect(own).toBeEnabled();
    expect(other).toBeEnabled();
    expect(screen.queryByRole("dialog")).toBeNull();
    await user.click(other);
    const input = screen.getByTestId("activity-comment-input");
    await waitFor(() => expect(input).toHaveFocus());
    expect((input as HTMLTextAreaElement).value).toContain(
      "Reply to Maya Patel",
    );
    expect((input as HTMLTextAreaElement).value).toContain(
      `${NATIVE_PAGE}#comment-42`,
    );
    expect((input as HTMLTextAreaElement).value).not.toContain(
      "Looking into it.",
    );
    expect(screen.getByTestId("activity-comment-post")).toBeDisabled();
    await user.type(input, "I will check the allocation as well.");
    const text = (input as HTMLTextAreaElement).value;
    await user.click(screen.getByTestId("activity-comment-post"));
    await screen.findByTestId("activity-comment-error");
    await user.click(screen.getByRole("button", { name: "Close" }));
    await user.click(other);
    expect(screen.getByTestId("activity-comment-input")).toHaveValue(text);
    await user.click(screen.getByTestId("activity-comment-post"));
    await screen.findByTestId("activity-comment-posted");
    const persisted = await screen.findByTestId("activity-entry-g5-comment:43");
    expect(
      within(persisted).getByRole("link", {
        name: "Reply to Maya Patel",
      }),
    ).toHaveAttribute("href", `${NATIVE_PAGE}#comment-42`);
    expect(requests).toHaveLength(2);
    expect(requests[1]).toEqual(requests[0]);
    expect(requests[1].destination).toEqual({
      trackerId: "tracker-g5",
      externalIssueId: "712",
    });
    expect(requests[1].body).toBe(text);
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByTestId("activity-comment-composer")).toBeNull();
    await user.click(own);
    expect(
      (screen.getByTestId("activity-comment-input") as HTMLTextAreaElement)
        .value,
    ).toContain("Reply to Alex Chen");
  });

  it("keeps native issue permalinks and own-comment editing without repeated labels", async () => {
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({
          data: page({
            primaryIssue: {
              provider: "g5",
              linkId: "link-g5",
              displayKey: "ACME-12",
              canonicalUrl: NATIVE_PAGE,
              locator: { repositoryId: "g5", number: 12 },
              contentTrusted: true,
            },
            entries: nativeEntries,
            composer: {
              provider: "g5",
              state: "ready",
              githubLogin: null,
              destination: { trackerId: "tracker-g5", externalIssueId: "712" },
              displayKey: "ACME-12",
            },
          }),
        }),
      ),
    );
    renderPanel();
    const user = userEvent.setup();

    const opened = await screen.findByTestId("activity-entry-g5-issue:712");
    expect(opened).toHaveTextContent("Maya Patel opened ACME-12");
    expect(opened).not.toHaveTextContent("G5 issue");
    expect(opened).not.toHaveTextContent("GitHub");
    expect(within(opened).getByRole("link")).toHaveAttribute(
      "href",
      "/g5labs/issues/g5/12",
    );
    const comment = screen.getByTestId("activity-entry-g5-comment:41");
    expect(comment).toHaveTextContent("Alex Chen commented");
    expect(comment).not.toHaveTextContent("G5 comment");
    expect(screen.getByTestId("activity-entry-g5-event:7")).toHaveTextContent(
      "Maya Patel closed the issue",
    );
    expect(within(comment).getByRole("link")).toHaveAttribute(
      "href",
      "/g5labs/issues/g5/12#comment-41",
    );

    // Alex (the viewer) wrote 41; Maya's 42 is not editable.
    expect(
      screen
        .getAllByTestId(/^activity-comment-edit-\d+$/)
        .map((button) => button.getAttribute("data-testid")),
    ).toEqual(["activity-comment-edit-41"]);
    await user.click(screen.getByTestId("activity-comment-edit-41"));
    expect(
      screen.getByTestId("activity-comment-editor-41"),
    ).toBeInTheDocument();
  });
});

describe("preview runs (g5.ui.activity.preview-row)", () => {
  const PREVIEW_LIVE: ActivityEntryDTO = {
    key: "task:p1",
    at: "2026-01-06T09:00:00Z",
    source: "workspace",
    kind: "task",
    taskId: "p1",
    name: "Preview · full-stack",
    taskKind: "preview",
    status: "running",
    reason: null,
    preview: { profile: "full-stack", port: 3000, stage: "serving" },
  };
  const PREVIEW_CLOSING: ActivityEntryDTO = {
    ...PREVIEW_LIVE,
    key: "task:p2",
    taskId: "p2",
    preview: { profile: "full-stack", port: 3000, stage: "closing" },
  };
  const PREVIEW_CRASHED: ActivityEntryDTO = {
    key: "task:p3",
    at: "2026-01-05T10:45:00Z",
    source: "workspace",
    kind: "task",
    taskId: "p3",
    name: "Preview · full-stack",
    taskKind: "preview",
    status: "failed",
    reason: null,
    preview: {
      profile: "full-stack",
      port: 3000,
      ending: { kind: "failed", cause: "app_exited", exitCode: 7 },
    },
  };
  const PREVIEW_CLOSED: ActivityEntryDTO = {
    ...PREVIEW_CRASHED,
    key: "task:p4",
    taskId: "p4",
    status: "cancelled",
    reason: "owner_cancel",
    preview: {
      profile: null,
      port: 3000,
      ending: { kind: "closed", reason: "owner_cancel", close: "clean" },
    },
  };
  const PREVIEW_STOPPED: ActivityEntryDTO = {
    ...PREVIEW_CRASHED,
    key: "task:p5",
    taskId: "p5",
    status: "cancelled",
    reason: "idle_stop",
    preview: {
      profile: "full-stack",
      port: 3000,
      ending: { kind: "stopped", reason: "idle_stop" },
    },
  };

  function renderWithPreview(onOpenPreview = vi.fn()) {
    renderApp(
      <Routes>
        <Route
          path="/:org/:workspace"
          element={
            <WorkspaceActivityPanel
              workspaceId={WORKSPACE_ID}
              onOpenCommit={vi.fn()}
              onOpenPreview={onOpenPreview}
            />
          }
        />
      </Routes>,
      { route: "/g5labs/current", user: createMockUser() },
    );
    return onOpenPreview;
  }

  it("badges a live run with its stage and opens the preview log from the row and the rail word", async () => {
    server.use(
      http.get(ACTIVITY_URL, () => HttpResponse.json({ data: page() })),
      http.get(CURRENT_URL, () =>
        HttpResponse.json({
          data: { entries: [PREVIEW_LIVE, PREVIEW_CLOSING] },
        }),
      ),
    );
    const onOpenPreview = renderWithPreview();
    const row = await screen.findByTestId("activity-entry-task:p1");
    expect(row).toHaveAttribute("data-current", "true");
    expect(row).toHaveTextContent("Preview · full-stack");
    expect(row).not.toHaveTextContent("running");
    expect(
      screen.getByTestId("activity-current-badge-task:p1"),
    ).toHaveTextContent("Current · Serving on port 3000");
    expect(
      screen.getByTestId("activity-current-badge-task:p2"),
    ).toHaveTextContent("Current · Closing");
    await userEvent
      .setup()
      .click(screen.getByTestId("activity-open-preview-p1"));
    expect(onOpenPreview).toHaveBeenCalledWith({ taskId: "p1", log: true });
    expect(screen.queryByTestId("route-echo")).toBeNull();
  });

  it("titles an ended run by its ending and profile, words the cause, and opens the log only when there is one", async () => {
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({
          data: page({
            entries: [PREVIEW_CRASHED, PREVIEW_CLOSED, PREVIEW_STOPPED],
          }),
        }),
      ),
      http.get(CURRENT_URL, () => HttpResponse.json({ data: { entries: [] } })),
    );
    const onOpenPreview = renderWithPreview();
    const crashed = await screen.findByTestId("activity-entry-task:p3");
    expect(crashed).toHaveTextContent("Preview failed · full-stack");
    expect(crashed).toHaveTextContent("Application exited with status 7");
    expect(crashed).not.toHaveTextContent("Preview failed · full-stack failed");
    expect(screen.queryByTestId("activity-current-badge-task:p3")).toBeNull();
    const closed = screen.getByTestId("activity-entry-task:p4");
    expect(closed).toHaveTextContent("Preview closed · default profile");
    expect(closed).toHaveTextContent("Closed by you");
    expect(closed).not.toHaveTextContent("cancelled");
    const stopped = screen.getByTestId("activity-entry-task:p5");
    expect(stopped).toHaveTextContent("Preview stopped · full-stack");
    expect(stopped).toHaveTextContent(
      "Stopped because the workspace went idle",
    );
    const user = userEvent.setup();
    await user.click(screen.getByTestId("activity-open-preview-p3"));
    expect(onOpenPreview).toHaveBeenLastCalledWith({ taskId: "p3", log: true });
    await user.click(screen.getByTestId("activity-open-preview-p4"));
    expect(onOpenPreview).toHaveBeenLastCalledWith({
      taskId: "p4",
      log: false,
    });
    await user.click(screen.getByTestId("activity-open-preview-p5"));
    expect(onOpenPreview).toHaveBeenLastCalledWith({
      taskId: "p5",
      log: false,
    });
    // No preview row is a link to the task route.
    expect(within(crashed).queryByRole("link", { name: /Preview/ })).toBeNull();
  });

  it("renders a projected preview row as plain text for a reader without the preview dialog", async () => {
    server.use(
      http.get(ACTIVITY_URL, () =>
        HttpResponse.json({ data: page({ entries: [PREVIEW_CRASHED] }) }),
      ),
      http.get(CURRENT_URL, () => HttpResponse.json({ data: { entries: [] } })),
    );
    renderPanel();
    const crashed = await screen.findByTestId("activity-entry-task:p3");
    expect(crashed).toHaveTextContent("Preview failed · full-stack");
    expect(crashed).toHaveTextContent("Application exited with status 7");
    expect(within(crashed).queryByRole("button")).toBeNull();
    expect(within(crashed).queryByRole("link")).toBeNull();
  });

  it("keeps the plain task rendering for a preview row the server did not project", async () => {
    server.use(
      http.get(ACTIVITY_URL, () => HttpResponse.json({ data: page() })),
      http.get(CURRENT_URL, () => HttpResponse.json({ data: { entries: [] } })),
    );
    renderWithPreview();
    expect(
      await screen.findByTestId("activity-entry-task:t1"),
    ).toHaveTextContent("Preview succeeded");
  });
});
