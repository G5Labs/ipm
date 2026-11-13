/**
 * ReviewWorkspaceCard — the Approvals page's Review Workspace section for a
 * viewer who does not own the workspace. Start review forks the source into a
 * reviewer-owned review workspace and opens it; a persisted review shows its
 * VM state and reopens the same workspace.
 */

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it } from "vitest";

import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import type { WorkspaceReview } from "../../lib/types";
import { ReviewWorkspaceCard } from "./ReviewWorkspaceCard";

const ORG_SLUG = "g5labs";
const SOURCE_ID = "ws-source-1";
const REVIEW_ID = "ws-review-1";

function reviewFixture(overrides?: Partial<WorkspaceReview>): WorkspaceReview {
  return {
    forkId: "fork-1",
    orgId: "org-g5labs",
    sourceWorkspaceId: SOURCE_ID,
    sourceWorkspaceName: "kaihui-test3",
    reviewWorkspaceId: REVIEW_ID,
    reviewWorkspaceName: "kaihui-test3-review-alex-chen",
    reviewerUserId: "user-alex",
    reviewerDisplayName: "Alex Chen",
    vmStatus: "stopped",
    vmStartedAt: null,
    vmLastActivityAt: null,
    lastFlushedAt: "2026-09-23T14:31:00.000Z",
    createdAt: "2026-09-22T16:10:00.000Z",
    ...overrides,
  };
}

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location-probe">{location.pathname}</div>;
}

function renderCard() {
  return renderApp(
    <Routes>
      <Route
        path="/:org/:workspace"
        element={
          <>
            <ReviewWorkspaceCard
              workspaceId={SOURCE_ID}
              workspaceName="kaihui-test3"
              orgSlug={ORG_SLUG}
            />
            <LocationProbe />
          </>
        }
      />
    </Routes>,
    { route: `/${ORG_SLUG}/kaihui-test3` },
  );
}

describe("ReviewWorkspaceCard", () => {
  beforeEach(() => {
    try {
      sessionStorage.clear();
    } catch {
      // ignore
    }
  });

  it("offers Start review when the viewer has no review workspace yet, forks on click and opens it", async () => {
    const started: unknown[] = [];
    server.use(
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review`, () =>
        HttpResponse.json({
          data: { asSource: null, asReview: null, openSuggestions: 0 },
        }),
      ),
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/start`,
        async ({ request }) => {
          started.push(await request.json());
          return HttpResponse.json(
            {
              data: {
                review: reviewFixture({ vmStatus: "starting" }),
                created: true,
                resume: null,
                resumeError: null,
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    renderCard();

    expect(
      await screen.findByText("Review kaihui-test3 in your own copy"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("workspace-review-stripe")).toBeInTheDocument();
    const start = screen.getByRole("button", { name: "Start review" });
    await userEvent.click(start);

    await waitFor(() => expect(started).toHaveLength(1));
    await waitFor(() =>
      expect(screen.getByTestId("location-probe")).toHaveTextContent(
        `/${ORG_SLUG}/kaihui-test3-review-alex-chen`,
      ),
    );
    // The review workspace page consumes this one-shot marker to seed the
    // review prompt.
    expect(
      sessionStorage.getItem(`g5.review-workspace.seed:${REVIEW_ID}`),
    ).toBe("1");
  });

  it("shows the persisted review workspace with its VM state and reopens it", async () => {
    let starts = 0;
    server.use(
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review`, () =>
        HttpResponse.json({
          data: {
            asSource: reviewFixture(),
            asReview: null,
            openSuggestions: 2,
          },
        }),
      ),
      http.post(`*/api/v1/workspaces/${SOURCE_ID}/review/start`, () => {
        starts += 1;
        return HttpResponse.json({
          data: {
            review: reviewFixture({ vmStatus: "starting" }),
            created: false,
            resume: null,
            resumeError: null,
          },
        });
      }),
    );
    renderCard();

    const card = await screen.findByTestId("review-workspace-card");
    expect(
      await within(card).findByTestId("workspace-review-badge"),
    ).toBeInTheDocument();
    expect(
      within(card).getByRole("link", { name: "kaihui-test3-review-alex-chen" }),
    ).toHaveAttribute("href", `/${ORG_SLUG}/kaihui-test3-review-alex-chen`);
    expect(
      within(card).getByTestId("review-workspace-vm-status"),
    ).toHaveTextContent("VM detached");
    expect(card).toHaveTextContent("they appear under Reviews above");

    await userEvent.click(
      within(card).getByRole("button", { name: "Reopen review" }),
    );
    await waitFor(() => expect(starts).toBe(1));
    await waitFor(() =>
      expect(screen.getByTestId("location-probe")).toHaveTextContent(
        `/${ORG_SLUG}/kaihui-test3-review-alex-chen`,
      ),
    );
    // A reopened copy keeps its chat: no review prompt is seeded again.
    expect(
      sessionStorage.getItem(`g5.review-workspace.seed:${REVIEW_ID}`),
    ).toBeNull();
  });

  it("explains what Start review is doing while the fork runs", async () => {
    let release: (() => void) | null = null;
    server.use(
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review`, () =>
        HttpResponse.json({
          data: { asSource: null, asReview: null, openSuggestions: 0 },
        }),
      ),
      http.post(`*/api/v1/workspaces/${SOURCE_ID}/review/start`, async () => {
        await new Promise<void>((resolve) => {
          release = resolve;
        });
        return HttpResponse.json(
          {
            data: {
              review: reviewFixture({ vmStatus: "starting" }),
              created: true,
              resume: null,
              resumeError: null,
              sync: null,
            },
          },
          { status: 201 },
        );
      }),
    );
    renderCard();

    await userEvent.click(
      await screen.findByRole("button", { name: "Start review" }),
    );
    const progress = await screen.findByTestId("review-workspace-progress");
    expect(progress).toHaveTextContent(
      "Creating a workspace copy and getting your review assistant ready.",
    );
    expect(
      screen.getByRole("button", { name: /Setting up your review workspace/ }),
    ).toBeDisabled();
    (release as unknown as () => void)();
    await waitFor(() =>
      expect(screen.getByTestId("location-probe")).toHaveTextContent(
        `/${ORG_SLUG}/kaihui-test3-review-alex-chen`,
      ),
    );
  });

  it("carries a VM resume failure to the review workspace page", async () => {
    server.use(
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review`, () =>
        HttpResponse.json({
          data: { asSource: null, asReview: null, openSuggestions: 0 },
        }),
      ),
      http.post(`*/api/v1/workspaces/${SOURCE_ID}/review/start`, () =>
        HttpResponse.json(
          {
            data: {
              review: reviewFixture({ vmStatus: "stopped" }),
              created: true,
              resume: null,
              resumeError: "workspace lifecycle signing is unavailable",
            },
          },
          { status: 201 },
        ),
      ),
    );
    renderCard();

    await userEvent.click(
      await screen.findByRole("button", { name: "Start review" }),
    );
    await waitFor(() =>
      expect(screen.getByTestId("location-probe")).toHaveTextContent(
        `/${ORG_SLUG}/kaihui-test3-review-alex-chen`,
      ),
    );
    expect(
      sessionStorage.getItem(`g5.review-workspace.resume-error:${REVIEW_ID}`),
    ).toBe("workspace lifecycle signing is unavailable");
  });

  it("surfaces a refused start instead of navigating", async () => {
    server.use(
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review`, () =>
        HttpResponse.json({
          data: { asSource: null, asReview: null, openSuggestions: 0 },
        }),
      ),
      http.post(`*/api/v1/workspaces/${SOURCE_ID}/review/start`, () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "could not choose a free review workspace name",
            },
          },
          { status: 409 },
        ),
      ),
    );
    renderCard();

    await userEvent.click(
      await screen.findByRole("button", { name: "Start review" }),
    );
    expect(
      await screen.findByTestId("review-workspace-start-error"),
    ).toHaveTextContent("could not choose a free review workspace name");
    expect(screen.getByTestId("location-probe")).toHaveTextContent(
      `/${ORG_SLUG}/kaihui-test3`,
    );
  });
});
