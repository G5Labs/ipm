/**
 * ReviewRoundsOverview: on the reviewer's page, every unresolved review of
 * the source with each of its rounds, theirs first; resolved ones on
 * request.
 */
import React from "react";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import type { ReviewSuggestion } from "../../lib/types";
import {
  ReviewRoundsOverview,
  reviewThreads,
  threadUnresolved,
} from "./ReviewRoundsOverview";

const SOURCE_ID = "ws-source-1";
const ME = "user-alex";
const OTHER = "user-sam";

function publication(overrides: Record<string, unknown>) {
  return {
    id: "pub",
    orgId: "org-1",
    sourceWorkspaceId: SOURCE_ID,
    reviewWorkspaceId: "ws-review",
    reviewerUserId: ME,
    authorKind: "human",
    authorLabel: "alex",
    round: 1,
    baseCommitVector: [],
    summary: "",
    verdict: "request_changes",
    scores: null,
    supersededBy: null,
    createdAt: "2026-09-23T10:00:00.000Z",
    kind: "round",
    openCount: 0,
    acceptedCount: 0,
    rejectedCount: 0,
    resolvedCount: 0,
    supersededCount: 0,
    artifactCount: 0,
    ...overrides,
  };
}

function suggestion(overrides: Partial<ReviewSuggestion>): ReviewSuggestion {
  return {
    id: "sug",
    publicationId: "pub",
    findingId: null,
    repoId: "repo-api",
    path: "src/main.rs",
    kind: "hunk",
    beforeLines: "a\n",
    afterLines: "b\n",
    anchorStart: 1,
    contentHash: "h",
    anchorContext: null,
    authorKind: "human",
    authorLabel: "alex",
    note: "",
    commentId: "c",
    status: "open",
    statusBy: null,
    statusAt: null,
    supersedes: null,
    adoptedFrom: null,
    createdAt: "2026-09-23T10:00:00.000Z",
    ...overrides,
  };
}

const publications = [
  publication({ id: "me-2", round: 2, createdAt: "2026-09-24T10:00:00.000Z" }),
  publication({
    id: "me-1",
    round: 1,
    supersededBy: "me-2",
    verdict: "comment_only",
  }),
  publication({
    id: "sam-1",
    reviewerUserId: OTHER,
    authorLabel: "sam",
    verdict: "approve",
    createdAt: "2026-09-25T10:00:00.000Z",
  }),
  publication({
    id: "note-1",
    kind: "annotation",
    verdict: "comment_only",
    createdAt: "2026-09-25T11:00:00.000Z",
  }),
];
const suggestions = [
  suggestion({ id: "s-2a", publicationId: "me-2", supersedes: "s-1a" }),
  suggestion({
    id: "s-2b",
    publicationId: "me-2",
    path: "NEW.md",
    kind: "new_file",
    anchorStart: null,
    beforeLines: null,
  }),
  suggestion({ id: "s-1a", publicationId: "me-1", status: "superseded" }),
  suggestion({ id: "s-sam", publicationId: "sam-1", status: "accepted" }),
];

describe("ReviewRoundsOverview", () => {
  it("counts a carried edit once and keeps its pending round current after an accepted checkpoint", () => {
    const rounds = [
      publication({
        id: "current",
        round: 2,
        baseCommitVector: [{ repoId: "docs", sourceHead: "before" }],
      }),
      publication({ id: "history", round: 1, supersededBy: "current" }),
    ];
    const rows = [
      suggestion({
        id: "history:pending",
        publicationId: "history",
        editId: "pending",
        parentSuggestionId: "history",
        status: "superseded",
      }),
      suggestion({
        id: "current:pending",
        publicationId: "current",
        editId: "pending",
        parentSuggestionId: "current",
        status: "open",
      }),
      suggestion({
        id: "history:accepted",
        publicationId: "history",
        editId: "accepted",
        parentSuggestionId: "history",
        status: "accepted",
      }),
      suggestion({
        id: "current:accepted",
        publicationId: "current",
        editId: "accepted",
        parentSuggestionId: "current",
        status: "accepted",
      }),
    ];
    const [thread] = reviewThreads(rounds, rows, {}, [], { docs: "after" });
    expect(thread.openCount).toBe(1);
    expect(thread.outdatedOpenCount).toBe(0);
    expect(thread.acceptedCount).toBe(1);
    expect(threadUnresolved(thread)).toBe(true);
  });

  it("groups rounds into reviews, unresolved first, with each round's delta", () => {
    const threads = reviewThreads(publications, suggestions, {
      [ME]: "Alex Chen",
      [OTHER]: "Sam Lee",
    });
    expect(threads.map((thread) => thread.authorLabel)).toEqual([
      "Alex Chen",
      "Sam Lee",
    ]);
    expect(threads[0].rounds.map((round) => round.round)).toEqual([2, 1]);
    expect(threads[0].openCount).toBe(2);
    expect(threads[1].openCount).toBe(0);
    expect(threads[1].acceptedCount).toBe(1);
  });

  it("keeps a review unresolved while a finding of it is open, even with no suggestions", () => {
    const reviewerRound = publication({
      id: "reviewer-round",
      reviewerUserId: OTHER,
      authorKind: "human",
      authorLabel: "Sam Lee",
      createdAt: "2026-09-26T10:00:00.000Z",
    });
    const threads = reviewThreads([reviewerRound], [], { [OTHER]: "Sam Lee" }, [
      {
        id: "fnd-1",
        publicationId: "reviewer-round",
        title: "Exit code is wrong",
        severity: "P1",
        dimension: "correctness",
        evidence: "",
        actions: [],
        status: "open",
        position: 0,
        repoId: null,
        paths: [],
        createdAt: "2026-09-26T10:00:00.000Z",
      },
      {
        id: "fnd-2",
        publicationId: "reviewer-round",
        title: "Already handled",
        severity: "P3",
        dimension: "style",
        evidence: "",
        actions: [],
        status: "accepted",
        position: 1,
        repoId: null,
        paths: [],
        createdAt: "2026-09-26T10:00:00.000Z",
      },
    ]);
    expect(threads).toHaveLength(1);
    expect(threads[0].openCount).toBe(0);
    expect(threads[0].openFindings).toBe(1);
    expect(threadUnresolved(threads[0])).toBe(true);
  });

  it("keeps open items of a round about an earlier commit apart, never as unresolved", () => {
    const OLD = "a".repeat(40);
    const NEW = "b".repeat(40);
    const threads = reviewThreads(
      [
        publication({
          id: "me-2",
          round: 2,
          baseCommitVector: [{ repoId: "repo-api", sourceHead: OLD }],
        }),
      ],
      [suggestion({ id: "s", publicationId: "me-2" })],
      { [ME]: "Alex Chen" },
      [],
      { "repo-api": NEW },
    );
    expect(threads[0].currentRoundIds.has("me-2")).toBe(false);
    expect(threads[0].openCount).toBe(0);
    expect(threads[0].outdatedOpenCount).toBe(1);
    expect(threadUnresolved(threads[0])).toBe(false);
  });

  it("marks rounds about an earlier commit outdated in the list", async () => {
    const OLD = "a".repeat(40);
    const NEW = "b".repeat(40);
    server.use(
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review/publications`, () =>
        HttpResponse.json({
          data: {
            publications: [
              publication({
                id: "me-2",
                round: 2,
                baseCommitVector: [{ repoId: "repo-api", sourceHead: OLD }],
              }),
            ],
            findings: [],
            reviewerDisplayNames: { [ME]: "Alex Chen" },
            sourceHeads: { "repo-api": NEW },
          },
        }),
      ),
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review/suggestions`, () =>
        HttpResponse.json({
          data: {
            suggestions: [
              suggestion({ id: "s-2a", publicationId: "me-2" }),
              suggestion({ id: "s-2b", publicationId: "me-2", path: "b.rs" }),
            ],
          },
        }),
      ),
    );
    renderApp(
      <ReviewRoundsOverview
        sourceWorkspaceId={SOURCE_ID}
        sourceWorkspaceName="kaihui-test3"
        viewerUserId={ME}
      />,
    );
    const overview = await screen.findByTestId("review-rounds-overview");
    // Nothing is unresolved: the open items are about a commit the owner
    // left behind.
    expect(
      within(overview).getByTestId("review-rounds-overview-summary"),
    ).toHaveTextContent("all reviews resolved");
    await userEvent.click(
      within(overview).getByTestId("review-rounds-overview-show-resolved"),
    );
    expect(
      within(overview).getByTestId("review-thread-round-outdated-me-2"),
    ).toHaveTextContent("Outdated");
    expect(
      within(overview).getByTestId(`review-thread-outdated-open-${ME}-human`),
    ).toHaveTextContent("2 open on an earlier commit");
  });

  it("shows unresolved reviews with their rounds, and resolved ones on request", async () => {
    server.use(
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review/publications`, () =>
        HttpResponse.json({
          data: {
            publications,
            findings: [],
            reviewerDisplayNames: { [ME]: "Alex Chen", [OTHER]: "Sam Lee" },
          },
        }),
      ),
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review/suggestions`, () =>
        HttpResponse.json({ data: { suggestions } }),
      ),
    );
    renderApp(
      <ReviewRoundsOverview
        sourceWorkspaceId={SOURCE_ID}
        sourceWorkspaceName="kaihui-test3"
        viewerUserId={ME}
      />,
    );
    const overview = await screen.findByTestId("review-rounds-overview");
    expect(
      await within(overview).findByTestId("review-rounds-overview-summary"),
    ).toHaveTextContent("1 unresolved of 2");
    const mine = within(overview).getByTestId(`review-thread-${ME}-human`);
    expect(mine).toHaveTextContent("Alex Chen (you)");
    expect(mine).toHaveTextContent("Unresolved · 2 open");
    const rounds = within(mine).getAllByTestId(/review-thread-round-/);
    expect(rounds).toHaveLength(2);
    expect(rounds[0]).toHaveTextContent("Round 2");
    expect(rounds[0]).toHaveTextContent("Changes requested");
    expect(rounds[0]).toHaveTextContent(
      "since round 1: 1 new · 1 carried over · 0 dropped",
    );
    expect(rounds[1]).toHaveTextContent("Round 1");
    // Sam's review is resolved: hidden until asked for.
    expect(
      within(overview).queryByTestId(`review-thread-${OTHER}-human`),
    ).not.toBeInTheDocument();
    await userEvent.click(
      within(overview).getByTestId("review-rounds-overview-show-resolved"),
    );
    const sams = within(overview).getByTestId(`review-thread-${OTHER}-human`);
    expect(sams).toHaveTextContent("Resolved");
    expect(sams).toHaveTextContent("Approved");
  });
});
