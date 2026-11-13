import { expect, it } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router-dom";
import { http, HttpResponse } from "msw";
import { createTestJwt, renderApp } from "../test/render";
import { createMockUser } from "../test/fixtures";
import { server } from "../test/msw/server";
import { RequireAuth } from "../hooks/useAuth";
import { ApprovalReportPage } from "./ApprovalReportPage";
import { APPROVAL_REPORT_PATH } from "../lib/approvalReports";

function renderReport(org = "g5labs", user = createMockUser()) {
  return renderApp(
    <Routes>
      <Route
        path={APPROVAL_REPORT_PATH}
        element={
          <RequireAuth>
            <ApprovalReportPage />
          </RequireAuth>
        }
      />
    </Routes>,
    { route: `/${org}/review-reports/report/ws/exact-run`, user },
  );
}
it("renders the retained report and exposes exact raw bytes without editing or commit browsing", async () => {
  const document =
    "---\r\nschemaVersion: 3\r\nrepoName: app\r\n---\r\n# Findings\r\n\r\n**Readable finding**\r\n\r\n| Repository | Outcome |\r\n|---|---|\r\n| app | Approved |\r\n";
  const reads: string[] = [];
  server.use(
    http.get(
      "/api/v1/workspaces/:workspace/approvals/reviews/:review/report",
      ({ params }) => {
        reads.push(`${params.workspace}/${params.review}`);
        return HttpResponse.json({
          data: { document, sha256: "retained-digest" },
        });
      },
    ),
  );
  renderReport();
  expect(
    await screen.findByRole("heading", { name: "Findings" }),
  ).toBeVisible();
  expect(screen.getByRole("table")).toBeVisible();
  expect(screen.getByText("Readable finding").tagName).toBe("STRONG");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(screen.queryByText("Browse reviewed commits")).not.toBeInTheDocument();
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Raw" }));
  expect(screen.getByTestId("approval-report-source").textContent).toBe(
    document,
  );
  expect(screen.getByRole("button", { name: "Raw" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await user.click(screen.getByRole("button", { name: "View" }));
  const viewer = screen.getByTestId("agent-review-report-view");
  await user.click(within(viewer).getByText("Report frontmatter"));
  expect(viewer).toHaveTextContent("repoName: app");
  expect(reads).toEqual(["ws/exact-run"]);
});
it("reports read failures and retries the same retained artifact", async () => {
  let forbidden = true;
  server.use(
    http.get("/api/v1/workspaces/ws/approvals/reviews/exact-run/report", () =>
      forbidden
        ? HttpResponse.json(
            { error: { message: "Workspace access denied" } },
            { status: 403 },
          )
        : HttpResponse.json({
            data: {
              document: null,
              failureStage: "execution",
              failureCause: "Provider unavailable",
            },
          }),
    ),
  );
  renderReport();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Workspace access denied",
  );
  expect(
    screen.queryByTestId("agent-review-report-view"),
  ).not.toBeInTheDocument();
  forbidden = false;
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(
    await screen.findByText("No decision report was produced."),
  ).toBeVisible();
  expect(screen.getByText(/Review failed during execution/)).toHaveTextContent(
    "Provider unavailable",
  );
  expect(screen.queryByRole("button", { name: "Raw" })).not.toBeInTheDocument();
});

it.each([false, true])(
  "holds report reads until the URL organization is adopted (wrong token: %s)",
  async (wrongOrg) => {
    const user = createMockUser();
    user.orgs[1].role = "viewer";
    const reads: string[] = [];
    const switches: unknown[] = [];
    server.use(
      http.post("/api/v1/auth/refresh", async ({ request }) => {
        switches.push(await request.json());
        return HttpResponse.json({
          data: {
            jwt: createTestJwt(user, wrongOrg ? "org-g5labs" : "org-acme"),
          },
        });
      }),
      http.get(
        "/api/v1/workspaces/ws/approvals/reviews/exact-run/report",
        ({ request }) => {
          const jwt = request.headers
            .get("Authorization")!
            .replace("Bearer ", "");
          const claims = JSON.parse(atob(jwt.split(".")[1]));
          reads.push(claims.org);
          return claims.org === "org-acme"
            ? HttpResponse.json({ data: { document: "# Acme report" } })
            : HttpResponse.json(
                { error: { message: "Wrong organization" } },
                { status: 403 },
              );
        },
      ),
    );
    renderReport("acme", user);
    if (wrongOrg) {
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "Could not switch to Acme",
      );
      expect(reads).toEqual([]);
    } else {
      expect(
        await screen.findByRole("heading", { name: "Acme report" }),
      ).toBeVisible();
      expect(reads).toEqual(["org-acme"]);
    }
    expect(switches).toEqual([{ orgId: "org-acme" }]);
  },
);
it("refuses an inaccessible URL organization without requesting a report", async () => {
  const reads: string[] = [];
  server.use(
    http.get("/api/v1/workspaces/ws/approvals/reviews/exact-run/report", () => {
      reads.push("report");
      return HttpResponse.json({ data: { document: "# Should not load" } });
    }),
  );
  renderReport("unknown");
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "You do not have access to this organization",
  );
  expect(reads).toEqual([]);
});

it("collapses only the Governing prompt in the rendered view and keeps Raw exact (g5.governance.approval.report-view)", async () => {
  const document =
    "---\nschemaVersion: 6\nresult: rejected\n---\n## Governing prompt\n\nReview **cancellation** only.\n\n## Verdict\n\nRejected: cancellation leaks resources.\n\n## Review scope and provenance\n\napp `abc123`\n";
  server.use(
    http.get(
      "/api/v1/workspaces/:workspace/approvals/reviews/:review/report",
      () => HttpResponse.json({ data: { document, sha256: "digest" } }),
    ),
  );
  renderReport();
  const verdict = await screen.findByRole("heading", { name: "Verdict" });
  expect(verdict).toBeVisible();
  expect(
    screen.getByRole("heading", { name: "Review scope and provenance" }),
  ).toBeVisible();
  const prompt = screen.getByTestId("approval-report-governing-prompt");
  expect(prompt).not.toHaveAttribute("open");
  // It stays a level-2 section heading like the others.
  expect(
    within(prompt).getByRole("heading", { level: 2, name: "Governing prompt" }),
  ).toBeVisible();
  expect(within(prompt).getByText("cancellation")).not.toBeVisible();
  const user = userEvent.setup();
  // A native disclosure: browsers also toggle it with Enter and Space.
  await user.click(within(prompt).getByText("Governing prompt"));
  expect(prompt).toHaveAttribute("open");
  expect(within(prompt).getByText("cancellation")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Raw" }));
  expect(screen.getByTestId("approval-report-source").textContent).toBe(
    document,
  );
});
