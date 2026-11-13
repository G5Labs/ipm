import { memo, useCallback, useLayoutEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import type {
  SourceCommit,
  SourceRepo,
} from "../../lib/sourceControlSelection";

export function committedDate(seconds: number) {
  const date = new Date(seconds * 1000);
  return `${date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })} · ${date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`;
}
export function HistoryCommit({
  repo,
  commit,
  checked,
  onToggle,
  marker,
}: {
  repo: Pick<SourceRepo, "name" | "comparison" | "parentName">;
  commit: SourceCommit;
  checked: boolean;
  onToggle: (sha: string, checked: boolean) => void;
  marker?: string;
}) {
  const fromParent = repo.comparison?.parentMerges.includes(commit.sha);
  return (
    <label
      className={`source-control-option${marker ? " source-control-base" : ""}`}
    >
      <input
        type="checkbox"
        className="source-control-checkbox"
        aria-label={`${commit.sha.slice(0, 7)} in ${repo.name}`}
        checked={checked}
        onChange={(e) => onToggle(commit.sha, e.target.checked)}
      />
      <span className="min-w-0">
        {marker && <strong className="source-control-subject">{marker}</strong>}
        <span className="source-control-subject">
          {commit.summary || commit.sha.slice(0, 7)}
        </span>
        <span className="source-control-meta">
          <code>{commit.sha.slice(0, 10)}</code>
          {fromParent
            ? ` · Merged from ${repo.parentName ?? "parent"}`
            : commit.parents.length > 1
              ? " · Merge · first parent"
              : ""}
        </span>
        <span className="source-control-meta">
          {commit.authorName} · Committed {committedDate(commit.committedAt)}
        </span>
      </span>
    </label>
  );
}

// Keep real controls for keyboard navigation, focus and find-in-page. The
// browser can skip layout/paint for distant pages, and memoization keeps an
// appended page or an unrelated selection from rerendering earlier controls.
const HistoryPageRows = memo(
  function HistoryPageRows({
    commits,
    name,
    chosen,
    onToggle,
  }: {
    commits: SourceCommit[];
    name: string;
    chosen: string[];
    onToggle: (sha: string, checked: boolean) => void;
  }) {
    return (
      <div
        className="source-control-history-page"
        style={{ containIntrinsicSize: `auto ${commits.length * 64}px` }}
      >
        {commits.map((commit) => (
          <HistoryCommit
            key={commit.sha}
            repo={{ name }}
            commit={commit}
            checked={chosen.includes(commit.sha)}
            onToggle={onToggle}
          />
        ))}
      </div>
    );
  },
  (previous, next) =>
    previous.commits === next.commits &&
    previous.name === next.name &&
    previous.onToggle === next.onToggle &&
    previous.commits.every(
      ({ sha }) => previous.chosen.includes(sha) === next.chosen.includes(sha),
    ),
);
function HistorySection({
  workspaceId,
  repo,
  section,
  label,
  chosen,
  onToggle,
  remember,
}: {
  workspaceId: string;
  repo: SourceRepo;
  section: string;
  label: string;
  chosen: string[];
  onToggle: (sha: string, checked: boolean) => void;
  remember: (commits: SourceCommit[]) => void;
}) {
  const [page, setPage] = useState<{
    pages: SourceCommit[][];
    nextCursor: string | null;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const toggle = useRef(onToggle);
  useLayoutEffect(() => {
    toggle.current = onToggle;
  }, [onToggle]);
  const onCommitToggle = useCallback(
    (sha: string, checked: boolean) => toggle.current(sha, checked),
    [],
  );
  async function load() {
    setBusy(true);
    setError("");
    try {
      const result = await api.post<{
        commits: SourceCommit[];
        nextCursor: string | null;
      }>(`/api/v1/workspaces/${workspaceId}/source-control/history`, {
        repoId: repo.repoId,
        pushed: repo.pushed,
        parent: repo.parent ?? null,
        base: repo.base,
        workspaceBase: repo.workspaceBase ?? null,
        section,
        cursor: page?.nextCursor ?? null,
      });
      setPage((previous) => ({
        pages: [...(previous?.pages ?? []), result.commits],
        nextCursor: result.nextCursor,
      }));
      remember(result.commits);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <details
      className="source-control-history"
      onToggle={(e) => {
        if (e.currentTarget.open && !page && !busy && !error) void load();
      }}
    >
      <summary>{label}</summary>
      {page?.pages.map((commits, index) => (
        <HistoryPageRows
          key={index}
          commits={commits}
          name={repo.name}
          chosen={chosen}
          onToggle={onCommitToggle}
        />
      ))}
      {busy && (
        <p role="status" className="source-control-notice">
          Loading history…
        </p>
      )}
      {error && (
        <p role="alert" className="source-control-notice">
          {error}{" "}
          <button
            type="button"
            data-testid="source-control-history-retry"
            className="source-control-link"
            onClick={() => void load()}
          >
            Retry
          </button>
        </p>
      )}
      {page?.nextCursor && !busy && !error && (
        <button
          type="button"
          data-testid="source-control-history-more"
          className="source-control-link"
          onClick={() => void load()}
        >
          Show more
        </button>
      )}
    </details>
  );
}
export function SourceControlHistory({
  workspaceId,
  repo,
  chosen,
  onToggle,
  remember,
}: {
  workspaceId: string;
  repo: SourceRepo;
  chosen: string[];
  onToggle: (sha: string, checked: boolean) => void;
  remember: (commits: SourceCommit[]) => void;
}) {
  const comparison = repo.comparison;
  const combined = repo.base === repo.workspaceBase;
  const props = { workspaceId, repo, chosen, onToggle, remember };
  const cut = JSON.stringify([
    workspaceId,
    repo.pushed,
    repo.base,
    repo.workspaceBase,
    comparison?.workspaceBaseInHistory,
  ]);
  if (repo.unpushedOnly)
    return (
      <>
        <p className="source-control-notice">No outgoing comparison</p>
        <HistorySection
          key={cut}
          {...props}
          section="history"
          label="Show history"
        />
      </>
    );
  if (!comparison) return null;
  return (
    <>
      <HistoryCommit
        repo={repo}
        commit={comparison.mergeBaseCommit}
        checked={chosen.includes(comparison.mergeBaseCommit.sha)}
        onToggle={onToggle}
        marker={combined ? "Merge base · Workspace base" : "Merge base"}
      />
      {!!comparison.sinceWorkspaceBaseCount &&
        comparison.workspaceBaseInHistory && (
          <HistorySection
            key={`since:${cut}`}
            {...props}
            section="since"
            label={`Since workspace base · ${comparison.sinceWorkspaceBaseCount} commits`}
          />
        )}
      {!combined &&
        repo.workspaceBase &&
        (comparison.workspaceBase ? (
          <div>
            <HistoryCommit
              repo={repo}
              commit={comparison.workspaceBase}
              checked={chosen.includes(repo.workspaceBase)}
              onToggle={onToggle}
              marker="Workspace base"
            />
            {!comparison.workspaceBaseInHistory && (
              <p className="source-control-notice">Not in current history</p>
            )}
          </div>
        ) : (
          <label className="source-control-option source-control-base">
            <input
              aria-label={`Workspace base unavailable in ${repo.name}`}
              type="checkbox"
              disabled
              className="source-control-checkbox"
            />
            <span>
              <strong>Workspace base</strong>
              <code className="block break-all">{repo.workspaceBase}</code>
              <span>Unavailable</span>
            </span>
          </label>
        ))}
      <HistorySection
        key={`earlier:${cut}`}
        {...props}
        section={comparison.workspaceBaseInHistory ? "earlier" : "history"}
        label={
          comparison.workspaceBaseInHistory ? "Earlier history" : "Show history"
        }
      />
    </>
  );
}
