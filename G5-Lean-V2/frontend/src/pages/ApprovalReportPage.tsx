import { useAuth } from "../hooks/useAuth";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import { ApprovalReportDocument } from "../components/org/ApprovalReviewReader";
import {
  FormalReviewInspection,
  type FormalReviewReport,
} from "../components/org/FormalReviewInspection";

/** Retained artifacts only; never mounts a file editor or live workspace. */
export function ApprovalReportPage() {
  const { org, workspaceId = "", reviewId = "" } = useParams();
  const { user, switchOrg } = useAuth();
  const routeOrg = user?.orgs.find((membership) => membership.orgSlug === org);
  const routeOrgActive = !!routeOrg && user?.activeOrgId === routeOrg.orgId;
  const [failedSwitch, setFailedSwitch] = useState<string | null>(null);
  const attemptedOrg = useRef<string | null>(null);
  useEffect(() => {
    if (!routeOrg || routeOrgActive) {
      if (routeOrgActive) attemptedOrg.current = null;
      return;
    }
    if (attemptedOrg.current === routeOrg.orgId) return;
    attemptedOrg.current = routeOrg.orgId;
    void switchOrg(routeOrg.orgId).catch(() => setFailedSwitch(routeOrg.orgId));
  }, [routeOrg, routeOrgActive, switchOrg]);
  const report = useQuery({
    queryKey: [
      "approval-report",
      user?.id,
      routeOrg?.orgId,
      workspaceId,
      reviewId,
    ],
    enabled: routeOrgActive,
    queryFn: ({ signal }) =>
      api.get<FormalReviewReport>(
        `${routes.workspaceApprovals.requestAgentReview(workspaceId)}/${encodeURIComponent(reviewId)}/report`,
        { signal },
      ),
  });
  if (!routeOrg || !routeOrgActive)
    return (
      <main className="p-6 text-[var(--g5-text0)]">
        <p
          role={
            !routeOrg || failedSwitch === routeOrg.orgId ? "alert" : "status"
          }
        >
          {!routeOrg
            ? "You do not have access to this organization."
            : failedSwitch === routeOrg.orgId
              ? `Could not switch to ${routeOrg.orgName}. Reload to try again.`
              : `Switching to ${routeOrg.orgName}…`}
        </p>
      </main>
    );
  const document = report.data?.document;
  return (
    <main className="min-h-screen bg-[var(--g5-bg0)] px-4 py-6 text-[var(--g5-text0)] sm:px-8">
      <div className="min-w-0 w-full">
        <header className="mb-6 flex flex-wrap items-center justify-between gap-3">
          <h1 className="text-xl font-semibold">
            Agent review report{" "}
            <span className="text-sm font-normal text-[var(--g5-text2)]">
              Read only
            </span>
          </h1>
        </header>
        {report.isPending && <p role="status">Loading report…</p>}
        {report.error && (
          <p role="alert">
            Could not load report: {report.error.message}{" "}
            <button
              type="button"
              data-testid="approval-report-retry"
              onClick={() => void report.refetch()}
            >
              Retry
            </button>
          </p>
        )}
        {report.data && (
          <>
            {report.data.failureCause && (
              <p role="status">
                Review failed
                {report.data.failureStage
                  ? ` during ${report.data.failureStage}`
                  : ""}
                : {report.data.failureCause}
              </p>
            )}
            {document != null ? (
              <ApprovalReportDocument document={document} />
            ) : (
              <p>No decision report was produced.</p>
            )}
            <details className="mt-6 text-xs">
              <summary>Review details</summary>
              {report.data.sha256 && (
                <p className="mt-2 break-all">
                  SHA-256: <code>{report.data.sha256}</code>
                </p>
              )}
              <FormalReviewInspection
                workspaceId={workspaceId}
                reviewId={reviewId}
              />
            </details>
          </>
        )}
      </div>
    </main>
  );
}
