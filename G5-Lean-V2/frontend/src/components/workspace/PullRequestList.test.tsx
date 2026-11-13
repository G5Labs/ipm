import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  useCloseWorkspacePullRequest,
  useMergeOverridePreview,
  useMergeWorkspacePullRequest,
  useRefreshWorkspacePullRequest,
  useWorkspacePullRequests,
} from "../../hooks/useWorkspace";
import type {
  MergeOverridePreview,
  WorkspacePullRequestSummary,
} from "../../lib/types";
import { PullRequestList, MERGE_ADMISSION_UNKNOWN } from "./PullRequestList";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";

vi.mock("../../hooks/useWorkspace", () => ({
  useCloseWorkspacePullRequest: vi.fn(),
  useMergeWorkspacePullRequest: vi.fn(),
  useRefreshWorkspacePullRequest: vi.fn(),
  useWorkspacePullRequests: vi.fn(),
  // No override offered unless a test serves a preview.
  useMergeOverridePreview: vi.fn(() => ({
    status: "loading",
    refetch: vi.fn(),
  })),
  useMergeWithOverride: vi.fn(() => ({
    mutate: vi.fn(),
    mutateAsync: vi.fn(),
    isLoading: false,
    error: null,
    data: undefined,
    reset: vi.fn(),
  })),
}));

const mockedUseOverridePreview = vi.mocked(useMergeOverridePreview);

const mockedUsePullRequests = vi.mocked(useWorkspacePullRequests);
const mockedUseMerge = vi.mocked(useMergeWorkspacePullRequest);
const mockedUseRefresh = vi.mocked(useRefreshWorkspacePullRequest);
const mockedUseClose = vi.mocked(useCloseWorkspacePullRequest);
const pullRequest = {
  id: "pr-1",
  sourceWorkspaceId: "ws-source",
  sourceWorkspaceName: "Feature",
  targetWorkspaceId: "ws-target",
  targetWorkspaceName: "Base",
  status: "open",
  localMergeStatus: "ready",
  providerStatus: "local_only",
  title: "Feature into Base",
  sourceHeads: [],
  targetHeads: [],
  preflight: {},
  lastPreflightFingerprint: "fingerprint",
  blockingMessage: null,
  createdAt: "2026-09-01T00:00:00Z",
  updatedAt: "2026-09-01T00:00:00Z",
  mergedAt: null,
  // The server's verdict (g5.governance.approval.requirements): admitted.
  mergeAdmission: { admitted: true, reason: null, requirements: [] },
} as unknown as WorkspacePullRequestSummary;

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

const OWNER_GATE = {
  kind: "base" as const,
  canWrite: false,
  live: false,
  canStart: false,
  reviewCopy: false,
  reviewIdentityKnown: true,
  owner: true,
  workspaceNoun: "the workspace",
};

describe("PullRequestList merge admission (g5.governance.approval.requirements)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockedUseMerge.mockReturnValue(
      mutationHook(vi.fn()) as unknown as ReturnType<
        typeof useMergeWorkspacePullRequest
      >,
    );
    mockedUseRefresh.mockReturnValue(
      mutationHook(vi.fn()) as unknown as ReturnType<
        typeof useRefreshWorkspacePullRequest
      >,
    );
    mockedUseClose.mockReturnValue(
      mutationHook(vi.fn()) as unknown as ReturnType<
        typeof useCloseWorkspacePullRequest
      >,
    );
  });

  it("disables Merge with the server's reason while an IE requirement refuses admission", () => {
    mockedUsePullRequests.mockReturnValue({
      status: "ok",
      data: [
        {
          ...pullRequest,
          mergeAdmission: {
            admitted: false,
            reason: "Approval required before merging: Ship (pending)",
            requirements: [
              {
                definitionId: "def-1",
                definitionName: "Ship",
                definitionVersion: 1,
                state: "pending",
              },
            ],
          },
        },
      ],
    } as ReturnType<typeof useWorkspacePullRequests>);
    render(
      <WorkspaceLiveGateProvider value={OWNER_GATE}>
        <PullRequestList workspaceId="ws-target" />
      </WorkspaceLiveGateProvider>,
    );
    const button = screen.getByTestId("pull-request-merge-button-pr-1");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      "Approval required before merging: Ship (pending)",
    );
    expect(screen.getByTestId("pull-request-admission-pr-1")).toHaveTextContent(
      "Approval required",
    );
  });

  it("keeps Merge enabled when the admission is granted", () => {
    mockedUsePullRequests.mockReturnValue({
      status: "ok",
      data: [
        {
          ...pullRequest,
          mergeAdmission: { admitted: true, reason: null, requirements: [] },
        },
      ],
    } as ReturnType<typeof useWorkspacePullRequests>);
    render(
      <WorkspaceLiveGateProvider value={OWNER_GATE}>
        <PullRequestList workspaceId="ws-target" />
      </WorkspaceLiveGateProvider>,
    );
    expect(screen.getByTestId("pull-request-merge-button-pr-1")).toBeEnabled();
    expect(screen.queryByTestId("pull-request-admission-pr-1")).toBeNull();
  });

  it("treats a missing admission as unknown: Merge disabled until the summary refreshes", () => {
    mockedUsePullRequests.mockReturnValue({
      status: "ok",
      data: [{ ...pullRequest, mergeAdmission: undefined }],
    } as ReturnType<typeof useWorkspacePullRequests>);
    render(
      <WorkspaceLiveGateProvider value={OWNER_GATE}>
        <PullRequestList workspaceId="ws-target" />
      </WorkspaceLiveGateProvider>,
    );
    const button = screen.getByTestId("pull-request-merge-button-pr-1");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", MERGE_ADMISSION_UNKNOWN);
    expect(screen.getByTestId("pull-request-admission-pr-1")).toHaveTextContent(
      "not evaluated yet",
    );
  });
});

describe("PullRequestList merge boundary", () => {
  let merge: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();
    merge = vi.fn().mockResolvedValue({ pullRequest });
    mockedUsePullRequests.mockReturnValue({
      status: "ok",
      data: [pullRequest],
    } as ReturnType<typeof useWorkspacePullRequests>);
    mockedUseMerge.mockReturnValue(
      mutationHook(merge) as unknown as ReturnType<
        typeof useMergeWorkspacePullRequest
      >,
    );
    mockedUseRefresh.mockReturnValue(
      mutationHook(vi.fn()) as unknown as ReturnType<
        typeof useRefreshWorkspacePullRequest
      >,
    );
    mockedUseClose.mockReturnValue(
      mutationHook(vi.fn()) as unknown as ReturnType<
        typeof useCloseWorkspacePullRequest
      >,
    );
  });

  it("hides refresh, close and merge from a reader (g5.workspace.ui.reader-publication)", () => {
    render(
      <WorkspaceLiveGateProvider
        value={{
          kind: "reader",
          canWrite: false,
          live: false,
          canStart: false,
          reviewCopy: false,
          owner: false,
          reviewIdentityKnown: true,
          workspaceNoun: "the workspace",
        }}
      >
        <PullRequestList workspaceId="ws-target" dirtyCount={3} />
      </WorkspaceLiveGateProvider>,
    );
    expect(screen.getByText(pullRequest.title)).toBeInTheDocument();
    expect(screen.queryByTestId("pull-request-merge-button-pr-1")).toBeNull();
    expect(screen.queryByTestId("pull-request-close-button-pr-1")).toBeNull();
    expect(screen.queryByTestId("pull-request-refresh-button-pr-1")).toBeNull();
  });

  it("keeps refresh, close and merge for Base's owner, whose durable gate is closed", () => {
    render(
      <WorkspaceLiveGateProvider
        value={{
          kind: "base",
          canWrite: false,
          live: false,
          canStart: false,
          reviewCopy: false,
          reviewIdentityKnown: true,
          owner: true,
          workspaceNoun: "the workspace",
        }}
      >
        <PullRequestList workspaceId="ws-target" dirtyCount={3} />
      </WorkspaceLiveGateProvider>,
    );
    expect(screen.getByTestId("pull-request-merge-button-pr-1")).toBeEnabled();
    expect(screen.getByTestId("pull-request-close-button-pr-1")).toBeEnabled();
    expect(
      screen.getByTestId("pull-request-refresh-button-pr-1"),
    ).toBeEnabled();
  });

  it("gives an organization administrator who does not own Base the row actions (g5.governance.pr.base-admin)", async () => {
    render(
      <WorkspaceLiveGateProvider
        value={{
          kind: "base",
          canWrite: false,
          live: false,
          canStart: false,
          reviewCopy: false,
          reviewIdentityKnown: true,
          owner: false,
          orgAdmin: true,
          workspaceNoun: "the workspace",
        }}
      >
        <PullRequestList workspaceId="ws-target" dirtyCount={3} />
      </WorkspaceLiveGateProvider>,
    );
    expect(screen.getByTestId("pull-request-close-button-pr-1")).toBeEnabled();
    expect(
      screen.getByTestId("pull-request-refresh-button-pr-1"),
    ).toBeEnabled();
    fireEvent.click(screen.getByTestId("pull-request-merge-button-pr-1"));
    await waitFor(() =>
      expect(merge).toHaveBeenCalledWith({ pullRequestId: "pr-1" }),
    );
  });

  it.each([
    ["a member who neither owns nor administers Base", "base", false],
    [
      "an organization administrator on a workspace they do not own",
      "reader",
      true,
    ],
  ] as const)("hides the row actions from %s", (_label, kind, orgAdmin) => {
    render(
      <WorkspaceLiveGateProvider
        value={{
          kind,
          canWrite: false,
          live: false,
          canStart: false,
          reviewCopy: false,
          reviewIdentityKnown: true,
          owner: false,
          orgAdmin,
          workspaceNoun: "the workspace",
        }}
      >
        <PullRequestList workspaceId="ws-target" dirtyCount={3} />
      </WorkspaceLiveGateProvider>,
    );
    expect(screen.getByText(pullRequest.title)).toBeInTheDocument();
    expect(screen.queryByTestId("pull-request-merge-button-pr-1")).toBeNull();
    expect(screen.queryByTestId("pull-request-close-button-pr-1")).toBeNull();
    expect(screen.queryByTestId("pull-request-refresh-button-pr-1")).toBeNull();
  });

  it("sends the merge request directly to the workspace pull-request API", async () => {
    render(<PullRequestList workspaceId="ws-target" dirtyCount={3} />);

    fireEvent.click(screen.getByTestId("pull-request-merge-button-pr-1"));

    await waitFor(() => expect(merge).toHaveBeenCalledOnce());
    expect(mockedUseMerge).toHaveBeenCalledWith("ws-target");
    expect(merge).toHaveBeenCalledWith({ pullRequestId: "pr-1" });
  });

  it("shows automatic refresh failure independently of the previous ready preflight", () => {
    mockedUsePullRequests.mockReturnValue({
      status: "ok",
      data: [
        {
          ...pullRequest,
          providerStatus: "open",
          blockingMessage:
            "Automatic PR refresh failed; check provider access and choose Update PR.",
        },
      ],
    } as ReturnType<typeof useWorkspacePullRequests>);
    render(<PullRequestList workspaceId="ws-target" dirtyCount={0} />);
    expect(screen.getByTestId("pull-request-error-pr-1")).toHaveTextContent(
      "choose Update PR",
    );
    expect(screen.queryByTitle("Ready to merge")).not.toBeInTheDocument();
    // Exact merge/recovery admission remains on the server.
    expect(screen.getByTestId("pull-request-merge-button-pr-1")).toBeEnabled();
  });

  it("locks every row action while merging and prevents rapid duplicates", async () => {
    let releaseMerge: () => void = () => {};
    merge.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          releaseMerge = resolve;
        }),
    );
    render(<PullRequestList workspaceId="ws-target" dirtyCount={1} />);

    const mergeButton = screen.getByTestId("pull-request-merge-button-pr-1");
    fireEvent.click(mergeButton);
    fireEvent.click(mergeButton);

    await waitFor(() => expect(mergeButton).toBeDisabled());
    expect(
      screen.getByTestId("pull-request-refresh-button-pr-1"),
    ).toBeDisabled();
    expect(screen.getByTestId("pull-request-close-button-pr-1")).toBeDisabled();
    expect(merge).toHaveBeenCalledOnce();

    releaseMerge();
    await waitFor(() => expect(mergeButton).toBeEnabled());
  });

  it("surfaces a merge error and unlocks the row", async () => {
    merge.mockRejectedValueOnce(new Error("Branch head moved"));
    render(<PullRequestList workspaceId="ws-target" dirtyCount={1} />);

    fireEvent.click(screen.getByTestId("pull-request-merge-button-pr-1"));

    await waitFor(() =>
      expect(screen.getByTestId("pull-request-merge-error")).toHaveTextContent(
        "Branch head moved",
      ),
    );
    expect(merge).toHaveBeenCalledOnce();
    expect(screen.getByTestId("pull-request-merge-button-pr-1")).toBeEnabled();
  });
});

describe("PullRequestList merge with override (g5.workbench.ship-to-parent)", () => {
  const blockedPullRequest = {
    ...pullRequest,
    mergeAdmission: {
      admitted: false,
      reason: "Approval required before merging: Ship (pending)",
      requirements: [],
    },
  } as WorkspacePullRequestSummary;

  const preview: MergeOverridePreview = {
    sourceHeads: [],
    blockers: {
      approvals: [
        {
          definitionId: "def-1",
          definitionName: "Ship",
          definitionVersion: 1,
          state: "pending",
        },
      ],
      checks: [],
      github: [],
    },
    notOverridable: [],
    authority: { g5: { allowed: true }, github: { allowed: true } },
    selfOverride: false,
    digest: "digest-1",
  };

  function servePreview(data: MergeOverridePreview) {
    mockedUseOverridePreview.mockReturnValue({
      status: "ok",
      data,
      refetch: vi.fn(),
    });
  }

  function renderList(rows: WorkspacePullRequestSummary[]) {
    mockedUsePullRequests.mockReturnValue({
      status: "ok",
      data: rows,
    } as ReturnType<typeof useWorkspacePullRequests>);
    render(
      <WorkspaceLiveGateProvider value={OWNER_GATE}>
        <PullRequestList workspaceId="ws-target" />
      </WorkspaceLiveGateProvider>,
    );
  }

  beforeEach(() => {
    vi.clearAllMocks();
    mockedUseOverridePreview.mockReturnValue({
      status: "loading",
      refetch: vi.fn(),
    });
    mockedUseMerge.mockReturnValue(
      mutationHook(vi.fn()) as unknown as ReturnType<
        typeof useMergeWorkspacePullRequest
      >,
    );
    mockedUseRefresh.mockReturnValue(
      mutationHook(vi.fn()) as unknown as ReturnType<
        typeof useRefreshWorkspacePullRequest
      >,
    );
    mockedUseClose.mockReturnValue(
      mutationHook(vi.fn()) as unknown as ReturnType<
        typeof useCloseWorkspacePullRequest
      >,
    );
  });

  it("offers Merge with override beside a blocked Merge when the server grants it", () => {
    servePreview(preview);
    renderList([blockedPullRequest]);
    expect(screen.getByTestId("pull-request-merge-button-pr-1")).toBeDisabled();
    expect(
      screen.getByTestId("pull-request-pr-1-merge-override"),
    ).toHaveTextContent("Merge with override…");
    expect(mockedUseOverridePreview).toHaveBeenCalledWith(
      "ws-target",
      "pr-1",
      true,
    );
  });

  it("offers none when the server denies the group present", () => {
    servePreview({
      ...preview,
      authority: {
        g5: { allowed: false, reason: "Needs an organization admin." },
        github: { allowed: true },
      },
    });
    renderList([blockedPullRequest]);
    expect(screen.queryByTestId("pull-request-pr-1-merge-override")).toBeNull();
    expect(
      screen.getByTestId("pull-request-pr-1-merge-override-denied"),
    ).toHaveTextContent("Needs an organization admin.");
  });

  it("does not read the override while Merge is available", () => {
    servePreview(preview);
    renderList([pullRequest]);
    expect(screen.getByTestId("pull-request-merge-button-pr-1")).toBeEnabled();
    expect(screen.queryByTestId("pull-request-pr-1-merge-override")).toBeNull();
    expect(mockedUseOverridePreview).toHaveBeenCalledWith(
      "ws-target",
      "pr-1",
      false,
    );
  });

  it("marks a repository already merged on GitHub outside G5", () => {
    renderList([
      {
        ...pullRequest,
        providerRepos: [
          {
            id: "provider-1",
            repoId: "repo-code",
            provider: "github",
            providerRepoFullName: "acme/web",
            sourceRemoteBranch: "g5/feature",
            targetRemoteBranch: "main",
            providerPrNumber: 7,
            providerPrUrl: "https://github.com/acme/web/pull/7",
            providerStatus: "merged",
            sourceHeadSha: null,
            targetHeadSha: null,
            providerHeadSha: null,
            providerBaseSha: null,
            lastError: null,
            lastSyncedAt: null,
            updatedAt: "2026-09-01T00:00:00Z",
            mergedOutsideG5: true,
          },
        ],
      } as WorkspacePullRequestSummary,
    ]);
    expect(
      screen.getByTestId("pull-request-merged-outside-provider-1"),
    ).toHaveTextContent("merged on GitHub outside G5");
  });
});
