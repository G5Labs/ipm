import { useState } from "react";
import { useWorkspaceReviewFlow } from "../../hooks/useWorkspaceReviewFlow";
import {
  approvalHeads,
  approvalVersionState,
} from "../../lib/approvalVersions";
import { api } from "../../lib/api";
import { routes } from "../../lib/routes";
import type { FormalReviewReport } from "../org/FormalReviewInspection";
import { AnchoredPopup } from "../ui/AnchoredPopup";
import { formatApprovalDateTime } from "../../lib/dateTime";
import {
  CURRENTNESS_CIRCLE,
  currentnessGlyph,
  currentnessTone,
} from "../../lib/approvalRequirementRoster";

export function ApprovalVersionIndicator({
  workspaceId,
  reviewed,
  reviewId,
  reviewer,
  decidedAt,
}: {
  workspaceId: string;
  reviewed: unknown;
  reviewId?: string | null;
  /** Reviewer whose settled review this evidence belongs to. */
  reviewer?: string;
  decidedAt?: string;
}) {
  const flow = useWorkspaceReviewFlow(workspaceId);
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const [report, setReport] = useState<FormalReviewReport | null>(null);
  const [reportError, setReportError] = useState(false);
  const [reportLoading, setReportLoading] = useState(false);
  const status = approvalVersionState(
    reviewed,
    flow.error ? null : flow.data?.heads,
  );
  const before = approvalHeads(reviewed);
  const current = flow.error ? null : approvalHeads(flow.data?.heads);
  const ids = [
    ...new Set([
      ...(before ?? []).map((h) => h.repoId),
      ...(current ?? []).map((h) => h.repoId),
    ]),
  ];
  return (
    <>
      <button
        data-testid="approval-version-indicator"
        type="button"
        aria-haspopup="dialog"
        aria-label={`Reviewed commits: ${status}`}
        title={`Reviewed commits: ${status}`}
        className={`inline-flex h-5 min-w-5 items-center justify-center rounded px-0.5 ${currentnessTone(status)}`}
        onClick={(event) => {
          setAnchor(event.currentTarget);
          if (reviewId) {
            setReportError(false);
            setReportLoading(true);
            void api
              .get<FormalReviewReport>(
                `${routes.workspaceApprovals.requestAgentReview(workspaceId)}/${encodeURIComponent(reviewId)}/report`,
              )
              .then(setReport)
              .catch(() => setReportError(true))
              .finally(() => setReportLoading(false));
          }
        }}
      >
        <span aria-hidden="true" className={CURRENTNESS_CIRCLE}>
          {currentnessGlyph(status)}
        </span>
      </button>
      {anchor && (
        <AnchoredPopup
          anchor={anchor}
          title="Reviewed and current commits"
          ariaLabel="Reviewed and current commits"
          widthClass="w-[min(48rem,calc(100vw-16px))]"
          testId="approval-version-popup"
          onClose={() => setAnchor(null)}
        >
          {reviewer && (
            <p className="mb-1">
              {reviewer}
              {decidedAt ? ` · ${formatApprovalDateTime(decidedAt)}` : ""}
            </p>
          )}
          <p className="mb-3 text-xs">
            Pushed source commits · {status}. Version currentness is independent
            of the decision and merge eligibility.
          </p>
          {!before && <p>Recorded source vector unavailable.</p>}
          {!current && <p>Current pushed source vector unavailable.</p>}
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead>
                <tr>
                  <th>Repository</th>
                  <th>Reviewed source</th>
                  <th>Current source</th>
                  <th className="pl-3">Status</th>
                </tr>
              </thead>
              <tbody>
                {ids.map((id) => (
                  <tr
                    key={id}
                    data-testid="approval-version-repository"
                    className="border-t border-[var(--g5-stroke)] align-top"
                  >
                    <th className="py-3 pr-3">
                      {flow.data?.repos.find((r) => r.id === id)?.name ?? id}
                    </th>
                    <td className="pr-3 break-all">
                      <code>
                        {before?.find((h) => h.repoId === id)?.headCommit ??
                          (before ? "Not a member" : "Unknown")}
                      </code>
                      <p className="mt-1 text-[var(--g5-text2)]">
                        {before?.find((h) => h.repoId === id)?.branchName ??
                          "Recorded branch unavailable"}
                      </p>
                    </td>
                    <td className="break-all">
                      <code>
                        {current?.find((h) => h.repoId === id)?.headCommit ??
                          (current ? "Not a member" : "Unknown")}
                      </code>
                      <p className="mt-1 text-[var(--g5-text2)]">
                        {current?.find((h) => h.repoId === id)?.branchName ??
                          "Current branch unavailable"}
                      </p>
                    </td>
                    <td className="py-3 pl-3">
                      {repositoryCurrentness(before, current, id)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className="mt-3 text-xs">Source workspace: {workspaceId}</p>
          {reviewId ? (
            <>
              <p className="mt-3 text-xs">Recorded parent HEAD commits</p>
              <p className="text-xs">
                Target workspace:{" "}
                {report?.assignment?.resolution.targetWorkspaceId ?? "Unknown"}
              </p>
              {report?.assignment?.resolution.targetHeads?.map((h) => (
                <p className="my-2 break-all text-xs" key={h.repoId}>
                  {report.assignment?.resolution.repositoryNames?.[h.repoId] ??
                    flow.data?.repos.find((r) => r.id === h.repoId)?.name ??
                    h.repoId}
                  : <code>{h.headCommit}</code> · {h.branchName}
                </p>
              ))}
              {report?.assignment?.resolution.workspaceBaseHeads && (
                <>
                  <p className="mt-3 text-xs">
                    Workspace base commits (review comparison)
                  </p>
                  {report.assignment.resolution.workspaceBaseHeads.map((h) => (
                    <p className="my-2 break-all text-xs" key={h.repoId}>
                      {report.assignment?.resolution.repositoryNames?.[
                        h.repoId
                      ] ??
                        flow.data?.repos.find((r) => r.id === h.repoId)?.name ??
                        h.repoId}
                      : <code>{h.headCommit}</code> · {h.branchName}
                    </p>
                  ))}
                </>
              )}
              {reportLoading && !report && (
                <p className="text-xs">Loading recorded target/base context…</p>
              )}
              {!reportLoading &&
                (!report?.assignment?.resolution.targetHeads ||
                  reportError) && (
                  <p className="text-xs">
                    Recorded target/base context unavailable.
                  </p>
                )}
            </>
          ) : (
            <p className="mt-3 text-xs">
              Historical target/base context was not recorded with this
              decision.
            </p>
          )}
        </AnchoredPopup>
      )}
    </>
  );
}

/**
 * One repository's own comparison: a repository that matches stays Current
 * even when another repository in the vector moved.
 */
function repositoryCurrentness(
  before: ReturnType<typeof approvalHeads>,
  current: ReturnType<typeof approvalHeads>,
  repoId: string,
): string {
  if (!before || !current) return "Unknown";
  const reviewed = before.find((h) => h.repoId === repoId);
  const now = current.find((h) => h.repoId === repoId);
  if (!reviewed) return "Added";
  if (!now) return "Removed";
  return reviewed.headCommit === now.headCommit ? "Current" : "Changed";
}
