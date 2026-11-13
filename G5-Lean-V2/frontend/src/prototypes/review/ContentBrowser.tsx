import type { ReactNode } from "react";
import { FileText, Check, ChevronRight } from "lucide-react";
import { Button } from "../../components/ui/UI";
import { WORKBENCH_PHASES } from "../../lib/workbenchDefaults";
import {
  changedRepos,
  comparison,
  contentModes,
  files,
  modeLabels,
  scopes,
  type Decision,
  type Mode,
  type Role,
  type State,
  type View,
} from "./model";

export function ContentBrowser({
  state,
  role,
  mode,
  setMode,
  scope,
  setScope,
  previous,
  view,
  selectedId,
  onSelect,
  onEdit,
  discussion,
  vector,
}: {
  state: State;
  role: Role;
  mode: Mode;
  setMode: (m: Mode) => void;
  scope: string;
  setScope: (s: string) => void;
  previous?: Decision;
  view: View;
  selectedId: string;
  onSelect: (id: string) => void;
  onEdit: (id: string, text: string) => void;
  discussion: ReactNode;
  vector: ReactNode;
}) {
  const data = comparison(state, role, mode, previous);
  const visible = data.files.filter(
    (f) => view === "files" || view === "source" || f.kind === view,
  );
  const selected = files.find((f) => f.id === selectedId)!;
  const live =
    role === "owner" &&
    state.running &&
    ["all", "uncommitted", "unpushed"].includes(data.effectiveMode);
  return (
    <>
      <div className="rm-content-toolbar">
        <label>
          Show
          <select
            aria-label="File view"
            value={data.effectiveMode}
            onChange={(e) => setMode(e.target.value as Mode)}
          >
            {contentModes(role, state.running, state.pr).map((m) => (
              <option value={m} key={m}>
                {modeLabels[m]}
              </option>
            ))}
          </select>
        </label>
        {data.effectiveMode === "last" && (
          <label>
            Review type
            <select
              aria-label="Comparison review type"
              value={scope}
              onChange={(e) => setScope(e.target.value)}
            >
              {scopes.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.title}
                </option>
              ))}
            </select>
          </label>
        )}
        {vector}
      </div>
      <div className="rm-comparison-caption">
        {data.effectiveMode === "all" ? (
          `${data.afterLabel} · all files, no comparison`
        ) : (
          <>
            <strong>{data.beforeLabel}</strong>
            <span>→</span>
            <strong>{data.afterLabel}</strong>
          </>
        )}
      </div>
      {mode !== data.effectiveMode && (
        <div className="rm-inline-note">
          Live work is unavailable. Showing pushed changes against the workspace
          base.
        </div>
      )}
      {data.effectiveMode === "last" && (
        <div className="rm-inline-note">
          {previous
            ? `Compared with your ${previous.result === "approved" ? "approval" : "changes request"} · ${previous.time} · ${scopes.find((s) => s.id === scope)?.title}. Changed repositories: ${changedRepos(previous.revision, state.pushed).join(", ") || "none"}.`
            : "You have not submitted this review type yet. Showing pushed changes against the workspace base."}
        </div>
      )}
      {live && (
        <p className="rm-footnote">
          Owner-only current content. Formal reviews always attach to the pushed
          revision.
        </p>
      )}
      <div className="rm-content-layout">
        <aside className="rm-file-list">
          <div className="rm-section-label">
            {data.effectiveMode === "all" ? "ALL FILES" : "CHANGED FILES"} ·{" "}
            {visible.length}
          </div>
          {visible.map((f) => (
            <button
              data-testid="review-mock-select-file"
              key={f.id}
              className={f.id === selectedId ? "active" : ""}
              onClick={() => onSelect(f.id)}
            >
              <FileText size={14} />
              <span>
                {f.title}
                <small>
                  {f.repo} / {f.path}
                </small>
              </span>
            </button>
          ))}
          {visible.length === 0 && <p>No changes in this comparison.</p>}
        </aside>
        <section className="rm-file-detail">
          {!visible.some((f) => f.id === selectedId) ? (
            <div className="rm-empty">
              <Check size={25} />
              <h2>
                {visible.length ? "Choose a file" : "No changes in this view"}
              </h2>
              <p>
                {visible.length
                  ? "Select a file from the list."
                  : "Try another comparison or browse all files."}
              </p>
              <Button kind="secondary" onClick={() => setMode("all")}>
                Browse all files
              </Button>
            </div>
          ) : (
            <>
              <div className="rm-file-heading">
                <small>
                  {selected.repo} / {selected.path}
                </small>
                <h2>{selected.title}</h2>
                <span className="rm-phase-tag">
                  {WORKBENCH_PHASES[selected.phase].name}
                </span>
              </div>
              {view === "ontology" && (
                <div className="rm-semantic">
                  <span className="rm-badge">Decision · preview.session</span>
                  <h3>Make cleanup failures explicit</h3>
                  <p>
                    Clarifies the session lifecycle: failures remain visible and
                    the owner can retry. Profile ownership and
                    cleanup-before-stop ordering are preserved.
                  </p>
                  <div className="rm-relations">
                    <span>PreviewSession</span>
                    <ChevronRight size={14} />
                    <span>PreviewProfile</span>
                    <ChevronRight size={14} />
                    <span>Cleanup command</span>
                  </div>
                </div>
              )}
              <div
                className={
                  data.effectiveMode === "all" ? "rm-file-content" : "rm-diff"
                }
              >
                {data.effectiveMode !== "all" && (
                  <div>
                    <div className="rm-diff-label">{data.beforeLabel}</div>
                    <pre>{data.before.files[selectedId]}</pre>
                  </div>
                )}
                <div>
                  <div className="rm-diff-label">{data.afterLabel}</div>
                  {live ? (
                    <textarea
                      aria-label="Current file contents"
                      value={data.after.files[selectedId]}
                      onChange={(e) => onEdit(selectedId, e.target.value)}
                    />
                  ) : (
                    <pre>{data.after.files[selectedId]}</pre>
                  )}
                </div>
              </div>
              <div className="rm-file-discussion">
                <h3>Discussion</h3>
                <p className="rm-muted">
                  These comments also appear in the workspace timeline.
                </p>
                {discussion}
              </div>
            </>
          )}
        </section>
      </div>
    </>
  );
}
