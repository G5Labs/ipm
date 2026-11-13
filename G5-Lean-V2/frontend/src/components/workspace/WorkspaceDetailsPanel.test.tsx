import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { WorkspaceDetailsPanel } from "./WorkspaceDetailsPanel";
import {
  WorkspaceLiveGateProvider,
  type WorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import { useAuth } from "../../hooks/useAuth";
import { useWorkspace } from "../../hooks/useWorkspace";
import type { WorkspaceDetail } from "../../lib/types";
import { useOrgMembers } from "../../hooks/useOrg";
import { http, HttpResponse } from "msw";
import { server } from "../../test/msw/server";

// The panel reads `useParams`/`useNavigate` for the relocated repos region
// (center-panel D75), and its branch affordances (`BranchTrackingInline`,
// `MakeRemoteMenu`) run React Query hooks — so every render needs a routed
// context and a QueryClient.
function panelTree(queryClient: QueryClient, workspaceId: string) {
  return (
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={["/org-g5labs/payments"]}>
        <Routes>
          <Route
            path="/:org/:workspace"
            element={<WorkspaceDetailsPanel workspaceId={workspaceId} />}
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

/** The page's Download code action, offered from the header to anyone. */
function renderPanelWithDownload(onDownloadSource: () => void, busy = false) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={["/org-g5labs/payments"]}>
        <Routes>
          <Route
            path="/:org/:workspace"
            element={
              <WorkspaceDetailsPanel
                workspaceId="ws-1"
                onDownloadSource={onDownloadSource}
                downloadSourceInProgress={busy}
              />
            }
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

function renderPanel() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const result = render(panelTree(queryClient, "ws-1"));
  return {
    ...result,
    rerenderPanel: (workspaceId = "ws-1") =>
      result.rerender(panelTree(queryClient, workspaceId)),
  };
}

const {
  detail,
  stats,
  members,
  setPolicyMutateAsync,
  patchMutateAsync,
  renameMutateAsync,
} = vi.hoisted(() => ({
  setPolicyMutateAsync: vi.fn(),
  patchMutateAsync: vi.fn(),
  renameMutateAsync: vi.fn(),
  detail: {
    id: "ws-1",
    orgId: "org-1",
    name: "Payments workspace",
    isBase: false,
    readOnly: false,
    description: "Handles billing.",
    clonedFromId: "ws-base",
    docsRepoId: null,
    ownerUserId: "user-1",
    createdBy: "user-1",
    createdAt: "2026-06-01T00:00:00Z",
    updatedAt: "2026-06-02T00:00:00Z",
    pushMode: "manual",
    branches: [
      {
        workspaceId: "workspace-1",

        repoId: "repo-1",
        repoName: "api",
        branchName: "feat/x",
      },
    ],
    docsFolderPath: "docs/ws-1",
  } as unknown as WorkspaceDetail,
  stats: {
    // Retained tasks contribute 1.23; ordinary conversations contribute 1.00.
    totalCostUsd: 2.23,
    tasksCompleted: 4,
    totalTasks: 6,
    concurrentTasks: 1,
    tasksStopped: 2,
    byTask: [
      {
        taskId: "task-1",
        taskType: "agent",
        taskName: "sync main",
        costUsd: 0.8,
      },
      {
        taskId: "task-2",
        taskType: "agent",
        taskName: "chat session",
        costUsd: 0.43,
      },
    ],
    branches: [],
  },
  members: [
    {
      userId: "user-1",
      email: "owner@example.com",
      displayName: "Ada Owner",
      avatarUrl: null,
      role: "admin" as const,
      joinedAt: "2026-01-01T00:00:00Z",
    },
  ],
}));

// Tests replace `detail.branches`; restore the default set before each test.
const defaultBranches = detail.branches;

function mutationHook(mutateAsync: ReturnType<typeof vi.fn>) {
  return {
    mutate: vi.fn(),
    mutateAsync,
    isLoading: false,
    error: null,
    data: undefined,
    reset: vi.fn(),
  };
}

vi.mock("../../hooks/useAuth", () => ({
  useAuth: vi.fn(),
}));

vi.mock("../../hooks/useWorkspace", async (importOriginal) => ({
  // The relocated repos region (center-panel D75) renders children
  // (branch affordances) that consume other hooks from this
  // module — keep the real implementations for those and stub only the four
  // hooks the panel itself reads.
  ...(await importOriginal<typeof import("../../hooks/useWorkspace")>()),
  useWorkspace: vi.fn(() => ({
    status: "ok",
    data: detail,
    refreshError: null,
  })),
  useWorkspaceStats: vi.fn(() => ({ status: "ok", data: stats })),
  usePatchWorkspace: vi.fn(() => mutationHook(patchMutateAsync)),
  useRenameWorkspace: vi.fn(() => mutationHook(renameMutateAsync)),
  useSetWorkspaceMutationPolicy: vi.fn(() =>
    mutationHook(setPolicyMutateAsync),
  ),
}));

vi.mock("../../hooks/useOrg", () => ({
  useOrgMembers: vi.fn(() => ({ status: "ok", data: members })),
}));

const asOwner = () =>
  (useAuth as unknown as ReturnType<typeof vi.fn>).mockReturnValue({
    user: {
      id: "user-1",
      activeOrgId: "org-1",
      orgs: [{ orgId: "org-1", orgName: "Org", orgSlug: "org", role: "admin" }],
    },
    hasScope: (s: string) => s === "workspaces:write",
  });

const asNonOwner = () =>
  (useAuth as unknown as ReturnType<typeof vi.fn>).mockReturnValue({
    user: {
      id: "user-9",
      activeOrgId: "org-1",
      orgs: [
        { orgId: "org-1", orgName: "Org", orgSlug: "org", role: "member" },
      ],
    },
    hasScope: () => false,
  });

beforeEach(() => {
  vi.clearAllMocks();
  setPolicyMutateAsync.mockResolvedValue({
    policy: "read_only",
    blockers: [],
    message: "Workspace is now read-only.",
  });
  patchMutateAsync.mockResolvedValue(undefined);
  (useWorkspace as unknown as ReturnType<typeof vi.fn>).mockReturnValue({
    status: "ok",
    data: detail,
  });
  detail.readOnly = false;
  detail.branches = defaultBranches;
  detail.mutationPolicyTransition = undefined;
  asOwner();
});

describe("WorkspaceDetailsPanel", () => {
  it("labels a repo without a remote binding as local only", () => {
    detail.branches = [
      {
        workspaceId: "workspace-1",
        repoId: "repo-1",
        repoName: "api",
        branchName: "feat/x",
      },
    ] as unknown as WorkspaceDetail["branches"];

    renderPanel();

    const pill = screen.getByTestId("repo-status-pill");
    expect(pill).toHaveAttribute("data-state", "local");
    expect(pill).toHaveTextContent("Local only — not published");
    expect(screen.queryByTestId("repo-tracking-inline")).toBeNull();
  });

  it("stacks one card per repo, each with its own status", () => {
    detail.branches = [
      {
        workspaceId: "workspace-1",
        repoId: "repo-1",
        repoName: "api",
        branchName: "feat/x",
      },
      {
        workspaceId: "workspace-1",
        repoId: "repo-2",
        repoName: "web",
        branchName: "feat/x",
      },
    ] as unknown as WorkspaceDetail["branches"];

    renderPanel();

    expect(screen.getAllByTestId("repo-chip")).toHaveLength(2);
    expect(screen.getAllByTestId("repo-status-pill")).toHaveLength(2);
    expect(
      screen.getAllByTestId("workspace-repo-checkout-commands"),
    ).toHaveLength(2);
  });

  it("renders a tracked repo's status pill outside the chip, with no glyph", async () => {
    detail.branches = [
      {
        workspaceId: "workspace-1",
        repoId: "repo-1",
        repoName: "api",
        branchName: "feat/x",
        trackingMode: "remote_tracking",
        remoteRepoName: "acme/api",
        remoteBranch: "main",
      },
    ] as unknown as WorkspaceDetail["branches"];
    server.use(
      http.get("/api/v1/workspaces/ws-1/repos/repo-1/sync_runs", () =>
        HttpResponse.json({
          data: {
            branch: {
              workspaceId: "ws-1",
              repos: [
                {
                  repoId: "repo-1",
                  repoName: "api",
                  repoBranchName: "feat/x",
                  trackingMode: "remote_tracking",
                  headCommit: "deadbeef",
                  remoteBranch: "main",
                  baselineCommit: "deadbeef",
                  remoteTipAdvisorySha: "cafebabe",
                },
              ],
            },
            runs: { data: [], total: 0, page: 1, perPage: 20 },
          },
        }),
      ),
      http.get("/api/v1/workspaces/ws-1/repos/repo-1/merge_runs", () =>
        HttpResponse.json({ data: { items: [] } }),
      ),
    );

    renderPanel();

    await waitFor(() =>
      expect(screen.getByTestId("repo-status-pill")).toHaveAttribute(
        "data-state",
        "behind",
      ),
    );
    const pill = screen.getByTestId("repo-status-pill");
    const chip = screen.getByTestId("repo-chip");
    expect(chip).not.toContainElement(pill);
    expect(chip.querySelectorAll("button")).toHaveLength(0);
    expect(screen.getByTestId("repo-tracking-expander")).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  describe("Make Remote (g5.workspace.remote-ui.actions)", () => {
    const gate = (
      over: Partial<WorkspaceLiveGate> = {},
    ): WorkspaceLiveGate => ({
      kind: "live",
      canWrite: true,
      live: true,
      canStart: false,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner: true,
      orgAdmin: false,
      workspaceNoun: "the workspace",
      ...over,
    });
    const baseGate = (orgAdmin: boolean) =>
      gate({
        kind: "base",
        canWrite: false,
        live: false,
        owner: false,
        orgAdmin,
        workspaceNoun: "Base",
      });

    function renderWithGate(value: WorkspaceLiveGate) {
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      return render(
        <WorkspaceLiveGateProvider value={value}>
          {panelTree(queryClient, "ws-1")}
        </WorkspaceLiveGateProvider>,
      );
    }

    it("is a labelled button beside the local-only pill, not a glyph on the chip", () => {
      renderWithGate(gate({ orgAdmin: true }));

      const button = screen.getByRole("button", { name: "Make Remote" });
      const chip = screen.getByTestId("repo-chip");
      expect(chip).not.toContainElement(button);
      expect(chip.querySelectorAll("button")).toHaveLength(0);
      const pill = screen.getByTestId("repo-status-pill");
      expect(pill).toHaveTextContent("Local only — not published");
      expect(button.closest("div")).toContainElement(pill);
    });

    it.each([
      [
        "Base's owner, an administrator",
        gate({ kind: "base", canWrite: false, live: false, orgAdmin: true }),
      ],
      ["an organization administrator on Base", baseGate(true)],
    ])("is offered on Base to %s", (_label, value) => {
      vi.mocked(useWorkspace).mockReturnValue({
        status: "ok",
        refreshError: null,
        data: { ...detail, isBase: true, clonedFromId: null },
      });
      renderWithGate(value);
      expect(
        screen.getByRole("button", { name: "Make Remote" }),
      ).toBeInTheDocument();
    });

    it.each([
      ["a plain member on Base", baseGate(false)],
      ["a review copy", gate({ reviewCopy: true })],
      ["a non-admin owner off Base", gate()],
      [
        "a non-owner administrator off Base",
        gate({ canWrite: false, owner: false, orgAdmin: true }),
      ],
    ])("is not offered to %s", (_label, value) => {
      renderWithGate(value);
      expect(screen.getByTestId("repo-status-pill")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Make Remote" })).toBeNull();
    });

    it("keeps an open dialog when promotion turns the card remote-tracked", () => {
      server.use(
        http.get("/api/v1/workspaces/ws-1/repos/repo-1/sync_runs", () =>
          HttpResponse.json({
            data: {
              branch: { workspaceId: "ws-1", repos: [] },
              runs: { data: [], total: 0, page: 1, perPage: 20 },
            },
          }),
        ),
        http.get("/api/v1/workspaces/ws-1/repos/repo-1/merge_runs", () =>
          HttpResponse.json({ data: { items: [] } }),
        ),
      );
      const { rerenderPanel } = renderPanel();
      fireEvent.click(screen.getByRole("button", { name: "Make Remote" }));
      fireEvent.change(screen.getByTestId("make-remote-git-url"), {
        target: { value: "https://github.com/acme/api.git" },
      });

      vi.mocked(useWorkspace).mockReturnValue({
        status: "ok",
        refreshError: null,
        data: {
          ...detail,
          branches: [
            {
              workspaceId: "workspace-1",
              repoId: "repo-1",
              repoName: "api",
              branchName: "feat/x",
              trackingMode: "remote_tracking",
              remoteRepoName: "acme/api",
              remoteBranch: "main",
            },
          ] as unknown as WorkspaceDetail["branches"],
        },
      });
      rerenderPanel();

      expect(screen.queryByRole("button", { name: "Make Remote" })).toBeNull();
      expect(screen.getByTestId("make-remote-git-url")).toHaveValue(
        "https://github.com/acme/api.git",
      );
    });
  });

  describe("Publish (g5.workspace.remote-ui.actions)", () => {
    const gate = (
      over: Partial<WorkspaceLiveGate> = {},
    ): WorkspaceLiveGate => ({
      kind: "live",
      canWrite: true,
      live: true,
      canStart: false,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner: true,
      orgAdmin: false,
      workspaceNoun: "the workspace",
      ...over,
    });

    function renderEligible(value: WorkspaceLiveGate) {
      detail.branches = [
        {
          workspaceId: "workspace-1",
          repoId: "repo-1",
          repoName: "api",
          branchName: "feat/x",
          trackingMode: "pinned",
          remoteEligible: true,
        },
      ] as unknown as WorkspaceDetail["branches"];
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      return render(
        <WorkspaceLiveGateProvider value={value}>
          {panelTree(queryClient, "ws-1")}
        </WorkspaceLiveGateProvider>,
      );
    }

    it("is a labelled card button beside the local-only pill, with no glyph on the chip", () => {
      renderEligible(gate());

      const chip = screen.getByTestId("repo-chip");
      expect(chip.querySelectorAll("button")).toHaveLength(0);
      expect(
        chip.querySelector("svg.lucide-cloud-upload, svg.lucide-upload-cloud"),
      ).toBeNull();

      const button = screen.getByRole("button", { name: "Publish" });
      expect(chip).not.toContainElement(button);
      const pill = screen.getByTestId("repo-status-pill");
      expect(pill).toHaveTextContent("Local only — not published");
      expect(button.closest("div")).toContainElement(pill);
      // A publish-eligible branch publishes; it is not offered Make Remote.
      expect(screen.queryByRole("button", { name: "Make Remote" })).toBeNull();
    });

    it("opens the publish dialog from the card button", async () => {
      renderEligible(gate());
      fireEvent.click(screen.getByRole("button", { name: "Publish" }));
      expect(
        await screen.findByTestId("publish-branch-dialog"),
      ).toBeInTheDocument();
      expect(screen.getByTestId("publish-branch-button")).toHaveTextContent(
        "Publish to Remote",
      );
    });

    it.each([
      ["a reader", gate({ canWrite: false, owner: false })],
      ["a review copy", gate({ reviewCopy: true })],
    ])("is not offered to %s", (_label, value) => {
      renderEligible(value);
      expect(screen.getByTestId("repo-status-pill")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Publish" })).toBeNull();
      expect(
        screen.getByTestId("repo-chip").querySelectorAll("button"),
      ).toHaveLength(0);
    });
  });

  it("omits the Owner field for a protected root with another display name", () => {
    vi.mocked(useWorkspace).mockReturnValue({
      status: "ok",
      refreshError: null,
      data: { ...detail, name: "Mainline", isBase: true, clonedFromId: null },
    });
    renderPanel();
    expect(screen.queryByText("Owner")).toBeNull();
    expect(screen.queryByText("Ada Owner")).toBeNull();
    expect(screen.getByText("Created")).toBeInTheDocument();
  });

  it.each([false, true])(
    "keeps Base read-only without policy controls when legacy readOnly=%s",
    (readOnly) => {
      vi.mocked(useWorkspace).mockReturnValue({
        status: "ok",
        refreshError: null,
        data: { ...detail, isBase: true, readOnly },
      });
      renderPanel();
      expect(
        screen.getByTestId("workspace-readonly-badge"),
      ).toBeInTheDocument();
      expect(screen.queryByTestId("workspace-readonly-toggle")).toBeNull();
      expect(screen.queryByTestId("workspace-readonly-modal")).toBeNull();
    },
  );

  it("preserves editability controls for a working child named Base", async () => {
    vi.mocked(useWorkspace).mockReturnValue({
      status: "ok",
      refreshError: null,
      data: { ...detail, name: "Base", isBase: false },
    });
    renderPanel();
    expect(screen.getByTestId("workspace-readonly-toggle")).toBeEnabled();
    fireEvent.click(screen.getByTestId("workspace-description-edit"));
    fireEvent.change(screen.getByTestId("workspace-description-input"), {
      target: { value: "Working Base notes" },
    });
    fireEvent.click(screen.getByTestId("workspace-description-save"));
    await waitFor(() =>
      expect(patchMutateAsync).toHaveBeenCalledWith({
        description: "Working Base notes",
      }),
    );
  });

  it.each([false, true])(
    "omits Base description editing when legacy readOnly=%s",
    (readOnly) => {
      vi.mocked(useWorkspace).mockReturnValue({
        status: "ok",
        refreshError: null,
        data: { ...detail, isBase: true, readOnly },
      });
      renderPanel();
      expect(screen.queryByTestId("workspace-description-edit")).toBeNull();
      expect(screen.getByText("Handles billing.")).toBeVisible();
      expect(patchMutateAsync).not.toHaveBeenCalled();
    },
  );

  it("hides a retained description editor when switching from a child to Base", () => {
    const { rerenderPanel } = renderPanel();
    fireEvent.click(screen.getByTestId("workspace-description-edit"));
    fireEvent.change(screen.getByTestId("workspace-description-input"), {
      target: { value: "Child draft" },
    });
    vi.mocked(useWorkspace).mockReturnValue({
      status: "ok",
      refreshError: null,
      data: { ...detail, id: "ws-base", isBase: true, readOnly: false },
    });
    rerenderPanel("ws-base");
    expect(screen.queryByTestId("workspace-description-input")).toBeNull();
    expect(screen.queryByTestId("workspace-description-save")).toBeNull();
    expect(screen.queryByTestId("workspace-description-edit")).toBeNull();
    expect(screen.getByText("Handles billing.")).toBeVisible();
    expect(patchMutateAsync).not.toHaveBeenCalled();
  });

  it("offers the policy toggle and description edit disabled with the reason on a review copy (g5.governance.review-workspace.exclusions)", () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <WorkspaceLiveGateProvider
        value={{
          kind: "live",
          canWrite: true,
          live: true,
          canStart: false,
          reviewCopy: true,
          owner: true,
          reviewIdentityKnown: true,
          workspaceNoun: "your review workspace",
        }}
      >
        {panelTree(queryClient, "ws-1")}
      </WorkspaceLiveGateProvider>,
    );
    const toggle = screen.getByTestId("workspace-readonly-toggle");
    expect(toggle).toBeDisabled();
    expect(toggle.getAttribute("title")).toMatch(
      /review copy follows its source/,
    );
    const edit = screen.getByTestId("workspace-description-edit");
    expect(edit).toBeDisabled();
    expect(edit.getAttribute("title")).toMatch(
      /review copy follows its source/,
    );
  });

  it("keeps the policy toggle waiting while the page has not learned if this is a copy", () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <WorkspaceLiveGateProvider
        value={{
          kind: "live",
          canWrite: true,
          live: true,
          canStart: false,
          reviewCopy: false,
          reviewIdentityKnown: false,
          owner: true,
          workspaceNoun: "the workspace",
        }}
      >
        {panelTree(queryClient, "ws-1")}
      </WorkspaceLiveGateProvider>,
    );
    const toggle = screen.getByTestId("workspace-readonly-toggle");
    expect(toggle).toBeDisabled();
    expect(toggle.getAttribute("title")).toMatch(
      /Checking whether this is a review copy/,
    );
    expect(screen.getByTestId("workspace-description-edit")).toBeDisabled();
  });

  it("omits description editing for an owner of a read-only child", () => {
    detail.readOnly = true;
    renderPanel();
    expect(screen.queryByTestId("workspace-description-edit")).toBeNull();
    expect(screen.getByTestId("workspace-readonly-toggle")).toBeEnabled();
  });

  it("renders identity, resolved owner, activity, and estimated cost", () => {
    renderPanel();
    expect(
      screen.getByRole("heading", { name: "Payments workspace" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Handles billing.")).toBeInTheDocument();
    // ownerUserId user-1 resolves via org members to a display name.
    expect(screen.getByText("Ada Owner")).toBeInTheDocument();
    // Activity from stats: total tasks (6) and the stopped-task count (2),
    // with an explicit "Stopped" tile so cancelled chats are accounted for.
    expect(screen.getByText("6")).toBeInTheDocument();
    expect(screen.getByText("Stopped")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    // Spend framed as estimated token cost, not charged.
    expect(screen.getByText("Estimated token cost")).toBeInTheDocument();
    // The headline "Estimated total token cost" is the workspace-stats total.
    expect(screen.getByText("$2.23")).toBeInTheDocument();
    expect(screen.getByText("Other usage")).toBeInTheDocument();
    expect(screen.getByText("$1.00")).toBeInTheDocument();
    // Task rows are preserved alongside ordinary conversation spend.
    expect(screen.getByText("sync main")).toBeInTheDocument();
    expect(screen.getByText("$0.80")).toBeInTheDocument();
    expect(screen.getByText("chat session")).toBeInTheDocument();
    expect(screen.getByText("$0.43")).toBeInTheDocument();
  });

  it("shows an owner avatar even when ownerUserId resolves to no org member", () => {
    // Regression guard for the Owner row: before the shared IdentityAvatar it
    // rendered a bare id string with no avatar whenever the roster lookup
    // missed (@ref software_architecture/client/shared-components#D39).
    (useOrgMembers as unknown as ReturnType<typeof vi.fn>).mockReturnValueOnce({
      status: "ok",
      data: [],
    });

    renderPanel();

    expect(screen.getByText("user-1")).toBeInTheDocument();
    const avatars = screen.getAllByTestId("identity-avatar");
    expect(avatars.length).toBeGreaterThan(0);
    expect(avatars[0]).toHaveTextContent("U");
  });

  it("lets the owner open the confirm modal and transition to read-only via drain", async () => {
    renderPanel();
    const toggle = screen.getByTestId("workspace-readonly-toggle");
    expect(toggle).not.toBeDisabled();
    fireEvent.click(toggle);
    expect(screen.getByTestId("workspace-readonly-modal")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("workspace-readonly-confirm"));
    await waitFor(() =>
      expect(setPolicyMutateAsync).toHaveBeenCalledWith({
        policy: "read_only",
        mode: "drain",
      }),
    );
  });

  it("disables the toggle for a non-owner", () => {
    asNonOwner();
    renderPanel();
    expect(screen.getByTestId("workspace-readonly-toggle")).toBeDisabled();
  });

  it("shows the read-only badge and a 'Make editable' toggle for a read-only workspace", () => {
    detail.readOnly = true;
    renderPanel();
    expect(screen.getByTestId("workspace-readonly-badge")).toBeInTheDocument();
    expect(screen.getByText("Make editable")).toBeInTheDocument();
  });

  it("offers 'Make editable' + a Freezing badge while a drain is in progress and reverts via a read_write request", async () => {
    // Server-truth freezing (still mutation_policy=read_write, so not readOnly).
    detail.mutationPolicyTransition = "freezing";
    renderPanel();
    expect(screen.getByTestId("workspace-freezing-badge")).toBeInTheDocument();
    // The escape-hatch control: not effectively read-write → "Make editable".
    expect(screen.getByText("Make editable")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("workspace-readonly-toggle"));
    fireEvent.click(screen.getByTestId("workspace-readonly-confirm"));
    await waitFor(() =>
      expect(setPolicyMutateAsync).toHaveBeenCalledWith({
        policy: "read_write",
      }),
    );
  });

  it("keeps the modal draining and shows the message on an async (202) freezing response", async () => {
    setPolicyMutateAsync.mockResolvedValue({
      policy: "read_write",
      transition: "freezing",
      blockers: [],
      message:
        "Freezing the workspace and draining in-flight work in the background.",
    });
    renderPanel();
    fireEvent.click(screen.getByTestId("workspace-readonly-toggle"));
    fireEvent.click(screen.getByTestId("workspace-readonly-confirm"));
    await waitFor(() =>
      expect(
        screen.getByTestId("workspace-readonly-draining"),
      ).toBeInTheDocument(),
    );
    expect(screen.getByTestId("workspace-readonly-message")).toHaveTextContent(
      /draining in-flight work/i,
    );
  });

  it("surfaces a still-generating chat task blocker + message from a drain-timeout revert", async () => {
    // A live chat's liveness is its non-terminal chat JOB (a stopped chat has no
    // such task and does not block). An actively-generating chat that never
    // quiesces fail-safe reverts and reports the chat task as a blocker.
    setPolicyMutateAsync.mockResolvedValue({
      policy: "read_write",
      blockers: [
        { kind: "task", id: "task-1", detail: "chat task is running" },
      ],
      message:
        "Cannot make the workspace read-only yet — resolve the blockers below, then retry.",
    });
    renderPanel();
    fireEvent.click(screen.getByTestId("workspace-readonly-toggle"));
    fireEvent.click(screen.getByTestId("workspace-readonly-confirm"));
    await waitFor(() =>
      expect(
        screen.getByTestId("workspace-readonly-blockers"),
      ).toBeInTheDocument(),
    );
    expect(screen.getByText(/chat task is running/i)).toBeInTheDocument();
    expect(screen.getByTestId("workspace-readonly-message")).toHaveTextContent(
      /resolve the blockers/i,
    );
  });

  it("renders structured blockers returned from a refused transition", async () => {
    setPolicyMutateAsync.mockResolvedValue({
      policy: "read_write",
      blockers: [
        { kind: "task", id: "task-1", detail: "Task task-1 is still running" },
      ],
    });
    renderPanel();
    fireEvent.click(screen.getByTestId("workspace-readonly-toggle"));
    fireEvent.click(screen.getByTestId("workspace-readonly-confirm"));
    await waitFor(() =>
      expect(
        screen.getByTestId("workspace-readonly-blockers"),
      ).toBeInTheDocument(),
    );
    expect(
      screen.getByText(/Task task-1 is still running/),
    ).toBeInTheDocument();
  });

  it("lets the owner edit the description via usePatchWorkspace", async () => {
    renderPanel();
    fireEvent.click(screen.getByTestId("workspace-description-edit"));
    const input = screen.getByTestId("workspace-description-input");
    fireEvent.change(input, { target: { value: "Updated billing notes." } });
    fireEvent.click(screen.getByTestId("workspace-description-save"));
    await waitFor(() =>
      expect(patchMutateAsync).toHaveBeenCalledWith({
        description: "Updated billing notes.",
      }),
    );
  });

  it("does not offer description editing to a non-owner", () => {
    asNonOwner();
    renderPanel();
    expect(
      screen.queryByTestId("workspace-description-edit"),
    ).not.toBeInTheDocument();
  });

  it("POSIX single-quotes every user-influenced checkout-command argument so a pasted command cannot inject (center-panel D75)", () => {
    // A user-typed external repo/branch name carrying shell metacharacters
    // must not evaluate when the copied command is pasted into a shell.
    detail.branches = [
      {
        workspaceId: "workspace-1",

        repoId: "repo-evil",
        repoName: "api; $(rm -rf ~)",
        branchName: "feat/`touch pwned`",
      },
    ] as unknown as WorkspaceDetail["branches"];

    renderPanel();

    const block = screen.getByTestId("workspace-repo-checkout-commands");
    const text = block.textContent ?? "";
    // Single-quoted arguments — the metacharacters live inside '...' and are
    // inert. Double quotes (which still evaluate $(...) / backticks) must not
    // be used to wrap the directory argument.
    expect(text).toContain("git clone '");
    expect(text).toContain("'api; $(rm -rf ~)'");
    expect(text).toContain("cd 'api; $(rm -rf ~)'");
    expect(text).toContain("git checkout 'feat/`touch pwned`'");
    expect(text).not.toContain('"api; $(rm -rf ~)"');
  });
});

describe("WorkspaceDetailsPanel Download code (ui g5.ui.workbench-shell)", () => {
  it("offers the page's download from the header and reports progress", async () => {
    const onDownloadSource = vi.fn();
    const { rerender } = renderPanelWithDownload(onDownloadSource);
    const button = await screen.findByTestId("workspace-download-source");
    expect(button).toHaveTextContent("Download code");
    fireEvent.click(button);
    expect(onDownloadSource).toHaveBeenCalledTimes(1);
    rerender(
      <QueryClientProvider
        client={
          new QueryClient({ defaultOptions: { queries: { retry: false } } })
        }
      >
        <MemoryRouter initialEntries={["/org-g5labs/payments"]}>
          <Routes>
            <Route
              path="/:org/:workspace"
              element={
                <WorkspaceDetailsPanel
                  workspaceId="ws-1"
                  onDownloadSource={onDownloadSource}
                  downloadSourceInProgress
                />
              }
            />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    );
    const busy = await screen.findByTestId("workspace-download-source");
    expect(busy).toBeDisabled();
    expect(busy).toHaveTextContent("Downloading…");
  });

  it("shows no download without the page's action", async () => {
    renderPanel();
    await screen.findByTestId("workspace-readonly-toggle");
    expect(screen.queryByTestId("workspace-download-source")).toBeNull();
  });

  // Rename lives in the Overview Name row (g5.workspace.rename): the owner of
  // a non-Base workspace, read-only or a review copy included, opens the
  // rename dialog there; former names sit beneath the name.
  describe("Overview Name row", () => {
    it("offers Rename to the owner, above Description, and renames through the dialog", async () => {
      asOwner();
      vi.mocked(useWorkspace).mockReturnValue({
        status: "ok",
        refreshError: null,
        data: { ...detail, formerNames: ["old-name"] },
      });
      renameMutateAsync.mockResolvedValue({
        id: detail.id,
        name: "renamed",
        formerNames: [detail.name, "old-name"],
      });
      renderPanel();
      const row = screen.getByTestId("workspace-name-row");
      expect(within(row).getByText("Name")).toBeVisible();
      expect(within(row).getByTestId("workspace-name")).toHaveTextContent(
        detail.name,
      );
      expect(
        within(row).getByTestId("workspace-former-names"),
      ).toHaveTextContent("formerly old-name");
      const overview = row.parentElement!;
      expect(overview.firstElementChild).toBe(row);
      expect(within(overview).getByText("Description")).toBeVisible();

      fireEvent.click(within(row).getByTestId("workspace-name-rename"));
      expect(
        screen.getByRole("heading", { name: `Rename ${detail.name}` }),
      ).toBeVisible();
      fireEvent.change(screen.getByTestId("rename-workspace-input"), {
        target: { value: "renamed" },
      });
      fireEvent.click(screen.getByTestId("rename-workspace-confirm"));
      await waitFor(() =>
        expect(renameMutateAsync).toHaveBeenCalledWith({ name: "renamed" }),
      );
      await waitFor(() =>
        expect(screen.queryByTestId("rename-workspace-dialog")).toBeNull(),
      );
    });

    it("offers Rename on a read-only workspace but not on Base, a personal workspace or to a non-owner", () => {
      asOwner();
      vi.mocked(useWorkspace).mockReturnValue({
        status: "ok",
        refreshError: null,
        data: { ...detail, readOnly: true },
      });
      renderPanel();
      expect(screen.getByTestId("workspace-name-rename")).toBeVisible();
      expect(screen.queryByTestId("workspace-description-edit")).toBeNull();
      cleanup();

      vi.mocked(useWorkspace).mockReturnValue({
        status: "ok",
        refreshError: null,
        data: { ...detail, isBase: true },
      });
      renderPanel();
      expect(screen.queryByTestId("workspace-name-rename")).toBeNull();
      expect(screen.getByTestId("workspace-name")).toHaveTextContent(
        detail.name,
      );
      cleanup();

      // g5.personal.names: the system names a personal workspace once.
      vi.mocked(useWorkspace).mockReturnValue({
        status: "ok",
        refreshError: null,
        data: { ...detail, kind: "personal" },
      });
      renderPanel();
      expect(screen.queryByTestId("workspace-name-rename")).toBeNull();
      cleanup();

      asNonOwner();
      vi.mocked(useWorkspace).mockReturnValue({
        status: "ok",
        refreshError: null,
        data: detail,
      });
      renderPanel();
      expect(screen.queryByTestId("workspace-name-rename")).toBeNull();
    });
  });
});
