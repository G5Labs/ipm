import { useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import {
  reviewUrl,
  type ReviewOverview,
  type VectorRevision,
} from "../../hooks/useWorkspaceReviewFlow";
import { ReviewDialog, reviewButton } from "./ReviewDialog";
export function WorkspaceCommitVectors({
  rows,
  overview,
  onSelect,
}: {
  rows: VectorRevision[];
  overview: ReviewOverview;
  onSelect?: (r: VectorRevision) => void;
}) {
  // A recorded cut can contain repositories no longer in current membership.
  // Keep every member visible so an old revision remains a complete vector.
  const repos = [...overview.repos];
  for (const row of rows) {
    for (const head of row.heads) {
      if (!repos.some((repo) => repo.id === head.repoId)) {
        repos.push({
          id: head.repoId,
          name: head.repoId,
          role: "",
          rootPath: "",
        });
      }
    }
  }
  const isCurrent = (row: VectorRevision) =>
    row.heads.length === overview.heads.length &&
    row.heads.every((head) =>
      overview.heads.some(
        (current) =>
          current.repoId === head.repoId &&
          current.headCommit === head.headCommit,
      ),
    );
  return (
    <div className="overflow-x-auto">
      <table
        aria-label="Workspace commit vectors"
        className="w-full text-left text-xs"
      >
        <thead>
          <tr>
            <th className="p-3">Workspace revision</th>
            {repos.map((r) => (
              <th className="p-3" key={r.id}>
                {r.name}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.id} className="border-t border-[var(--g5-stroke)]">
              <td className="p-3">
                <span>
                  {row.id === "current" || isCurrent(row)
                    ? "Current pushed state"
                    : row.observedAt
                      ? new Date(row.observedAt).toLocaleString()
                      : row.id === "before"
                        ? "Comparison baseline"
                        : row.id === "after"
                          ? "Compared revision"
                          : "Captured pushed revision"}
                </span>
                {overview.baseHeads?.length === row.heads.length &&
                  overview.baseHeads.every((b) =>
                    row.heads.some(
                      (h) =>
                        h.repoId === b.repoId && h.headCommit === b.headCommit,
                    ),
                  ) && <p>Workspace base</p>}
                {row.workspaceId !== overview.heads[0]?.workspaceId && (
                  <p>Inherited history</p>
                )}
                {onSelect && row.id !== "current" && (
                  <button
                    type="button"
                    data-testid="review-vector-inspect"
                    className="block text-[var(--g5-info)]"
                    onClick={() => onSelect(row)}
                  >
                    Changes from previous
                  </button>
                )}
              </td>
              {repos.map((repo) => {
                const h = row.heads.find((h) => h.repoId === repo.id);
                const pushed = row.pushedAt[repo.id];
                return (
                  <td key={repo.id} data-repo-id={repo.id} className="p-3">
                    <code
                      aria-label={h ? `Commit ${h.headCommit}` : undefined}
                      title={h?.headCommit}
                    >
                      {h?.headCommit.slice(0, 10) ??
                        "Not a member at this revision"}
                    </code>
                    {h && (
                      <CommitDetails
                        workspaceId={
                          overview.heads[0]?.workspaceId ?? row.workspaceId
                        }
                        repoId={repo.id}
                        head={h.headCommit}
                        revisionId={
                          /^[0-9a-f-]{36}$/.test(row.id) ? row.id : undefined
                        }
                      />
                    )}

                    <div className="mt-1 text-[var(--g5-text2)]">
                      {pushed ? (
                        <time aria-label="Pushed at" dateTime={pushed}>
                          {new Date(pushed).toLocaleString()}
                        </time>
                      ) : (
                        "Push time unknown"
                      )}
                    </div>
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
export function WorkspaceRevisionDialog({
  workspaceId,
  overview,
  history = false,
  onClose,
  onSelect,
}: {
  workspaceId: string;
  overview: ReviewOverview;
  history?: boolean;
  onClose: () => void;
  onSelect?: (r: VectorRevision) => void;
}) {
  const query = useInfiniteQuery({
    queryKey: ["workspace-review-history", workspaceId],
    initialPageParam: "",
    queryFn: ({ pageParam }) =>
      api.get<{ rows: VectorRevision[]; nextCursor: string | null }>(
        `${reviewUrl(workspaceId)}/history${pageParam ? `?before=${encodeURIComponent(pageParam)}` : ""}`,
      ),
    getNextPageParam: (p) => p.nextCursor ?? undefined,
    enabled: history,
  });
  const current: VectorRevision = {
    id: "current",
    workspaceId,
    observedAt: overview.pr?.createdAt ?? "",
    heads: overview.heads,
    pushedAt: overview.pushedAt,
  };
  return (
    <ReviewDialog
      title={history ? "Commit History" : "Pushed revision"}
      onClose={onClose}
    >
      <WorkspaceCommitVectors
        rows={
          history
            ? (() => {
                const rows = query.data?.pages.flatMap((p) => p.rows) ?? [];
                const recorded = rows[0];
                return recorded &&
                  JSON.stringify(
                    recorded.heads.map((h) => [h.repoId, h.headCommit]).sort(),
                  ) ===
                    JSON.stringify(
                      current.heads.map((h) => [h.repoId, h.headCommit]).sort(),
                    )
                  ? rows
                  : [current, ...rows];
              })()
            : [current]
        }
        overview={overview}
        onSelect={history ? onSelect : undefined}
      />
      {history && query.isPending && <p role="status">Loading history…</p>}
      {history && query.error && (
        <p role="alert">
          Could not load history.{" "}
          <button
            type="button"
            data-testid="review-history-retry"
            onClick={() => void query.refetch()}
          >
            Retry
          </button>
        </p>
      )}
      {history && query.data?.pages[0]?.rows.length === 0 && (
        <p>
          No recorded vector history yet. The current pushed state is available;
          earlier push times are unknown.
        </p>
      )}
      {history && query.hasNextPage && (
        <button
          type="button"
          data-testid="review-history-older"
          className={reviewButton}
          disabled={query.isFetchingNextPage}
          onClick={() => void query.fetchNextPage()}
        >
          Load earlier revisions
        </button>
      )}
    </ReviewDialog>
  );
}

function CommitDetails({
  workspaceId,
  repoId,
  head,
  revisionId,
}: {
  workspaceId: string;
  repoId: string;
  head: string;
  revisionId?: string;
}) {
  const [open, setOpen] = useState(false);
  const history = useInfiniteQuery({
    queryKey: [
      "workspace-review-commit-details",
      workspaceId,
      repoId,
      revisionId,
      head,
    ],
    initialPageParam: "",
    enabled: open,
    queryFn: ({ pageParam }) => {
      const q = new URLSearchParams({ repoId });
      if (revisionId) q.set("revisionId", revisionId);
      if (pageParam) q.set("before", pageParam);
      return api.get<{
        headCommit: string;
        commits: {
          sha: string;
          summary: string;
          authorName: string;
          committedAt: number;
        }[];
        nextCursor: string | null;
      }>(`${reviewUrl(workspaceId)}/commits?${q}`);
    },
    getNextPageParam: (p) => p.nextCursor ?? undefined,
  });
  const moved = history.data?.pages[0]?.headCommit !== head;
  return (
    <div className="mt-1">
      <button
        type="button"
        data-testid="review-commit-details"
        className="text-[var(--g5-info)]"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        Commit details
      </button>
      {open && (
        <div className="max-h-64 min-w-52 overflow-auto">
          {history.isPending ? (
            <p role="status">Loading commits…</p>
          ) : history.error ? (
            <p role="alert">
              Could not load commits.{" "}
              <button
                data-testid="review-commits-retry"
                type="button"
                onClick={() => void history.refetch()}
              >
                Retry
              </button>
            </p>
          ) : moved ? (
            <p role="status">
              The pushed revision changed. Reopen the revision to inspect its
              commits.
            </p>
          ) : (
            <>
              {history.data?.pages
                .flatMap((p) => p.commits)
                .map((c) => (
                  <div
                    key={c.sha}
                    className="border-t border-[var(--g5-stroke)] py-2"
                  >
                    <code title={c.sha}>{c.sha.slice(0, 10)}</code>
                    <p>{c.summary}</p>
                    <p className="text-[var(--g5-text2)]">
                      {c.authorName} · Committed{" "}
                      {new Date(c.committedAt * 1000).toLocaleString()}
                    </p>
                  </div>
                ))}
              {history.hasNextPage && (
                <button
                  data-testid="review-commits-older"
                  type="button"
                  disabled={history.isFetchingNextPage}
                  onClick={() => void history.fetchNextPage()}
                >
                  Load older commits
                </button>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
