import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AsyncState,
  CommitDiffResponse,
  CommitFileDiffResponse,
} from "../../lib/types";
import { ApiError } from "../../lib/api";
import { WorkspaceNamesProvider } from "../../hooks/useWorkspaceNames";
import { renderApp } from "../../test/render";

const { mockUseBranchCommitDiff, mockUseBranchCommitFileDiff } = vi.hoisted(
  () => ({
    mockUseBranchCommitDiff: vi.fn(),
    mockUseBranchCommitFileDiff: vi.fn(),
  }),
);

vi.mock("../../hooks/useTasks", () => ({
  useBranchCommitDiff: (...args: unknown[]) => mockUseBranchCommitDiff(...args),
  useBranchCommitFileDiff: (...args: unknown[]) =>
    mockUseBranchCommitFileDiff(...args),
}));

import { WorkspaceCommitDiffView } from "./WorkspaceCommitDiffView";

const sha = "a".repeat(40);
const parentSha = "b".repeat(40);

function commitDiff(
  overrides: Partial<CommitDiffResponse> = {},
): CommitDiffResponse {
  return {
    commit: {
      sha,
      shortSha: sha.slice(0, 7),
      summary: "Add history view",
      authorName: "Ada Lovelace",
      committedAt: "2026-07-15T12:00:00Z",
      parentCount: 1,
    },
    comparisonBase: { kind: "parent", sha: parentSha },
    firstParentDiff: false,
    files: [
      {
        status: "modified",
        oldPath: "src/history.ts",
        newPath: "src/history.ts",
        additions: 2,
        deletions: 1,
      },
      {
        status: "renamed",
        oldPath: "src/old.ts",
        newPath: "src/new.ts",
        additions: 0,
        deletions: 0,
      },
    ],
    totalAdditions: 2,
    totalDeletions: 1,
    ...overrides,
  };
}

function textFileDiff(path: string): CommitFileDiffResponse {
  return {
    path,
    status: "text",
    additions: 2,
    deletions: 1,
    hunks: [
      {
        startLine: 1,
        header: "@@ -1 +1,2 @@",
        lines: [
          { type: "deletion", oldLine: 1, content: "old" },
          { type: "addition", newLine: 1, content: "new" },
        ],
      },
    ],
  };
}

function setCommit(state: AsyncState<CommitDiffResponse>): void {
  mockUseBranchCommitDiff.mockReturnValue(state);
}

function setFile(state: AsyncState<CommitFileDiffResponse>): void {
  mockUseBranchCommitFileDiff.mockReturnValue(state);
}

describe("WorkspaceCommitDiffView", () => {
  beforeEach(() => {
    mockUseBranchCommitDiff.mockReset();
    mockUseBranchCommitFileDiff.mockReset();
    setFile({ status: "loading" });
  });

  it("shows immutable commit metadata and lazily selects the first changed file", async () => {
    const user = userEvent.setup();
    setCommit({ status: "ok", data: commitDiff() });
    setFile({ status: "ok", data: textFileDiff("src/history.ts") });

    renderApp(
      <WorkspaceCommitDiffView
        repoId="repo-code"
        repoName="Code"
        workspaceId="workspace-1"
        sha={sha}
      />,
    );

    expect(screen.getByText("Add history view")).toBeInTheDocument();
    expect(
      screen.getByText(/Compared with parent bbbbbbb/),
    ).toBeInTheDocument();
    expect(screen.getByText("Read-only")).toBeInTheDocument();
    await waitFor(() =>
      expect(mockUseBranchCommitFileDiff).toHaveBeenLastCalledWith(
        "workspace-1",
        "repo-code",
        sha,
        "src/history.ts",
      ),
    );
    expect(screen.getByTestId("commit-file-diff")).toHaveTextContent("new");
    expect(
      screen.getByText(/Renamed · src\/old.ts → src\/new.ts/),
    ).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: /Renamed · src\/old.ts/ }),
    );
    await waitFor(() =>
      expect(mockUseBranchCommitFileDiff).toHaveBeenLastCalledWith(
        "workspace-1",
        "repo-code",
        sha,
        "src/new.ts",
      ),
    );
  });

  it("labels Project Docs paths with workspace names while selecting raw paths", async () => {
    const user = userEvent.setup();
    const workspaceFolderId = "447dd8fd-bc1f-438a-b309-34802979529d";
    const addedPath = `docs/${workspaceFolderId}/a.md`;
    const renamedOldPath = `docs/${workspaceFolderId}/old.md`;
    const renamedNewPath = `docs/${workspaceFolderId}/new.md`;
    setCommit({
      status: "ok",
      data: commitDiff({
        files: [
          {
            status: "added",
            oldPath: null,
            newPath: addedPath,
            additions: 3,
            deletions: 0,
          },
          {
            status: "renamed",
            oldPath: renamedOldPath,
            newPath: renamedNewPath,
            additions: 0,
            deletions: 0,
          },
        ],
      }),
    });
    setFile({ status: "ok", data: textFileDiff(addedPath) });

    renderApp(
      <WorkspaceNamesProvider value={{ [workspaceFolderId]: "Project Atlas" }}>
        <WorkspaceCommitDiffView
          repoId="repo-docs"
          repoName="Docs"
          workspaceId="workspace-1"
          sha={sha}
        />
      </WorkspaceNamesProvider>,
    );

    const addedDisplay = "Added · docs/Project Atlas/a.md";
    const renamedDisplay =
      "Renamed · docs/Project Atlas/old.md → docs/Project Atlas/new.md";
    const renamedAria = `${renamedDisplay}, 0 additions, 0 deletions`;
    expect(screen.getAllByText(addedDisplay)).toHaveLength(2);
    expect(
      screen.getByRole("button", {
        name: `${addedDisplay}, 3 additions, 0 deletions`,
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: renamedAria,
      }),
    ).toBeInTheDocument();
    expect(screen.queryByText(new RegExp(workspaceFolderId))).toBeNull();
    await waitFor(() =>
      expect(mockUseBranchCommitFileDiff).toHaveBeenLastCalledWith(
        "workspace-1",
        "repo-docs",
        sha,
        addedPath,
      ),
    );

    await user.click(screen.getByRole("button", { name: renamedAria }));
    await waitFor(() =>
      expect(mockUseBranchCommitFileDiff).toHaveBeenLastCalledWith(
        "workspace-1",
        "repo-docs",
        sha,
        renamedNewPath,
      ),
    );
  });

  it("renders an invalid commit timestamp without throwing", () => {
    const data = commitDiff();
    data.commit.committedAt = "not-a-timestamp";
    setCommit({ status: "ok", data });

    renderApp(
      <WorkspaceCommitDiffView
        repoId="repo-code"
        repoName="Code"
        workspaceId="workspace-1"
        sha={sha}
      />,
    );

    const time = screen.getByText("Unknown time");
    expect(time).toBeInTheDocument();
    expect(time).not.toHaveAttribute("datetime");
  });

  it.each([
    ["binary" as const, /Binary file/],
    ["tooLarge" as const, /100 KiB or 3,000-line/],
  ])("renders the %s file state without partial hunks", (status, message) => {
    setCommit({ status: "ok", data: commitDiff() });
    setFile({
      status: "ok",
      data: {
        path: "src/history.ts",
        status,
        additions: 0,
        deletions: 0,
        hunks: [],
      },
    });

    renderApp(
      <WorkspaceCommitDiffView
        repoId="repo-code"
        repoName="Code"
        workspaceId="workspace-1"
        sha={sha}
      />,
    );

    expect(screen.getByText(message)).toBeInTheDocument();
    expect(screen.queryByTestId("commit-file-diff")).not.toBeInTheDocument();
  });

  it("distinguishes empty commits and removed repositories", () => {
    setCommit({ status: "ok", data: commitDiff({ files: [] }) });
    const view = renderApp(
      <WorkspaceCommitDiffView
        repoId="repo-code"
        repoName="Code"
        workspaceId="workspace-1"
        sha={sha}
      />,
    );
    expect(screen.getByText(/Empty commit/)).toBeInTheDocument();

    setCommit({
      status: "error",
      error: new ApiError({
        code: "NOT_FOUND",
        message: "missing",
        status: 404,
      }),
    });
    view.unmount();
    renderApp(
      <WorkspaceCommitDiffView
        repoId="repo-code"
        repoName="Code"
        workspaceId="workspace-1"
        sha={sha}
      />,
    );
    expect(screen.getByText(/no longer available/)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Retry" }),
    ).not.toBeInTheDocument();
  });
});
