/**
 * ReviewsPanel — pushed reviews and their suggestions on a source workspace.
 * The owner accepts into the running workspace, edits before accepting,
 * rejects, resolves comments; a reviewer withdraws; readers reply and leave
 * comments without a fork. Accepting is offered only while the VM runs.
 * The owner can hand a whole round to their agent, never one hunk.
 */

import { createMockWorkspaceVm } from "../../test/fixtures";
import React from "react";
import {
  act,
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { describe, expect, it, vi } from "vitest";

import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import type {
  ReviewSuggestion,
  ReviewPublication,
  WorkspaceVmStatus,
} from "../../lib/types";
import { MOCK_WORKSPACE_DETAILS } from "../../data/mock";
import { ReviewsPanel, competingClusters, roundDelta } from "./ReviewsPanel";
import { reviewEditFixture } from "../../test/reviewEdits";
import { expandReviewSuggestionEdits } from "../../lib/reviewSuggestionEdits";
import { getApiToken, setApiToken, tokenPrincipal } from "../../lib/api";
import { sha256Utf8 } from "../../lib/commentAnchors";

// A blob made in the page is jsdom's (FileReader only); one taken from a
// fetched response is Node's (`text()` only).
async function readBlobText(blob: Blob): Promise<string> {
  if (typeof (blob as { text?: unknown }).text === "function") {
    return blob.text();
  }
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsText(blob);
  });
}

const SOURCE_ID = "ws-source-1";
const OWNER = "user-owner";
const REVIEWER = "user-alex";

function suggestion(overrides: Partial<ReviewSuggestion>): ReviewSuggestion {
  return {
    id: "sug-1",
    publicationId: "pub-1",
    findingId: null,
    repoId: "repo-api",
    path: "src/main.rs",
    kind: "hunk",
    beforeLines: "return 1;\n",
    afterLines: "return 0;\n",
    anchorStart: 3,
    contentHash: "abc",
    anchorContext: null,
    authorKind: "human",
    authorLabel: "alex",
    note: "Early return should succeed.",
    commentId: "c-1",
    status: "open",
    statusBy: null,
    statusAt: null,
    supersedes: null,
    adoptedFrom: null,
    createdAt: "2026-09-23T14:31:00.000Z",
    ...overrides,
  };
}

function publications() {
  return {
    publications: [
      {
        id: "pub-1",
        orgId: "org-g5labs",
        sourceWorkspaceId: SOURCE_ID,
        reviewWorkspaceId: "ws-review-1",
        reviewerUserId: REVIEWER,
        authorKind: "human",
        authorLabel: "alex",
        round: 2,
        baseCommitVector: [] as unknown,
        summary: "Two fixes, one question.",
        verdict: "request_changes",
        scores: null,
        supersededBy: null,
        createdAt: "2026-09-23T14:31:00.000Z",
        openCount: 2,
        acceptedCount: 0,
        rejectedCount: 0,
        resolvedCount: 0,
        supersededCount: 0,
        artifactCount: 0,
      },
      {
        id: "pub-0",
        orgId: "org-g5labs",
        sourceWorkspaceId: SOURCE_ID,
        reviewWorkspaceId: "ws-review-1",
        reviewerUserId: REVIEWER,
        authorKind: "human",
        authorLabel: "alex",
        round: 1,
        baseCommitVector: [] as unknown,
        summary: "First pass.",
        verdict: "comment_only",
        scores: null,
        supersededBy: "pub-1",
        createdAt: "2026-09-22T10:00:00.000Z",
        openCount: 0,
        acceptedCount: 1,
        rejectedCount: 0,
        resolvedCount: 0,
        supersededCount: 1,
        artifactCount: 0,
      },
    ],
    findings: [
      {
        id: "fnd-1",
        publicationId: "pub-1",
        title: "Exit code is wrong",
        severity: "P1",
        dimension: "correctness",
        evidence: "main returns 1 on success.",
        actions: [],
        status: "open",
        position: 0,
        repoId: null,
        paths: ["src/main.rs"],
        createdAt: "2026-09-23T14:31:00.000Z",
      },
    ],
    reviewerDisplayNames: { [REVIEWER]: "Alex Chen" },
  };
}

function seed(options: {
  vmStatus?: WorkspaceVmStatus;
  /** The source's writability: a read-only source takes no decision or reply. */
  sourceReadOnly?: boolean;
  /** Other fields of the source's detail (Base, a freezing transition). */
  sourceDetail?: Record<string, unknown>;
  suggestions?: ReviewSuggestion[];
  publications?: ReturnType<typeof publications> & {
    sourceHeads?: Record<string, string>;
    sourceHeadTimes?: Record<string, string>;
  };
}) {
  const list = options.suggestions ?? [
    suggestion({ id: "sug-1", findingId: "fnd-1" }),
    suggestion({
      id: "sug-2",
      kind: "comment",
      path: "README.md",
      beforeLines: null,
      afterLines: null,
      anchorStart: 12,
      note: "Is this still accurate?",
      commentId: "c-2",
    }),
  ];
  server.use(
    http.get(`*/api/v1/workspaces/${SOURCE_ID}/review/publications`, () =>
      HttpResponse.json({ data: options.publications ?? publications() }),
    ),
    http.get(
      `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions`,
      ({ request }) => {
        // Like the server: `kind` and `status` narrow the rows (the kept
        // artifacts list asks for kind=artifact&status=accepted).
        const url = new URL(request.url);
        const kind = url.searchParams.get("kind");
        const status = url.searchParams.get("status");
        return HttpResponse.json({
          data: {
            suggestions: list.filter(
              (row) =>
                (!kind || row.kind === kind) &&
                (!status || row.status === status),
            ),
          },
        });
      },
    ),
    http.get(`*/api/v1/workspaces/${SOURCE_ID}`, () =>
      HttpResponse.json({
        data: {
          ...MOCK_WORKSPACE_DETAILS["task-graph-rework"],
          id: SOURCE_ID,
          name: "kaihui-test3",
          ownerUserId: OWNER,
          isBase: false,
          readOnly: options.sourceReadOnly ?? false,
          ...options.sourceDetail,
        },
      }),
    ),
    http.get(`*/api/v1/workspaces/${SOURCE_ID}/vm`, () =>
      HttpResponse.json({
        data: createMockWorkspaceVm({
          orgId: "org-g5labs",
          workspaceId: SOURCE_ID,
          ownerUserId: OWNER,
          status: options.vmStatus ?? "running",
        }),
      }),
    ),
  );
}

function renderPanel(
  viewer: { id: string; owns: boolean },
  extra: Partial<React.ComponentProps<typeof ReviewsPanel>> = {},
) {
  return renderApp(
    <ReviewsPanel
      workspaceId={SOURCE_ID}
      workspaceName="kaihui-test3"
      viewerUserId={viewer.id}
      viewerOwnsWorkspace={viewer.owns}
      {...extra}
    />,
  );
}

describe("durable child review dispositions", () => {
  it("counts lineage by stable edit UUID rather than compound card or parent IDs", () => {
    const rounds = publications()
      .publications as unknown as ReviewPublication[];
    const old = (editId: string) =>
      suggestion({
        id: `old-parent:${editId}`,
        editId,
        parentSuggestionId: "old-parent",
        publicationId: "pub-0",
        status: "superseded",
      });
    const current = (editId: string) =>
      suggestion({
        id: `new-parent:${editId}`,
        editId,
        parentSuggestionId: "new-parent",
        publicationId: "pub-1",
        supersedes: "old-parent",
      });
    expect(
      roundDelta(rounds[0], rounds, [
        old("carried"),
        old("dropped"),
        current("carried"),
        current("new"),
      ]),
    ).toEqual({ previousRound: 1, added: 1, carried: 1, dropped: 1 });
  });

  it("keeps a publication's pending child current after its sibling checkpoint advances the source", async () => {
    const content = "Alpha unchanged middle clause Omega.\n";
    const parent = suggestion({
      id: "old-head-parent",
      beforeLines: content,
      afterLines: content.replace("Alpha", "Beta").replace("Omega", "Gamma"),
      edits: [
        reviewEditFixture("accepted-child", content, 0, 5, "Beta", {
          status: "accepted",
          statusAt: "2026-10-01T16:10:00Z",
        }),
        reviewEditFixture(
          "pending-child",
          content,
          content.indexOf("Omega"),
          content.indexOf("Omega") + 5,
          "Gamma",
        ),
      ],
    });
    const rounds = publications();
    rounds.publications[0].baseCommitVector = [
      { repoId: "repo-api", sourceHead: "old-head" },
    ];
    rounds.publications[0].openCount = 1;
    rounds.publications[0].acceptedCount = 1;
    seed({
      suggestions: [parent],
      publications: {
        ...rounds,
        sourceHeads: { "repo-api": "accepted-checkpoint" },
      },
    });
    renderPanel({ id: OWNER, owns: true });
    const pending = await screen.findByTestId(
      "review-suggestion-old-head-parent:pending-child",
    );
    expect(
      within(pending).getByRole("button", { name: "Accept" }),
    ).toBeEnabled();
    expect(
      screen.getByTestId("review-suggestion-old-head-parent:accepted-child"),
    ).toHaveTextContent("Accepted");
    expect(screen.getByTestId("reviews-panel-open-count")).toHaveTextContent(
      "1 open suggestion",
    );
    expect(
      screen.queryByTestId(
        "review-suggestion-outdated-old-head-parent:pending-child",
      ),
    ).toBeNull();
  });

  it("does not cluster disjoint child edits by their shared line, but does retain actual overlap", () => {
    const content = "Alpha unchanged middle clause Omega.\n";
    const alpha = suggestion({
      id: "alpha-parent",
      publicationId: "publication-alpha",
      beforeLines: content,
      afterLines: content.replace("Alpha", "Beta"),
      edits: [reviewEditFixture("alpha-child", content, 0, 5, "Beta")],
    });
    const omega = suggestion({
      id: "omega-parent",
      publicationId: "publication-omega",
      beforeLines: content,
      afterLines: content.replace("Omega", "Gamma"),
      edits: [
        reviewEditFixture(
          "omega-child",
          content,
          content.indexOf("Omega"),
          content.indexOf("Omega") + 5,
          "Gamma",
        ),
      ],
    });
    expect(
      competingClusters(expandReviewSuggestionEdits([alpha, omega])),
    ).toEqual([]);
    omega.edits = [
      reviewEditFixture("conflicting-alpha", content, 0, 5, "Delta"),
    ];
    const clusters = competingClusters(
      expandReviewSuggestionEdits([alpha, omega]),
    );
    expect(clusters).toHaveLength(1);
    expect(clusters[0].suggestions.map((row) => row.id)).toEqual([
      "alpha-parent:alpha-child",
      "omega-parent:conflicting-alpha",
    ]);
  });

  it("preserves decided children while retiring pending children with their parent", () => {
    const content = "Alpha Omega.\n";
    const parent = suggestion({
      status: "superseded",
      edits: [
        reviewEditFixture("accepted", content, 0, 5, "Beta", {
          status: "accepted",
        }),
        reviewEditFixture("old-open", content, 6, 11, "Gamma"),
      ],
    });
    expect(
      expandReviewSuggestionEdits([parent]).map((row) => row.status),
    ).toEqual(["accepted", "superseded"]);
  });

  it.each(["accepted", "rejected"])(
    "withdraws both remaining children in Reviews while retaining a %s child and its original and scoped discussions",
    async (settledStatus) => {
      const content = "Alpha middle Omega.\n";
      const parent = suggestion({
        id: "withdraw-parent",
        beforeLines: content,
        afterLines: "Beta changed Gamma.\n",
        edits: [
          reviewEditFixture("alpha", content, 0, 5, "Beta"),
          reviewEditFixture("settled", content, 6, 12, "changed", {
            status: settledStatus,
            statusAt: "2026-10-01T16:00:00Z",
            commentId: "settled-discussion",
          }),
          reviewEditFixture("omega", content, 13, 18, "Gamma"),
        ],
      });
      const writes: unknown[] = [];
      const discussions: Array<string | null> = [];
      seed({ suggestions: [parent] });
      server.use(
        http.post(
          `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/withdraw-parent/withdraw`,
          async ({ request }) => {
            writes.push(await request.json());
            return HttpResponse.json({
              data: {
                suggestion: {
                  ...parent,
                  status: "withdrawn",
                  statusAt: "2026-10-01T16:10:00Z",
                  edits: parent.edits!.map((edit) =>
                    edit.status === "open"
                      ? {
                          ...edit,
                          status: "withdrawn",
                          statusAt: "2026-10-01T16:10:00Z",
                        }
                      : edit,
                  ),
                },
                writtenHash: null,
                alreadyApplied: false,
              },
            });
          },
        ),
        http.get(
          `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/withdraw-parent/replies`,
          ({ request }) => {
            discussions.push(new URL(request.url).searchParams.get("editId"));
            return HttpResponse.json({ data: { replies: [] } });
          },
        ),
      );
      renderPanel({ id: REVIEWER, owns: false });
      const id = "withdraw-parent:alpha";
      const withdraw = await screen.findByTestId(
        `review-suggestion-withdraw-${id}`,
      );
      expect(withdraw).toHaveTextContent("Withdraw all open edits");
      await userEvent.click(withdraw);
      expect(
        screen.getByTestId(`review-suggestion-withdraw-confirm-${id}`),
      ).toHaveTextContent(
        "Withdraw all 2 remaining open edits in this change?",
      );
      expect(writes).toEqual([]);
      await userEvent.click(
        screen.getByTestId(`review-suggestion-withdraw-go-${id}`),
      );
      await waitFor(() => expect(writes).toEqual([{}]));
      for (const child of ["alpha", "omega"]) {
        await waitFor(() =>
          expect(
            screen.getByTestId(`review-suggestion-withdraw-parent:${child}`),
          ).toHaveTextContent("Withdrawn"),
        );
        expect(
          screen.queryByTestId(
            `review-suggestion-withdraw-withdraw-parent:${child}`,
          ),
        ).toBeNull();
      }
      const settled = screen.getByTestId(
        "review-suggestion-withdraw-parent:settled",
      );
      expect(settled).toHaveTextContent(
        settledStatus === "accepted" ? "Accepted" : "Rejected",
      );
      await userEvent.click(
        within(settled).getByRole("button", { name: "Discussion" }),
      );
      await waitFor(() => expect(discussions).toContain("settled"));
      await userEvent.click(
        within(settled).getByRole("button", {
          name: "Original review discussion",
        }),
      );
      await waitFor(() => expect(discussions).toContain(null));
    },
  );

  it.each(["aaaaaaaaaaaa", "a", "é🦀é🦀", "aaaa\nnew\nnew"])(
    "compares captured child coordinates after an accepted sibling becomes %j",
    (replacement) => {
      const content = "aaaa keep bb keep cc.\n";
      const current = replacement + content.slice(4);
      const bytes = (value: string) => new TextEncoder().encode(value).length;
      const bb = content.indexOf("bb");
      const parent = suggestion({
        id: "shifted-parent",
        publicationId: "shifted-publication",
        beforeLines: content,
        expectedBeforeLines: current,
        edits: [
          reviewEditFixture("accepted-prefix", content, 0, 4, replacement, {
            status: "accepted",
            acceptedAfterText: replacement,
          }),
          reviewEditFixture("pending-bb", content, bb, bb + 2, "BB", {
            currentStart: bytes(current.slice(0, current.indexOf("bb"))),
            currentEnd: bytes(current.slice(0, current.indexOf("bb") + 2)),
          }),
        ],
      });
      const competitor = (word: string) => {
        const at = content.indexOf(word);
        return suggestion({
          id: `other-${word}`,
          publicationId: `other-publication-${word}`,
          beforeLines: content,
          edits: [reviewEditFixture(`edit-${word}`, content, at, at + 2, "XX")],
        });
      };
      expect(
        competingClusters(
          expandReviewSuggestionEdits([parent, competitor("cc")]),
        ),
      ).toEqual([]);
      expect(
        competingClusters(
          expandReviewSuggestionEdits([parent, competitor("bb")]),
        ).map((cluster) => cluster.suggestions.map((row) => row.id)),
      ).toEqual([["shifted-parent:pending-bb", "other-bb:edit-bb"]]);
      expect(
        competingClusters(
          expandReviewSuggestionEdits([
            parent,
            { ...competitor("bb"), contentHash: "another-captured-file" },
          ]),
        ),
      ).toEqual([]);
    },
  );

  it("compares captured line coordinates across different parent anchors after a line shift", () => {
    const content = "aaaa\nbb keep cc.\nquiet\ndd\n";
    const replacement = "aaaa\nnew\nnew";
    const current = replacement + content.slice(4);
    const bytes = (value: string) => new TextEncoder().encode(value).length;
    const parent = suggestion({
      id: "line-parent",
      publicationId: "line-publication",
      anchorStart: 5,
      beforeLines: content,
      expectedBeforeLines: current,
      edits: [
        reviewEditFixture("settled", content, 0, 4, replacement, {
          status: "accepted",
        }),
        reviewEditFixture("pending", content, 5, 7, "BB", {
          currentStart: bytes(current.slice(0, current.indexOf("bb"))),
          currentEnd: bytes(current.slice(0, current.indexOf("bb") + 2)),
        }),
      ],
    });
    const competitor = (word: string, line: number) =>
      suggestion({
        id: `captured-${word}`,
        publicationId: `publication-${word}`,
        anchorStart: line,
        beforeLines: `${word}\n`,
        edits: [reviewEditFixture(word, `${word}\n`, 0, 2, "XX")],
      });
    expect(
      competingClusters(
        expandReviewSuggestionEdits([parent, competitor("dd", 8)]),
      ),
    ).toEqual([]);
    expect(
      competingClusters(
        expandReviewSuggestionEdits([parent, competitor("bb", 6)]),
      ).map((cluster) => cluster.suggestions.map((row) => row.id)),
    ).toEqual([["line-parent:pending", "captured-bb:bb"]]);
  });

  it("does not connect ordered same-parent insertions as competing proposals", () => {
    const content = "Intro.\n";
    const children = expandReviewSuggestionEdits([
      suggestion({
        id: "ordered-parent",
        beforeLines: content,
        afterLines: content + "First.\nSecond.\n",
        edits: [
          reviewEditFixture(
            "first",
            content,
            content.length,
            content.length,
            "First.\n",
          ),
          reviewEditFixture(
            "second",
            content,
            content.length,
            content.length,
            "Second.\n",
          ),
        ],
      }),
    ]);
    expect(competingClusters(children)).toEqual([]);
    // Even a separately presented instance cannot turn siblings into rivals.
    expect(
      competingClusters([
        children[0],
        { ...children[1], publicationId: "another-display-instance" },
      ]),
    ).toEqual([]);
  });

  it("does not infer captured coordinates from a current frame when the captured parent source is absent", () => {
    const insertion = reviewEditFixture("insert", "", 0, 0, "New.\n");
    const children = expandReviewSuggestionEdits([
      suggestion({
        id: "missing-capture",
        publicationId: "missing-publication",
        beforeLines: null,
        expectedBeforeLines: "Current.\n",
        edits: [insertion],
      }),
      suggestion({
        id: "known-capture",
        publicationId: "known-publication",
        beforeLines: "",
        edits: [{ ...insertion, id: "known-insert" }],
      }),
    ]);
    expect(competingClusters(children)).toEqual([]);
  });
});

describe("ReviewsPanel", () => {
  it("labels a reader's file suggestions, which are no review round, as Suggestions", async () => {
    const base = publications();
    seed({
      suggestions: [suggestion({ id: "sug-1", findingId: null })],
      publications: {
        ...base,
        publications: [
          // A reader's push from FILES: an annotation, no review workspace.
          {
            ...base.publications[0],
            reviewWorkspaceId: null,
            kind: "annotation",
            round: 3,
            summary: "",
            verdict: "comment_only",
          } as unknown as (typeof base.publications)[number],
        ],
        findings: [],
      },
    });
    renderPanel({ id: OWNER, owns: true });
    const card = await screen.findByTestId("review-publication-pub-1");
    expect(card).toHaveTextContent("Suggestions");
    expect(card).not.toHaveTextContent("Round 3");
  });

  it("lists the current round with its findings and hides earlier rounds until asked", async () => {
    seed({});
    renderPanel({ id: OWNER, owns: true });

    const panel = await screen.findByTestId("reviews-panel");
    expect(
      within(panel).getByTestId("reviews-panel-open-count"),
    ).toHaveTextContent("2 open suggestions · 1 review");
    const current = within(panel).getByTestId("review-publication-pub-1");
    expect(current).toHaveTextContent("Alex Chen");
    expect(current).toHaveTextContent("Changes requested");
    expect(current).toHaveTextContent("Round 2");
    expect(current).toHaveTextContent("Two fixes, one question.");
    const finding = within(current).getByTestId("review-finding-fnd-1");
    expect(finding).toHaveTextContent("Exit code is wrong");
    expect(finding).toHaveTextContent("−return 1;");
    expect(finding).toHaveTextContent("+return 0;");
    expect(
      screen.queryByTestId("review-publication-pub-0"),
    ).not.toBeInTheDocument();

    await userEvent.click(
      within(panel).getByTestId("reviews-panel-toggle-earlier"),
    );
    expect(
      await screen.findByTestId("review-publication-pub-0"),
    ).toHaveTextContent("Earlier round");
  });

  it("lets the owner accept a suggestion into the running workspace", async () => {
    const bodies: unknown[] = [];
    seed({});
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-1/accept`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            data: {
              suggestion: suggestion({
                id: "sug-1",
                findingId: "fnd-1",
                status: "accepted",
                statusAt: "2026-09-23T15:00:00.000Z",
              }),
              writtenHash: "def",
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    renderPanel({ id: OWNER, owns: true });

    await userEvent.click(
      await screen.findByTestId("review-suggestion-accept-sug-1"),
    );
    await waitFor(() => expect(bodies).toEqual([{}]));
    // The comment suggestion has Resolve, not Accept; the reviewer-only
    // Withdraw is absent for the owner.
    expect(
      screen.getByTestId("review-suggestion-resolve-sug-2"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("review-suggestion-accept-sug-2"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("review-suggestion-withdraw-sug-1"),
    ).not.toBeInTheDocument();
  });

  it("shows pending publication when an applied acceptance could not push", async () => {
    seed({});
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-1/accept`,
        () =>
          HttpResponse.json({
            data: {
              suggestion: suggestion({ id: "sug-1", status: "accepted" }),
              writtenHash: "def",
              alreadyApplied: false,
              checkpointPending: true,
            },
          }),
      ),
    );
    renderPanel({ id: OWNER, owns: true });
    await userEvent.click(
      await screen.findByTestId("review-suggestion-accept-sug-1"),
    );
    expect(
      await screen.findByText(/commit\/push did not finish/i),
    ).toBeVisible();
  });

  it("accepts an edited replacement and rejects with a reason", async () => {
    const accepts: unknown[] = [];
    const rejects: unknown[] = [];
    seed({});
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-1/accept`,
        async ({ request }) => {
          accepts.push(await request.json());
          return HttpResponse.json({
            data: {
              suggestion: suggestion({ id: "sug-1", status: "accepted" }),
              writtenHash: "def",
              alreadyApplied: false,
            },
          });
        },
      ),
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/reject`,
        async ({ request }) => {
          rejects.push(await request.json());
          return HttpResponse.json({
            data: {
              suggestion: suggestion({ id: "sug-2", status: "rejected" }),
              writtenHash: null,
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    renderPanel({ id: OWNER, owns: true });

    await userEvent.click(
      await screen.findByTestId("review-suggestion-edit-toggle-sug-1"),
    );
    const editor = screen.getByTestId("review-suggestion-edit-sug-1");
    await userEvent.clear(editor);
    await userEvent.type(editor, "return 2;");
    await userEvent.click(screen.getByTestId("review-suggestion-accept-sug-1"));
    await waitFor(() => expect(accepts).toEqual([{ afterLines: "return 2;" }]));

    await userEvent.click(screen.getByTestId("review-suggestion-reject-sug-2"));
    // Enter in the reason box submits, like the Reject button.
    await userEvent.type(
      screen.getByTestId("review-suggestion-reject-reason-sug-2"),
      "Still accurate.{Enter}",
    );
    await waitFor(() =>
      expect(rejects).toEqual([{ reason: "Still accurate." }]),
    );
  });

  it.each([
    { who: "another person's token arrives", another: true },
    { who: "the same person's token is renewed", another: false },
  ])(
    "accepts a finding only for the principal the panel was shown to when $who before it catches up",
    async ({ another }) => {
      const accepted: string[] = [];
      seed({});
      server.use(
        http.post(
          `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/:id/accept`,
          ({ params }) => {
            accepted.push(String(params.id));
            return HttpResponse.json({
              data: {
                suggestion: suggestion({ id: "sug-1", status: "accepted" }),
                writtenHash: "def",
                alreadyApplied: false,
              },
            });
          },
        ),
      );
      renderPanel({ id: OWNER, owns: true });
      const acceptAll = await screen.findByTestId(
        "review-finding-accept-fnd-1",
      );
      await waitFor(() => expect(acceptAll).toBeEnabled());
      const shownTo = tokenPrincipal(getApiToken());
      expect(shownTo).not.toBeNull();
      const [sub, org] = (shownTo as string).split(":");

      // The token arrives from outside React, as a refresh's does, and the
      // finding is accepted before the panel has re-rendered for it.
      const encode = (value: object) =>
        btoa(JSON.stringify(value))
          .replace(/\+/g, "-")
          .replace(/\//g, "_")
          .replace(/=+$/, "");
      const env = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
      env.IS_REACT_ACT_ENVIRONMENT = false;
      try {
        setApiToken(
          `${encode({ alg: "HS256" })}.${encode({ sub: another ? "user-bob" : sub, org, scopes: [] })}.renewed`,
        );
      } finally {
        env.IS_REACT_ACT_ENVIRONMENT = true;
      }
      act(() => {
        fireEvent.click(acceptAll);
      });

      if (another) {
        expect(
          await screen.findByText(/signed in as someone else/),
        ).toBeInTheDocument();
        expect(accepted).toHaveLength(0);
      } else {
        await waitFor(() => expect(accepted).toEqual(["sug-1"]));
      }
    },
  );

  it("locks an edited replacement while it is accepted, so later typing cannot be lost", async () => {
    const bodies: unknown[] = [];
    let release!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    seed({});
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-1/accept`,
        async ({ request }) => {
          bodies.push(await request.json());
          await held;
          return HttpResponse.json({
            data: {
              suggestion: suggestion({ id: "sug-1", status: "accepted" }),
              writtenHash: "def",
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    renderPanel({ id: OWNER, owns: true });
    await userEvent.click(
      await screen.findByTestId("review-suggestion-edit-toggle-sug-1"),
    );
    const edit = screen.getByTestId("review-suggestion-edit-sug-1");
    await userEvent.clear(edit);
    await userEvent.type(edit, "return 2;");
    await userEvent.click(screen.getByTestId("review-suggestion-accept-sug-1"));
    await waitFor(() => expect(bodies).toEqual([{ afterLines: "return 2;" }]));
    expect(edit).toHaveAttribute("readonly");
    await userEvent.type(edit, " // more");
    expect(edit).toHaveValue("return 2;");
    release();
    await waitFor(() =>
      expect(screen.queryByTestId("review-suggestion-edit-sug-1")).toBeNull(),
    );
    expect(bodies).toHaveLength(1);
  });

  it("keeps a typed reason and an edited replacement readable, read-only, after the ownership moves", async () => {
    const decisions: unknown[] = [];
    seed({});
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/:id/:action`,
        async ({ request }) => {
          decisions.push(await request.json());
          return HttpResponse.json({ data: {} });
        },
      ),
    );
    const Harness: React.FC = () => {
      const [owns, setOwns] = React.useState(true);
      return (
        <>
          <button type="button" onClick={() => setOwns(false)}>
            Ownership moves
          </button>
          <ReviewsPanel
            workspaceId={SOURCE_ID}
            workspaceName="kaihui-test3"
            viewerUserId={OWNER}
            viewerOwnsWorkspace={owns}
          />
        </>
      );
    };
    renderApp(<Harness />);
    await userEvent.click(
      await screen.findByTestId("review-suggestion-edit-toggle-sug-1"),
    );
    const edit = screen.getByTestId("review-suggestion-edit-sug-1");
    await userEvent.type(edit, " // tidier");
    await userEvent.click(screen.getByTestId("review-suggestion-reject-sug-2"));
    await userEvent.type(
      screen.getByTestId("review-suggestion-reject-reason-sug-2"),
      "Not now",
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Ownership moves" }),
    );
    const reason = screen.getByTestId("review-suggestion-reject-reason-sug-2");
    expect(reason).toHaveValue("Not now");
    expect(reason).toHaveAttribute("readonly");
    expect(
      screen.getByTestId("review-suggestion-reject-confirm-sug-2"),
    ).toBeDisabled();
    expect(edit).toHaveAttribute("readonly");
    expect(
      screen.getByTestId("review-suggestion-edit-closed-sug-1"),
    ).toHaveTextContent("can no longer be accepted here");

    // Each can be let go; nothing was sent.
    await userEvent.click(
      screen.getByTestId("review-suggestion-reject-cancel-sug-2"),
    );
    expect(
      screen.queryByTestId("review-suggestion-reject-reason-sug-2"),
    ).toBeNull();
    await userEvent.click(
      screen.getByTestId("review-suggestion-edit-dismiss-sug-1"),
    );
    expect(screen.queryByTestId("review-suggestion-edit-sug-1")).toBeNull();
    expect(decisions).toEqual([]);
  });

  it("does not offer Accept while the owner's VM is stopped", async () => {
    seed({ vmStatus: "stopped" });
    renderPanel({ id: OWNER, owns: true });

    const accept = await screen.findByTestId("review-suggestion-accept-sug-1");
    expect(accept).toBeDisabled();
    expect(screen.getByTestId("reviews-panel-vm-stopped")).toHaveTextContent(
      "Start the workspace to accept suggestions",
    );
    // The chip offers the header's Start right there.
    const started = vi.fn();
    window.addEventListener("g5:workspace-start", started);
    await userEvent.click(screen.getByTestId("reviews-panel-start-workspace"));
    expect(started).toHaveBeenCalledOnce();
    window.removeEventListener("g5:workspace-start", started);
    expect(screen.getByTestId("review-finding-accept-fnd-1")).toBeDisabled();
  });

  it("names a starting VM and offers no Start while it starts (g5.workspace.ui.vm-state)", async () => {
    seed({ vmStatus: "starting" });
    renderPanel({ id: OWNER, owns: true });

    const accept = await screen.findByTestId("review-suggestion-accept-sug-1");
    expect(accept).toBeDisabled();
    // The panel's chip names the state from the VM read; the cards' titles
    // take the page's gate, which this panel-only render has no provider for.
    expect(screen.getByTestId("reviews-panel-vm-stopped")).toHaveTextContent(
      "The workspace is starting; you can accept suggestions once it is running.",
    );
    expect(screen.queryByTestId("reviews-panel-start-workspace")).toBeNull();
    expect(screen.queryByTestId("workspace-commit")).not.toBeInTheDocument();
  });

  it("groups overlapping proposals from different reviewers as competing", async () => {
    seed({
      suggestions: [
        suggestion({ id: "sug-1", findingId: "fnd-1", anchorStart: 3 }),
        suggestion({
          id: "sug-7",
          publicationId: "pub-0",
          anchorStart: 3,
          afterLines: "return 2;\n",
          authorLabel: "bo",
        }),
        suggestion({
          id: "sug-8",
          publicationId: "pub-0",
          anchorStart: 40,
          afterLines: "x\n",
        }),
      ],
    });
    renderPanel({ id: OWNER, owns: true });

    const competing = await screen.findByTestId("reviews-panel-competing");
    expect(competing).toHaveTextContent("Competing proposals");
    expect(competing).toHaveTextContent("src/main.rs:3");
    expect(
      within(competing).getByTestId("review-suggestion-sug-1"),
    ).toBeInTheDocument();
    expect(
      within(competing).getByTestId("review-suggestion-sug-7"),
    ).toHaveTextContent("+return 2;");
    // A lone hunk on other lines is not competing.
    expect(
      within(competing).queryByTestId("review-suggestion-sug-8"),
    ).not.toBeInTheDocument();
  });

  it("lists artifacts with open and download links", async () => {
    seed({
      suggestions: [
        suggestion({
          id: "art-1",
          kind: "artifact",
          path: "mock.html",
          beforeLines: null,
          afterLines: null,
          anchorStart: null,
          note: "Clickable mockup of the new flow.",
          blobOid: "a".repeat(40),
          blobSha256: "b".repeat(64),
          bytes: 2048,
          contentType: "text/html; charset=utf-8",
        }),
      ],
    });
    renderPanel({ id: OWNER, owns: true });

    // An artifact lives in its round's card, not in a list of its own.
    const artifacts = await screen.findByTestId("review-suggestion-art-1");
    expect(artifacts).toHaveTextContent("mock.html");
    expect(screen.queryByTestId("reviews-panel-artifacts")).toBeNull();
    // Open and Download fetch through the authenticated client (an anchor
    // could not carry the bearer token) and hand the bytes to the browser.
    const fetched: string[] = [];
    server.use(
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/art-1/artifact`,
        ({ request }) => {
          fetched.push(request.headers.get("authorization") ?? "");
          return new HttpResponse("<h1>Mock</h1>", {
            headers: { "content-type": "text/html; charset=utf-8" },
          });
        },
      ),
    );
    // The test setup stubs URL.createObjectURL; the spy hands out distinct
    // URLs so the frame's src is checked against the one the panel got.
    const objectUrls: string[] = [];
    const createObjectURL = vi
      .spyOn(URL, "createObjectURL")
      .mockImplementation(() => {
        const url = `blob:mock-${objectUrls.length}`;
        objectUrls.push(url);
        return url;
      });
    await userEvent.click(
      within(artifacts).getByTestId("review-artifact-open-art-1"),
    );
    const preview = await screen.findByTestId("review-artifact-preview-art-1");
    expect(fetched[0]).toMatch(/^Bearer /);
    const frame = preview.querySelector("iframe");
    expect(frame).toHaveAttribute("sandbox", "");
    expect(frame).toHaveAttribute("src", objectUrls[0]);
    // The previewed document carries the restrictive CSP itself: a blob
    // URL has no response headers to rely on.
    const previewed = createObjectURL.mock.calls[0][0] as Blob;
    expect(previewed.type).toBe("text/html");
    const previewedText = await readBlobText(previewed);
    expect(previewedText).toContain('http-equiv="Content-Security-Policy"');
    // Scripts are forbidden by the document's own policy (a blob URL can be
    // opened outside the sandboxed frame), not merely by the frame.
    expect(previewedText).toContain("default-src 'none'");
    expect(previewedText).not.toContain("script-src");
    expect(previewedText).not.toContain("<script");
    expect(previewedText).toContain("<h1>Mock</h1>");
    // A download hands over the uploaded bytes untouched.
    await userEvent.click(
      within(artifacts).getByTestId("review-artifact-download-art-1"),
    );
    await waitFor(() => expect(createObjectURL).toHaveBeenCalledTimes(2));
    const downloaded = createObjectURL.mock.calls[1][0] as Blob;
    const downloadedText = await readBlobText(downloaded);
    expect(downloadedText).toBe("<h1>Mock</h1>");
    // The artifact card in its publication: no Accept, only the links.
    const card = screen.getByTestId("review-suggestion-art-1");
    expect(card).toHaveTextContent("2.0 KB");
    expect(
      within(card).queryByTestId("review-suggestion-accept-art-1"),
    ).not.toBeInTheDocument();
  });

  it("links every change to its file and shows what a round changed since the previous one", async () => {
    // Round 2 carries the hunk from round 1 (a thread continues), adds a
    // new file and drops a hunk round 1 had.
    seed({
      suggestions: [
        suggestion({ id: "sug-1", findingId: "fnd-1", supersedes: "old-1" }),
        suggestion({
          id: "sug-new",
          kind: "new_file",
          path: "NOTES.md",
          beforeLines: null,
          afterLines: "# Notes\n",
          anchorStart: null,
        }),
        suggestion({
          id: "old-1",
          publicationId: "pub-0",
          status: "superseded",
        }),
        suggestion({
          id: "old-2",
          publicationId: "pub-0",
          path: "src/lib.rs",
          status: "superseded",
        }),
      ],
    });
    const opened: unknown[][] = [];
    renderPanel(
      { id: OWNER, owns: true },
      { onOpenFile: (...args: unknown[]) => opened.push(args) },
    );
    const card = await screen.findByTestId("review-suggestion-sug-1");
    await userEvent.click(
      within(card).getByTestId("review-suggestion-open-file-sug-1"),
    );
    expect(opened).toEqual([["src/main.rs", "repo-api", 3]]);
    // The proposed file is not in the workspace yet: nothing to open.
    expect(
      within(screen.getByTestId("review-suggestion-sug-new")).queryByTestId(
        "review-suggestion-open-file-sug-new",
      ),
    ).toBeNull();
    expect(screen.getByTestId("review-round-delta-pub-1")).toHaveTextContent(
      "since round 1: 1 new · 1 carried over · 1 dropped",
    );
  });

  it("offers SVG and PDF artifacts for download only, raster images inline", async () => {
    seed({
      suggestions: [
        suggestion({
          id: "art-svg",
          kind: "artifact",
          path: "diagram.svg",
          beforeLines: null,
          afterLines: null,
          anchorStart: null,
          blobOid: "c".repeat(40),
          blobSha256: "d".repeat(64),
          bytes: 512,
          contentType: "image/svg+xml",
        }),
        suggestion({
          id: "art-pdf",
          kind: "artifact",
          path: "report.pdf",
          beforeLines: null,
          afterLines: null,
          anchorStart: null,
          blobOid: "e".repeat(40),
          blobSha256: "f".repeat(64),
          bytes: 4096,
          contentType: "application/pdf",
        }),
        suggestion({
          id: "art-png",
          kind: "artifact",
          path: "shot.png",
          beforeLines: null,
          afterLines: null,
          anchorStart: null,
          blobOid: "1".repeat(40),
          blobSha256: "2".repeat(64),
          bytes: 8192,
          contentType: "image/png",
        }),
      ],
    });
    renderPanel({ id: OWNER, owns: true });
    await screen.findByTestId("review-suggestion-art-png");
    const artifacts = screen.getByTestId("review-publication-pub-1");
    // An SVG is a document that may carry scripts: no inline preview whose
    // blob URL could be opened on this origin. A PDF has no sandboxed
    // viewer. Both download only; the PNG opens inline.
    for (const id of ["art-svg", "art-pdf"]) {
      expect(
        within(artifacts).queryByTestId(`review-artifact-open-${id}`),
      ).not.toBeInTheDocument();
      expect(
        within(artifacts).getByTestId(`review-artifact-download-${id}`),
      ).toBeInTheDocument();
    }
    expect(
      within(artifacts).getByTestId("review-artifact-open-art-png"),
    ).toBeInTheDocument();
  });

  it("keeps an artifact in its round: Keep needs no VM and marks it Kept", async () => {
    const rows: ReviewSuggestion[] = [
      suggestion({
        id: "art-1",
        kind: "artifact",
        path: "mock.png",
        beforeLines: null,
        afterLines: null,
        anchorStart: null,
        blobOid: "a".repeat(40),
        blobSha256: "b".repeat(64),
        bytes: 70,
        contentType: "image/png",
      }),
    ];
    const decided: string[] = [];
    // The owner's VM is stopped: keeping an artifact needs no VM.
    seed({ suggestions: rows, vmStatus: "stopped" });
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/art-1/accept`,
        () => {
          decided.push("accept");
          rows[0] = {
            ...rows[0],
            status: "accepted",
            statusAt: "2026-09-25T12:00:00.000Z",
          };
          return HttpResponse.json({
            data: {
              suggestion: rows[0],
              writtenHash: null,
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    renderPanel({ id: OWNER, owns: true });
    const card = await screen.findByTestId("review-suggestion-art-1");
    expect(screen.queryByTestId("reviews-panel-kept-artifacts")).toBeNull();
    expect(
      within(card).queryByTestId("review-suggestion-accept-art-1"),
    ).toBeNull();
    const keep = within(card).getByTestId("review-artifact-keep-art-1");
    expect(keep).toBeEnabled();
    await userEvent.click(keep);
    await waitFor(() => expect(decided).toEqual(["accept"]));
    await waitFor(() =>
      expect(
        screen.getByTestId("review-suggestion-status-art-1"),
      ).toHaveTextContent("Kept"),
    );
    expect(screen.queryByTestId("review-artifact-keep-art-1")).toBeNull();
    expect(screen.queryByTestId("review-suggestion-reject-art-1")).toBeNull();
  });

  it("groups rounds by the commit they reviewed: the current commit first, earlier ones collapsed", async () => {
    const OLD = "a".repeat(40);
    const NEW = "b".repeat(40);
    const base = publications();
    const outdated = {
      ...base.publications[0],
      baseCommitVector: [{ repoId: "repo-api", sourceHead: OLD }],
    };
    const sam = {
      ...base.publications[0],
      id: "pub-sam",
      reviewerUserId: "user-sam",
      authorLabel: "sam",
      round: 1,
      baseCommitVector: [{ repoId: "repo-api", sourceHead: NEW }],
      openCount: 1,
      createdAt: "2026-09-24T09:00:00.000Z",
    };
    seed({
      suggestions: [
        suggestion({ id: "sug-1", findingId: "fnd-1" }),
        suggestion({ id: "sug-2", path: "src/lib.rs", anchorStart: 8 }),
        suggestion({
          id: "sug-sam",
          publicationId: "pub-sam",
          authorLabel: "sam",
          path: "src/other.rs",
        }),
        // Accepted into a file the live report lists as dirty.
        suggestion({
          id: "sug-sam-acc",
          publicationId: "pub-sam",
          authorLabel: "sam",
          path: "src/done.rs",
          status: "accepted",
        }),
        // Accepted in the outdated round, into a clean file.
        suggestion({
          id: "sug-old-acc",
          path: "src/old.rs",
          status: "accepted",
          statusAt: "2026-09-24T09:00:00.000Z",
        }),
        // Accepted in the outdated round, into the dirty file: flagged too,
        // whatever its round or when it was decided.
        suggestion({
          id: "sug-old-late",
          path: "src/done.rs",
          anchorStart: 40,
          status: "accepted",
          statusAt: "2026-09-26T09:00:00.000Z",
        }),
      ],
      publications: {
        ...base,
        publications: [outdated, base.publications[1], sam],
        sourceHeads: { "repo-api": NEW },
      },
    });
    // The workspace's repo list names each head's repository; the live
    // changes report says which files still hold uncommitted edits.
    server.use(
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/changes`, () =>
        HttpResponse.json({
          data: {
            workspace_id: SOURCE_ID,
            source: "live",
            attached: true,
            members: [
              {
                repo_id: "repo-api",
                root_path: "api",
                errors: [],
                dirty: [{ status: "M", path: "src/done.rs" }],
              },
            ],
          },
        }),
      ),
      http.get(`*/api/v1/workspaces/${SOURCE_ID}`, () =>
        HttpResponse.json({
          data: {
            id: SOURCE_ID,
            orgId: "org-g5labs",
            name: "kaihui-test3",
            isBase: false,
            description: null,
            clonedFromId: null,
            docsRepoId: null,
            docsBranchName: null,
            docsBranchId: null,
            createdBy: OWNER,
            ownerUserId: OWNER,
            createdAt: "2026-09-20T00:00:00.000Z",
            updatedAt: "2026-09-20T00:00:00.000Z",
            pushMode: "manual",
            branches: [
              { id: "br-1", repoId: "repo-api", repoName: "api", name: "w" },
            ],
            docsFolderPath: `docs/${SOURCE_ID}`,
          },
        }),
      ),
    );
    renderPanel({ id: OWNER, owns: true });

    const panel = await screen.findByTestId("reviews-panel");
    const current = within(panel).getByTestId("reviews-commit-group-current");
    await waitFor(() =>
      expect(current).toHaveTextContent(
        "Reviewed at api bbbbbbb · current commit",
      ),
    );
    expect(
      within(current).getByTestId("review-publication-pub-sam"),
    ).toBeInTheDocument();
    // The header tells current from outdated, and what awaits a commit
    // (a live region, so the count is announced when it changes).
    expect(
      within(panel).getByTestId("reviews-panel-open-count"),
    ).toHaveTextContent("1 open · 2 outdated · 2 reviews");
    const uncommitted = within(panel).getByTestId(
      "reviews-panel-accepted-uncommitted",
    );
    await waitFor(() =>
      expect(uncommitted).toHaveTextContent(
        "2 accepted changes in files with uncommitted edits",
      ),
    );
    expect(uncommitted).toHaveAttribute("role", "status");
    expect(
      within(current).getByTestId("review-suggestion-status-sug-sam-acc"),
    ).toHaveTextContent("Accepted · file has uncommitted changes");
    // The earlier commit's round is collapsed under its commit, with what it
    // still holds; opening it shows the round marked outdated, its cards
    // saying so, Accept still offered for one that still fits.
    const old = within(panel).getByTestId("reviews-commit-group-aaaaaaa");
    expect(old).toHaveTextContent(
      "Reviewed at api aaaaaaa · outdated: 2 open, 0 accepted",
    );
    expect(
      within(old).queryByTestId("review-publication-pub-1"),
    ).not.toBeInTheDocument();
    await userEvent.click(
      within(old).getByTestId("reviews-commit-toggle-aaaaaaa"),
    );
    const card = within(old).getByTestId("review-publication-pub-1");
    expect(
      within(card).getByTestId("review-round-outdated-pub-1"),
    ).toBeInTheDocument();
    expect(
      within(card).getByTestId("review-suggestion-outdated-sug-1"),
    ).toHaveTextContent(
      "From an earlier commit of kaihui-test3; it may no longer apply.",
    );
    expect(
      within(card).getByTestId("review-suggestion-accept-sug-1"),
    ).toBeInTheDocument();
    // Of the outdated round's accepted rows, the one in a clean file is
    // plainly accepted; the one in the dirty file is flagged and counted,
    // whatever its round or the time it was decided.
    expect(
      within(card).getByTestId("review-suggestion-status-sug-old-acc"),
    ).toHaveTextContent(/^Accepted$/);
    expect(
      within(card).getByTestId("review-suggestion-status-sug-old-late"),
    ).toHaveTextContent("Accepted · file has uncommitted changes");
    expect(uncommitted).toHaveTextContent(
      "2 accepted changes in files with uncommitted edits",
    );
    // The current round's cards carry no such line.
    expect(
      within(current).queryByTestId("review-suggestion-outdated-sug-sam"),
    ).not.toBeInTheDocument();
  });

  it.each([
    { id: OWNER, owns: true },
    { id: REVIEWER, owns: false },
  ])("keeps commit controls out of Reviews for $id", async (viewer) => {
    seed({});
    renderPanel(viewer);
    await screen.findByTestId("review-publication-pub-1");
    expect(screen.queryByTestId("workspace-commit")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-commit-dialog"),
    ).not.toBeInTheDocument();
  });

  it("keeps human review controls without agent delegation", async () => {
    seed({});
    renderPanel({ id: OWNER, owns: true });
    const card = await screen.findByTestId("review-publication-pub-1");
    await within(card).findByTestId("review-suggestion-sug-1");
    expect(screen.queryByText("Review with my agent")).not.toBeInTheDocument();
    expect(
      within(card).getByTestId("review-suggestion-accept-sug-1"),
    ).toBeInTheDocument();
  });

  it("lets the reviewer withdraw and any reader reply", async () => {
    const withdrawn: string[] = [];
    const replies: unknown[] = [];
    seed({});
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-1/withdraw`,
        () => {
          withdrawn.push("sug-1");
          return HttpResponse.json({
            data: {
              suggestion: suggestion({ id: "sug-1", status: "withdrawn" }),
              writtenHash: null,
              alreadyApplied: false,
            },
          });
        },
      ),
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
        () => HttpResponse.json({ data: { replies: [] } }),
      ),
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
        async ({ request }) => {
          replies.push(await request.json());
          return HttpResponse.json({ data: { replies: [] } }, { status: 201 });
        },
      ),
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/changes`, () =>
        HttpResponse.json({
          data: {
            workspace_id: SOURCE_ID,
            source: "durable",
            attached: false,
            members: [
              {
                repo_id: "repo-api",
                root_path: "api",
                dirty: [],
                errors: [],
              },
            ],
          },
        }),
      ),
    );
    renderPanel({ id: REVIEWER, owns: false });

    // No owner controls for the reviewer.
    expect(
      await screen.findByTestId("review-suggestion-withdraw-sug-1"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("review-suggestion-accept-sug-1"),
    ).not.toBeInTheDocument();
    await userEvent.click(
      screen.getByTestId("review-suggestion-withdraw-sug-1"),
    );
    // Withdraw asks first; nothing goes out until confirmed, the confirm
    // takes focus, and Keep hands it back to Withdraw.
    expect(withdrawn).toEqual([]);
    const go = screen.getByTestId("review-suggestion-withdraw-go-sug-1");
    await waitFor(() => expect(go).toHaveFocus());
    await userEvent.click(
      screen.getByTestId("review-suggestion-withdraw-keep-sug-1"),
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("review-suggestion-withdraw-sug-1"),
      ).toHaveFocus(),
    );
    expect(withdrawn).toEqual([]);
    await userEvent.click(
      screen.getByTestId("review-suggestion-withdraw-sug-1"),
    );
    await userEvent.click(
      screen.getByTestId("review-suggestion-withdraw-go-sug-1"),
    );
    await waitFor(() => expect(withdrawn).toEqual(["sug-1"]));

    await userEvent.click(
      screen.getByTestId("review-suggestion-replies-toggle-sug-2"),
    );
    await userEvent.type(
      await screen.findByTestId("review-suggestion-reply-input-sug-2"),
      "Yes, checked today.{Enter}",
    );
    await waitFor(() =>
      expect(replies).toEqual([{ content: "Yes, checked today." }]),
    );
  });

  it("sends one reply per send and shows the person's name", async () => {
    const replies: unknown[] = [];
    let release!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    seed({});
    server.use(
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
        () =>
          HttpResponse.json({
            data: {
              replies: [
                {
                  id: "reply-person",
                  commentId: "c-2",
                  authorId: OWNER,
                  authorName: "Olive",
                  authorAvatar: null,
                  content: "Kept: the retry needs it.",
                  createdAt: "2026-09-23T15:00:00.000Z",
                },
              ],
            },
          }),
      ),
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
        async ({ request }) => {
          replies.push(await request.json());
          await held;
          return HttpResponse.json({ data: { replies: [] } }, { status: 201 });
        },
      ),
    );
    renderPanel({ id: REVIEWER, owns: false });

    await userEvent.click(
      await screen.findByTestId("review-suggestion-replies-toggle-sug-2"),
    );
    const thread = await screen.findByTestId("review-suggestion-replies-sug-2");
    expect(await within(thread).findByText("Olive")).toBeInTheDocument();
    const input = within(thread).getByTestId(
      "review-suggestion-reply-input-sug-2",
    );
    await userEvent.type(input, "Thanks{Enter}{Enter}");
    await waitFor(() => expect(replies).toEqual([{ content: "Thanks" }]));
    release();
    await waitFor(() => expect(input).toHaveValue(""));
    expect(replies).toHaveLength(1);
  });

  it("keeps a typed reply, and why its send failed, while the discussion is hidden", async () => {
    const replies: unknown[] = [];
    let refuse!: () => void;
    const held = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    seed({});
    server.use(
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
        () => HttpResponse.json({ data: { replies: [] } }),
      ),
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
        async ({ request }) => {
          replies.push(await request.json());
          await held;
          return HttpResponse.json(
            {
              error: {
                code: "WORKSPACE_READ_ONLY",
                message: "The workspace became read-only",
                requestId: "request-refused",
              },
            },
            { status: 409 },
          );
        },
      ),
    );
    renderPanel({ id: REVIEWER, owns: false });
    const toggle = await screen.findByTestId(
      "review-suggestion-replies-toggle-sug-2",
    );
    await userEvent.click(toggle);
    const input = await screen.findByTestId(
      "review-suggestion-reply-input-sug-2",
    );
    await userEvent.type(input, "Is it?{Enter}");
    await waitFor(() => expect(replies).toHaveLength(1));

    // Hidden while it is being sent, refused meanwhile, shown again: the
    // words and the reason are there, and nothing was sent twice.
    await userEvent.click(toggle);
    expect(
      screen.getByTestId("review-suggestion-replies-sug-2"),
    ).not.toBeVisible();
    refuse();
    await userEvent.click(toggle);
    const shown = screen.getByTestId("review-suggestion-replies-sug-2");
    expect(shown).toBeVisible();
    expect(
      await within(shown).findByText("The workspace became read-only"),
    ).toBeInTheDocument();
    expect(
      within(shown).getByTestId("review-suggestion-reply-input-sug-2"),
    ).toHaveValue("Is it?");
    expect(replies).toHaveLength(1);
  });

  it("keeps discussions readable but offers no reply or decision on a read-only source", async () => {
    // The owner's VM is running: only the source's writability hides them.
    seed({ sourceReadOnly: true });
    server.use(
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
        () => HttpResponse.json({ data: { replies: [] } }),
      ),
    );
    renderPanel({ id: OWNER, owns: true });
    await screen.findByTestId("review-suggestion-sug-1");
    expect(
      screen.queryByTestId("review-finding-accept-fnd-1"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("review-round-agent-pub-1"),
    ).not.toBeInTheDocument();
    await userEvent.click(
      await screen.findByTestId("review-suggestion-replies-toggle-sug-2"),
    );
    expect(
      await screen.findByTestId("review-suggestion-replies-closed-sug-2"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("review-suggestion-reply-input-sug-2"),
    ).not.toBeInTheDocument();
    for (const control of ["accept", "reject", "withdraw"]) {
      expect(
        screen.queryByTestId(`review-suggestion-${control}-sug-1`),
      ).not.toBeInTheDocument();
    }
  });

  it("keeps the cards and a typed reply when a refresh fails, and says so", async () => {
    seed({});
    server.use(
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
        () => HttpResponse.json({ data: { replies: [] } }),
      ),
    );
    const { queryClient } = renderPanel({ id: REVIEWER, owns: false });
    await userEvent.click(
      await screen.findByTestId("review-suggestion-replies-toggle-sug-2"),
    );
    const input = await screen.findByTestId(
      "review-suggestion-reply-input-sug-2",
    );
    await userEvent.type(input, "Half an answer");
    const card = screen.getByTestId("review-suggestion-sug-2");

    // The service goes away and every list fails to refresh.
    const unavailable = () =>
      HttpResponse.json(
        {
          error: {
            code: "SERVICE_UNAVAILABLE",
            message: "The service is restarting",
            requestId: "request-down",
          },
        },
        { status: 503 },
      );
    server.use(
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/publications`,
        unavailable,
      ),
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions`,
        unavailable,
      ),
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
        unavailable,
      ),
    );
    await queryClient.invalidateQueries();
    expect(
      await screen.findByTestId("reviews-panel-refresh-error"),
    ).toHaveTextContent("The service is restarting");
    expect(
      await screen.findByTestId(
        "review-suggestion-replies-refresh-error-sug-2",
      ),
    ).toBeInTheDocument();
    expect(screen.getByTestId("review-suggestion-sug-2")).toBe(card);
    expect(input).toHaveValue("Half an answer");

    // Back: the next refresh clears the note; the text is still there.
    seed({});
    server.use(
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
        () => HttpResponse.json({ data: { replies: [] } }),
      ),
    );
    await queryClient.invalidateQueries();
    await waitFor(() =>
      expect(
        screen.queryByTestId("reviews-panel-refresh-error"),
      ).not.toBeInTheDocument(),
    );
    expect(input).toHaveValue("Half an answer");
  });

  it("offers no reply or decision while a closed source's state cannot be refreshed, until it is writable", async () => {
    const closedStates: Array<Parameters<typeof seed>[0]> = [
      { sourceReadOnly: true },
      { sourceDetail: { isBase: true } },
      { sourceDetail: { mutationPolicyTransition: "freezing" } },
    ];
    for (const closed of closedStates) {
      cleanup();
      seed(closed);
      server.use(
        http.get(
          `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/replies`,
          () => HttpResponse.json({ data: { replies: [] } }),
        ),
      );
      const { queryClient } = renderPanel({ id: OWNER, owns: true });
      await screen.findByTestId("review-suggestion-sug-1");
      await userEvent.click(
        screen.getByTestId("review-suggestion-replies-toggle-sug-2"),
      );
      const closedControls = () => {
        for (const control of ["accept", "reject", "withdraw"]) {
          expect(
            screen.queryByTestId(`review-suggestion-${control}-sug-1`),
          ).not.toBeInTheDocument();
        }
        expect(
          screen.queryByTestId("review-suggestion-reply-input-sug-2"),
        ).not.toBeInTheDocument();
      };
      closedControls();

      // Only the source's detail fails to refresh: nothing reopens.
      server.use(
        http.get(`*/api/v1/workspaces/${SOURCE_ID}`, () =>
          HttpResponse.json(
            {
              error: {
                code: "SERVICE_UNAVAILABLE",
                message: "The service is restarting",
                requestId: "request-down",
              },
            },
            { status: 503 },
          ),
        ),
      );
      await queryClient.invalidateQueries({ queryKey: ["workspace"] });
      await waitFor(() =>
        expect(
          queryClient.getQueryState(["workspace", SOURCE_ID])?.status,
        ).toBe("error"),
      );
      // The source as last loaded: closed, for the reason known then.
      expect(
        screen.getByTestId("review-suggestion-replies-closed-sug-2"),
      ).toBeInTheDocument();
      closedControls();

      // Writable again: the controls return.
      seed({});
      await queryClient.invalidateQueries({ queryKey: ["workspace"] });
      expect(
        await screen.findByTestId("review-suggestion-reply-input-sug-2"),
      ).toBeInTheDocument();
      expect(
        screen.getByTestId("review-suggestion-accept-sug-1"),
      ).toBeInTheDocument();
    }
  });

  it("shows a decision from its own answer when the lists cannot be read again", async () => {
    seed({});
    let listsDown = false;
    const unavailable = () =>
      HttpResponse.json(
        {
          error: {
            code: "SERVICE_UNAVAILABLE",
            message: "The service is restarting",
            requestId: "request-down",
          },
        },
        { status: 503 },
      );
    server.use(
      // Until the decision, the lists answer as seeded.
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review/suggestions`, () =>
        listsDown ? unavailable() : undefined,
      ),
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/review/publications`, () =>
        listsDown ? unavailable() : undefined,
      ),
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/reject`,
        () => {
          listsDown = true;
          return HttpResponse.json({
            data: {
              suggestion: suggestion({
                id: "sug-2",
                kind: "comment",
                path: "README.md",
                beforeLines: null,
                afterLines: null,
                anchorStart: 12,
                note: "Is this still accurate?",
                commentId: "c-2",
                status: "rejected",
                statusBy: OWNER,
                statusAt: "2026-09-30T11:00:00Z",
              }),
              writtenHash: null,
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    renderPanel({ id: OWNER, owns: true });
    await userEvent.click(
      await screen.findByTestId("review-suggestion-reject-sug-2"),
    );
    await userEvent.click(
      screen.getByTestId("review-suggestion-reject-confirm-sug-2"),
    );

    expect(
      await screen.findByTestId("reviews-panel-refresh-error"),
    ).toHaveTextContent("The service is restarting");
    expect(
      screen.getByTestId("review-suggestion-status-sug-2"),
    ).toHaveTextContent("Rejected");
    expect(screen.queryByTestId("review-suggestion-reject-sug-2")).toBeNull();
    expect(screen.queryByTestId("review-suggestion-resolve-sug-2")).toBeNull();
  });

  it("keeps a typed rejection reason readable but sends nothing while the source is read-only", async () => {
    const rejects: unknown[] = [];
    seed({});
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-2/reject`,
        async ({ request }) => {
          rejects.push(await request.json());
          return HttpResponse.json({
            data: {
              suggestion: suggestion({ id: "sug-2", status: "rejected" }),
              writtenHash: null,
              alreadyApplied: false,
            },
          });
        },
      ),
    );
    const { queryClient } = renderPanel({ id: OWNER, owns: true });
    await userEvent.click(
      await screen.findByTestId("review-suggestion-reject-sug-2"),
    );
    await userEvent.type(
      screen.getByTestId("review-suggestion-reject-reason-sug-2"),
      "Not now",
    );

    // The source turns read-only while the form is open: the reason stays
    // readable, and cannot be changed or sent.
    seed({ sourceReadOnly: true });
    await queryClient.invalidateQueries();
    await waitFor(() =>
      expect(
        screen.getByTestId("review-suggestion-reject-reason-sug-2"),
      ).toHaveAttribute("readonly"),
    );
    expect(
      screen.getByTestId("review-suggestion-reject-reason-sug-2"),
    ).toHaveValue("Not now");
    expect(
      screen.getByTestId("review-suggestion-reject-confirm-sug-2"),
    ).toBeDisabled();
    expect(rejects).toEqual([]);

    // Writable again: the reason typed earlier is still there.
    seed({});
    await queryClient.invalidateQueries();
    const reason = await screen.findByTestId(
      "review-suggestion-reject-reason-sug-2",
    );
    expect(reason).toHaveValue("Not now");
    await userEvent.click(
      screen.getByTestId("review-suggestion-reject-confirm-sug-2"),
    );
    await waitFor(() => expect(rejects).toEqual([{ reason: "Not now" }]));
  });

  describe("keeps a discussion's reply when the panel regroups", () => {
    function serveReplies() {
      server.use(
        http.get(
          `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/:id/replies`,
          () => HttpResponse.json({ data: { replies: [] } }),
        ),
      );
    }

    async function typeReply(id: string, words: string) {
      await userEvent.click(
        await screen.findByTestId(`review-suggestion-replies-toggle-${id}`),
      );
      const input = await screen.findByTestId(
        `review-suggestion-reply-input-${id}`,
      );
      await userEvent.type(input, words);
      return input;
    }

    it("when earlier rounds are hidden and shown again", async () => {
      seed({
        suggestions: [
          suggestion({ id: "sug-1", findingId: "fnd-1" }),
          suggestion({
            id: "sug-7",
            publicationId: "pub-0",
            kind: "comment",
            beforeLines: null,
            afterLines: null,
            status: "resolved",
            note: "Settled last round?",
          }),
        ],
      });
      serveReplies();
      renderPanel({ id: OWNER, owns: true });
      const toggle = await screen.findByTestId("reviews-panel-toggle-earlier");
      await userEvent.click(toggle);
      const input = await typeReply("sug-7", "Still settled");

      await userEvent.click(toggle);
      expect(screen.queryByTestId("review-publication-pub-0")).toBeNull();
      await userEvent.click(toggle);
      expect(screen.getByTestId("review-suggestion-reply-input-sug-7")).toBe(
        input,
      );
      expect(input).toHaveValue("Still settled");
    });

    it("when its outdated commit group is collapsed and opened again", async () => {
      const OLD = "a".repeat(40);
      const NEW = "b".repeat(40);
      const base = publications();
      seed({
        suggestions: [
          suggestion({ id: "sug-1", findingId: "fnd-1" }),
          suggestion({
            id: "sug-sam",
            publicationId: "pub-sam",
            path: "src/other.rs",
          }),
        ],
        publications: {
          ...base,
          publications: [
            {
              ...base.publications[0],
              baseCommitVector: [{ repoId: "repo-api", sourceHead: OLD }],
            },
            {
              ...base.publications[0],
              id: "pub-sam",
              reviewerUserId: "user-sam",
              round: 1,
              baseCommitVector: [{ repoId: "repo-api", sourceHead: NEW }],
            },
          ],
          sourceHeads: { "repo-api": NEW },
        },
      });
      serveReplies();
      renderPanel({ id: OWNER, owns: true });
      const toggle = await screen.findByTestId("reviews-commit-toggle-aaaaaaa");
      await userEvent.click(toggle);
      const input = await typeReply("sug-1", "Does it still apply?");

      await userEvent.click(toggle);
      expect(screen.queryByTestId("review-publication-pub-1")).toBeNull();
      await userEvent.click(toggle);
      expect(screen.getByTestId("review-suggestion-reply-input-sug-1")).toBe(
        input,
      );
      expect(input).toHaveValue("Does it still apply?");
    });

    it("when its competing proposals are decided and it returns to its round", async () => {
      const rival = suggestion({
        id: "sug-7",
        publicationId: "pub-0",
        anchorStart: 3,
        afterLines: "return 2;\n",
        authorLabel: "bo",
      });
      seed({
        suggestions: [suggestion({ id: "sug-1", findingId: "fnd-1" }), rival],
      });
      serveReplies();
      const { queryClient } = renderPanel({ id: OWNER, owns: true });
      const competing = await screen.findByTestId("reviews-panel-competing");
      // Shown once: in the cluster, and its round points there.
      expect(
        within(competing).getByTestId("review-suggestion-sug-1"),
      ).toBeInTheDocument();
      expect(
        within(screen.getByTestId("review-publication-pub-1")).getByTestId(
          "review-suggestion-competing-sug-1",
        ),
      ).toBeInTheDocument();
      const input = await typeReply("sug-1", "Why not 2?");

      // The rival is rejected elsewhere: the cluster dissolves.
      seed({
        suggestions: [
          suggestion({ id: "sug-1", findingId: "fnd-1" }),
          { ...rival, status: "rejected" },
        ],
      });
      await queryClient.invalidateQueries();
      await waitFor(() =>
        expect(screen.queryByTestId("reviews-panel-competing")).toBeNull(),
      );
      const round = screen.getByTestId("review-publication-pub-1");
      expect(
        within(round).getByTestId("review-suggestion-reply-input-sug-1"),
      ).toBe(input);
      expect(input).toHaveValue("Why not 2?");
    });
  });

  it.each([
    { how: "its last open suggestion is decided elsewhere", commit: false },
    { how: "the source commits and its round becomes outdated", commit: true },
  ])(
    "keeps a round in view, with a reply being sent and its refusal, when $how",
    async ({ commit }) => {
      const OLD = "a".repeat(40);
      const NEW = "b".repeat(40);
      const base = publications();
      const roundAt = (head: string) => ({
        ...base.publications[0],
        baseCommitVector: [{ repoId: "repo-api", sourceHead: head }],
      });
      const earlierRound = {
        ...base.publications[1],
        baseCommitVector: [{ repoId: "repo-api", sourceHead: OLD }],
        openCount: 1,
      };
      const comment = suggestion({
        id: "sug-7",
        publicationId: "pub-0",
        kind: "comment",
        beforeLines: null,
        afterLines: null,
        note: "Is this still needed?",
      });
      seed({
        suggestions: [suggestion({ id: "sug-1", findingId: "fnd-1" }), comment],
        publications: {
          ...base,
          publications: [roundAt(OLD), earlierRound],
          sourceHeads: { "repo-api": OLD },
        },
      });
      const posted: unknown[] = [];
      let refuse!: () => void;
      const held = new Promise<void>((resolve) => {
        refuse = resolve;
      });
      server.use(
        http.get(
          `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/:id/replies`,
          () => HttpResponse.json({ data: { replies: [] } }),
        ),
        http.post(
          `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-7/replies`,
          async ({ request }) => {
            posted.push(await request.json());
            await held;
            return HttpResponse.json(
              {
                error: {
                  code: "SUGGESTION_DECIDED",
                  message: "The suggestion was decided meanwhile",
                  requestId: "request-refused",
                },
              },
              { status: 409 },
            );
          },
        ),
      );
      const { queryClient } = renderPanel({ id: OWNER, owns: true });
      await userEvent.click(
        await screen.findByTestId("review-suggestion-replies-toggle-sug-7"),
      );
      const input = await screen.findByTestId(
        "review-suggestion-reply-input-sug-7",
      );
      await userEvent.type(input, "Yes, for now{Enter}");
      await waitFor(() => expect(posted).toHaveLength(1));

      if (commit) {
        // The source commits: every round here is about an earlier commit.
        seed({
          suggestions: [
            suggestion({ id: "sug-1", findingId: "fnd-1" }),
            comment,
          ],
          publications: {
            ...base,
            publications: [roundAt(OLD), earlierRound],
            sourceHeads: { "repo-api": NEW },
          },
        });
      } else {
        // Its last open suggestion is resolved elsewhere: the round would
        // drop into the earlier rounds.
        seed({
          suggestions: [
            suggestion({ id: "sug-1", findingId: "fnd-1" }),
            { ...comment, status: "resolved" },
          ],
          publications: {
            ...base,
            publications: [roundAt(OLD), { ...earlierRound, openCount: 0 }],
            sourceHeads: { "repo-api": OLD },
          },
        });
      }
      await queryClient.invalidateQueries();
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 50));
      });
      expect(screen.getByTestId("review-publication-pub-0")).toBeVisible();
      expect(screen.getByTestId("review-suggestion-reply-input-sug-7")).toBe(
        input,
      );

      refuse();
      expect(
        await screen.findByText("The suggestion was decided meanwhile"),
      ).toBeInTheDocument();
      expect(input).toHaveValue("Yes, for now");
      expect(posted).toHaveLength(1);
      if (!commit) {
        // Its words cleared, the round goes to the earlier rounds.
        await userEvent.clear(input);
        await waitFor(() =>
          expect(screen.queryByTestId("review-publication-pub-0")).toBeNull(),
        );
      }
    },
  );
});

describe("the current commit's text changes by paragraph", () => {
  const content =
    "# Guide\n\nFirst paragraph here.\n\nThe API adopts it for existing organizations with no image policy, so their workspaces can start. See [setup](setup.md).\n\nLast paragraph here.\n";
  const at = (text: string) => content.indexOf(text);
  async function seedParagraph(
    file: "readable" | "unreadable",
    options: { fileDelayMs?: number; refreshAfterAccept?: boolean } = {},
  ) {
    const contentHash = await sha256Utf8(content);
    // Accepting a1 decides it: the list read after that says so, as the
    // server's would.
    const accepted: unknown[] = [];
    const decided = (edit: ReturnType<typeof reviewEditFixture>) =>
      accepted.length > 0 && edit.id === "a1"
        ? {
            ...edit,
            status: "accepted",
            statusBy: OWNER,
            statusAt: "2026-09-24T09:00:00.000Z",
            acceptedAfterText: edit.afterText,
          }
        : edit;
    const base = publications();
    const names = { ...base.reviewerDisplayNames, "user-bo": "Bo Park" };
    seed({
      publications: {
        ...base,
        publications: [
          base.publications[0],
          {
            ...base.publications[0],
            id: "pub-2",
            reviewerUserId: "user-bo",
            authorLabel: "bo",
            round: 1,
            summary: "One wording change.",
            createdAt: "2026-09-23T15:00:00.000Z",
            openCount: 1,
          },
        ],
        reviewerDisplayNames: names,
      },
    });
    const rows = () => [
      suggestion({
        id: "sug-a",
        findingId: "fnd-1",
        repoId: "repo-docs",
        path: "docs/guide.md",
        beforeLines: content,
        afterLines: content,
        anchorStart: 1,
        contentHash,
        edits: [
          decided(
            reviewEditFixture(
              "a1",
              content,
              at("existing organizations with"),
              at("existing organizations with") +
                "existing organizations with".length,
              "qq ss www",
            ),
          ),
          reviewEditFixture(
            "a2",
            content,
            at("workspaces"),
            at("workspaces") + "workspaces".length,
            "qqq",
          ),
        ],
      }),
      suggestion({
        id: "sug-b",
        publicationId: "pub-2",
        repoId: "repo-docs",
        path: "docs/guide.md",
        beforeLines: content,
        afterLines: content,
        anchorStart: 1,
        contentHash,
        authorLabel: "bo",
        edits: [
          reviewEditFixture(
            "b1",
            content,
            at("workspaces"),
            at("workspaces") + "workspaces".length,
            "work spaces",
          ),
        ],
      }),
    ];
    server.use(
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions`,
        ({ request }) => {
          const url = new URL(request.url);
          const kind = url.searchParams.get("kind");
          const status = url.searchParams.get("status");
          return HttpResponse.json({
            data: {
              suggestions: rows().filter(
                (row) =>
                  (!kind || row.kind === kind) &&
                  (!status || row.status === status),
              ),
            },
          });
        },
      ),
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-a/accept`,
        async ({ request }) => {
          accepted.push(await request.json());
          return HttpResponse.json({
            data: {
              suggestion: suggestion({ id: "sug-a", status: "accepted" }),
              writtenHash: "def",
              alreadyApplied: false,
              checkpointPending: true,
            },
          });
        },
      ),
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/:id/replies`,
        () => HttpResponse.json({ data: { replies: [] } }),
      ),
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/files/docs/guide.md`,
        async () =>
          // A slow read, when asked for; once a change is accepted the refresh
          // fails, as a transient error would.
          (await new Promise((resolve) =>
            setTimeout(resolve, options.fileDelayMs ?? 0),
          ),
          accepted.length > 0 && !options.refreshAfterAccept)
            ? HttpResponse.json(
                { error: { code: "UNAVAILABLE", message: "VM busy" } },
                { status: 503 },
              )
            : file === "readable"
              ? HttpResponse.json({
                  data: {
                    path: "docs/guide.md",
                    content,
                    size: content.length,
                    encoding: "utf-8",
                    hash: accepted.length > 0 ? "guide-v2" : "guide-v1",
                    source: "committed",
                    repoOid: "oid-base",
                    snapshotAt: null,
                    vmId: null,
                  },
                })
              : HttpResponse.json(
                  { error: { code: "NOT_FOUND", message: "No such file" } },
                  { status: 404 },
                ),
      ),
    );
    return accepted;
  }

  it("shows each edited paragraph once with every reviewer's marks, its neighbours and its cards", async () => {
    const accepted = await seedParagraph("readable");
    const opened: unknown[][] = [];
    renderPanel(
      { id: OWNER, owns: true },
      { onOpenFile: (...args: unknown[]) => opened.push(args) },
    );
    const section = await screen.findByTestId(
      "reviews-file-repo-docs:docs/guide.md",
    );
    expect(section).toHaveTextContent("docs/guide.md");
    expect(section).toHaveTextContent("3 open");
    // The discussion's state on the paragraph's card is the suggestion's:
    // opened, then closed again, whichever card shows the suggestion next
    // shows it closed but mounted, as it was left.
    const toggle = () =>
      within(
        screen.getByTestId("reviews-file-repo-docs:docs/guide.md"),
      ).getByTestId("review-suggestion-replies-toggle-sug-a:a1");
    fireEvent.click(toggle());
    await waitFor(() =>
      expect(
        within(
          screen.getByTestId("reviews-file-repo-docs:docs/guide.md"),
        ).getByTestId("review-suggestion-replies-sug-a:a1"),
      ).toBeVisible(),
    );
    fireEvent.click(toggle());
    expect(toggle()).toHaveTextContent("Discussion");
    // The paragraph's heading opens the file at the paragraph's line, not
    // at the proposal's anchor.
    await within(section).findByTestId("reviews-paragraph-group-sug-a:a1");
    fireEvent.click(
      within(section).getByTestId("reviews-paragraph-open-sug-a:a1"),
    );
    expect(opened).toEqual([["docs/guide.md", "repo-docs", 5]]);
    expect(section).toHaveTextContent("Alex Chen");
    expect(section).toHaveTextContent("Bo Park");
    // The paragraph, once, with both reviewers' changes marked in it.
    const paragraph = await within(section).findByTestId(
      "reviews-paragraph-group-sug-a:a1",
    );
    const preview = within(paragraph).getByTestId(/^spec-suggestion-preview-/);
    // An unchanged link in the edited paragraph opens the file it names.
    fireEvent.click(within(preview).getByRole("link", { name: "setup" }));
    expect(opened[1]).toEqual(["docs/setup.md", "repo-docs", undefined]);
    expect(preview).toHaveTextContent(
      "existing organizations withqq ss www no image policy",
    );
    expect(preview).toHaveTextContent("workspacesqqq");
    expect(preview).not.toHaveTextContent("work spaces");
    expect(
      preview.querySelector('del[data-review-anchor="sug-a:a1"]'),
    ).toHaveTextContent("existing organizations with");
    // The paragraphs around it, for context.
    const context = within(paragraph).getAllByTestId(
      "reviews-paragraph-context",
    );
    expect(context.map((el) => el.textContent)).toEqual([
      "First paragraph here.",
      "Last paragraph here.",
    ]);
    expect(
      within(section).getAllByTestId(/^reviews-paragraph-group-/),
    ).toHaveLength(1);
    // The cards under it: one for the compatible change, one box for the
    // conflicting pair, naming who is previewed (cards carry the reviewer's
    // label, as every card in the panel does).
    const cards = within(paragraph).getByTestId("reviews-paragraph-cards");
    expect(
      within(cards).getByTestId("spec-suggestion-card-sug-a:a1"),
    ).toHaveTextContent("alex");
    const box = within(cards).getByTestId(/^spec-review-conflict-/);
    expect(box).toHaveTextContent("2 suggestions for the same text");
    expect(box).toHaveTextContent("Previewing alex.");
    expect(
      within(box).getByTestId("spec-suggestion-card-sug-b:b1"),
    ).toHaveTextContent("bo");
    fireEvent.click(
      within(
        within(box).getByTestId("spec-suggestion-card-sug-b:b1"),
      ).getByRole("radio", { name: "Preview in document" }),
    );
    expect(preview).toHaveTextContent("workspaceswork spaces");
    expect(box).toHaveTextContent("Previewing bo.");
    // The reviews keep their cards and say what is shown above; nothing
    // competes outside the box.
    expect(
      screen.getByTestId("review-publication-in-sections-pub-1"),
    ).toHaveTextContent("2 changes are shown above, with their paragraphs.");
    expect(
      screen.getByTestId("review-publication-in-sections-pub-2"),
    ).toHaveTextContent("1 change is shown above, with its paragraph.");
    expect(screen.queryByTestId("reviews-panel-competing")).toBeNull();
    expect(screen.getAllByTestId("review-suggestion-sug-a:a1")).toHaveLength(1);
    // The finding's batch acceptance stays offered while its changes are
    // shown above.
    expect(screen.getByTestId("review-finding-accept-fnd-1")).toBeEnabled();
    // Accepting from the paragraph's card sends the same request as before,
    // and what the decision reported stays with the suggestion when its card
    // moves back to its round, decided.
    fireEvent.click(
      within(
        within(cards).getByTestId("spec-suggestion-card-sug-a:a1"),
      ).getByRole("button", { name: "Accept" }),
    );
    await waitFor(() => expect(accepted).toEqual([{ editId: "a1" }]));
    await waitFor(() =>
      expect(screen.getByTestId("review-publication-pub-1")).toContainElement(
        screen.getByTestId("review-suggestion-sug-a:a1"),
      ),
    );
    expect(screen.getByTestId("review-suggestion-sug-a:a1")).toHaveTextContent(
      "commit/push did not finish",
    );
    const moved = screen.getByTestId("review-suggestion-sug-a:a1");
    expect(
      within(moved).getByTestId("review-suggestion-replies-toggle-sug-a:a1"),
    ).toHaveTextContent(/^Discussion$/);
    expect(
      within(moved).getByTestId("review-suggestion-replies-sug-a:a1"),
    ).not.toBeVisible();
    expect(
      screen.getByTestId("review-publication-in-sections-pub-1"),
    ).toHaveTextContent("1 change is shown above, with its paragraph.");
    // The file is read again after the acceptance; a failed refresh keeps
    // the paragraphs and says so.
    await within(section).findByTestId("reviews-file-refresh-failed");
  });

  it("reveals a deep-linked change in its paragraph once the file has loaded", async () => {
    const accepted = await seedParagraph("readable", {
      fileDelayMs: 300,
      refreshAfterAccept: true,
    });
    server.use(
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-a/accept`,
        async ({ request }) => {
          accepted.push(await request.json());
          return HttpResponse.json({
            data: {
              suggestion: suggestion({ id: "sug-a", status: "accepted" }),
              writtenHash: "def",
              alreadyApplied: false,
              checkpointPending: false,
            },
          });
        },
      ),
    );
    const scrolled: Element[] = [];
    const scrollIntoView = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    try {
      renderPanel(
        { id: OWNER, owns: true },
        { revealTarget: "review-suggestion-sug-a:a2" },
      );
      // The fallback cards show while the file loads; the reveal waits.
      await screen.findByTestId("reviews-file-loading");
      await screen.findByTestId("reviews-paragraph-group-sug-a:a1", undefined, {
        timeout: 3000,
      });
      // The reveal follows the section until its changes are placed (the
      // digest computed), and may scroll once more as it settles: wait for
      // placement, then until no further scroll arrives.
      await waitFor(() =>
        expect(
          screen.getByTestId("reviews-file-repo-docs:docs/guide.md"),
        ).toHaveAttribute(
          "data-state",
          expect.stringMatching(/:[0-9a-f]{64}$/),
        ),
      );
      await waitFor(() => expect(scrolled.length).toBeGreaterThan(0));
      for (let tries = 0; tries < 10; tries++) {
        const before = scrolled.length;
        await act(async () => {
          await new Promise((resolve) => setTimeout(resolve, 300));
        });
        if (scrolled.length === before) break;
      }
      const last = scrolled[scrolled.length - 1];
      expect(last).toHaveAttribute("data-testid", "review-suggestion-sug-a:a2");
      expect(
        last.closest('[data-testid^="reviews-paragraph-group-"]'),
      ).not.toBeNull();
      // The reveal is done: a section changing later (the file read again
      // after a decision) does not replay it.
      const revealed = scrolled.length;
      fireEvent.click(
        within(screen.getByTestId("spec-suggestion-card-sug-a:a1")).getByRole(
          "button",
          { name: "Accept" },
        ),
      );
      await waitFor(() => expect(accepted).toHaveLength(1));
      await waitFor(() =>
        expect(screen.getByTestId("review-publication-pub-1")).toContainElement(
          screen.getByTestId("review-suggestion-sug-a:a1"),
        ),
      );
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 100));
      });
      expect(scrolled).toHaveLength(revealed);
    } finally {
      Element.prototype.scrollIntoView = scrollIntoView;
    }
  });

  it("reveals a deep link after a file's section has left", async () => {
    const contentHash = await sha256Utf8(content);
    const separator = content.indexOf("\n\nThe API") + 1;
    let decided = false;
    const rows = () => [
      suggestion({
        id: "sug-s",
        repoId: "repo-docs",
        path: "docs/guide.md",
        beforeLines: content,
        afterLines: content,
        anchorStart: 1,
        contentHash,
        edits: [
          {
            ...reviewEditFixture("s1", content, separator, separator, "\n"),
            ...(decided
              ? {
                  status: "accepted",
                  statusBy: OWNER,
                  statusAt: "2026-09-24T09:00:00.000Z",
                  acceptedAfterText: "\n",
                }
              : {}),
          },
        ],
      }),
    ];
    seed({});
    server.use(
      http.get(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions`,
        ({ request }) => {
          const url = new URL(request.url);
          const kind = url.searchParams.get("kind");
          const status = url.searchParams.get("status");
          return HttpResponse.json({
            data: {
              suggestions: rows().filter(
                (row) =>
                  (!kind || row.kind === kind) &&
                  (!status || row.status === status),
              ),
            },
          });
        },
      ),
      http.post(
        `*/api/v1/workspaces/${SOURCE_ID}/review/suggestions/sug-s/accept`,
        () => {
          decided = true;
          return HttpResponse.json({
            data: {
              suggestion: suggestion({ id: "sug-s", status: "accepted" }),
              writtenHash: "def",
              alreadyApplied: false,
              checkpointPending: false,
            },
          });
        },
      ),
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/files/docs/guide.md`, () =>
        HttpResponse.json({
          data: {
            path: "docs/guide.md",
            content,
            size: content.length,
            encoding: "utf-8",
            hash: "guide-v1",
            source: "committed",
            repoOid: "oid-base",
            snapshotAt: null,
            vmId: null,
          },
        }),
      ),
    );
    let setReveal!: (target: string | undefined) => void;
    function Host() {
      const [revealTarget, set] = React.useState<string | undefined>();
      React.useEffect(() => {
        setReveal = set;
      }, [set]);
      return (
        <ReviewsPanel
          workspaceId={SOURCE_ID}
          workspaceName="kaihui-test3"
          viewerUserId={OWNER}
          viewerOwnsWorkspace
          revealTarget={revealTarget}
        />
      );
    }
    const scrolled: Element[] = [];
    const scrollIntoView = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    try {
      renderApp(<Host />);
      const paragraph = await screen.findByTestId(
        "reviews-paragraph-group-sug-s:s1",
      );
      // Accepting the file's only change: the section leaves.
      fireEvent.click(
        within(paragraph).getByTestId("review-suggestion-accept-sug-s:s1"),
      );
      await waitFor(() =>
        expect(
          screen.queryByTestId("reviews-file-repo-docs:docs/guide.md"),
        ).toBeNull(),
      );
      // A deep link to the decided change still reveals it, in its round.
      await act(async () => setReveal("review-suggestion-sug-s:s1"));
      await waitFor(() => expect(scrolled.length).toBeGreaterThan(0));
      expect(scrolled[scrolled.length - 1]).toHaveAttribute(
        "data-testid",
        "review-suggestion-sug-s:s1",
      );
    } finally {
      Element.prototype.scrollIntoView = scrollIntoView;
    }
  });

  it("frames a change between paragraphs with the paragraphs either side and names its own line", async () => {
    const contentHash = await sha256Utf8(content);
    const separator = content.indexOf("\n\nThe API") + 1;
    seed({
      suggestions: [
        suggestion({
          id: "sug-s",
          repoId: "repo-docs",
          path: "docs/guide.md",
          beforeLines: content,
          afterLines: content,
          anchorStart: 1,
          contentHash,
          edits: [reviewEditFixture("s1", content, separator, separator, "\n")],
        }),
      ],
    });
    server.use(
      http.get(`*/api/v1/workspaces/${SOURCE_ID}/files/docs/guide.md`, () =>
        HttpResponse.json({
          data: {
            path: "docs/guide.md",
            content,
            size: content.length,
            encoding: "utf-8",
            hash: "guide-v1",
            source: "committed",
            repoOid: "oid-base",
            snapshotAt: null,
            vmId: null,
          },
        }),
      ),
    );
    const opened: unknown[][] = [];
    renderPanel(
      { id: OWNER, owns: true },
      { onOpenFile: (...args: unknown[]) => opened.push(args) },
    );
    const paragraph = await screen.findByTestId(
      "reviews-paragraph-group-sug-s:s1",
    );
    expect(paragraph).toHaveTextContent("docs/guide.md:4");
    expect(
      within(paragraph)
        .getAllByTestId("reviews-paragraph-context")
        .map((el) => el.textContent),
    ).toEqual([
      "First paragraph here.",
      "The API adopts it for existing organizations with no image policy, so their workspaces can start. See setup.",
    ]);
    expect(
      within(paragraph).getByTestId("spec-review-source-diff"),
    ).toBeInTheDocument();
    fireEvent.click(
      within(paragraph).getByTestId("reviews-paragraph-open-sug-s:s1"),
    );
    expect(opened).toEqual([["docs/guide.md", "repo-docs", 4]]);
  });

  it("shows each change alone under the file when the file cannot be read", async () => {
    await seedParagraph("unreadable");
    renderPanel({ id: OWNER, owns: true });
    const section = await screen.findByTestId(
      "reviews-file-repo-docs:docs/guide.md",
    );
    expect(
      await within(section).findByTestId("reviews-file-unreadable"),
    ).toHaveTextContent("each change is shown alone");
    expect(
      within(section).getByTestId("review-suggestion-sug-a:a1"),
    ).toHaveTextContent("+qq ss www");
    expect(
      within(section).getByTestId("review-suggestion-sug-b:b1"),
    ).toBeInTheDocument();
    expect(
      within(section).queryByTestId(/^reviews-paragraph-group-/),
    ).toBeNull();
    expect(
      screen.getByTestId("review-publication-in-sections-pub-1"),
    ).toHaveTextContent("2 changes are shown above");
  });
});
