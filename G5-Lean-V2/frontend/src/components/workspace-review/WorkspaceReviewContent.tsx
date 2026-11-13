import {
  SourceControlMenu,
  SourceControlDiffWorkspace,
} from "../source-control/SourceControlBrowser";
import { randomUuid } from "../../lib/randomUuid";
import { createCommentAnchor } from "../../lib/commentAnchors";
import { hunkDiffLines } from "../workspace/ReviewHunkDiff";
import { useEffect, useState, type ReactNode } from "react";
import { useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { useAuth } from "../../hooks/useAuth";
import {
  policyFreeGateKind,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import {
  reviewUrl,
  useWorkspaceReviewFlow,
  useRefreshReview,
  type ReviewChanges,
  type ReviewComment,
} from "../../hooks/useWorkspaceReviewFlow";
import { WorkspaceCommitVectors } from "./WorkspaceCommitVectors";
import { WorkspaceThread } from "./WorkspaceDiscussion";
import { reviewButton, reviewInput } from "./ReviewDialog";
export type ReviewSection = "code" | "docs" | "files" | "ontology";
export function WorkspaceReviewSidebar({
  enabled = true,
  section,
  children,
}: {
  enabled?: boolean;
  workspaceId: string;
  section: ReviewSection;
  children?: ReactNode;
}) {
  return enabled && section === "code" ? <SourceControlMenu /> : children;
}

type LineReviewDraft = {
  key: string;
  line: number | null;
  text: string;
  id: string;
  busy: boolean;
  error: string;
};
function loadLineReviewDraft(key: string): LineReviewDraft {
  let saved;
  try {
    saved = JSON.parse(sessionStorage.getItem(key) ?? "null");
  } catch {
    // A corrupt comparison draft must not reuse another comparison's identity.
  }
  return {
    key,
    line: saved?.line ?? null,
    text: saved?.text ?? "",
    id: saved?.id ?? randomUuid(),
    busy: false,
    error: "",
  };
}
function LegacyWorkspaceReviewContent({
  workspaceId,
}: {
  workspaceId: string;
}) {
  const [params, setParams] = useSearchParams();
  const { user } = useAuth();
  const refresh = useRefreshReview(workspaceId);
  const overview = useWorkspaceReviewFlow(workspaceId);
  const gate = useWorkspaceLiveGate();
  const canComment =
    !!overview.data && !overview.data.workspace.readOnly && !overview.error;
  const repo = params.get("reviewRepo");
  const path = params.get("reviewPath");
  const live = !!gate && policyFreeGateKind(gate) === "live" && gate.owner;
  const selectedMode = params.get("mode") ?? params.get("reviewMode") ?? "base";
  const mode =
    !live && ["uncommitted", "unpushed"].includes(selectedMode)
      ? "all"
      : selectedMode;
  const query = new URLSearchParams({ mode });
  for (const key of ["definitionId", "phase", "revisionId", "comment"]) {
    const v = params.get(key);
    if (v) query.set(key === "comment" ? "commentId" : key, v);
  }
  if (repo && path) {
    query.set("repoId", repo);
    query.set("path", path);
  }
  const changes = useQuery({
    queryKey: [
      "workspace-review-diff",
      workspaceId,
      query.toString(),
      overview.data?.heads,
      live,
      mode === "last_review" ? overview.data?.decisions : undefined,
    ],
    queryFn: () =>
      api.get<ReviewChanges>(`${reviewUrl(workspaceId)}/changes?${query}`),
    refetchInterval:
      live && ["all", "uncommitted", "unpushed"].includes(mode) ? 5000 : false,
  });
  const comments = useQuery({
    queryKey: ["workspace-review-comments", workspaceId, repo, path],
    queryFn: () =>
      api.get<ReviewComment[]>(
        `/api/v1/workspaces/${workspaceId}/comments?${new URLSearchParams({ repoId: repo ?? "", filePath: path ?? "", perPage: "100" })}`,
      ),
    enabled: !!repo && !!path,
    refetchInterval: 5000,
  });
  const draftKey = `g5:line-review:${user?.id}:${workspaceId}:${repo}:${path}:${query}:${changes.data?.file?.afterCommit ?? "current"}`;
  const [draft, setDraft] = useState(() => loadLineReviewDraft(draftKey));
  const { line, text, id: messageId, busy, error } = draft;
  const draftReady = draft.key === draftKey;
  useEffect(() => {
    setDraft((current) =>
      current.key === draftKey ? current : loadLineReviewDraft(draftKey),
    );
  }, [draftKey]);
  // A response can arrive after navigation or after opening another thread.
  // It only updates the exact draft that submitted it.
  const updateDraft = (patch: Partial<LineReviewDraft>) => {
    setDraft((current) =>
      current.key === draftKey && current.id === messageId
        ? { ...current, ...patch }
        : current,
    );
  };
  const saveDraft = (
    nextLine: number | null,
    nextText: string,
    id = messageId,
  ) => {
    if (!draftReady || busy) return;
    setDraft({ ...draft, line: nextLine, text: nextText, id });
    sessionStorage.setItem(
      draftKey,
      JSON.stringify({ line: nextLine, text: nextText, id }),
    );
  };
  if (changes.isPending)
    return (
      <p role="status" className="p-5">
        Loading comparison…
      </p>
    );
  if (changes.error)
    return (
      <div role="alert" className="p-5">
        Could not load comparison.{" "}
        {changes.error instanceof Error ? changes.error.message : ""}{" "}
        <button
          data-testid="review-diff-retry"
          className={reviewButton}
          onClick={() => void changes.refetch()}
        >
          Retry
        </button>
      </div>
    );
  const data = changes.data;
  const file = data.file;
  if (!file)
    return (
      <div className="h-full overflow-auto p-5">
        <h2 className="mb-3 font-semibold">Workspace changes</h2>
        {overview.data && (
          <>
            <div data-testid="comparison-before">
              <WorkspaceCommitVectors
                overview={overview.data}
                rows={
                  data.beforeHeads
                    ? [
                        {
                          id: "before",
                          workspaceId,
                          observedAt: "",
                          heads: data.beforeHeads,
                          pushedAt: {},
                        },
                      ]
                    : []
                }
              />
            </div>
            <div data-testid="comparison-after">
              <WorkspaceCommitVectors
                overview={overview.data}
                rows={[
                  {
                    id: "after",
                    workspaceId,
                    observedAt: "",
                    heads: data.afterHeads,
                    pushedAt: {},
                  },
                ]}
              />
            </div>
          </>
        )}
        {!data.baselineAvailable && (
          <p role="status">
            {mode === "last_review"
              ? "No review has been submitted for this type yet."
              : "No recorded comparison baseline is available."}
          </p>
        )}
        {data.files
          .filter(
            (f) =>
              !params.get("section") ||
              ["files", "code"].includes(params.get("section")!) ||
              f.kind === params.get("section"),
          )
          .map((f) => (
            <button
              key={`${f.repoId}:${f.path}`}
              data-testid={
                mode === "history"
                  ? "workspace-change-row"
                  : "review-content-file"
              }
              className="block p-2 text-sm text-[var(--g5-info)]"
              onClick={() => {
                const next = new URLSearchParams(params);
                next.set("reviewRepo", f.repoId);
                next.set("reviewPath", f.path);
                setParams(next);
              }}
            >
              {f.path}
              {["base", "parent", "last_review", "history"].includes(mode) &&
              f.additions !== null &&
              f.deletions !== null
                ? ` +${f.additions} −${f.deletions}`
                : ""}
            </button>
          ))}
      </div>
    );
  if (file.binary)
    return (
      <div className="p-5">
        <h2 className="font-mono">{file.path}</h2>
        <p role="status">
          Binary file. A text diff and line comments are unavailable for this
          file.
        </p>
      </div>
    );
  const diffRows = hunkDiffLines(
    mode === "all" ? file.after : file.before,
    file.after,
    1,
    1,
  );
  const renderedRows = diffRows.slice(0, 3000);
  return (
    <div
      className="h-full overflow-auto p-4"
      data-testid="workspace-review-diff"
    >
      <h2 className="mb-2 break-all font-mono text-sm">{file.path}</h2>
      {params.has("comment") &&
        overview.data?.heads.find((h) => h.repoId === file.repoId)
          ?.headCommit !== file.afterCommit && (
          <p role="status" className="mb-3 text-sm">
            Historical discussion: showing the original pushed file and line,
            even if it has since been renamed or deleted.
          </p>
        )}
      <p className="mb-3 text-xs text-[var(--g5-text2)]">
        {file.beforeCommit?.slice(0, 10) ?? "New file"} →{" "}
        {file.afterCommit?.slice(0, 10) ?? "Current workspace"}
      </p>
      <div
        className={`grid ${mode === "all" ? "grid-cols-1" : "grid-cols-2"} overflow-x-auto rounded border border-[var(--g5-stroke)] text-xs`}
      >
        {mode !== "all" && (
          <div data-testid="diff-before" className="overflow-auto py-3">
            {file.before === null && <p>File did not exist</p>}
            {renderedRows.map((row, i) => (
              <pre
                key={i}
                className={`min-h-5 whitespace-pre px-2 ${row.type === "deletion" ? "bg-red-500/10" : ""}`}
              >
                {row.type !== "addition"
                  ? `${row.oldLine ?? ""} ${row.content}`
                  : " "}
              </pre>
            ))}
          </div>
        )}
        <div
          data-testid="diff-after"
          className="overflow-auto border-l border-[var(--g5-stroke)] py-3"
        >
          {file.after === null && <p>File deleted</p>}
          {renderedRows.map((row, i) => (
            <div
              key={i}
              className={`flex min-h-5 min-w-max ${row.type === "addition" ? "bg-green-500/10" : ""}`}
            >
              {row.newLine !== undefined ? (
                <>
                  <button
                    type="button"
                    data-testid={`comment-line-${row.newLine}`}
                    disabled={!draftReady || busy}
                    aria-label={`Comment on ${row.type === "addition" ? "added " : ""}line ${row.newLine}`}
                    className="mr-2 px-2 text-[var(--g5-info)]"
                    onClick={() =>
                      saveDraft(row.newLine ?? 1, text, randomUuid())
                    }
                  >
                    {row.newLine} +
                  </button>
                  <pre className="whitespace-pre">{row.content || " "}</pre>
                </>
              ) : (
                <pre> </pre>
              )}
            </div>
          ))}
        </div>
      </div>
      {diffRows.length > 3000 && (
        <p role="status">
          Showing the first 3,000 diff lines. Open the file in Files to inspect
          the rest.
        </p>
      )}

      {line && draftReady && (
        <form
          className="mt-4"
          onSubmit={async (e) => {
            e.preventDefault();
            if (
              !draftReady ||
              busy ||
              !canComment ||
              !text.trim() ||
              file.after === null
            )
              return;
            updateDraft({ busy: true, error: "" });
            try {
              const content = file.after ?? "";
              await api.post(`/api/v1/workspaces/${workspaceId}/comments`, {
                id: messageId,
                scope: "line",
                repoId: file.repoId,
                filePath: file.path,
                ...(await createCommentAnchor(content, line)),
                commitSha: file.afterCommit,
                content: text,
              });
              updateDraft({ text: "", line: null });
              // Preserve a replacement draft created while this response waited.
              try {
                if (
                  JSON.parse(sessionStorage.getItem(draftKey) ?? "null")?.id ===
                  messageId
                )
                  sessionStorage.removeItem(draftKey);
              } catch {
                /* Leave an unreadable replacement for normal hydration recovery. */
              }
              await refresh();
            } catch (e) {
              updateDraft({
                error: e instanceof Error ? e.message : "Comment failed",
              });
            } finally {
              updateDraft({ busy: false });
            }
          }}
        >
          <label>
            Comment on line {line}
            <textarea
              data-testid="draft-comment-input"
              disabled={busy}
              aria-label="Comment"
              className={reviewInput}
              value={text}
              onChange={(e) => saveDraft(line, e.target.value)}
            />
          </label>
          <button
            type="submit"
            data-testid="draft-comment-save"
            className={reviewButton}
            disabled={busy || !canComment || !text.trim()}
          >
            Add comment
          </button>
          {error && <p role="alert">{error}</p>}
        </form>
      )}
      <section aria-label="File discussion" className="mt-5 space-y-4">
        {comments.data?.map((c) => (
          <article
            key={c.id}
            id={`comment-${c.id}`}
            className="rounded border border-[var(--g5-stroke)] p-3"
          >
            <p className="text-xs text-[var(--g5-text2)]">
              {c.authorName} · line {c.lineNumber}
              {c.commitSha !== file.afterCommit ? " · earlier revision" : ""}
            </p>
            <WorkspaceThread comment={c} />
          </article>
        ))}
      </section>
    </div>
  );
}

export function WorkspaceReviewContent(props: { workspaceId: string }) {
  const [params] = useSearchParams();
  return params.has("reviewPath") || params.get("mode") === "history" ? (
    <LegacyWorkspaceReviewContent {...props} />
  ) : (
    <SourceControlDiffWorkspace />
  );
}
