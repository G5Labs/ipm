import React, { useCallback, useMemo, useState } from "react";
import { Folder, FolderOpen } from "lucide-react";
import { useFileTreeRead } from "../../hooks/useFiles";
import type { ReviewSuggestionPathCount } from "../../lib/types";
import type { BranchSummary } from "../../lib/types";
import { fileNameFromPath } from "../../lib/utils";
import { FileTree, type FileTreeEntry } from "./FileTree";
import { TreeNode } from "./TreeNode";

export interface WorkspaceFileTreeProps {
  branches: BranchSummary[];
  organization: string;
  selectedFile: { branchId: string; path: string } | null;
  onSelectFile: (branchId: string, path: string) => void;
  onCreateFile?: (branchId: string, parentPath: string | null) => void;
  onCreateFolder?: (branchId: string, parentPath: string | null) => void;
  onDeleteEntry?: (branchId: string, entry: FileTreeEntry) => void;
  /** Open review suggestion counts per (repo, path). */
  suggestionCounts?: ReviewSuggestionPathCount[];
}

function repositoryLabel(branch: BranchSummary): string {
  if (branch.trackingMode === "remote_tracking" && branch.remoteRepoName) {
    return fileNameFromPath(branch.remoteRepoName);
  }
  return branch.repoName;
}

interface ExpandedRepositoryTreeProps {
  branch: BranchSummary;
  expandedPaths: Set<string>;
  selectedPath: string | null;
  onSelectFile: (path: string) => void;
  onToggleExpand: (path: string) => void;
  onCreateFile?: (parentPath: string | null) => void;
  onCreateFolder?: (parentPath: string | null) => void;
  onDeleteEntry?: (entry: FileTreeEntry) => void;
  badges?: Map<string, number>;
}

function ExpandedRepositoryTree({
  branch,
  expandedPaths,
  selectedPath,
  onSelectFile,
  onToggleExpand,
  onCreateFile,
  onCreateFolder,
  onDeleteEntry,
  badges,
}: ExpandedRepositoryTreeProps) {
  const tree = useFileTreeRead(
    branch.workspaceId,
    branch.repoId,
    undefined,
    -1,
  );

  if (tree.status === "loading") {
    return (
      <div role="group" className="px-9 py-3 text-xs text-[var(--g5-text2)]">
        Loading files…
      </div>
    );
  }

  if (tree.status === "error") {
    return (
      <div role="group" className="px-9 py-3 text-xs text-[var(--g5-blocked)]">
        Could not load files: {tree.error.message}
      </div>
    );
  }

  // The owner's live checkout was replaced by the durable cut: say so, so
  // uncommitted work that is not listed is not taken as gone
  // (g5.documents.read.labels).
  const fallbackNotice = tree.data.liveFallback ? (
    <div
      role="note"
      data-testid="workspace-file-tree-source"
      className="px-9 py-2 text-xs text-[var(--g5-text2)]"
    >
      {tree.data.liveFallback === "entry_cap"
        ? "Showing committed files: the live checkout has too many files to list."
        : tree.data.liveFallback === "runtime_outdated"
          ? "Showing committed files: restart the workspace to list the live checkout."
          : "Showing committed files instead of the live checkout."}
    </div>
  ) : null;

  // The listing stopped at the tree's cap: say the subtree has more, so
  // a file that is not listed is not taken as absent.
  const truncatedNotice = tree.data.truncated ? (
    <div
      role="note"
      data-testid="workspace-file-tree-truncated"
      className="px-9 py-2 text-xs text-[var(--g5-text2)]"
    >
      Showing the first files only: this folder has too many files to list.
    </div>
  ) : null;

  if (tree.data.entries.length === 0) {
    return (
      <div role="group">
        {fallbackNotice}
        {truncatedNotice}
        <div className="px-9 py-3 text-xs text-[var(--g5-text2)]">
          No files found
        </div>
      </div>
    );
  }

  return (
    <>
      {fallbackNotice}
      {truncatedNotice}
      <FileTree
        entries={tree.data.entries}
        selectedPath={selectedPath}
        expandedPaths={expandedPaths}
        onSelectFile={onSelectFile}
        onToggleExpand={onToggleExpand}
        onCreateFile={onCreateFile}
        onCreateFolder={onCreateFolder}
        onDeleteEntry={onDeleteEntry}
        rootRole="group"
        depthOffset={1}
        badges={badges}
      />
    </>
  );
}

interface RepositoryFileRootProps {
  branch: BranchSummary;
  suggestionCounts?: ReviewSuggestionPathCount[];
  selectedFile: WorkspaceFileTreeProps["selectedFile"];
  onSelectFile: WorkspaceFileTreeProps["onSelectFile"];
  onCreateFile?: WorkspaceFileTreeProps["onCreateFile"];
  onCreateFolder?: WorkspaceFileTreeProps["onCreateFolder"];
  onDeleteEntry?: WorkspaceFileTreeProps["onDeleteEntry"];
}

function RepositoryFileRoot({
  branch,
  suggestionCounts,
  selectedFile,
  onSelectFile,
  onCreateFile,
  onCreateFolder,
  onDeleteEntry,
}: RepositoryFileRootProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const badges = useMemo(() => {
    const map = new Map<string, number>();
    for (const count of suggestionCounts ?? []) {
      if (count.repoId === branch.repoId && count.openCount > 0) {
        map.set(count.path, count.openCount);
      }
    }
    return map;
  }, [suggestionCounts, branch.repoId]);
  const toggleRoot = useCallback(() => setIsExpanded((value) => !value), []);
  const togglePath = useCallback((path: string) => {
    setExpandedPaths((previous) => {
      const next = new Set(previous);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);
  const selectedPath =
    selectedFile?.branchId === branch.repoId ? selectedFile.path : null;

  return (
    <TreeNode
      label={repositoryLabel(branch)}
      depth={0}
      isSelected={false}
      isExpanded={isExpanded}
      isExpandable
      icon={
        isExpanded ? (
          <FolderOpen
            size={14}
            className="text-[var(--g5-info)]"
            aria-hidden="true"
          />
        ) : (
          <Folder
            size={14}
            className="text-[var(--g5-info)]"
            aria-hidden="true"
          />
        )
      }
      labelClassName="text-[var(--g5-info)]"
      dataNodeId={branch.repoId}
      onClick={toggleRoot}
      onToggleExpand={toggleRoot}
    >
      <ExpandedRepositoryTree
        branch={branch}
        badges={badges}
        expandedPaths={expandedPaths}
        selectedPath={selectedPath}
        onSelectFile={(path) => onSelectFile(branch.repoId, path)}
        onToggleExpand={togglePath}
        onCreateFile={
          onCreateFile
            ? (parentPath) => onCreateFile(branch.repoId, parentPath)
            : undefined
        }
        onCreateFolder={
          onCreateFolder
            ? (parentPath) => onCreateFolder(branch.repoId, parentPath)
            : undefined
        }
        onDeleteEntry={
          onDeleteEntry
            ? (entry) => onDeleteEntry(branch.repoId, entry)
            : undefined
        }
      />
    </TreeNode>
  );
}

export function WorkspaceFileTree({
  branches,
  selectedFile,
  onSelectFile,
  onCreateFile,
  onCreateFolder,
  onDeleteEntry,
  suggestionCounts,
}: WorkspaceFileTreeProps) {
  return (
    <div role="tree" aria-label="Workspace repository files">
      {branches.map((branch) => (
        <RepositoryFileRoot
          key={branch.repoId}
          branch={branch}
          selectedFile={selectedFile}
          onSelectFile={onSelectFile}
          onCreateFile={onCreateFile}
          onCreateFolder={onCreateFolder}
          onDeleteEntry={onDeleteEntry}
          suggestionCounts={suggestionCounts}
        />
      ))}
    </div>
  );
}
