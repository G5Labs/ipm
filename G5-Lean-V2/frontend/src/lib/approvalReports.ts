export const APPROVAL_REPORT_PATH =
  "/:org/review-reports/report/:workspaceId/:reviewId";
export function approvalReportHref(
  orgSlug: string,
  workspaceId: string,
  reviewId: string,
) {
  return `/${encodeURIComponent(orgSlug)}/review-reports/report/${encodeURIComponent(workspaceId)}/${encodeURIComponent(reviewId)}`;
}
