import React, { useCallback, useEffect, useMemo, useState } from "react";
import { File, FileCode, Folder, FolderOpen, Plus, Trash2 } from "lucide-react";
import { useWorkspaceNames } from "../../hooks/useWorkspaceNames";
import { fileNameFromPath, labelWorkspaceFolderInPath } from "../../lib/utils";
import { useLiveGateReason } from "../../lib/workspaceLiveGate";
import { TreeNode, INTERNAL_WS_PREFIX } from "./TreeNode";
import {
  CompactContextMenu,
  CompactContextMenuItem,
} from "./CompactContextMenu";

// ---------------------------------------------------------------------------
// FileTree types
// ---------------------------------------------------------------------------

/**
 * Local FileTreeEntry interface for the FileTree component.
 * Accepts both 'dir' (from spec) and 'directory' (from data-layer types)
 * for maximum compatibility.
 */
export interface FileTreeEntry {
  name: string;
  path: string;
  type: "file" | "dir" | "directory";
  children?: FileTreeEntry[];
}

export interface FileTreeProps {
  entries: FileTreeEntry[];
  selectedPath: string | null;
  expandedPaths: Set<string>;
  onSelectFile: (path: string) => void;
  onToggleExpand: (path: string) => void;
  onCreateFile?: (parentPath: string | null) => void;
  onCreateFolder?: (parentPath: string | null) => void;
  onDeleteEntry?: (entry: FileTreeEntry) => void;
  rootRole?: "tree" | "group";
  depthOffset?: number;
  /** Open review suggestion counts by file path, shown as a trailing badge. */
  badges?: Map<string, number>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function isDirectory(entry: FileTreeEntry): boolean {
  return entry.type === "dir" || entry.type === "directory";
}

function parentPath(path: string): string | null {
  const normalized = path.replace(/\/+$/, "");
  const index = normalized.lastIndexOf("/");
  return index > 0 ? normalized.slice(0, index) : null;
}

// ---------------------------------------------------------------------------
// File-type icon mapping
// ---------------------------------------------------------------------------

function getFileIcon(
  entry: FileTreeEntry,
  isExpanded: boolean,
): React.ReactNode {
  if (isDirectory(entry)) {
    return isExpanded ? <FolderOpen size={14} /> : <Folder size={14} />;
  }

  // Source code files
  const ext = entry.name.split(".").pop()?.toLowerCase();
  const codeExtensions = new Set([
    "ts",
    "tsx",
    "js",
    "jsx",
    "rs",
    "py",
    "go",
    "cpp",
    "c",
    "h",
    "hpp",
    "java",
    "rb",
    "php",
    "swift",
    "kt",
    "scala",
    "cs",
    "vue",
    "svelte",
  ]);
  if (ext && codeExtensions.has(ext)) {
    return <FileCode size={14} />;
  }

  return <File size={14} />;
}

// ---------------------------------------------------------------------------
// FileTreeNode (internal) — single file/directory
// ---------------------------------------------------------------------------

interface FileTreeNodeProps {
  entry: FileTreeEntry;
  depth: number;
  workspaceNamesById: Record<string, string>;
  selectedPath: string | null;
  expandedPaths: Set<string>;
  onSelectFile: (path: string) => void;
  onToggleExpand: (path: string) => void;
  onOpenContextMenu?: (
    event: React.MouseEvent<HTMLDivElement>,
    entry: FileTreeEntry,
  ) => void;
  badges?: Map<string, number>;
}

const FileTreeNode = React.memo(function FileTreeNode({
  entry,
  depth,
  workspaceNamesById,
  selectedPath,
  expandedPaths,
  onSelectFile,
  onToggleExpand,
  onOpenContextMenu,
  badges,
}: FileTreeNodeProps) {
  const isDir = isDirectory(entry);
  const isExpanded = expandedPaths.has(entry.path);
  const isSelected = selectedPath === entry.path;
  const isExpandable = isDir && (entry.children?.length ?? 0) > 0;
  const displayPath = labelWorkspaceFolderInPath(
    entry.path,
    workspaceNamesById,
  );
  const displayName =
    displayPath === entry.path ? entry.name : fileNameFromPath(displayPath);

  return (
    <TreeNode
      label={displayName}
      depth={depth}
      isSelected={isSelected}
      isExpanded={isExpanded}
      isExpandable={isExpandable}
      icon={getFileIcon(entry, isExpanded)}
      onClick={() => {
        if (!isDir) {
          onSelectFile(entry.path);
        } else {
          onToggleExpand(entry.path);
        }
      }}
      onToggleExpand={() => onToggleExpand(entry.path)}
      onContextMenu={(event) => onOpenContextMenu?.(event, entry)}
      trailing={
        !isDir && badges?.get(entry.path) ? (
          <span
            data-testid="file-suggestion-badge"
            className="rounded-full bg-[var(--g5-warn)]/20 px-1.5 text-[10px] font-medium leading-4 text-[var(--g5-text0)]"
            title={`${badges.get(entry.path)} open review suggestion${badges.get(entry.path) === 1 ? "" : "s"}`}
          >
            {badges.get(entry.path)}
          </span>
        ) : undefined
      }
    >
      {/* Recursively render children when expanded — filter internal workspace entries, sorted dirs-first, then alphabetically */}
      {entry.children
        ?.filter((child) => !child.name.startsWith(INTERNAL_WS_PREFIX))
        .sort((a, b) => {
          const aIsDir = isDirectory(a);
          const bIsDir = isDirectory(b);
          if (aIsDir !== bIsDir) return aIsDir ? -1 : 1;
          return a.name.localeCompare(b.name);
        })
        .map((child) => (
          <FileTreeNode
            key={child.path}
            entry={child}
            depth={depth + 1}
            workspaceNamesById={workspaceNamesById}
            selectedPath={selectedPath}
            expandedPaths={expandedPaths}
            onSelectFile={onSelectFile}
            onToggleExpand={onToggleExpand}
            onOpenContextMenu={onOpenContextMenu}
            badges={badges}
          />
        ))}
    </TreeNode>
  );
});

// ---------------------------------------------------------------------------
// FileTree — main component
// ---------------------------------------------------------------------------

export function FileTree({
  entries,
  selectedPath,
  expandedPaths,
  onSelectFile,
  onToggleExpand,
  onCreateFile,
  onCreateFolder,
  onDeleteEntry,
  rootRole = "tree",
  depthOffset = 0,
  badges,
}: FileTreeProps) {
  const workspaceNamesById = useWorkspaceNames();
  // Creating and deleting files writes the running workspace's checkout:
  // the owner keeps the items while the VM is stopped, disabled with the
  // reason (`g5.workspace.ui.unavailable`).
  const writeBlocked = useLiveGateReason("change files");
  const [contextMenu, setContextMenu] = useState<{
    entry: FileTreeEntry | null;
    parentPath: string | null;
    x: number;
    y: number;
  } | null>(null);

  useEffect(() => {
    if (!contextMenu) return;

    const close = () => setContextMenu(null);
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };

    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [contextMenu]);

  const openContextMenu = useCallback(
    (event: React.MouseEvent<HTMLDivElement>, entry: FileTreeEntry) => {
      if (!onCreateFile && !onCreateFolder && !onDeleteEntry) return;
      event.preventDefault();
      event.stopPropagation();
      setContextMenu({
        entry,
        parentPath: isDirectory(entry) ? entry.path : parentPath(entry.path),
        x: event.clientX,
        y: event.clientY,
      });
    },
    [onCreateFile, onCreateFolder, onDeleteEntry],
  );

  const openRootContextMenu = useCallback(
    (event: React.MouseEvent<HTMLDivElement>) => {
      if (!onCreateFile && !onCreateFolder) return;
      event.preventDefault();
      setContextMenu({
        entry: null,
        parentPath: null,
        x: event.clientX,
        y: event.clientY,
      });
    },
    [onCreateFile, onCreateFolder],
  );

  const confirmDeleteEntry = useCallback(() => {
    if (!contextMenu || !onDeleteEntry) return;
    const { entry } = contextMenu;
    if (!entry) return;
    setContextMenu(null);
    onDeleteEntry(entry);
  }, [contextMenu, onDeleteEntry]);

  const createFile = useCallback(() => {
    if (!contextMenu || !onCreateFile) return;
    const { parentPath } = contextMenu;
    setContextMenu(null);
    onCreateFile(parentPath);
  }, [contextMenu, onCreateFile]);

  const createFolder = useCallback(() => {
    if (!contextMenu || !onCreateFolder) return;
    const { parentPath } = contextMenu;
    setContextMenu(null);
    onCreateFolder(parentPath);
  }, [contextMenu, onCreateFolder]);

  // Filter out internal workspace marker folders, then sort: directories first, then alphabetically
  const sortedEntries = useMemo(() => {
    return [...entries]
      .filter((e) => !e.name.startsWith(INTERNAL_WS_PREFIX))
      .sort((a, b) => {
        const aIsDir = isDirectory(a);
        const bIsDir = isDirectory(b);
        if (aIsDir !== bIsDir) {
          return aIsDir ? -1 : 1;
        }
        return a.name.localeCompare(b.name);
      });
  }, [entries]);

  if (sortedEntries.length === 0) {
    return (
      <div onContextMenu={openRootContextMenu}>
        <div className="px-3 py-4 text-xs text-[var(--g5-text2)] text-center">
          No files found
        </div>
        {contextMenu && (onCreateFile || onCreateFolder) ? (
          <CompactContextMenu
            x={contextMenu.x}
            y={contextMenu.y}
            label="File actions"
          >
            {onCreateFile ? (
              <CompactContextMenuItem
                icon={<Plus size={13} />}
                label="New File"
                onClick={createFile}
                blockedReason={writeBlocked}
              />
            ) : null}
            {onCreateFolder ? (
              <CompactContextMenuItem
                icon={<Folder size={13} />}
                label="New Folder"
                onClick={createFolder}
                blockedReason={writeBlocked}
              />
            ) : null}
          </CompactContextMenu>
        ) : null}
      </div>
    );
  }

  return (
    <>
      <div role={rootRole} onContextMenu={openRootContextMenu}>
        {sortedEntries.map((entry) => (
          <FileTreeNode
            key={entry.path}
            entry={entry}
            depth={depthOffset}
            badges={badges}
            workspaceNamesById={workspaceNamesById}
            selectedPath={selectedPath}
            expandedPaths={expandedPaths}
            onSelectFile={onSelectFile}
            onToggleExpand={onToggleExpand}
            onOpenContextMenu={openContextMenu}
          />
        ))}
      </div>
      {contextMenu && (onCreateFile || onCreateFolder || onDeleteEntry) ? (
        <CompactContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          label="File actions"
        >
          {onCreateFile ? (
            <CompactContextMenuItem
              icon={<Plus size={13} />}
              label="New File"
              onClick={createFile}
              blockedReason={writeBlocked}
            />
          ) : null}
          {onCreateFolder ? (
            <CompactContextMenuItem
              icon={<Folder size={13} />}
              label="New Folder"
              onClick={createFolder}
              blockedReason={writeBlocked}
            />
          ) : null}
          {contextMenu.entry && onDeleteEntry ? (
            <CompactContextMenuItem
              icon={<Trash2 size={13} />}
              label="Delete"
              tone="danger"
              onClick={confirmDeleteEntry}
              blockedReason={writeBlocked}
            />
          ) : null}
        </CompactContextMenu>
      ) : null}
    </>
  );
}
