import {
  formatCommitTime,
  sameVector,
  type Repo,
  type Revision,
  type WorkspaceCommit,
} from "./model";

/** One workspace revision per row, with every repository member kept visible. */
export function CommitVectorTable({
  entries,
  previous,
  current,
  base,
  onSelect,
}: {
  entries: WorkspaceCommit[];
  previous?: WorkspaceCommit;
  current: Revision;
  base: Revision;
  onSelect?: (entry: WorkspaceCommit) => void;
}) {
  const repos = [
    ...new Set(entries.flatMap((entry) => Object.keys(entry.revision.heads))),
  ] as Repo[];
  return (
    <div
      className="rm-vector-table-scroll"
      role="region"
      aria-label="Workspace commit vectors"
      // Keep horizontal and vertical table scrolling keyboard-accessible.
      // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
      tabIndex={0}
    >
      <table className="rm-vector-table">
        <caption>Workspace commit vectors</caption>
        <thead>
          <tr>
            <th scope="col">Workspace revision</th>
            {repos.map((repo) => (
              <th key={repo} scope="col">
                {repo}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {entries.map((entry, index) => {
            const before = entries[index + 1] || previous;
            return (
              <tr key={entry.id}>
                <th scope="row">
                  <div className="rm-vector-markers">
                    {sameVector(entry.revision, current) && (
                      <span className="rm-badge good">Current pushed</span>
                    )}
                    {sameVector(entry.revision, base) && (
                      <span className="rm-badge">Workspace base</span>
                    )}
                  </div>
                  {onSelect ? (
                    <button
                      data-testid="review-mock-select-revision"
                      className="rm-vector-subject"
                      onClick={() => onSelect(entry)}
                    >
                      {entry.summary}
                    </button>
                  ) : (
                    <span className="rm-vector-subject">{entry.summary}</span>
                  )}
                  <span className="rm-vector-meta">{entry.author}</span>
                  <span className="rm-vector-meta">
                    Recorded{" "}
                    <time dateTime={entry.recordedAt}>
                      {formatCommitTime(entry.recordedAt)}
                    </time>
                  </span>
                </th>
                {repos.map((repo) => {
                  const sha = entry.revision.heads[repo];
                  const info = entry.commits[repo];
                  const unchanged = before?.revision.heads[repo] === sha;
                  return (
                    <td key={repo}>
                      <div
                        className={`rm-vector-sha ${unchanged ? "unchanged" : "changed"}`}
                      >
                        <code
                          title={
                            info
                              ? `${info.summary}\nCommitted ${formatCommitTime(info.committedAt)} by ${info.author}`
                              : undefined
                          }
                        >
                          {sha || "Not present"}
                        </code>
                        {before && (
                          <span title="Compared with the preceding recorded workspace revision">
                            {unchanged ? "Unchanged" : "Updated"}
                          </span>
                        )}
                      </div>
                      <span className="rm-vector-meta">
                        Pushed{" "}
                        <time dateTime={info?.pushedAt}>
                          {formatCommitTime(info?.pushedAt)}
                        </time>
                      </span>
                    </td>
                  );
                })}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
