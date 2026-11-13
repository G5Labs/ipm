import type { DiffHunk } from "./types";
import type { ChangeFileKind } from "./changeFiles";
export interface SourceCommit {
  paths?: string[];
  sha: string;
  summary: string;
  authorName: string;
  committedAt: number;
  parents: string[];
}
export interface SourceRepo {
  repoId: string;
  name: string;
  base: string | null;
  workspaceBase?: string | null;
  parent?: string | null;
  parentName?: string | null;
  historyCommits?: SourceCommit[];
  comparison?: {
    mergeBase: string;
    mergeBaseCommit: SourceCommit;
    parentMerges: string[];
    workspaceBase: SourceCommit | null;
    workspaceBaseInHistory: boolean;
    sinceWorkspaceBaseCount: number;
  } | null;
  /** Personal repositories expose current unpushed work without a fork base. */
  unpushedOnly?: boolean;
  pushed: string;
  commits: SourceCommit[];
  truncated: boolean;
  error: string | null;
  live: {
    selectable?: boolean;
    unavailableReason?: string;
  } | null;
}
export interface SourceMenu {
  generation?: number;
  fingerprint: string;
  repos: SourceRepo[];
  private: boolean;
}
export interface RepoSelection {
  checked: string[];
  followAll: boolean;
}
export type SourceSelection = Record<string, RepoSelection>;
export const UNPUSHED = "unpushed";
export function sourceChildren(repo: SourceRepo): string[] {
  return [
    ...(repo.live && repo.live.selectable !== false ? [UNPUSHED] : []),
    ...repo.commits.map((c) => c.sha),
  ];
}
export function selectAll(menu: SourceMenu): SourceSelection {
  return Object.fromEntries(
    menu.repos.map((r) => [
      r.repoId,
      { checked: r.error ? [] : sourceChildren(r), followAll: true },
    ]),
  );
}
/** Represent the captured pushed vector using the menu's bounded ancestry. */
export function sourcePushedSelection(
  menu: SourceMenu,
  heads: Record<string, string>,
): SourceSelection {
  return Object.fromEntries(
    menu.repos.map((repo) => {
      const head = heads[repo.repoId];
      let checked: string[] = [];
      if (!repo.error && head && head !== repo.base) {
        if (head === repo.pushed)
          checked = repo.commits.map((commit) => commit.sha);
        else {
          const commits = new Map(
            repo.commits.map((commit) => [commit.sha, commit]),
          );
          const seen = new Set<string>();
          const pending = [head];
          while (pending.length) {
            const sha = pending.pop()!;
            if (seen.has(sha)) continue;
            seen.add(sha);
            const commit = commits.get(sha);
            if (commit) pending.push(...commit.parents);
          }
          checked = repo.commits
            .filter((commit) => seen.has(commit.sha))
            .map((commit) => commit.sha);
          // An older captured tip must stay explicit rather than selecting newer
          // commits or pretending the bounded menu contains its missing history.
          if (!commits.has(head)) checked.push(head);
        }
      }
      return [repo.repoId, { checked, followAll: false }];
    }),
  );
}
export function reconcileSelection(
  _before: SourceMenu,
  after: SourceMenu,
  selection: SourceSelection,
): SourceSelection {
  return Object.fromEntries(
    after.repos.map((repo) => {
      const old = selection[repo.repoId];
      if (!old) return [repo.repoId, { checked: [], followAll: false }];
      const available = sourceChildren(repo);
      // Full selections follow the outgoing range. Custom commit IDs stay
      // pending until the server confirms absence; unloaded rows prove nothing.
      const retained =
        old.followAll && !repo.error
          ? available
          : old.checked.filter(
              (id) => id !== UNPUSHED || available.includes(id),
            );
      return [
        repo.repoId,
        {
          checked: [
            ...new Set([
              ...retained,
              ...(old.followAll && !repo.error ? available : []),
            ]),
          ],
          followAll: old.followAll,
        },
      ];
    }),
  );
}
export interface SourcePatch {
  kind: ChangeFileKind;
  fileKey?: string;
  repoId: string;
  repoName: string;
  label: string;
  before: unknown;
  after: string | null;
  file: {
    path: string;
    oldPath: string | null;
    change: string;
    diff: {
      status: string;
      additions: number;
      deletions: number;
      hunks: DiffHunk[];
    };
  };
}
export interface SourceSnapshot {
  menu?: SourceMenu;
  selectedCommits?: Record<string, SourceCommit>;
  emptyOwnChanges?: boolean;
  patches: SourcePatch[];
  fileCount: number;
  private: boolean;
}
export function sourceApplyInput(menu: SourceMenu, selection: SourceSelection) {
  return {
    fingerprint: menu.fingerprint,
    repos: menu.repos
      .filter(
        (r) =>
          selection[r.repoId]?.checked.length ||
          (selection[r.repoId]?.followAll &&
            !r.unpushedOnly &&
            !r.error &&
            r.base === r.pushed),
      )
      .map((r) => ({
        repoId: r.repoId,
        base: r.base,
        pushed: r.pushed,
        commits: selection[r.repoId].checked.filter((id) => id !== UNPUSHED),
        includeUnpushed: selection[r.repoId].checked.includes(UNPUSHED),
        allPushed:
          !r.unpushedOnly &&
          !r.error &&
          (selection[r.repoId].followAll ||
            (!r.truncated &&
              r.commits.length > 0 &&
              r.commits.every((c) =>
                selection[r.repoId].checked.includes(c.sha),
              ) &&
              selection[r.repoId].checked.every(
                (id) => id === UNPUSHED || r.commits.some((c) => c.sha === id),
              ))),
      })),
  };
}

export interface SourceListRefresh {
  refreshed: true;
  menu: SourceMenu;
  missingCommits?: Record<string, string[]>;
}
