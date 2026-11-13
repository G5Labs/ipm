import { useState } from "react";
import { useParams } from "react-router-dom";
import { api } from "../../lib/api";
import { useAuth } from "../../hooks/useAuth";
import { useOrgApprovalDefinitions } from "../../hooks/useOrgApprovals";
import {
  changedRepositories,
  useRefreshReview,
  useWorkspaceReviewFlow,
  type ReviewHead,
  type ReviewRequest,
} from "../../hooks/useWorkspaceReviewFlow";
import { ReviewDialog, reviewInput, reviewButton } from "./ReviewDialog";
import { WorkspaceCommitVectors } from "./WorkspaceCommitVectors";
type Draft = {
  comment: string;
  outcome: "approved" | "rejected";
  heads: ReviewHead[];
  pushedAt: Record<string, string>;
};
export function WorkspaceReviewActions({
  workspaceId,
  definitionId,
  disabled = false,
}: {
  workspaceId: string;
  definitionId: string;
  disabled?: boolean;
}) {
  const { user } = useAuth();
  const { org } = useParams();
  const overview = useWorkspaceReviewFlow(workspaceId);
  const refresh = useRefreshReview(workspaceId);
  const library = useOrgApprovalDefinitions(
    user?.orgs.find((o) => o.orgSlug === org)?.orgId ?? "",
  );
  const [dialog, setDialog] = useState<"review" | null>(null);
  const [requestId, setRequestId] = useState("");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [inspecting, setInspecting] = useState(false);
  if (!overview.data)
    return (
      <p role={overview.error ? "alert" : "status"}>
        {overview.error ? "Could not load reviews." : "Loading reviews…"}
        {overview.error && (
          <button
            data-testid="review-overview-retry"
            onClick={() => void overview.refetch()}
          >
            Retry
          </button>
        )}
      </p>
    );
  const data = overview.data;
  const owner = data.workspace.ownerUserId === user?.id;
  const definition =
    library.status === "ok"
      ? library.data.find((d) => d.id === definitionId)
      : null;
  const viewer = definition?.humans.find((h) => h.userId === user?.id);
  const requests = data.requests.filter(
    (r) =>
      r.definitionId === definitionId &&
      r.state === "pending" &&
      r.actorUserId === user?.id &&
      !!viewer?.stillMember &&
      !viewer.deactivatedAt &&
      (!owner || (r.allowSelfApproval && definition?.allowSelfApproval)),
  );
  const draftKey = (id: string) =>
    `g5:formal-review:${user?.id}:${workspaceId}:${data.pr?.id}:${id}`;
  const chooseRequest = (r: ReviewRequest, outcome?: Draft["outcome"]) => {
    setRequestId(r.id);
    setError("");
    let saved: Draft | null = null;
    try {
      saved = JSON.parse(
        sessionStorage.getItem(draftKey(r.id)) ?? "null",
      ) as Draft | null;
    } catch {
      /* New draft */
    }
    const next: Draft = saved ?? {
      comment: "",
      outcome: outcome ?? "approved",
      heads: data.heads,
      pushedAt: data.pushedAt,
    };
    setDraft(next);
    sessionStorage.setItem(draftKey(r.id), JSON.stringify(next));
  };
  const updateDraft = (update: Partial<Draft>) => {
    if (!draft) return;
    const next = { ...draft, ...update };
    setDraft(next);
    sessionStorage.setItem(draftKey(requestId), JSON.stringify(next));
  };
  const changed = draft ? changedRepositories(draft.heads, data.heads) : [];
  return (
    <section aria-label="Workspace reviews" className="mt-2">
      {requests.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {(["approved", "rejected"] as const).map((outcome) => (
            <button
              key={outcome}
              disabled={
                disabled ||
                data.workspace.readOnly ||
                !!overview.error ||
                library.status !== "ok"
              }
              data-testid={
                outcome === "approved"
                  ? "workspace-submit-review"
                  : "workspace-reject-review"
              }
              className={reviewButton}
              onClick={() => {
                chooseRequest(requests[0], outcome);
                setDialog("review");
              }}
            >
              {outcome === "approved" ? "Approve" : "Reject"}
            </button>
          ))}
        </div>
      )}
      {dialog === "review" && draft && (
        <ReviewDialog
          busy={busy}
          title="Submit review"
          onClose={() => setDialog(null)}
        >
          <form
            onSubmit={async (e) => {
              e.preventDefault();
              // Listed requests are decidable with or without a PR
              // (g5.governance.approval.request-pr); the server decides.
              const r = requests.find((r) => r.id === requestId);
              if (
                busy ||
                disabled ||
                !!overview.error ||
                data.workspace.readOnly
              )
                return;
              if (!r) {
                setError(
                  "This review request is no longer available. Your draft has been kept.",
                );
                return;
              }
              setBusy(true);
              setError("");
              try {
                await api.post(
                  `/api/v1/workspaces/${workspaceId}/approvals/decisions`,
                  {
                    definitionId: r.definitionId,
                    requestId: r.id,
                    outcome: draft.outcome,
                    ...(draft.comment.trim() ? { comment: draft.comment } : {}),
                    reviewedGitHeads: draft.heads,
                  },
                );
                sessionStorage.removeItem(draftKey(r.id));
                setDialog(null);
                setDraft(null);
                await refresh();
              } catch (e) {
                setError(e instanceof Error ? e.message : "Review failed");
              } finally {
                setBusy(false);
              }
            }}
          >
            <label>
              Approval type
              <select
                disabled={busy}
                aria-label="Approval type"
                className={reviewInput}
                value={requestId}
                onChange={(e) => {
                  const r = requests.find((r) => r.id === e.target.value);
                  if (r) chooseRequest(r);
                }}
              >
                {data.requests
                  .filter(
                    (r) =>
                      requests.some((eligible) => eligible.id === r.id) ||
                      r.id === requestId,
                  )
                  .map((r) => (
                    <option key={r.id} value={r.id}>
                      {r.name}
                    </option>
                  ))}
              </select>
            </label>
            {data.requests.find((r) => r.id === requestId)?.note && (
              <div className="mt-4 rounded border border-[var(--g5-stroke)] p-3">
                <p className="text-xs font-semibold">Requester’s note</p>
                <p className="whitespace-pre-wrap text-sm">
                  {data.requests.find((r) => r.id === requestId)?.note}
                </p>
              </div>
            )}
            <p className="mt-4 text-sm">
              This review applies to the captured pushed revision.
            </p>
            <WorkspaceCommitVectors
              overview={data}
              rows={[
                {
                  id: "captured",
                  workspaceId,
                  observedAt: "",
                  heads: draft.heads,
                  pushedAt: draft.pushedAt ?? {},
                },
              ]}
            />
            {changed.length > 0 && (
              <div
                role="status"
                className="my-3 rounded border border-[var(--g5-warn)] p-3 text-sm"
              >
                A new push is available in{" "}
                {changed
                  .map((id) => data.repos.find((r) => r.id === id)?.name ?? id)
                  .join(", ")}
                . Your review still uses the captured revision.
                <button
                  type="button"
                  data-testid="review-inspect-latest"
                  className={reviewButton}
                  onClick={() => setInspecting((v) => !v)}
                >
                  Inspect latest pushed revision
                </button>
                {inspecting && (
                  <div>
                    <WorkspaceCommitVectors
                      overview={data}
                      rows={[
                        {
                          id: "current",
                          workspaceId,
                          observedAt: "",
                          heads: data.heads,
                          pushedAt: data.pushedAt,
                        },
                      ]}
                    />
                    <a
                      className="text-[var(--g5-info)]"
                      href="?view=source-control&reviewMode=base"
                      target="_blank"
                      rel="noreferrer"
                    >
                      Open latest changes (new tab)
                    </a>
                    <button
                      data-testid="review-target-latest"
                      type="button"
                      className={reviewButton}
                      onClick={() => {
                        updateDraft({
                          heads: data.heads,
                          pushedAt: data.pushedAt,
                        });
                        setInspecting(false);
                      }}
                    >
                      Use this revision for my review
                    </button>
                  </div>
                )}
              </div>
            )}
            <fieldset className="my-4">
              <legend>Review decision</legend>
              <label className="mr-5">
                <input
                  disabled={busy}
                  type="radio"
                  name="outcome"
                  checked={draft.outcome === "approved"}
                  onChange={() => updateDraft({ outcome: "approved" })}
                />{" "}
                Approve
              </label>
              <label>
                <input
                  disabled={busy}
                  type="radio"
                  name="outcome"
                  checked={draft.outcome === "rejected"}
                  onChange={() => updateDraft({ outcome: "rejected" })}
                />{" "}
                Request changes
              </label>
            </fieldset>
            <label>
              Review comment
              <textarea
                disabled={busy}
                aria-label="Review comment"
                className={`${reviewInput} min-h-32`}
                value={draft.comment}
                onChange={(e) => updateDraft({ comment: e.target.value })}
              />
            </label>
            {error && <p role="alert">{error}</p>}
            <button
              type="submit"
              data-testid="review-decision-submit"
              className={reviewButton}
              disabled={
                busy ||
                disabled ||
                !!overview.error ||
                data.workspace.readOnly ||
                (draft.outcome === "rejected" && !draft.comment.trim())
              }
            >
              Submit review
            </button>
          </form>
        </ReviewDialog>
      )}
    </section>
  );
}
