import { describe, expect, it } from "vitest";
import { matchRoutes } from "react-router-dom";
import { APPROVAL_REPORT_PATH, approvalReportHref } from "./approvalReports";
import {
  WORKSPACE_SHELL_ROUTE_PATTERNS,
  classifyShellRoute,
} from "./shellRoutes";

// Use the same registrations and order as App so intersections retain real
// workspace navigation even when an organization uses the report prefix.
const routes = [
  { path: APPROVAL_REPORT_PATH, id: "report" },
  ...WORKSPACE_SHELL_ROUTE_PATTERNS.map((path) => ({ path, id: "workspace" })),
];
describe("approval report route isolation", () => {
  it.each([
    "intent-log",
    "task/123",
    "doc/report.md",
    "file/report.md",
    "guide/123",
    "commit/repo/sha",
    "issue/repo/704",
  ])("preserves a review-reports organization's %s route", (suffix) => {
    const path = `/review-reports/workspace/${suffix}`;
    expect(matchRoutes(routes, path)?.at(-1)?.route.id).toBe("workspace");
    expect(classifyShellRoute(path).kind).toBe("workspace");
    const namedWorkspace = `/g5labs/review-reports/${suffix}`;
    expect(matchRoutes(routes, namedWorkspace)?.at(-1)?.route.id).toBe(
      "workspace",
    );
    expect(classifyShellRoute(namedWorkspace).kind).toBe("workspace");
  });
  it("opens exact retained reports outside the workspace shell", () => {
    const path = approvalReportHref("g5labs", "workspace", "exact-report");
    expect(matchRoutes(routes, path)?.at(-1)).toMatchObject({
      route: { id: "report" },
      params: {
        org: "g5labs",
        workspaceId: "workspace",
        reviewId: "exact-report",
      },
    });
    expect(classifyShellRoute(path).kind).toBe("other");
  });
});
