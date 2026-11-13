import { useState } from "react";
import { ArrowLeft, X } from "lucide-react";
import { Button, Modal } from "../../components/ui/UI";
import { changedFiles, files, formatCommitTime, type State } from "./model";
import { CommitVectorTable } from "./CommitVectorTable";

export function CommitHistory({
  state,
  onClose,
}: {
  state: State;
  onClose: () => void;
}) {
  const [limit, setLimit] = useState(5);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [fileId, setFileId] = useState("");
  const index = state.history.findIndex((entry) => entry.id === selectedId);
  const entry = state.history[index];
  const previous = state.history[index + 1];
  const changed = entry
    ? previous
      ? changedFiles(previous.revision, entry.revision)
      : files.filter((f) => entry.revision.files[f.id] !== undefined)
    : [];
  const selectedFile = changed.find((f) => f.id === fileId) || changed[0];
  const commit = selectedFile && entry?.commits[selectedFile.repo];
  return (
    <Modal
      ariaLabel="Commit History"
      onClose={onClose}
      size="lg"
      className="rm-modal rm-commit-history"
    >
      <div className="rm-row">
        <h2>Commit History</h2>
        <button
          data-testid="review-mock-close-history"
          aria-label="Close commit history"
          onClick={onClose}
        >
          <X size={18} />
        </button>
      </div>
      {entry ? (
        <>
          <button
            data-testid="review-mock-all-revisions"
            className="rm-link rm-history-back"
            onClick={() => setSelectedId(null)}
          >
            <ArrowLeft size={13} /> All workspace revisions
          </button>
          <CommitVectorTable
            entries={[entry]}
            previous={previous}
            current={state.pushed}
            base={state.base}
          />
          <p className="rm-footnote">
            {previous
              ? `Changes since previous workspace revision: ${previous.summary}.`
              : "First recorded revision · initial files."}{" "}
            Read-only.
          </p>
          <div className="rm-history-files">
            {selectedFile ? (
              <>
                <label>
                  Changed file
                  <select
                    aria-label="Revision changed file"
                    value={selectedFile.id}
                    onChange={(e) => setFileId(e.target.value)}
                  >
                    {changed.map((f) => (
                      <option key={f.id} value={f.id}>
                        {f.repo} · {f.path}
                      </option>
                    ))}
                  </select>
                </label>
                <p className="rm-commit-detail">
                  {commit?.summary} · {commit?.author}
                  <br />
                  Committed{" "}
                  <time dateTime={commit?.committedAt}>
                    {formatCommitTime(commit?.committedAt)}
                  </time>
                </p>
                <div className="rm-diff">
                  <div>
                    <div className="rm-diff-label">
                      Previous revision ·{" "}
                      {previous?.revision.heads[selectedFile.repo] || "empty"}
                    </div>
                    <pre>
                      {previous?.revision.files[selectedFile.id] ??
                        "(File does not exist)"}
                    </pre>
                  </div>
                  <div>
                    <div className="rm-diff-label">
                      Selected revision ·{" "}
                      {entry.revision.heads[selectedFile.repo]}
                    </div>
                    <pre>
                      {entry.revision.files[selectedFile.id] ??
                        "(File does not exist)"}
                    </pre>
                  </div>
                </div>
              </>
            ) : (
              <p>No file changes in this revision.</p>
            )}
          </div>
        </>
      ) : (
        <>
          <p className="rm-history-intro">
            Complete workspace revisions ·{" "}
            {Object.keys(state.pushed.heads).length} repositories · newest first
          </p>
          <CommitVectorTable
            entries={state.history.slice(0, limit)}
            previous={state.history[limit]}
            current={state.pushed}
            base={state.base}
            onSelect={(selected) => {
              setSelectedId(selected.id);
              setFileId("");
            }}
          />
          <p className="rm-footnote">
            Each row contains every repository. “Unchanged” means the same
            commit as the preceding revision.
          </p>
          {limit < state.history.length ? (
            <Button
              kind="secondary"
              className="rm-history-more"
              onClick={() => setLimit((n) => n + 5)}
            >
              Load more
            </Button>
          ) : (
            <p className="rm-footnote">
              Beginning of recorded history · includes inherited revisions
              before the workspace base.
            </p>
          )}
        </>
      )}
    </Modal>
  );
}
