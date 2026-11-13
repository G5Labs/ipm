import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AlertCircle, GitCommitHorizontal, Loader2 } from "lucide-react";

import {
  useBranchCommitDiff,
  useBranchCommitFileDiff,
} from "../../hooks/useTasks";
import { useWorkspaceNames } from "../../hooks/useWorkspaceNames";
import { ApiError } from "../../lib/api";
import type { CommitDiffFileSummary, DiffData } from "../../lib/types";
import { labelWorkspaceFolderInPath } from "../../lib/utils";
import { DiffFileBrowser } from "./DiffFileBrowser";

const EMPTY_COMMIT_FILES: CommitDiffFileSummary[] = [];

export interface WorkspaceCommitDiffViewProps {
  repoId: string;
  repoName: string;
  workspaceId: string;
  sha: string;
}

function canonicalPath(file: CommitDiffFileSummary): string {
  return file.newPath ?? file.oldPath ?? "";
}

function fileDisplay(
  file: CommitDiffFileSummary,
  workspaceNamesById: Record<string, string>,
): string {
  const status = file.status[0].toUpperCase() + file.status.slice(1);
  const displayPath = (path: string) =>
    labelWorkspaceFolderInPath(path, workspaceNamesById);
  if (
    file.status === "renamed" &&
    file.oldPath &&
    file.newPath &&
    file.oldPath !== file.newPath
  ) {
    return `${status} · ${displayPath(file.oldPath)} → ${displayPath(file.newPath)}`;
  }
  return `${status} · ${displayPath(canonicalPath(file))}`;
}

export function WorkspaceCommitDiffView({
  repoId,
  repoName,
  workspaceId,
  sha,
}: WorkspaceCommitDiffViewProps) {
  const queryClient = useQueryClient();
  const workspaceNamesById = useWorkspaceNames();
  const commitState = useBranchCommitDiff(workspaceId, repoId, sha);
  const files =
    commitState.status === "ok" ? commitState.data.files : EMPTY_COMMIT_FILES;
  const firstPath = files.length > 0 ? canonicalPath(files[0]) : "";
  const [selection, setSelection] = useState({ sha, path: "" });
  const selectedPath =
    selection.sha === sha && selection.path ? selection.path : firstPath;
  const selectedSummary =
    files.find((file) => canonicalPath(file) === selectedPath) ?? files[0];
  const effectivePath = selectedSummary ? canonicalPath(selectedSummary) : "";
  const fileState = useBranchCommitFileDiff(
    workspaceId,
    repoId,
    sha,
    effectivePath,
  );
  const summaryByPath = useMemo(
    () => new Map(files.map((file) => [canonicalPath(file), file])),
    [files],
  );
  const diffs = useMemo<DiffData[]>(
    () =>
      files.map((file) => {
        const path = canonicalPath(file);
        const selectedHunks =
          path === effectivePath &&
          fileState.status === "ok" &&
          fileState.data.status === "text"
            ? fileState.data.hunks
            : [];
        return {
          repoId,
          repoName,
          file: path,
          additions: file.additions,
          deletions: file.deletions,
          hunks: selectedHunks,
        };
      }),
    [effectivePath, fileState, files, repoId, repoName],
  );
  const selectedDiff =
    diffs.find((diff) => diff.file === effectivePath) ?? diffs[0] ?? null;

  if (commitState.status === "loading") {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-sm text-[var(--g5-text2)]">
        <Loader2 size={16} className="animate-spin" />
        Loading commit…
      </div>
    );
  }

  if (commitState.status === "error") {
    const removed =
      commitState.error instanceof ApiError &&
      commitState.error.is("NOT_FOUND");
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-sm text-[var(--g5-text2)]">
        <AlertCircle size={18} className="text-[var(--g5-blocked)]" />
        <p>
          {removed
            ? "This repository or commit is no longer available in the workspace."
            : `Failed to load commit: ${commitState.error.message}`}
        </p>
        {!removed && (
          <button
            type="button"
            data-testid="workspace-commit-diff-retry"
            className="rounded border border-[var(--g5-stroke)] px-3 py-1.5 hover:bg-[var(--g5-bg2)]"
            onClick={() =>
              void queryClient.refetchQueries({
                queryKey: [
                  "source-control-commit-diff",
                  repoId,
                  workspaceId,
                  sha,
                ],
                exact: true,
              })
            }
          >
            Retry
          </button>
        )}
      </div>
    );
  }

  const commit = commitState.data.commit;
  const parsedTime = new Date(commit.committedAt);
  const exactTime = Number.isFinite(parsedTime.getTime())
    ? parsedTime.toISOString()
    : undefined;
  const comparison =
    commitState.data.comparisonBase.kind === "emptyTree"
      ? "Root commit · compared with empty tree"
      : commitState.data.firstParentDiff
        ? `Merge commit · first-parent diff against ${commitState.data.comparisonBase.sha.slice(0, 7)}`
        : `Compared with parent ${commitState.data.comparisonBase.sha.slice(0, 7)}`;

  const selectedContent = selectedDiff ? (
    fileState.status === "loading" ? (
      <div className="flex h-full items-center justify-center gap-2 text-sm text-[var(--g5-text2)]">
        <Loader2 size={16} className="animate-spin" />
        Loading file diff…
      </div>
    ) : fileState.status === "error" ? (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-sm text-[var(--g5-blocked)]">
        Failed to load file diff: {fileState.error.message}
        <button
          type="button"
          data-testid="workspace-commit-file-diff-retry"
          className="rounded border border-[var(--g5-stroke)] px-3 py-1.5 text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]"
          onClick={() =>
            void queryClient.refetchQueries({
              queryKey: [
                "source-control-commit-file-diff",
                repoId,
                workspaceId,
                sha,
                effectivePath,
              ],
              exact: true,
            })
          }
        >
          Retry
        </button>
      </div>
    ) : fileState.data.status === "binary" ? (
      <div className="flex h-full items-center justify-center px-6 text-center text-sm text-[var(--g5-text2)]">
        Binary file — textual hunks are not available.
      </div>
    ) : fileState.data.status === "tooLarge" ? (
      <div className="flex h-full items-center justify-center px-6 text-center text-sm text-[var(--g5-text2)]">
        This diff exceeds the 100 KiB or 3,000-line display limit. No partial
        patch is shown.
      </div>
    ) : undefined
  ) : undefined;

  return (
    <div
      data-testid="workspace-commit-diff-view"
      className="flex h-full flex-col bg-[var(--g5-bg0)]"
    >
      <header className="shrink-0 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-4 py-3">
        <div className="flex items-start gap-3">
          <GitCommitHorizontal
            size={16}
            className="mt-0.5 shrink-0 text-[var(--g5-text2)]"
          />
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold text-[var(--g5-text0)]">
              {commit.summary || "Untitled commit"}
            </div>
            <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs text-[var(--g5-text2)]">
              <span>{repoName}</span>
              <span className="font-mono" title={commit.sha}>
                {commit.shortSha}
              </span>
              <span>{commit.authorName || "Unknown author"}</span>
              <time dateTime={exactTime} title={exactTime ?? "Unknown time"}>
                {exactTime ?? "Unknown time"}
              </time>
              <span>{comparison}</span>
              <span>
                +{commitState.data.totalAdditions} −
                {commitState.data.totalDeletions}
              </span>
            </div>
          </div>
          <span className="text-xs text-[var(--g5-text2)]">Read-only</span>
        </div>
      </header>

      {files.length === 0 ? (
        <div className="flex flex-1 items-center justify-center px-6 text-center text-sm text-[var(--g5-text2)]">
          Empty commit — no files changed relative to its comparison base.
        </div>
      ) : (
        <DiffFileBrowser
          diffs={diffs}
          selectedKey={
            selectedDiff ? `${repoId}\u0000${selectedDiff.file}` : null
          }
          selectedDiff={selectedDiff}
          onSelectDiff={(diff) => setSelection({ sha, path: diff.file })}
          formatFilePath={(path) => {
            const summary = summaryByPath.get(path);
            return summary
              ? fileDisplay(summary, workspaceNamesById)
              : labelWorkspaceFolderInPath(path, workspaceNamesById);
          }}
          selectedContent={selectedContent}
          readOnlyLabel="Commit history"
          emptySelectionMessage="Select a changed file to view its diff."
          changedFilesTestId="commit-changed-files"
          fileButtonTestId="commit-changed-file"
          diffViewTestId="commit-file-diff"
        />
      )}
    </div>
  );
}
