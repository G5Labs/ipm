import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  issueErrorResponse,
  makeIssueLink,
  makeIssueSummary,
  serveExternalIssuesFeature,
} from "../../test/externalIssueFixtures";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { ImportTaskPicker } from "./ImportTaskPicker";

const linksUrl = "/api/v1/workspaces/ws-1/issue-links";
const searchUrl = "/api/v1/orgs/org-g5labs/issues/search";
const issues = [12, 13, 14, 15].map((number) =>
  makeIssueSummary({ locator: { repositoryId: "11", number } }),
);
function renderPicker(owner = true, writeBlockedReason?: string) {
  const onClose = vi.fn();
  const rendered = renderApp(
    <Routes>
      <Route
        path="/:org/:workspace"
        element={
          <ImportTaskPicker
            workspaceId="ws-1"
            viewerOwnsWorkspace={owner}
            writeBlockedReason={writeBlockedReason}
            onClose={onClose}
          />
        }
      />
    </Routes>,
    { route: "/g5labs/current", user: createMockUser() },
  );
  return { ...rendered, onClose };
}
beforeEach(() => {
  serveExternalIssuesFeature();
  server.use(
    http.get(linksUrl, () =>
      HttpResponse.json({ data: { links: [makeIssueLink()] } }),
    ),
    http.get(searchUrl, () =>
      HttpResponse.json({
        data: {
          results: issues,
          domainCount: 1,
          domainErrors: [],
          truncated: false,
        },
      }),
    ),
  );
});

describe("ImportTaskPicker", () => {
  it("prechecks existing links and imports sequentially, preserving per-row successes around a conflict", async () => {
    const posted: unknown[] = [];
    let release!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.post(linksUrl, async ({ request }) => {
        const body = (await request.json()) as { ref: string; role: string };
        posted.push(body);
        if (body.ref === "acme/api#13") await held;
        if (body.ref === "acme/api#14")
          return HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "Conflict",
                details: { code: "already_linked" },
              },
            },
            { status: 409 },
          );
        return HttpResponse.json(
          {
            data: makeIssueLink({
              issue: issues.find((issue) => issue.displayKey === body.ref),
            }),
          },
          { status: 201 },
        );
      }),
    );
    renderPicker();
    const linked = await screen.findByRole("checkbox", { name: "acme/api#12" });
    await waitFor(() => expect(linked).toBeChecked());
    expect(linked).toBeDisabled();
    const user = userEvent.setup();
    for (const number of [13, 14, 15])
      await user.click(
        screen.getByRole("checkbox", { name: `acme/api#${number}` }),
      );
    await user.click(screen.getByRole("button", { name: "Import 3 tasks" }));
    await waitFor(() => expect(posted).toHaveLength(1));
    expect(screen.getByRole("button", { name: "Importing…" })).toBeDisabled();
    release();
    await screen.findByTestId("workspace-issue-conflict-already_linked");
    await waitFor(() =>
      expect(screen.getAllByText("Imported")).toHaveLength(2),
    );
    expect(posted).toEqual(
      [13, 14, 15].map((number) => ({
        ref: `acme/api#${number}`,
        role: "related",
      })),
    );
    expect(screen.getByRole("button", { name: "Import 1 task" })).toBeEnabled();
  });
  it("shows a typed failure on its row", async () => {
    server.use(
      http.post(linksUrl, () =>
        issueErrorResponse(422, "outside_domain", "Domain removed"),
      ),
    );
    renderPicker();
    const checkbox = await screen.findByRole("checkbox", {
      name: "acme/api#13",
    });
    await waitFor(() => expect(checkbox).toBeEnabled());
    await userEvent.click(checkbox);
    await userEvent.click(
      screen.getByRole("button", { name: "Import 1 task" }),
    );
    expect(
      await screen.findByTestId("import-task-error-900013"),
    ).toHaveTextContent("Domain removed");
  });
  it.each([1, 2])(
    "distinguishes unavailable and partial search (domains=%s)",
    async (domainCount) => {
      server.use(
        http.get(searchUrl, () =>
          HttpResponse.json({
            data: {
              results: [],
              domainCount,
              domainErrors: [
                {
                  domainId: "d1",
                  displayKey: "acme/api",
                  issueError: "unavailable",
                  message: "Tracker offline",
                },
              ],
              truncated: false,
            },
          }),
        ),
      );
      renderPicker();
      expect(
        await screen.findByTestId("import-task-domain-error-d1"),
      ).toHaveTextContent("Tracker offline");
      expect(
        screen.getByText(
          domainCount === 1
            ? "Issue search is unavailable."
            : "No matching issues in the repositories that could be searched.",
        ),
      ).toBeInTheDocument();
      expect(screen.queryByText("No matching issues.")).toBeNull();
    },
  );
  it("debounces open issue searches and Escape or Close dismisses", async () => {
    const queries: string[] = [];
    server.use(
      http.get(searchUrl, ({ request }) => {
        const params = new URL(request.url).searchParams;
        queries.push(params.get("q") ?? "");
        expect(params.get("state")).toBe("open");
        expect(params.has("assignee")).toBe(false);
        return HttpResponse.json({
          data: {
            results: [],
            domainCount: 1,
            domainErrors: [],
            truncated: false,
          },
        });
      }),
    );
    const { onClose } = renderPicker();
    await screen.findByText("No matching issues.");
    const user = userEvent.setup();
    await user.type(screen.getByRole("searchbox"), "search bug");
    expect(queries).toEqual([""]);
    await waitFor(() => expect(queries).toEqual(["", "search bug"]), {
      timeout: 2500,
    });
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
  it("is owner-only", () => {
    renderPicker(false);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
  it("blocks importing for a review copy", async () => {
    renderPicker(true, "A review copy follows its source.");
    const checkbox = await screen.findByRole("checkbox", {
      name: "acme/api#13",
    });
    expect(checkbox).toBeDisabled();
    const button = within(screen.getByRole("dialog")).getByRole("button", {
      name: "Import 0 tasks",
    });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      "A review copy follows its source.",
    );
  });
});
