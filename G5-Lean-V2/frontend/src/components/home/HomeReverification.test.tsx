// Home's Base re-verification line and its dialog
// (ui g5.ui.home-reverification, g5.ui.home-reverification.dialog).
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, delay, http } from "msw";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { describe, expect, it } from "vitest";

import {
  REVERIFICATION_CANNOT_START_REASON,
  REVERIFICATION_INCREMENTAL_UNAVAILABLE_REASON,
  formatReverificationDate,
} from "../../lib/baseReverification";
import { baseReverificationChatPrompt } from "../../lib/reviewPrompts";
import type {
  BaseReverificationStatus,
  OrgMemberDTO,
  ReverificationPullRequest,
  ReverificationRun,
} from "../../lib/types";
import { readForkLandingState } from "../../lib/workspaceLanding";
import { server } from "../../test/msw/server";
import { HomeReverificationStatus } from "./HomeReverificationStatus";

const STATUS_PATH = "/api/v1/orgs/org1/home/base-reverification";

const MEMBERS: OrgMemberDTO[] = [
  {
    userId: "user-ada",
    email: "ada@example.com",
    displayName: "Ada Lovelace",
    avatarUrl: null,
    role: "member",
    joinedAt: "2026-01-01T00:00:00Z",
    deactivatedAt: null,
    activityRevision: 0,
    shutdownPending: false,
  },
  {
    userId: "user-grace",
    email: "grace@example.com",
    displayName: "Grace Hopper",
    avatarUrl: null,
    role: "member",
    joinedAt: "2026-01-01T00:00:00Z",
    deactivatedAt: null,
    activityRevision: 0,
    shutdownPending: false,
  },
];

const HEADS_F = [
  { repoId: "repo-api", rootPath: "api", commit: "f".repeat(40) },
];
const HEADS_S = [
  { repoId: "repo-api", rootPath: "api", commit: "a".repeat(40) },
];

function makeRun(
  overrides: Partial<ReverificationRun> = {},
): ReverificationRun {
  return {
    id: "run-1",
    kind: "full",
    state: "completed",
    workspaceId: "ws-run-1",
    workspaceName: "base-reverification-2026-10-01",
    workspaceDeleted: false,
    ownerUserId: "user-grace",
    startedBy: "user-grace",
    startedAt: "2026-10-01T09:00:00Z",
    completedAt: "2026-10-03T12:00:00Z",
    label: "Base re-verification 2026-10-01 (full)",
    baseHeads: HEADS_S,
    sinceHeads: null,
    ...overrides,
  };
}

function makePullRequest(n: number): ReverificationPullRequest {
  return {
    id: `pr-${n}`,
    number: 100 + n,
    title: `Change number ${n}`,
    mergedAt: `2026-10-${String(10 + n).padStart(2, "0")}T10:00:00Z`,
    mergedBy: null,
  };
}

type Enabled = Extract<BaseReverificationStatus, { enabled: true }>;

function makeStatus(overrides: Partial<Enabled> = {}): Enabled {
  const completed = makeRun();
  return {
    enabled: true,
    baseWorkspaceId: "base-1",
    baseWorkspaceName: "base",
    state: "stale",
    canStart: true,
    lastCompleted: completed,
    inProgress: null,
    pullRequestCount: 12,
    pullRequests: [1, 2, 3].map(makePullRequest),
    lines: { added: 3410, removed: 1022 },
    linesUnavailable: false,
    runs: [completed],
    runCount: 1,
    ...overrides,
  };
}

function serveStatus(status: BaseReverificationStatus, reads?: number[]) {
  server.use(
    http.get(STATUS_PATH, () => {
      reads?.push(Date.now());
      return HttpResponse.json({ data: status });
    }),
  );
}

const WorkspaceEcho = () => {
  const location = useLocation();
  const landing = readForkLandingState(location.state);
  return (
    <div data-testid="workspace-route">
      <span data-testid="workspace-path">
        {location.pathname}
        {location.search}
      </span>
      <span data-testid="landing-draft">{landing?.draft ?? ""}</span>
      <span data-testid="landing-header">
        {landing?.header ? JSON.stringify(landing.header) : ""}
      </span>
    </div>
  );
};

function renderLine(pageSize = 5, switchOn = true) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={["/org-one/home"]}>
        <Routes>
          <Route
            path="/:org/home"
            element={
              <HomeReverificationStatus
                switchOn={switchOn}
                orgId="org1"
                orgSlug="org-one"
                members={MEMBERS}
                pageSize={pageSize}
              />
            }
          />
          <Route path="/:org/:workspace" element={<WorkspaceEcho />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return { queryClient };
}

async function findLine() {
  return screen.findByTestId("home-reverification-status");
}

describe("Home re-verification line", () => {
  it("renders nothing while the status says the feature is off", async () => {
    const reads: number[] = [];
    serveStatus({ enabled: false }, reads);
    renderLine();
    await waitFor(() => expect(reads.length).toBe(1));
    expect(screen.queryByTestId("home-reverification")).toBeNull();
  });

  it("renders nothing while the status is loading", async () => {
    server.use(
      http.get(STATUS_PATH, async () => {
        await delay("infinite");
        return HttpResponse.json({ data: makeStatus() });
      }),
    );
    renderLine();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.queryByTestId("home-reverification")).toBeNull();
  });

  it("renders nothing when the status read fails", async () => {
    const reads: number[] = [];
    server.use(
      http.get(STATUS_PATH, () => {
        reads.push(1);
        return HttpResponse.json(
          { error: { code: "INTERNAL", message: "boom", requestId: "" } },
          { status: 500 },
        );
      }),
    );
    renderLine();
    await waitFor(() => expect(reads.length).toBe(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.queryByTestId("home-reverification")).toBeNull();
  });

  it("says Base was never re-verified, with the button open to forkers", async () => {
    serveStatus(
      makeStatus({
        state: "never",
        lastCompleted: null,
        pullRequestCount: 0,
        pullRequests: [],
        lines: null,
        runs: [],
      }),
    );
    renderLine();
    expect(await findLine()).toHaveTextContent(
      /^Base has never been re-verified$/,
    );
    expect(screen.getByTestId("home-reverification-open")).toBeEnabled();
  });

  it("says when Base was re-verified while no PR merged since", async () => {
    serveStatus(
      makeStatus({
        state: "current",
        pullRequestCount: 0,
        pullRequests: [],
        lines: { added: 0, removed: 0 },
      }),
    );
    renderLine();
    expect(await findLine()).toHaveTextContent(
      `Base was re-verified on ${formatReverificationDate("2026-10-03T12:00:00Z")}; no PRs merged since`,
    );
  });

  it("counts the merged PRs and net lines while stale, in orange text", async () => {
    serveStatus(makeStatus());
    renderLine();
    const line = await findLine();
    expect(line).toHaveTextContent(
      "Base has not been re-verified for 12 merged PRs (+3,410 −1,022 lines)",
    );
    // Orange text, not a warning banner.
    expect(line.className).toContain("text-[var(--g5-warn)]");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("uses the singular for one PR and omits lines it does not have", async () => {
    serveStatus(makeStatus({ pullRequestCount: 1, lines: null }));
    renderLine();
    expect(await findLine()).toHaveTextContent(
      /^Base has not been re-verified for 1 merged PR$/,
    );
  });

  it("says lines are unavailable when a repository could not be diffed", async () => {
    serveStatus(makeStatus({ linesUnavailable: true }));
    renderLine();
    expect(await findLine()).toHaveTextContent(
      "Base has not been re-verified for 12 merged PRs (lines unavailable)",
    );
  });

  it("names the run's workspace and owner while one is in progress, greying the button for everyone", async () => {
    const running = makeRun({
      id: "run-2",
      kind: "incremental",
      state: "in_progress",
      workspaceId: "ws-run-2",
      workspaceName: "base-reverification-2026-11-06",
      ownerUserId: "user-ada",
      startedBy: "user-ada",
      startedAt: "2026-11-06T09:00:00Z",
      completedAt: null,
    });
    serveStatus(
      makeStatus({ state: "in_progress", inProgress: running, canStart: true }),
    );
    renderLine();
    expect(await findLine()).toHaveTextContent(
      "Verification in progress in workspace base-reverification-2026-11-06 (by Ada Lovelace)",
    );
    expect(screen.getByTestId("home-reverification-workspace")).toHaveAttribute(
      "href",
      "/org-one/base-reverification-2026-11-06",
    );
    const button = screen.getByTestId("home-reverification-open");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      "A re-verification is already in progress in workspace base-reverification-2026-11-06",
    );
  });

  it("falls back to 'its owner' when the roster does not name the owner", async () => {
    serveStatus(
      makeStatus({
        state: "in_progress",
        inProgress: makeRun({
          state: "in_progress",
          workspaceName: "base-reverification-2026-11-06",
          ownerUserId: "user-gone",
          completedAt: null,
        }),
      }),
    );
    renderLine();
    expect(await findLine()).toHaveTextContent("(by its owner)");
  });

  it("greys the button with the reason for someone who cannot fork Base", async () => {
    serveStatus(makeStatus({ canStart: false }));
    renderLine();
    await findLine();
    const button = screen.getByTestId("home-reverification-open");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", REVERIFICATION_CANNOT_START_REASON);
    await userEvent.click(button);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("Re-run verification dialog", () => {
  async function openDialog() {
    await userEvent.click(
      await screen.findByTestId("home-reverification-open"),
    );
    return screen.findByRole("dialog", {
      name: "Re-run verification of Base",
    });
  }

  it("explains the run, shows the counts and lists the PRs and runs", async () => {
    serveStatus(
      makeStatus({
        pullRequests: [
          { ...makePullRequest(1), mergedBy: "user-grace" },
          makePullRequest(2),
          makePullRequest(3),
        ],
      }),
    );
    renderLine();
    const dialog = await openDialog();
    expect(
      within(dialog).getByTestId("home-reverification-notice"),
    ).toHaveTextContent(/very expensive/);
    expect(dialog).toHaveTextContent(/new workspace forked from Base/);
    expect(dialog).toHaveTextContent(/merge that workspace into Base/);
    expect(dialog).toHaveTextContent(/reported rather than fixed/);
    const counts = within(dialog).getByTestId("home-reverification-counts");
    expect(counts).toHaveTextContent("12 pull requests");
    expect(counts).toHaveTextContent("+3,410 −1,022 lines");

    const prs = within(dialog).getAllByTestId("home-reverification-pr");
    expect(prs).toHaveLength(3);
    expect(prs[0]).toHaveTextContent("Change number 1");
    expect(prs[0]).toHaveTextContent("#101");
    expect(prs[0]).toHaveTextContent(
      `${formatReverificationDate("2026-10-11T10:00:00Z")} by Grace Hopper`,
    );
    // A merge G5 recorded without a person names no one.
    expect(prs[1]).not.toHaveTextContent(/ by /);
    // The server lists at most 200 of the exact count.
    expect(
      within(dialog).getByTestId("home-reverification-prs-capped"),
    ).toHaveTextContent("Showing the newest 3 of 12.");

    const [run] = within(dialog).getAllByTestId("home-reverification-run");
    expect(run).toHaveTextContent("Full");
    expect(within(run).getByRole("link")).toHaveAttribute(
      "href",
      "/org-one/base-reverification-2026-10-01",
    );
    expect(run).toHaveTextContent(
      `by Grace Hopper, ${formatReverificationDate("2026-10-01T09:00:00Z")}`,
    );
    expect(run).toHaveTextContent(
      `Completed ${formatReverificationDate("2026-10-03T12:00:00Z")}`,
    );
  });

  it("names a deleted run workspace without linking to it", async () => {
    serveStatus(
      makeStatus({
        runs: [makeRun({ workspaceDeleted: true })],
      }),
    );
    renderLine();
    const dialog = await openDialog();
    const [run] = within(dialog).getAllByTestId("home-reverification-run");
    expect(run).toHaveTextContent("base-reverification-2026-10-01 (deleted)");
    expect(within(run).queryByRole("link")).toBeNull();
  });

  it("pages both lists by the person's page size", async () => {
    serveStatus(
      makeStatus({
        pullRequestCount: 3,
        runs: [
          makeRun({ id: "r3", state: "abandoned", completedAt: null }),
          makeRun({ id: "r2", workspaceName: null }),
          makeRun({ id: "r1" }),
        ],
      }),
    );
    renderLine(2);
    const dialog = await openDialog();
    expect(
      within(dialog).getAllByTestId("home-reverification-pr"),
    ).toHaveLength(2);
    await userEvent.click(
      within(dialog).getByTestId("home-reverification-prs-more"),
    );
    expect(
      within(dialog).getAllByTestId("home-reverification-pr"),
    ).toHaveLength(3);
    expect(
      within(dialog).queryByTestId("home-reverification-prs-more"),
    ).toBeNull();

    const runs = within(dialog).getAllByTestId("home-reverification-run");
    expect(runs).toHaveLength(2);
    expect(runs[0]).toHaveTextContent("Abandoned");
    expect(runs[1]).toHaveTextContent("Workspace unavailable");
    await userEvent.click(
      within(dialog).getByTestId("home-reverification-runs-more"),
    );
    expect(
      within(dialog).getAllByTestId("home-reverification-run"),
    ).toHaveLength(3);
  });

  it("offers only a full run until a run has completed, saying why", async () => {
    serveStatus(
      makeStatus({
        state: "never",
        lastCompleted: null,
        pullRequestCount: 0,
        pullRequests: [],
        lines: null,
        runs: [],
      }),
    );
    renderLine();
    const dialog = await openDialog();
    const incremental = within(dialog).getByTestId(
      "home-reverification-incremental",
    );
    expect(incremental).toBeDisabled();
    expect(incremental).toHaveAccessibleDescription(
      REVERIFICATION_INCREMENTAL_UNAVAILABLE_REASON,
    );
    expect(
      within(dialog).getByTestId("home-reverification-full"),
    ).toBeEnabled();
    expect(
      within(dialog).getByTestId("home-reverification-counts"),
    ).toHaveTextContent("Never");
  });

  it("keeps a refused start in the dialog with the server's reason", async () => {
    serveStatus(makeStatus());
    const keys: string[] = [];
    server.use(
      http.post("/api/v1/workspaces/base-1/clone", async ({ request }) => {
        const body = (await request.json()) as { idempotencyKey: string };
        keys.push(body.idempotencyKey);
        return HttpResponse.json(
          {
            error: {
              code: "REVERIFICATION_IN_PROGRESS",
              message:
                "A Base re-verification is already in progress in workspace base-reverification-2026-11-06.",
              requestId: "",
            },
          },
          { status: 409 },
        );
      }),
    );
    renderLine();
    const dialog = await openDialog();
    await userEvent.click(
      within(dialog).getByTestId("home-reverification-incremental"),
    );
    expect(
      await within(dialog).findByTestId("home-reverification-error"),
    ).toHaveTextContent(
      "A Base re-verification is already in progress in workspace base-reverification-2026-11-06.",
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.queryByTestId("workspace-route")).toBeNull();
    // A refusal admitted nothing: the next attempt is a new request.
    await userEvent.click(
      within(dialog).getByTestId("home-reverification-incremental"),
    );
    await waitFor(() => expect(keys).toHaveLength(2));
    expect(keys[0]).not.toBe(keys[1]);
  });

  it("replays the same request after an unknown outcome, even once its run shows in progress", async () => {
    const run = makeRun({
      id: "run-new",
      kind: "full",
      state: "in_progress",
      workspaceId: "ws-new",
      workspaceName: null,
      ownerUserId: "user-ada",
      startedBy: "user-ada",
      completedAt: null,
      label: "Base re-verification 2026-11-06 (full)",
      baseHeads: HEADS_F,
    });
    let admitted = false;
    const bodies: { idempotencyKey: string; name: string }[] = [];
    server.use(
      http.get(STATUS_PATH, () =>
        HttpResponse.json({
          data: admitted
            ? makeStatus({ state: "in_progress", inProgress: run })
            : makeStatus(),
        }),
      ),
      http.post("/api/v1/workspaces/base-1/clone", async ({ request }) => {
        bodies.push(
          (await request.json()) as { idempotencyKey: string; name: string },
        );
        if (!admitted) {
          // The fork was admitted, but its outcome never reached the browser.
          admitted = true;
          return HttpResponse.json(
            {
              error: {
                code: "SERVICE_UNAVAILABLE",
                message:
                  "Workspace fork is still in progress; retry the same request",
                requestId: "",
              },
            },
            { status: 503 },
          );
        }
        return HttpResponse.json(
          {
            data: {
              workspace: {
                id: "ws-new",
                name: "base-reverification-2026-11-06",
              },
              clonedBranches: 1,
              clonedDrafts: 0,
              carriedSessions: 0,
              issueLink: null,
              startedChat: null,
              reverificationRun: {
                ...run,
                workspaceName: "base-reverification-2026-11-06",
              },
            },
          },
          { status: 201 },
        );
      }),
    );
    renderLine();
    const dialog = await openDialog();
    await userEvent.click(
      within(dialog).getByTestId("home-reverification-full"),
    );
    expect(
      await within(dialog).findByTestId("home-reverification-error"),
    ).toHaveTextContent(/Choose it again to retry\.$/);
    // The status now shows the run in progress: the other choice is greyed,
    // and the one whose outcome is unknown stays offered.
    expect(
      await within(dialog).findByTestId("home-reverification-blocked"),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByTestId("home-reverification-incremental"),
    ).toBeDisabled();
    expect(
      within(dialog).getByTestId("home-reverification-full"),
    ).toBeEnabled();

    await userEvent.click(
      within(dialog).getByTestId("home-reverification-full"),
    );
    expect(await screen.findByTestId("workspace-path")).toHaveTextContent(
      "/org-one/base-reverification-2026-11-06?view=chat",
    );
    expect(bodies).toHaveLength(2);
    expect(bodies[1].idempotencyKey).toBe(bodies[0].idempotencyKey);
    expect(bodies[1].name).toBe(bodies[0].name);
    expect(screen.getByTestId("landing-header")).toHaveTextContent(
      JSON.stringify({ kind: "base-verification" }),
    );
  });

  it("keeps the dialog and its reason when the switch was turned off meanwhile", async () => {
    let enabled = true;
    server.use(
      http.get(STATUS_PATH, () =>
        HttpResponse.json({
          data: enabled ? makeStatus() : { enabled: false },
        }),
      ),
      http.post("/api/v1/workspaces/base-1/clone", () => {
        enabled = false;
        return HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message:
                "Conflict: Base re-verification is turned off for this organization",
              requestId: "",
            },
          },
          { status: 409 },
        );
      }),
    );
    renderLine();
    const dialog = await openDialog();
    await userEvent.click(
      within(dialog).getByTestId("home-reverification-full"),
    );
    expect(
      await screen.findByTestId("home-reverification-error"),
    ).toHaveTextContent(
      /^Base re-verification is turned off for this organization$/,
    );
    // The status re-read says off: the line goes, the dialog and its reason stay.
    await waitFor(() =>
      expect(screen.queryByTestId("home-reverification-status")).toBeNull(),
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByTestId("home-reverification-error")).toBeInTheDocument();
  });

  it("shows a validation refusal's own message, not its JSON", async () => {
    serveStatus(makeStatus());
    server.use(
      http.post("/api/v1/workspaces/base-1/clone", () =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message:
                'Validation error: {"field":"reverification","message":"Base has no code repository to re-verify"}',
              details: {
                field: "reverification",
                message: "Base has no code repository to re-verify",
              },
              requestId: "",
            },
          },
          { status: 422 },
        ),
      ),
    );
    renderLine();
    const dialog = await openDialog();
    await userEvent.click(
      within(dialog).getByTestId("home-reverification-full"),
    );
    expect(
      await within(dialog).findByTestId("home-reverification-error"),
    ).toHaveTextContent(/^Base has no code repository to re-verify$/);
  });

  it("shows a failed fork's refusal without its doubled code prefix", async () => {
    serveStatus(makeStatus());
    server.use(
      http.post("/api/v1/workspaces/base-1/clone", () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message:
                "Conflict: Conflict: This Base re-verification fork did not finish and was abandoned",
              requestId: "",
            },
          },
          { status: 409 },
        ),
      ),
    );
    renderLine();
    const dialog = await openDialog();
    await userEvent.click(
      within(dialog).getByTestId("home-reverification-full"),
    );
    expect(
      await within(dialog).findByTestId("home-reverification-error"),
    ).toHaveTextContent(
      /^This Base re-verification fork did not finish and was abandoned$/,
    );
  });

  it("names a run's workspace as being created, and its starter, until the fork creates it", async () => {
    serveStatus(
      makeStatus({
        state: "in_progress",
        inProgress: makeRun({
          id: "run-pending",
          state: "in_progress",
          workspaceName: null,
          ownerUserId: null,
          startedBy: "user-grace",
          completedAt: null,
        }),
      }),
    );
    renderLine();
    expect(
      await screen.findByTestId("home-reverification-status"),
    ).toHaveTextContent(
      "Verification in progress in a workspace being created (by Grace Hopper)",
    );
    expect(screen.queryByTestId("home-reverification-workspace")).toBeNull();
  });

  it("says when it lists only the newest runs", async () => {
    serveStatus(makeStatus({ runs: [makeRun()], runCount: 51 }));
    renderLine();
    const dialog = await openDialog();
    expect(
      within(dialog).getByTestId("home-reverification-runs-capped"),
    ).toHaveTextContent("Showing the newest 1 of 51 runs.");
  });

  it("builds the landing prompt from the status when the start response names no run", async () => {
    const run = makeRun({
      id: "run-new",
      kind: "full",
      state: "in_progress",
      workspaceId: "ws-new",
      workspaceName: "base-reverification-2026-11-06",
      completedAt: null,
      label: "Base re-verification 2026-11-06 (full)",
      baseHeads: HEADS_F,
    });
    let started = false;
    server.use(
      http.get(STATUS_PATH, () =>
        HttpResponse.json({
          data: started
            ? makeStatus({ state: "in_progress", inProgress: run })
            : makeStatus(),
        }),
      ),
      http.post("/api/v1/workspaces/base-1/clone", () => {
        started = true;
        return HttpResponse.json(
          {
            data: {
              workspace: {
                id: "ws-new",
                name: "base-reverification-2026-11-06",
              },
              clonedBranches: 1,
              clonedDrafts: 0,
              carriedSessions: 0,
              issueLink: null,
              startedChat: null,
            },
          },
          { status: 201 },
        );
      }),
    );
    renderLine();
    const dialog = await openDialog();
    await userEvent.click(
      within(dialog).getByTestId("home-reverification-full"),
    );
    expect(await screen.findByTestId("landing-draft")).toHaveTextContent(
      "Base re-verification (full)",
    );
    expect(screen.getByTestId("landing-header")).toHaveTextContent(
      JSON.stringify({ kind: "base-verification" }),
    );
  });

  it("starts a run, disabling both choices meanwhile, and lands on its seeded chat", async () => {
    const reads: number[] = [];
    serveStatus(makeStatus(), reads);
    const run = makeRun({
      id: "run-new",
      kind: "incremental",
      state: "in_progress",
      workspaceId: "ws-new",
      workspaceName: "base-reverification-2026-11-06",
      ownerUserId: "user-ada",
      startedBy: "user-ada",
      startedAt: "2026-11-06T09:00:00Z",
      completedAt: null,
      label: "Base re-verification 2026-11-06 (incremental)",
      baseHeads: HEADS_F,
      sinceHeads: HEADS_S,
    });
    let body: Record<string, unknown> | null = null;
    let release: () => void = () => {};
    const released = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.post("/api/v1/workspaces/base-1/clone", async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        await released;
        return HttpResponse.json(
          {
            data: {
              workspace: {
                id: "ws-new",
                name: "base-reverification-2026-11-06",
              },
              clonedBranches: 1,
              clonedDrafts: 0,
              carriedSessions: 0,
              issueLink: null,
              startedChat: null,
              reverificationRun: run,
            },
          },
          { status: 201 },
        );
      }),
    );
    renderLine();
    const dialog = await openDialog();
    const readsBefore = reads.length;
    await userEvent.click(
      within(dialog).getByTestId("home-reverification-incremental"),
    );
    expect(
      await within(dialog).findByTestId("home-reverification-starting"),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByTestId("home-reverification-incremental"),
    ).toBeDisabled();
    expect(
      within(dialog).getByTestId("home-reverification-full"),
    ).toBeDisabled();
    // Escape does not close the dialog while the start is in flight.
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    release();

    expect(await screen.findByTestId("workspace-path")).toHaveTextContent(
      "/org-one/base-reverification-2026-11-06?view=chat",
    );
    expect(body).toMatchObject({
      uniquifyName: true,
      reverification: { kind: "incremental" },
    });
    expect(body!.name).toMatch(/^base-reverification-\d{4}-\d{2}-\d{2}$/);
    expect(typeof body!.idempotencyKey).toBe("string");
    expect(screen.getByTestId("landing-draft").textContent).toBe(
      baseReverificationChatPrompt(run),
    );
    expect(screen.getByTestId("landing-header")).toHaveTextContent(
      JSON.stringify({ kind: "base-verification" }),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    // The status is re-read after the start.
    await waitFor(() => expect(reads.length).toBeGreaterThan(readsBefore));
  });

  it("closes on Escape", async () => {
    serveStatus(makeStatus());
    renderLine();
    await openDialog();
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
