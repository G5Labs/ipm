import { useQueries } from "@tanstack/react-query";
import { useState } from "react";
import {
  BookOpen,
  ChevronRight,
  FileQuestion,
  FolderTree,
  MessageCircle,
} from "lucide-react";
import { OntologyChangesButton } from "../editor/OntologyChangesView";
import { api } from "../../lib/api";
import type { BranchSummary, FileTreeEntry } from "../../lib/types";
import { toFileTreeRead, type FileTreeResponse } from "../../hooks/useFiles";
import { isCurrentOntologyPath } from "../../lib/currentOntology";
import {
  FINDINGS_LABEL,
  FINDINGS_PATHS,
  findingsFileOf,
} from "../../lib/findings";
import { readText } from "../../hooks/useVerificationEvidence";
import { ApiError } from "../../lib/api";
import { useWorkspaceReadAuthority } from "../../lib/workspaceLiveGate";

/**
 * How often the panel looks for findings files again: they change when a
 * verification run is reviewed, and a look reads the whole file.
 */
const FINDINGS_REFRESH_MS = 5 * 60_000;

/** Presentation over Files. No node identity, content scan or projection API. */
export function ontologyEntries(entries: FileTreeEntry[]): FileTreeEntry[] {
  return entries
    .flatMap((entry) => {
      if (entry.type === "file")
        return isCurrentOntologyPath(entry.path) ? [entry] : [];
      const children = ontologyEntries(entry.children ?? []);
      return children.length ? [{ ...entry, children }] : [];
    })
    .sort((a, b) => {
      if (a.path === ".g5/ontology/root.g5.md") return -1;
      if (b.path === ".g5/ontology/root.g5.md") return 1;
      return a.path.localeCompare(b.path);
    });
}

/** One display allowance across repositories; Files bounds every fetched subtree. */
export function boundOntologyTrees(
  trees: FileTreeEntry[][],
  maxEntries = 10_000,
  maxBytes = 3 * 1024 * 1024,
) {
  let truncated = false;
  const visit = (entries: FileTreeEntry[]): FileTreeEntry[] => {
    const result: FileTreeEntry[] = [];
    for (const entry of entries) {
      const cost =
        new TextEncoder().encode(JSON.stringify(entry.path)).length + 64;
      if (maxEntries === 0 || cost > maxBytes) {
        truncated = true;
        break;
      }
      maxEntries--;
      maxBytes -= cost;
      result.push(
        entry.children ? { ...entry, children: visit(entry.children) } : entry,
      );
    }
    return result;
  };
  return { trees: trees.map(visit), truncated };
}

function Entry({
  entry,
  repoId,
  onSelect,
  selected,
  search,
  onExplain,
  depth = 0,
}: {
  depth?: number;
  entry: FileTreeEntry;
  repoId: string;
  onSelect: (repoId: string, path: string) => void;
  selected?: { repoId: string; path: string } | null;
  search: string;
  onExplain?: (repoId: string, path: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const folder = entry.type === "directory";
  const isSelected =
    !folder && selected?.repoId === repoId && selected.path === entry.path;
  const isExpanded = expanded || !!search;
  const matches = (e: FileTreeEntry): boolean =>
    e.path.toLowerCase().includes(search) || !!e.children?.some(matches);
  if (search && !matches(entry)) return null;
  return (
    <li role="none" data-ontology-file={folder ? undefined : entry.path}>
      <div
        className={`group flex h-7 items-center border-l-2 text-sm transition-colors duration-[120ms] ${isSelected ? "border-[var(--g5-info)] bg-[var(--g5-bg2)]" : "border-transparent hover:bg-[var(--g5-bg2)]/50"}`}
        style={{ paddingLeft: depth * 16 + 12 }}
      >
        <button
          type="button"
          data-testid="ontology-file-treeitem"
          role="treeitem"
          aria-expanded={folder ? isExpanded : undefined}
          aria-selected={isSelected}
          className="flex h-full min-w-0 flex-1 items-center gap-1.5 text-left text-[var(--g5-text0)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--g5-info)]"
          onClick={() =>
            folder ? setExpanded(!expanded) : onSelect(repoId, entry.path)
          }
        >
          <span
            className={`flex h-4 w-4 shrink-0 items-center justify-center text-[var(--g5-text2)] ${folder ? "" : "invisible"}`}
          >
            <ChevronRight
              size={14}
              className={`transition-transform duration-[120ms] ${isExpanded ? "rotate-90" : ""}`}
            />
          </span>
          <span className="flex h-4 w-4 shrink-0 items-center justify-center">
            {folder ? (
              <FolderTree size={14} className="text-[var(--g5-ok)]" />
            ) : (
              <BookOpen size={14} className="text-[var(--g5-info)]" />
            )}
          </span>
          <span className="truncate">
            {entry.name.replace(/\.g5\.md$|\.md$/g, "")}
          </span>
        </button>
        {!folder && onExplain && (
          <button
            type="button"
            data-testid="ontology-file-explain-in-chat"
            aria-label={`Explain ${entry.name} in chat`}
            className="mr-2 rounded p-0.5 text-[var(--g5-text2)] opacity-0 transition-opacity duration-[120ms] hover:bg-[var(--g5-info)]/10 hover:text-[var(--g5-info)] group-hover:opacity-100 focus-visible:opacity-100 focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]"
            onClick={() => onExplain(repoId, entry.path)}
          >
            <MessageCircle size={12} />
          </button>
        )}
      </div>
      {folder && isExpanded && (
        <ul role="group">
          {entry.children?.map((child) => (
            <Entry
              key={child.path}
              entry={child}
              depth={depth + 1}
              repoId={repoId}
              onSelect={onSelect}
              selected={selected}
              search={search}
              onExplain={onExplain}
            />
          ))}
        </ul>
      )}
    </li>
  );
}

/**
 * A findings file of a repository (`g5.documents.findings.files`): listed
 * after its ontology files with its own symbol, and opened like them.
 */
function FindingsEntry({
  path,
  repoId,
  onSelect,
  selected,
}: {
  path: string;
  repoId: string;
  onSelect: (repoId: string, path: string) => void;
  selected?: { repoId: string; path: string } | null;
}) {
  const isSelected = selected?.repoId === repoId && selected.path === path;
  return (
    <li role="none" data-findings-file={path}>
      <div
        className={`flex h-7 items-center border-l-2 text-sm transition-colors duration-[120ms] ${isSelected ? "border-[var(--g5-info)] bg-[var(--g5-bg2)]" : "border-transparent hover:bg-[var(--g5-bg2)]/50"}`}
        style={{ paddingLeft: 12 }}
      >
        <button
          type="button"
          data-testid="ontology-findings-treeitem"
          role="treeitem"
          aria-selected={isSelected}
          className="flex h-full min-w-0 flex-1 items-center gap-1.5 text-left text-[var(--g5-text0)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--g5-info)]"
          onClick={() => onSelect(repoId, path)}
        >
          <span className="invisible flex h-4 w-4 shrink-0" />
          <span className="flex h-4 w-4 shrink-0 items-center justify-center">
            <FileQuestion
              size={14}
              className="text-[var(--g5-warn)]"
              aria-hidden
            />
          </span>
          <span className="truncate">
            {FINDINGS_LABEL[findingsFileOf(path)!]}
          </span>
        </button>
      </div>
    </li>
  );
}

export function OntologyTree({
  workspaceId,
  branches,
  onSelectFile,
  onOpenChanges,
  changesSelected = false,
  onExplainFile,
  selected,
  searchQuery = "",
}: {
  workspaceId: string;
  branches: BranchSummary[];
  onSelectFile: (repoId: string, path: string) => void;
  onOpenChanges?: () => void;
  changesSelected?: boolean;
  onExplainFile?: (repoId: string, path: string) => void;
  selected?: { repoId: string; path: string } | null;
  searchQuery?: string;
}) {
  const repos = [...new Map(branches.map((b) => [b.repoId, b])).values()];
  const reads = useQueries({
    queries: repos.map((repo) => ({
      queryKey: ["file-tree", workspaceId, repo.repoId, ".g5/ontology", -1],
      queryFn: ({ signal }: { signal: AbortSignal }) =>
        api.get<FileTreeResponse>(
          `/api/v1/workspaces/${workspaceId}/repos/${repo.repoId}/files`,
          { signal, params: { path: ".g5/ontology", depth: -1 } },
        ),
      select: toFileTreeRead,
      enabled: !!workspaceId,
      refetchInterval: 10_000,
    })),
  });
  // The findings files are read directly, since Files hides their folder,
  // and only in repositories that have an ontology.
  const authority = useWorkspaceReadAuthority();
  const findingsReads = useQueries({
    queries: repos.flatMap((repo, i) =>
      FINDINGS_PATHS.map((path) => ({
        queryKey: [
          "findings-file",
          workspaceId,
          repo.repoId,
          path,
          ...(authority ? [authority] : []),
        ],
        // A file that no longer exists drops out at the next look.
        queryFn: ({ signal }: { signal: AbortSignal }) =>
          readText(workspaceId, repo.repoId, path, signal).then(
            () => true,
            (error: unknown) => {
              if (error instanceof ApiError && error.status === 404)
                return false;
              throw error;
            },
          ),
        enabled: !!workspaceId && (reads[i]?.data?.entries.length ?? 0) > 0,
        retry: false,
        staleTime: FINDINGS_REFRESH_MS,
        refetchInterval: FINDINGS_REFRESH_MS,
      })),
    ),
  });
  const findingsOf = (repoIndex: number) =>
    FINDINGS_PATHS.filter(
      (path, k) =>
        findingsReads[repoIndex * FINDINGS_PATHS.length + k]?.data === true &&
        (!searchQuery ||
          path.toLowerCase().includes(searchQuery.toLowerCase()) ||
          FINDINGS_LABEL[findingsFileOf(path)!]
            .toLowerCase()
            .includes(searchQuery.toLowerCase())),
    );
  const bounded = boundOntologyTrees(
    reads.map((read) => (read.isError ? [] : (read.data?.entries ?? []))),
  );
  const entries = bounded.trees.map(ontologyEntries);
  const incomplete =
    bounded.truncated ||
    reads.some(
      (read) =>
        read.isPending ||
        read.isError ||
        read.data?.truncated ||
        read.data?.liveFallback,
    );
  const qualifying = entries.filter(
    (e, i) => e.length > 0 || findingsOf(i).length > 0,
  ).length;
  return (
    <div data-testid="ontology-tree">
      {onOpenChanges && (
        <OntologyChangesButton
          workspaceId={workspaceId}
          selected={changesSelected}
          onClick={onOpenChanges}
        />
      )}
      {bounded.truncated && (
        <p role="status" className="px-2 text-xs">
          Ontology listing reached the workspace display limit; results are
          incomplete.
        </p>
      )}
      {reads.map((read, i) => (
        <div key={repos[i].repoId}>
          {(entries[i].length > 0 ||
            findingsOf(i).length > 0 ||
            read.isError ||
            read.data?.truncated ||
            read.data?.liveFallback) &&
            (qualifying !== 1 || incomplete) && (
              <div className="px-2 pt-2 text-xs font-medium">
                {repos[i].repoName}
              </div>
            )}
          {read.isError && (
            <div role="alert" className="px-2 text-xs">
              Could not read ontology: {read.error.message}
            </div>
          )}
          {(read.data?.truncated || read.data?.liveFallback) && (
            <div role="status" className="px-2 text-xs">
              {read.data.truncated
                ? "Ontology listing is incomplete"
                : `Showing committed ontology (${read.data.liveFallback})`}
            </div>
          )}
          <ul role="tree" aria-label={`Ontology files in ${repos[i].repoName}`}>
            {entries[i].map((entry) => (
              <Entry
                key={entry.path}
                entry={entry}
                repoId={repos[i].repoId}
                onSelect={onSelectFile}
                selected={selected}
                search={searchQuery.toLowerCase()}
                onExplain={onExplainFile}
              />
            ))}
            {findingsOf(i).map((path) => (
              <FindingsEntry
                key={path}
                path={path}
                repoId={repos[i].repoId}
                onSelect={onSelectFile}
                selected={selected}
              />
            ))}
          </ul>
        </div>
      ))}
      {reads.some((read) => read.isPending) && (
        <p role="status" className="px-2 text-xs">
          Loading ontology…
        </p>
      )}
      {!incomplete && qualifying === 0 && (
        <p className="px-2 text-xs">No ontology files</p>
      )}
    </div>
  );
}
