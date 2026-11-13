/**
 * MyReviewPanel — the reviewer's own review tool on their copy: the changes
 * the next push would send (with notes and holds that persist in this
 * browser), new files, artifacts, skipped files, their comments on the
 * source, and how the owner handled their last round.
 */

import React from "react";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import type { ReviewSuggestion, WorkspaceReview } from "../../lib/types";
import { MyReviewPanel, ownLastRound } from "./MyReviewPanel";
import {
  WorkspaceLiveGateProvider,
  type WorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import { readReviewDrafts, writeReviewDrafts } from "./reviewDrafts";
import {
  resetDirtyEditorsForTests,
  setEditorDirty,
} from "../editor/dirtyEditors";
import {
  usePublishWorkspaceReview,
  useReviewPreview,
} from "../../hooks/useWorkspaceReview";

const SOURCE_ID = "ws-source-1";
const REVIEW_ID = "ws-review-1";
const ME = "user-alex";
const OTHER = "user-sam";
const HUNK_KEY = "0123456789abcdef0123456789abcdef";
const NEW_KEY = "fedcba9876543210fedcba9876543210";
const ART_KEY = "aaaabbbbccccddddeeeeffff00001111";

const review: WorkspaceReview = {
  forkId: "fork-1",
  orgId: "org-g5labs",
  sourceWorkspaceId: SOURCE_ID,
  sourceWorkspaceName: "kaihui-test3",
  reviewWorkspaceId: REVIEW_ID,
  reviewWorkspaceName: "kaihui-test3-review-alex-chen",
  reviewerUserId: ME,
  reviewerDisplayName: "Alex Chen",
  vmStatus: "running",
  vmStartedAt: null,
  vmLastActivityAt: null,
  lastFlushedAt: null,
  createdAt: "2026-09-22T16:10:00.000Z",
};

function publication(overrides: Record<string, unknown>) {
  return {
    id: "pub",
    orgId: "org-g5labs",
    sourceWorkspaceId: SOURCE_ID,
    reviewWorkspaceId: REVIEW_ID,
    reviewerUserId: ME,
    authorKind: "human",
    authorLabel: "alex",
    round: 1,
    baseCommitVector: [] as unknown,
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
    publicationId: "me-2",
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
  publication({ id: "me-1", round: 1, supersededBy: "me-2" }),
  publication({
    id: "sam-1",
    reviewerUserId: OTHER,
    authorLabel: "sam",
    createdAt: "2026-09-25T10:00:00.000Z",
  }),
  publication({ id: "me-note", kind: "annotation", verdict: "comment_only" }),
];
const suggestions = [
  suggestion({ id: "s-acc", status: "accepted" }),
  suggestion({ id: "s-rej", path: "src/lib.rs", status: "rejected" }),
  suggestion({ id: "s-open", path: "NEW.md", kind: "new_file" }),
  suggestion({ id: "s-sam", publicationId: "sam-1", path: "sam.rs" }),
  suggestion({
    id: "c-mine",
    publicationId: "me-note",
    kind: "comment",
    path: "README.md",
    anchorStart: 12,
    note: "Is this still accurate?",
  }),
  suggestion({
    id: "c-sam",
    publicationId: "sam-1",
    kind: "comment",
    path: "README.md",
    anchorStart: 3,
    note: "Sam's question",
  }),
];

/** What the current round already carries, per previewed item. */
type Published = {
  hunk?: boolean;
  newFile?: boolean;
  artifact?: boolean;
  round?: number;
  /** The note a push would carry for the hunk (with or without markers). */
  carriedNote?: string;
  /** The rows' status in the round (`open` unless said otherwise). */
  status?: string;
};

function inRound(published: Published, item: "hunk" | "newFile" | "artifact") {
  return published[item]
    ? {
        publicationId: "me-1",
        round: published.round ?? 2,
        suggestionId: `s-${String(item)}`,
        status: published.status ?? "open",
      }
    : null;
}

function carriedFor(
  published: Published,
  item: "hunk" | "newFile" | "artifact",
) {
  const note = item === "hunk" ? published.carriedNote : undefined;
  return published[item] || note !== undefined
    ? {
        suggestionId: `s-${String(item)}`,
        round: published.round ?? 2,
        note: note ?? "",
      }
    : null;
}

function preview(
  saved = { workspace: "live", committed: true },
  advanced = false,
  published: Published = {},
) {
  return {
    baseCommitVector: [
      {
        repoId: "repo-api",
        sourceBranch: "main",
        sourceHead: advanced ? "b".repeat(40) : "a".repeat(40),
        reviewBranch: "review",
        reviewHead: "c".repeat(40),
        mergeBase: "a".repeat(40),
      },
    ],
    files: [
      {
        repoId: "repo-api",
        path: "src/main.rs",
        kind: "hunk",
        hunks: 1,
        bytes: null,
        changes: [
          {
            key: HUNK_KEY,
            kind: "hunk",
            anchorStart: 3,
            beforeLines: "return 1;\n",
            afterLines: "return 0;\n",
            publishedIn: inRound(published, "hunk"),
            carried: carriedFor(published, "hunk"),
          },
        ],
      },
      {
        repoId: "repo-api",
        path: "NOTES.md",
        kind: "new_file",
        hunks: 0,
        bytes: 120,
        changes: [
          {
            key: NEW_KEY,
            kind: "new_file",
            anchorStart: null,
            beforeLines: null,
            afterLines: "# Notes\n",
            publishedIn: inRound(published, "newFile"),
            carried: carriedFor(published, "newFile"),
          },
        ],
        publishedIn: inRound(published, "newFile"),
        carried: carriedFor(published, "newFile"),
        key: NEW_KEY,
      },
      {
        repoId: "repo-api",
        path: "mock.png",
        kind: "artifact",
        hunks: 0,
        bytes: 1200,
        changes: [],
        publishedIn: inRound(published, "artifact"),
        carried: carriedFor(published, "artifact"),
        key: ART_KEY,
      },
    ],
    skipped: [{ repoId: "repo-api", path: "big.bin", reason: "too large" }],
    saved,
  };
}

function seed(
  options: {
    previewCalls?: { count: number };
    publications?: ReturnType<typeof publication>[];
    sourceHeads?: Record<string, string>;
    /** The source's branch moved past the copy's base. */
    advanced?: boolean;
    suggestions?: ReturnType<typeof suggestion>[];
    /** What the reviewer's current round already carries. */
    published?: Published;
    /** The source's suggestions never answer (still loading). */
    suggestionsPending?: boolean;
  } = {},
) {
  server.use(
    http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () => {
      if (options.previewCalls) options.previewCalls.count += 1;
      return HttpResponse.json({
        data: preview(
          undefined,
          options.advanced ?? false,
          options.published ?? {},
        ),
      });
    }),
    http.get(`*/api/v1/workspaces/${SOURCE_ID}/review/publications`, () =>
      HttpResponse.json({
        data: {
          publications: options.publications ?? publications,
          findings: [],
          reviewerDisplayNames: { [ME]: "Alex Chen", [OTHER]: "Sam Lee" },
          sourceHeads: options.sourceHeads ?? {},
        },
      }),
    ),
    http.get(`*/api/v1/workspaces/${SOURCE_ID}/review/suggestions`, () =>
      options.suggestionsPending
        ? new Promise<never>(() => {})
        : HttpResponse.json({
            data: { suggestions: options.suggestions ?? suggestions },
          }),
    ),
  );
}

/** Stands in for the push dialog: publishes the copy's review on click. */
function Publisher() {
  const publish = usePublishWorkspaceReview(REVIEW_ID);
  return (
    <button
      type="button"
      data-testid="publisher"
      onClick={() => void publish.mutateAsync({ verdict: "comment_only" })}
    >
      push
    </button>
  );
}

function renderPanel(
  extra: Partial<React.ComponentProps<typeof MyReviewPanel>> = {},
) {
  return renderApp(
    <MyReviewPanel
      review={review}
      viewerUserId={ME}
      org="g5labs"
      canPush
      {...extra}
    />,
  );
}

/** Stands in for another consumer that loaded the preview before the panel. */
function PreviewPrimer() {
  const preview = useReviewPreview(REVIEW_ID);
  return <span data-testid="primer">{preview.status}</span>;
}

/** The primer first; the panel only when the test mounts it. */
function LateMountedPanel() {
  const [mounted, setMounted] = React.useState(false);
  return (
    <>
      <PreviewPrimer />
      {mounted ? (
        <MyReviewPanel review={review} viewerUserId={ME} org="g5labs" canPush />
      ) : (
        <button
          type="button"
          data-testid="mount-panel"
          onClick={() => setMounted(true)}
        >
          mount
        </button>
      )}
    </>
  );
}

describe("MyReviewPanel", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("picks the viewer's own current human round", () => {
    expect(ownLastRound(publications, ME)?.id).toBe("me-2");
    expect(ownLastRound(publications, OTHER)?.id).toBe("sam-1");
    expect(ownLastRound(publications, "user-nobody")).toBeNull();
  });

  it("lists the copy's changes, new files, artifacts and skipped files from a preview", async () => {
    seed();
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    expect(
      await within(panel).findByTestId("my-review-saved"),
    ).toHaveTextContent("Saved from your running copy.");
    const change = within(panel).getByTestId(`my-review-change-${HUNK_KEY}`);
    expect(change).toHaveTextContent("line 3");
    expect(change).toHaveTextContent("−return 1;");
    expect(change).toHaveTextContent("+return 0;");
    expect(
      within(panel).getByTestId("my-review-file-NOTES.md"),
    ).toHaveTextContent("the owner decides whether it enters the workspace");
    expect(
      within(panel).getByTestId("my-review-artifact-mock.png"),
    ).toHaveTextContent("sent as an artifact");
    expect(
      within(panel).getByTestId("my-review-skipped-big.bin"),
    ).toHaveTextContent("too large");
    expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
      "2 of 2 changes will be pushed · 0 notes",
    );
  });

  it("keeps notes and holds in this browser across remounts, and counts them", async () => {
    seed();
    const first = renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    await userEvent.type(
      within(panel).getByTestId(`my-review-note-${HUNK_KEY}`),
      "Early return should succeed.",
    );
    await userEvent.click(
      within(panel).getByTestId(`my-review-include-${NEW_KEY}`),
    );
    await waitFor(() =>
      expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
        "1 of 2 changes will be pushed · 1 note",
      ),
    );
    expect(readReviewDrafts(REVIEW_ID)).toEqual({
      changeNotes: { [HUNK_KEY]: "Early return should succeed." },
      excludedChanges: [NEW_KEY],
      fileNotes: {},
      excludedFiles: [],
    });
    first.unmount();

    renderPanel();
    const again = await screen.findByTestId("my-review-panel");
    await within(again).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(within(again).getByTestId(`my-review-note-${HUNK_KEY}`)).toHaveValue(
      "Early return should succeed.",
    );
    expect(
      within(again).getByTestId(`my-review-include-${NEW_KEY}`),
    ).not.toBeChecked();
  });

  it("drops notes and holds on changes the preview no longer lists, and clears all on request", async () => {
    seed();
    writeReviewDrafts(REVIEW_ID, {
      changeNotes: {
        [HUNK_KEY]: "Still here.",
        stalekeystalekeystalekeystalekey1: "Edited since.",
      },
      excludedChanges: ["stalekeystalekeystalekeystalekey2"],
      fileNotes: { "gone.md": "No longer in the round." },
      excludedFiles: [
        { repoId: "repo-api", path: "gone.md" },
        { repoId: "repo-api", path: "NOTES.md" },
      ],
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    await waitFor(() =>
      expect(readReviewDrafts(REVIEW_ID)).toEqual({
        changeNotes: { [HUNK_KEY]: "Still here." },
        excludedChanges: [],
        fileNotes: {},
        excludedFiles: [{ repoId: "repo-api", path: "NOTES.md" }],
      }),
    );
    expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
      "1 of 2 changes will be pushed · 1 note",
    );
    // The panel is a page of its own now: its body scrolls with the page,
    // never in a bounded box of its own.
    const body = within(panel).getByTestId("my-review-body");
    expect(body.className).not.toContain("max-h-");
    await userEvent.click(within(panel).getByTestId("my-review-clear-drafts"));
    await waitFor(() =>
      expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
        "2 of 2 changes will be pushed · 0 notes",
      ),
    );
    expect(readReviewDrafts(REVIEW_ID)).toEqual({
      changeNotes: {},
      excludedChanges: [],
      fileNotes: {},
      excludedFiles: [],
    });
    expect(
      within(panel).queryByTestId("my-review-clear-drafts"),
    ).not.toBeInTheDocument();
  });

  it("keeps a draft a cached preview lacks until its own fetch lists it", async () => {
    // Another tab noted the new file after this tab's preview was cached.
    // The panel mounts later: it must not prune against that cached
    // preview, only against the one it fetches itself.
    seed();
    const full = preview();
    const cached = {
      ...full,
      files: full.files.filter((file) => file.path === "src/main.rs"),
    };
    let calls = 0;
    const releases: Array<() => void> = [];
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, async () => {
        calls += 1;
        if (calls === 1) return HttpResponse.json({ data: cached });
        await new Promise<void>((resolve) => {
          releases.push(resolve);
        });
        return HttpResponse.json({ data: full });
      }),
    );
    writeReviewDrafts(REVIEW_ID, {
      changeNotes: { [NEW_KEY]: "From the other tab." },
      excludedChanges: [],
      fileNotes: {},
      excludedFiles: [],
    });
    vi.useFakeTimers({ toFake: ["Date"] });
    try {
      vi.setSystemTime(new Date("2026-09-25T12:00:00.000Z"));
      renderApp(<LateMountedPanel />);
      await waitFor(() =>
        expect(screen.getByTestId("primer")).toHaveTextContent("ok"),
      );
      // The panel mounts later than the cached preview was fetched.
      vi.setSystemTime(new Date("2026-09-25T12:00:05.000Z"));
      await userEvent.click(screen.getByTestId("mount-panel"));
      const panel = await screen.findByTestId("my-review-panel");
      await waitFor(() => expect(calls).toBe(2));
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(readReviewDrafts(REVIEW_ID).changeNotes).toEqual({
        [NEW_KEY]: "From the other tab.",
      });
      releases[0]?.();
      await within(panel).findByTestId(`my-review-change-${NEW_KEY}`);
      await waitFor(() =>
        expect(
          within(panel).getByTestId("my-review-summary"),
        ).toHaveTextContent("2 of 2 changes will be pushed · 1 note"),
      );
      expect(readReviewDrafts(REVIEW_ID).changeNotes).toEqual({
        [NEW_KEY]: "From the other tab.",
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("opens a change in the editor at its line; there is no Refresh, the preview is taken once", async () => {
    const previewCalls = { count: 0 };
    seed({ previewCalls });
    const onOpenFile = vi.fn();
    renderPanel({ onOpenFile });
    const panel = await screen.findByTestId("my-review-panel");
    await userEvent.click(
      await within(panel).findByTestId(`my-review-open-${HUNK_KEY}`),
    );
    expect(onOpenFile).toHaveBeenCalledWith("src/main.rs", "repo-api", 3);
    await userEvent.click(
      await within(panel).findByTestId(`my-review-open-${NEW_KEY}`),
    );
    expect(onOpenFile).toHaveBeenLastCalledWith("NOTES.md", "repo-api", 1);
    expect(previewCalls.count).toBe(1);
    expect(within(panel).queryByTestId("my-review-refresh")).toBeNull();
    expect(within(panel).getByTestId("my-review-push")).toBeInTheDocument();
  });

  it("reads the owner's decision from the polled suggestions when the preview predates it", async () => {
    const bodies: unknown[] = [];
    seed({
      // The preview still says the hunk is open in round 2...
      published: { hunk: true, status: "open" },
      publications: [publication({ id: "me-1", round: 2 })],
      sourceHeads: { "repo-api": "a".repeat(40) },
      // ...while the source's rows (polled) say the owner rejected it.
      suggestions: [
        suggestion({ id: "s-hunk", publicationId: "me-1", status: "rejected" }),
      ],
    });
    server.use(
      http.post(
        `*/api/v1/workspaces/${REVIEW_ID}/review/changes/discard`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            data: {
              discarded: [
                {
                  key: HUNK_KEY,
                  repoId: "repo-api",
                  path: "src/main.rs",
                  kind: "hunk",
                },
              ],
              skipped: [],
            },
          });
        },
      ),
    );
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await waitFor(() =>
      expect(
        within(panel).getByTestId(`my-review-in-round-${HUNK_KEY}`),
      ).toHaveTextContent("In round 2 · rejected"),
    );
    await userEvent.click(
      within(panel).getByTestId(`my-review-discard-${HUNK_KEY}`),
    );
    await waitFor(() => expect(bodies).toEqual([{ keys: [HUNK_KEY] }]));
  });

  it("keeps a rejected proposal discardable after the source moved on, from the carried row", async () => {
    // The owner committed something unrelated: the "in round" markers are
    // gone, but the rejected row is still carried and its live status says
    // rejected, so the chip and the discard controls stay.
    seed({
      published: { hunk: true },
      sourceHeads: { "repo-api": "b".repeat(40) },
      publications: [publication({ id: "me-1", round: 2 })],
      suggestions: [
        suggestion({ id: "s-hunk", publicationId: "me-1", status: "rejected" }),
      ],
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await waitFor(() =>
      expect(
        within(panel).getByTestId(`my-review-in-round-${HUNK_KEY}`),
      ).toHaveTextContent("In round 2 · rejected"),
    );
    expect(
      within(panel).getByTestId(`my-review-discard-${HUNK_KEY}`),
    ).toBeInTheDocument();
    expect(
      within(panel).getByTestId("my-review-discard-rejected"),
    ).toBeInTheDocument();
  });

  it("asks before discarding a change whose file has unsaved edits in an editor, then drops them", async () => {
    const bodies: unknown[] = [];
    const dropped: string[] = [];
    resetDirtyEditorsForTests();
    setEditorDirty({}, "src/main.rs", () => dropped.push("src/main.rs"));
    seed({
      published: { hunk: true, status: "rejected" },
      publications: [publication({ id: "me-1", round: 2 })],
      sourceHeads: { "repo-api": "a".repeat(40) },
    });
    server.use(
      http.post(
        `*/api/v1/workspaces/${REVIEW_ID}/review/changes/discard`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            data: {
              discarded: [
                {
                  key: HUNK_KEY,
                  repoId: "repo-api",
                  path: "src/main.rs",
                  kind: "hunk",
                },
              ],
              skipped: [],
            },
          });
        },
      ),
    );
    try {
      renderPanel();
      const panel = await screen.findByTestId("my-review-panel");
      await userEvent.click(
        await within(panel).findByTestId(`my-review-discard-${HUNK_KEY}`),
      );
      const confirm = await within(panel).findByTestId(
        "my-review-discard-unsaved-confirm",
      );
      expect(confirm).toHaveTextContent(
        "You have unsaved edits in src/main.rs; discard them too?",
      );
      // Nothing was posted yet.
      expect(bodies).toEqual([]);
      await userEvent.click(
        within(confirm).getByTestId("my-review-discard-unsaved-go"),
      );
      await waitFor(() => expect(bodies).toEqual([{ keys: [HUNK_KEY] }]));
      // The editor's edits were dropped after the rewrite, so it reloads.
      await waitFor(() => expect(dropped).toEqual(["src/main.rs"]));
      expect(
        within(panel).queryByTestId("my-review-discard-unsaved-confirm"),
      ).toBeNull();
    } finally {
      resetDirtyEditorsForTests();
    }
  });

  it("lets the reviewer discard a rejected change from the copy, one at a time", async () => {
    const bodies: unknown[] = [];
    seed({
      published: { hunk: true, newFile: true, status: "rejected" },
      publications: [publication({ id: "me-1", round: 2 })],
      sourceHeads: { "repo-api": "a".repeat(40) },
    });
    server.use(
      http.post(
        `*/api/v1/workspaces/${REVIEW_ID}/review/changes/discard`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            data: {
              discarded: [
                {
                  key: HUNK_KEY,
                  repoId: "repo-api",
                  path: "src/main.rs",
                  kind: "hunk",
                },
              ],
              skipped: [],
            },
          });
        },
      ),
    );
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    expect(
      await within(panel).findByTestId(`my-review-in-round-${HUNK_KEY}`),
    ).toHaveTextContent("In round 2 · rejected");
    await userEvent.click(
      within(panel).getByTestId(`my-review-discard-${HUNK_KEY}`),
    );
    await waitFor(() => expect(bodies).toEqual([{ keys: [HUNK_KEY] }]));
    expect(
      await within(panel).findByTestId("my-review-discard-outcome"),
    ).toHaveTextContent("Discarded 1 change");
  });

  it("keeps the Discard confirmation off, and sends nothing, when the copy stops while it is open (g5.workspace.ui.unavailable)", async () => {
    let posted = 0;
    seed({
      published: {
        hunk: true,
        newFile: true,
        artifact: true,
        status: "rejected",
      },
      publications: [publication({ id: "me-1", round: 2 })],
      sourceHeads: { "repo-api": "a".repeat(40) },
    });
    server.use(
      http.post(
        `*/api/v1/workspaces/${REVIEW_ID}/review/changes/discard`,
        () => {
          posted += 1;
          return HttpResponse.json({ data: { discarded: [], skipped: [] } });
        },
      ),
    );
    const gate = (kind: "live" | "stopped"): WorkspaceLiveGate => ({
      kind,
      canWrite: true,
      live: kind === "live",
      canStart: kind === "stopped",
      reviewCopy: true,
      reviewIdentityKnown: true,
      owner: true,
      workspaceNoun: "your review workspace",
    });
    function Host() {
      const [kind, setKind] = React.useState<"live" | "stopped">("live");
      return (
        <WorkspaceLiveGateProvider value={gate(kind)}>
          <button
            type="button"
            data-testid="stop-copy"
            onClick={() => setKind("stopped")}
          >
            stop
          </button>
          <MyReviewPanel
            review={review}
            viewerUserId={ME}
            org="g5labs"
            canPush
          />
        </WorkspaceLiveGateProvider>
      );
    }
    renderApp(<Host />);
    const panel = await screen.findByTestId("my-review-panel");
    await userEvent.click(
      await within(panel).findByTestId("my-review-discard-rejected"),
    );
    const go = within(panel).getByTestId("my-review-discard-rejected-go");
    expect(go).toBeEnabled();
    await userEvent.click(screen.getByTestId("stop-copy"));
    expect(go).toBeDisabled();
    expect(go).toHaveAttribute(
      "title",
      "Start your review workspace to discard changes.",
    );
    await userEvent.click(go);
    expect(posted).toBe(0);
  });

  it("discards every rejected change of the round at once, after a confirmation", async () => {
    const bodies: unknown[] = [];
    seed({
      published: {
        hunk: true,
        newFile: true,
        artifact: true,
        status: "rejected",
      },
      publications: [publication({ id: "me-1", round: 2 })],
      sourceHeads: { "repo-api": "a".repeat(40) },
    });
    server.use(
      http.post(
        `*/api/v1/workspaces/${REVIEW_ID}/review/changes/discard`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            data: {
              discarded: [
                {
                  key: HUNK_KEY,
                  repoId: "repo-api",
                  path: "src/main.rs",
                  kind: "hunk",
                },
                {
                  key: NEW_KEY,
                  repoId: "repo-api",
                  path: "NOTES.md",
                  kind: "new_file",
                },
              ],
              skipped: [{ key: ART_KEY, reason: "already gone" }],
            },
          });
        },
      ),
    );
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await userEvent.click(
      await within(panel).findByTestId("my-review-discard-rejected"),
    );
    const confirm = within(panel).getByTestId(
      "my-review-discard-rejected-confirm",
    );
    expect(confirm).toHaveTextContent(
      "Reverts 3 rejected changes in your copy; nothing on kaihui-test3 changes.",
    );
    await userEvent.click(
      within(confirm).getByTestId("my-review-discard-rejected-go"),
    );
    await waitFor(() =>
      expect(bodies).toEqual([{ keys: [HUNK_KEY, NEW_KEY, ART_KEY] }]),
    );
    const outcome = await within(panel).findByTestId(
      "my-review-discard-outcome",
    );
    expect(outcome).toHaveTextContent("Discarded 2 changes");
    expect(outcome).toHaveTextContent("Skipped: mock.png: already gone");
  });

  it("opens a later change at its copy-side line, shifted by the hunks before it", async () => {
    seed();
    // The first hunk turns one line into three (at source line 3); the
    // second sits at source line 10, so in the copy it starts at 12.
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () =>
        HttpResponse.json({
          data: {
            ...preview(),
            files: [
              {
                repoId: "repo-api",
                path: "src/main.rs",
                kind: "hunk",
                hunks: 2,
                bytes: null,
                changes: [
                  {
                    key: HUNK_KEY,
                    kind: "hunk",
                    anchorStart: 3,
                    beforeLines: "return 1;\n",
                    afterLines: "log();\nlog();\nreturn 0;\n",
                  },
                  {
                    key: "hunk-later",
                    kind: "hunk",
                    anchorStart: 10,
                    beforeLines: "old();\n",
                    afterLines: "new();\n",
                  },
                ],
              },
            ],
          },
        }),
      ),
    );
    const onOpenFile = vi.fn();
    renderPanel({ onOpenFile });
    const panel = await screen.findByTestId("my-review-panel");
    await userEvent.click(
      await within(panel).findByTestId("my-review-open-hunk-later"),
    );
    expect(onOpenFile).toHaveBeenCalledWith("src/main.rs", "repo-api", 12);
    await userEvent.click(
      within(panel).getByTestId(`my-review-open-${HUNK_KEY}`),
    );
    expect(onOpenFile).toHaveBeenLastCalledWith("src/main.rs", "repo-api", 3);
  });

  it("shows the viewer's own comments and last round with the owner's decisions", async () => {
    seed();
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    const mine = await within(panel).findByTestId("my-review-comment-c-mine");
    expect(mine).toHaveTextContent("README.md:12");
    expect(mine).toHaveTextContent("Is this still accurate?");
    expect(
      within(panel).queryByTestId("my-review-comment-c-sam"),
    ).not.toBeInTheDocument();
    const round = within(panel).getByTestId("my-review-last-round");
    expect(
      within(round).getByTestId("my-review-last-round-counts"),
    ).toHaveTextContent("Round 2 · 1 accepted · 1 rejected · 1 open");
    expect(
      within(round).getByTestId("my-review-round-suggestion-s-acc"),
    ).toHaveTextContent("Accepted");
    expect(
      within(round).getByTestId("my-review-round-suggestion-s-rej"),
    ).toHaveTextContent("Rejected");
    expect(
      within(round).queryByTestId("my-review-round-suggestion-s-sam"),
    ).not.toBeInTheDocument();
    expect(
      within(round).getAllByRole("link", { name: "Open on kaihui-test3" })[0],
    ).toHaveAttribute("href", "/g5labs/kaihui-test3?view=reviews");
  });

  it("labels a kept artifact of the last round as Kept", async () => {
    seed({
      suggestions: [
        ...suggestions,
        suggestion({
          id: "s-kept",
          kind: "artifact",
          path: "mock.png",
          beforeLines: null,
          afterLines: null,
          anchorStart: null,
          status: "accepted",
        }),
      ],
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    const round = await within(panel).findByTestId("my-review-last-round");
    expect(
      within(round).getByTestId("my-review-round-suggestion-s-kept"),
    ).toHaveTextContent("Kept");
    expect(
      within(round).getByTestId("my-review-round-suggestion-s-acc"),
    ).toHaveTextContent("Accepted");
  });

  it("marks the last round outdated once the source moved past the commit it reviewed, and says the source moved", async () => {
    seed({
      publications: [
        {
          ...publications[0],
          baseCommitVector: [
            { repoId: "repo-api", sourceHead: "a".repeat(40) },
          ],
        },
        ...publications.slice(1),
      ],
      sourceHeads: { "repo-api": "b".repeat(40) },
      advanced: true,
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    expect(
      await within(panel).findByTestId("my-review-last-round-outdated"),
    ).toHaveTextContent("reviewed an earlier commit of kaihui-test3");
    expect(
      await within(panel).findByTestId("my-review-source-advanced"),
    ).toHaveTextContent("kaihui-test3 has a new commit since your copy");
    // The round's open rows are not "removed": the preview marks nothing as
    // in the round because the source moved on, and a push retires nothing.
    expect(
      within(panel).getByTestId("my-review-summary"),
    ).not.toHaveTextContent("removed");
  });

  it("says the source moved on from its current tips when the preview predates the commit", async () => {
    // The preview's merge base is the tip it saw; the source's current tip
    // (with the rounds, refreshed on the review notification) is newer.
    seed({ sourceHeads: { "repo-api": "b".repeat(40) } });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    expect(
      await within(panel).findByTestId("my-review-source-advanced"),
    ).toHaveTextContent("kaihui-test3 has a new commit since your copy");
  });

  it("marks changes the current round already carries and counts only the new ones", async () => {
    seed({ published: { hunk: true, artifact: true, round: 3 } });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(
      within(panel).getByTestId(`my-review-in-round-${HUNK_KEY}`),
    ).toHaveTextContent("In round 3");
    expect(
      within(panel).queryByTestId(`my-review-in-round-${NEW_KEY}`),
    ).toBeNull();
    expect(
      within(panel).getByTestId("my-review-in-round-artifact-mock.png"),
    ).toHaveTextContent("In round 3");
    // The heading counts new changes among changed files: none here.
    expect(
      within(panel).getByText("Changes to push").parentElement,
    ).toHaveTextContent("· 0");
    expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
      "2 of 2 changes will be pushed · 1 new · 2 already in round 3 · 0 notes",
    );
    // The hunk stays included: a push carries it over unchanged.
    expect(
      within(panel).getByTestId(`my-review-include-${HUNK_KEY}`),
    ).toBeChecked();
    expect(within(panel).queryByTestId("my-review-nothing-new")).toBeNull();
    expect(within(panel).getByTestId("my-review-push")).toBeEnabled();
  });

  it("says nothing is new when every change is already in the round, and still offers Push", async () => {
    seed({
      published: { hunk: true, newFile: true, artifact: true },
      suggestions: [],
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(
      within(panel).getByTestId("my-review-nothing-new"),
    ).toHaveTextContent(
      "Nothing new since round 2; pushing again only updates your verdict or summary.",
    );
    // Push stays available: the dialog can still change the verdict or
    // the summary.
    expect(within(panel).getByTestId("my-review-push")).toBeEnabled();
    expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
      "0 new · 3 already in round 2",
    );
  });

  it("drops stale markers once the source's live tips moved past the preview's", async () => {
    // The preview (with markers) predates the owner's commit; the rounds'
    // live tips say the source moved: a push publishes against the new
    // commit, so nothing here is "in round N" any more.
    seed({
      published: { hunk: true, newFile: true, artifact: true },
      sourceHeads: { "repo-api": "b".repeat(40) },
      suggestions: [],
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(
      within(panel).queryByTestId(`my-review-in-round-${HUNK_KEY}`),
    ).toBeNull();
    expect(
      within(panel).queryByTestId("my-review-in-round-artifact-mock.png"),
    ).toBeNull();
    expect(
      within(panel).getByText("Changes to push").parentElement,
    ).toHaveTextContent("· 1");
    expect(within(panel).queryByTestId("my-review-nothing-new")).toBeNull();
    expect(
      within(panel).getByTestId("my-review-summary"),
    ).not.toHaveTextContent("already in round");
  });

  it("shows the note a change was pushed with, and emptying it clears it", async () => {
    seed({ published: { hunk: true, carriedNote: "Shout here" } });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    const note = await within(panel).findByTestId(`my-review-note-${HUNK_KEY}`);
    await waitFor(() => expect(note).toHaveValue("Shout here"));
    expect(
      within(panel).getByTestId(`my-review-note-carried-${HUNK_KEY}`),
    ).toHaveTextContent("Carried from round 2");
    // No draft was written by showing it.
    expect(readReviewDrafts(REVIEW_ID).changeNotes).toEqual({});
    await userEvent.clear(note);
    await waitFor(() =>
      expect(readReviewDrafts(REVIEW_ID).changeNotes).toEqual({
        [HUNK_KEY]: "",
      }),
    );
    expect(note).toHaveValue("");
    expect(
      within(panel).queryByTestId(`my-review-note-carried-${HUNK_KEY}`),
    ).toBeNull();
    expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
      "1 note changed",
    );
  });

  it("shows and clears a carried note even after the source moved on", async () => {
    // The markers are stale (the live tips moved past the preview's), so
    // nothing is "in round N"; a push would still carry the note, and the
    // editor shows it so it can be cleared.
    seed({
      published: { hunk: true, carriedNote: "Shout here" },
      sourceHeads: { "repo-api": "b".repeat(40) },
      suggestions: [],
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    const note = await within(panel).findByTestId(`my-review-note-${HUNK_KEY}`);
    await waitFor(() => expect(note).toHaveValue("Shout here"));
    expect(
      within(panel).queryByTestId(`my-review-in-round-${HUNK_KEY}`),
    ).toBeNull();
    expect(
      within(panel).getByTestId(`my-review-note-carried-${HUNK_KEY}`),
    ).toHaveTextContent("Carried from round 2");
    await userEvent.clear(note);
    await waitFor(() =>
      expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
        "1 note changed",
      ),
    );
    expect(readReviewDrafts(REVIEW_ID).changeNotes).toEqual({
      [HUNK_KEY]: "",
    });
  });

  it("says nothing about newness while the round's rows are still loading", async () => {
    seed({
      published: { hunk: true, newFile: true, artifact: true },
      suggestionsPending: true,
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(within(panel).queryByTestId("my-review-nothing-new")).toBeNull();
    expect(within(panel).getByTestId("my-review-push")).toBeEnabled();
  });

  it("says nothing about newness when the preview speaks of another round than the publications", async () => {
    seed({
      published: { hunk: true, newFile: true, artifact: true, round: 3 },
      suggestions: [],
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(within(panel).queryByTestId("my-review-nothing-new")).toBeNull();
  });

  it("has something to push when a note differs from the one the round carries", async () => {
    writeReviewDrafts(REVIEW_ID, {
      changeNotes: { [HUNK_KEY]: "Please double-check the return value." },
      excludedChanges: [],
      fileNotes: {},
      excludedFiles: [],
    });
    seed({
      published: {
        hunk: true,
        newFile: true,
        artifact: true,
        round: 3,
        carriedNote: "Carried note",
      },
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(within(panel).queryByTestId("my-review-nothing-new")).toBeNull();
    expect(within(panel).getByTestId("my-review-push")).toBeEnabled();
    expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
      "0 new · 3 already in round 3 · 1 note changed",
    );
  });

  it("has something to push when a published change is held back", async () => {
    writeReviewDrafts(REVIEW_ID, {
      changeNotes: {},
      excludedChanges: [HUNK_KEY],
      fileNotes: {},
      excludedFiles: [],
    });
    seed({
      published: { hunk: true, newFile: true, artifact: true, round: 3 },
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(within(panel).queryByTestId("my-review-nothing-new")).toBeNull();
    expect(within(panel).getByTestId("my-review-push")).toBeEnabled();
    expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
      "0 new · 3 already in round 3 · 1 held back",
    );
  });

  it("has something to push when the copy reverted a proposal of the round", async () => {
    seed({
      published: { hunk: true, newFile: true, artifact: true, round: 3 },
      suggestions: [
        // The round's hunk the preview still lists (referenced by key).
        suggestion({ id: "s-hunk", publicationId: "me-2" }),
        // A second hunk of the round the copy no longer carries.
        suggestion({ id: "s-gone", publicationId: "me-2", path: "src/lib.rs" }),
        // Decided rows are not "removed": nothing to retire.
        suggestion({
          id: "s-done",
          publicationId: "me-2",
          path: "src/done.rs",
          status: "accepted",
        }),
      ],
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(within(panel).queryByTestId("my-review-nothing-new")).toBeNull();
    expect(within(panel).getByTestId("my-review-push")).toBeEnabled();
    expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
      "· 1 removed",
    );
  });

  it("says nothing is new when the notes match the round and nothing is held back", async () => {
    writeReviewDrafts(REVIEW_ID, {
      changeNotes: { [HUNK_KEY]: "Same as pushed." },
      excludedChanges: [],
      fileNotes: {},
      excludedFiles: [],
    });
    seed({
      published: {
        hunk: true,
        newFile: true,
        artifact: true,
        carriedNote: "  Same as pushed. ",
      },
      suggestions: [],
    });
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    await within(panel).findByTestId("my-review-nothing-new");
    expect(within(panel).getByTestId("my-review-push")).toBeEnabled();
  });

  it("says nothing about the source while the copy sits on its latest commit", async () => {
    seed();
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(within(panel).queryByTestId("my-review-source-advanced")).toBeNull();
  });

  it("leaves one repository's file out without touching the same path elsewhere", async () => {
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () =>
        HttpResponse.json({
          data: {
            ...preview(),
            files: [
              {
                repoId: "repo-api",
                path: "README.md",
                kind: "hunk",
                hunks: 1,
                bytes: null,
                changes: [
                  {
                    key: HUNK_KEY,
                    kind: "hunk",
                    anchorStart: 1,
                    beforeLines: "a\n",
                    afterLines: "b\n",
                  },
                ],
              },
              {
                repoId: "repo-docs",
                path: "README.md",
                kind: "hunk",
                hunks: 1,
                bytes: null,
                changes: [
                  {
                    key: NEW_KEY,
                    kind: "hunk",
                    anchorStart: 1,
                    beforeLines: "c\n",
                    afterLines: "d\n",
                  },
                ],
              },
            ],
          },
        }),
      ),
    );
    renderPanel();
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    await userEvent.click(
      within(panel).getByTestId("my-review-include-file-repo-docs-README.md"),
    );
    await waitFor(() =>
      expect(within(panel).getByTestId("my-review-summary")).toHaveTextContent(
        "1 of 2 changes will be pushed",
      ),
    );
    expect(
      within(panel).getByTestId("my-review-include-file-repo-api-README.md"),
    ).toBeChecked();
    expect(readReviewDrafts(REVIEW_ID).excludedFiles).toEqual([
      { repoId: "repo-docs", path: "README.md" },
    ]);
  });

  it("shows a reader whose copy it is, no preview, and their rounds in neutral words", async () => {
    const previewCalls = { count: 0 };
    seed({ previewCalls });
    renderPanel({ canPush: false, viewerUserId: OTHER });
    const panel = await screen.findByTestId("my-review-panel");
    expect(within(panel).getByTestId("my-review-not-owner")).toHaveTextContent(
      "This is Alex Chen's copy; only they can push.",
    );
    expect(within(panel).queryByTestId("my-review-body")).toBeNull();
    expect(within(panel).queryByTestId("my-review-push")).toBeNull();
    expect(panel).not.toHaveTextContent("Saving your copy");
    expect(panel).toHaveTextContent("Their last round");
    expect(panel).toHaveTextContent("Their comments on kaihui-test3");
    expect(previewCalls.count).toBe(0);
    // The round and comments shown are the reviewer's (Alex's), not the
    // reader's own (Sam has a round and a comment on this source too).
    const round = within(panel).getByTestId("my-review-last-round");
    expect(
      await within(round).findByTestId("my-review-round-suggestion-s-acc"),
    ).toBeInTheDocument();
    expect(round).toHaveTextContent("Round 2");
    expect(
      within(round).queryByTestId("my-review-round-suggestion-s-sam"),
    ).toBeNull();
    expect(
      within(panel).getByTestId("my-review-comment-c-mine"),
    ).toBeInTheDocument();
    expect(within(panel).queryByTestId("my-review-comment-c-sam")).toBeNull();
  });

  it("takes a fresh preview after a push, so the chips say what the round now carries", async () => {
    const previewCalls = { count: 0 };
    seed({ previewCalls, sourceHeads: { "repo-api": "a".repeat(40) } });
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/publish`, () =>
        HttpResponse.json({
          data: {
            publication: publication({ id: "me-3", round: 3 }),
            findings: [],
            suggestions: [],
            skipped: [],
          },
        }),
      ),
    );
    renderApp(
      <>
        <Publisher />
        <MyReviewPanel review={review} viewerUserId={ME} org="g5labs" canPush />
      </>,
    );
    const panel = await screen.findByTestId("my-review-panel");
    await within(panel).findByTestId(`my-review-change-${HUNK_KEY}`);
    expect(previewCalls.count).toBe(1);
    // Nothing is in a round before the push.
    expect(
      within(panel).queryByTestId(`my-review-in-round-${HUNK_KEY}`),
    ).toBeNull();
    // The next preview (after the push) reports the hunk in round 3.
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () => {
        previewCalls.count += 1;
        return HttpResponse.json({
          data: preview(undefined, false, { hunk: true, round: 3 }),
        });
      }),
    );
    await userEvent.click(screen.getByTestId("publisher"));
    await waitFor(() => expect(previewCalls.count).toBe(2));
    expect(
      await within(panel).findByTestId(`my-review-in-round-${HUNK_KEY}`),
    ).toHaveTextContent("In round 3 · open");
  });

  it("reads an accepted change as plainly accepted: a reviewer cannot see the source's tree", async () => {
    seed({ sourceHeads: { "repo-api": "a".repeat(40) } });
    renderPanel();
    const round = await screen.findByTestId("my-review-last-round");
    expect(
      await within(round).findByTestId("my-review-round-suggestion-s-acc"),
    ).toHaveTextContent(/^src\/main\.rschangeAcceptedOpen on kaihui-test3$/);
  });

  it("reads a copy that is not ready as a hint with a retry", async () => {
    let calls = 0;
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () => {
        calls += 1;
        if (calls === 1) {
          return HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message:
                  "your review workspace is not ready to save changes; try again in a moment",
              },
            },
            { status: 409 },
          );
        }
        return HttpResponse.json({
          data: preview({ workspace: "stopped", committed: false }),
        });
      }),
    );
    renderPanel();
    const error = await screen.findByTestId("my-review-error");
    expect(error).toHaveTextContent("not ready to save changes");
    await userEvent.click(screen.getByTestId("my-review-retry"));
    expect(await screen.findByTestId("my-review-saved")).toHaveTextContent(
      "Your copy is stopped: this is what it saved last.",
    );
  });
});
