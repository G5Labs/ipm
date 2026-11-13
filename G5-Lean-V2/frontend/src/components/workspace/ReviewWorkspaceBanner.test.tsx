/**
 * ReviewWorkspaceBanner — the identity strip of a review workspace, home of
 * the Push review dialog (opened from the Review tab's one Push review
 * button, through the review-push event) and of Update from the source.
 */

import {
  act,
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
import type { WorkspaceReview } from "../../lib/types";
import { ReviewWorkspaceBanner } from "./ReviewWorkspaceBanner";
import { readReviewDrafts, writeReviewDrafts } from "./reviewDrafts";
import { requestReviewPush } from "./reviewWorkspace";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";
import { useReviewPreview } from "../../hooks/useWorkspaceReview";

const REVIEW_ID = "ws-review-1";

const review: WorkspaceReview = {
  forkId: "fork-1",
  orgId: "org-g5labs",
  sourceWorkspaceId: "ws-source-1",
  sourceWorkspaceName: "kaihui-test3",
  reviewWorkspaceId: REVIEW_ID,
  reviewWorkspaceName: "kaihui-test3-review-alex-chen",
  reviewerUserId: "user-alex",
  reviewerDisplayName: "Alex Chen",
  vmStatus: "running",
  vmStartedAt: "2026-09-23T14:02:00.000Z",
  vmLastActivityAt: "2026-09-23T14:20:00.000Z",
  lastFlushedAt: null,
  createdAt: "2026-09-22T16:10:00.000Z",
};

function publication(overrides: Record<string, unknown> = {}) {
  return {
    id: "pub-1",
    orgId: "org-g5labs",
    sourceWorkspaceId: "ws-source-1",
    reviewWorkspaceId: REVIEW_ID,
    reviewerUserId: "user-alex",
    authorKind: "human",
    authorLabel: "alex",
    round: 1,
    baseCommitVector: [],
    summary: "Fixed the early return.",
    verdict: "approve",
    scores: null,
    supersededBy: null,
    createdAt: "2026-09-23T14:31:00.000Z",
    ...overrides,
  };
}

/** A preview whose base vector says whether the source moved past the copy. */
function previewAdvanced(advanced: boolean) {
  return http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () =>
    HttpResponse.json({
      data: {
        files: [],
        skipped: [],
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
        saved: { workspace: "live", committed: false },
      },
    }),
  );
}

/** Stands in for My review: keeps the shared preview query populated. */
function PreviewPrimer() {
  const preview = useReviewPreview(REVIEW_ID);
  return (
    <>
      <span data-testid="primer">{preview.status}</span>
      <button
        type="button"
        data-testid="primer-refresh"
        onClick={preview.retry}
      >
        refresh
      </button>
    </>
  );
}

describe("ReviewWorkspaceBanner", () => {
  it("says when the source moved past the copy, in the banner and the push dialog", async () => {
    server.use(previewAdvanced(true));
    renderApp(
      <>
        <PreviewPrimer />
        <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />
      </>,
    );
    // The banner reads the preview My review keeps fresh; it never starts one.
    expect(
      await screen.findByTestId("review-source-advanced"),
    ).toHaveTextContent(
      "kaihui-test3 has a new commit since your copy; Update from kaihui-test3 to review it. Your next push will start a new group under that commit.",
    );
    act(() => requestReviewPush());
    const dialog = await screen.findByTestId("push-review-dialog");
    expect(
      await within(dialog).findByTestId("push-review-source-advanced"),
    ).toHaveTextContent("has a new commit since your copy");
  });

  it("says the source moved on from its current tips, before any new preview is taken", async () => {
    // The preview saw the copy on the source's tip; then the owner committed.
    // The rounds query of the source refreshes on the review notification
    // and carries the newer tip, so the warning shows without a preview.
    server.use(
      previewAdvanced(false),
      http.get(
        `*/api/v1/workspaces/${review.sourceWorkspaceId}/review/publications`,
        () =>
          HttpResponse.json({
            data: {
              publications: [],
              findings: [],
              reviewerDisplayNames: {},
              sourceHeads: { "repo-api": "b".repeat(40) },
            },
          }),
      ),
    );
    renderApp(
      <>
        <PreviewPrimer />
        <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />
      </>,
    );
    expect(
      await screen.findByTestId("review-source-advanced"),
    ).toHaveTextContent("kaihui-test3 has a new commit since your copy");
  });

  it("says nothing about the source while the copy sits on its latest commit", async () => {
    server.use(previewAdvanced(false));
    renderApp(
      <>
        <PreviewPrimer />
        <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />
      </>,
    );
    await waitFor(() =>
      expect(screen.getByTestId("primer")).toHaveTextContent("ok"),
    );
    expect(screen.queryByTestId("review-source-advanced")).toBeNull();
    act(() => requestReviewPush());
    const dialog = await screen.findByTestId("push-review-dialog");
    await waitFor(() =>
      expect(within(dialog).getByTestId("push-review-confirm")).toBeEnabled(),
    );
    expect(
      within(dialog).queryByTestId("push-review-source-advanced"),
    ).toBeNull();
  });

  it("waits for a preview of its own before it can push, not one already in flight", async () => {
    let calls = 0;
    const releases: Array<() => void> = [];
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, async () => {
        calls += 1;
        // The first preview (My review's) answers at once and populates the
        // shared query; every later one waits until the test lets it
        // through, so an older request still in flight when the dialog
        // opens can be told from the dialog's own.
        if (calls > 1) {
          await new Promise<void>((resolve) => {
            releases.push(resolve);
          });
        }
        return HttpResponse.json({
          data: {
            files: [
              {
                repoId: "repo-api",
                path: "src/main.rs",
                kind: "hunk",
                hunks: 1,
                bytes: null,
                changes: [],
              },
            ],
            skipped: [],
            baseCommitVector: [],
            saved: { workspace: "live", committed: true },
          },
        });
      }),
    );
    renderApp(
      <>
        <PreviewPrimer />
        <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />
      </>,
    );
    await waitFor(() =>
      expect(screen.getByTestId("primer")).toHaveTextContent("ok"),
    );
    // My review starts another preview (a refresh) that is still in flight
    // when the dialog opens.
    await userEvent.click(screen.getByTestId("primer-refresh"));
    await waitFor(() => expect(calls).toBe(2));
    act(() => requestReviewPush());
    const dialog = await screen.findByTestId("push-review-dialog");
    // The cached preview shows, but Push waits for the dialog's own save:
    // the in-flight request is cancelled and a new one made.
    expect(within(dialog).getByTestId("push-review-preview")).toHaveTextContent(
      "src/main.rs",
    );
    await waitFor(() => expect(calls).toBe(3));
    expect(within(dialog).getByTestId("push-review-confirm")).toBeDisabled();
    expect(within(dialog).getByTestId("push-review-confirm")).toHaveTextContent(
      "Saving your changes…",
    );
    // Letting the older request answer changes nothing: it was cancelled.
    releases[0]?.();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(within(dialog).getByTestId("push-review-confirm")).toBeDisabled();
    releases[1]?.();
    await waitFor(() =>
      expect(within(dialog).getByTestId("push-review-confirm")).toBeEnabled(),
    );
    expect(within(dialog).getByTestId("push-review-confirm")).toHaveTextContent(
      "Push review",
    );
  });

  it("cancels a first preview still in flight when it opens and waits for its own", async () => {
    // No cached preview yet: My review's very first request is still
    // running when the dialog opens. TanStack's `cancelRefetch` alone would
    // reuse that request; the dialog cancels it by hand and makes its own.
    let calls = 0;
    const releases: Array<() => void> = [];
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, async () => {
        calls += 1;
        await new Promise<void>((resolve) => {
          releases.push(resolve);
        });
        return HttpResponse.json({
          data: {
            files: [
              {
                repoId: "repo-api",
                path: "src/main.rs",
                kind: "hunk",
                hunks: 1,
                bytes: null,
                changes: [],
              },
            ],
            skipped: [],
            baseCommitVector: [],
            saved: { workspace: "live", committed: true },
          },
        });
      }),
    );
    renderApp(
      <>
        <PreviewPrimer />
        <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />
      </>,
    );
    await waitFor(() => expect(calls).toBe(1));
    expect(screen.getByTestId("primer")).toHaveTextContent("loading");
    act(() => requestReviewPush());
    const dialog = await screen.findByTestId("push-review-dialog");
    // The dialog's own request, not the primer's, is what it waits for.
    await waitFor(() => expect(calls).toBe(2));
    expect(within(dialog).getByTestId("push-review-confirm")).toBeDisabled();
    releases[0]?.();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(within(dialog).getByTestId("push-review-confirm")).toBeDisabled();
    releases[1]?.();
    await waitFor(() =>
      expect(within(dialog).getByTestId("push-review-confirm")).toBeEnabled(),
    );
    expect(within(dialog).getByTestId("push-review-preview")).toHaveTextContent(
      "src/main.rs",
    );
  });

  it("prunes drafts against its own fresh preview only, never a cached one", async () => {
    // Another tab noted and held a change ("k-new") this tab's cached
    // preview does not list yet. The dialog must not prune against that
    // cached preview while its own is in flight; once the fresh preview
    // lists the change, the draft stays and travels with the push.
    const bodies: unknown[] = [];
    let calls = 0;
    const releases: Array<() => void> = [];
    const change = (key: string, line: number) => ({
      key,
      kind: "hunk",
      anchorStart: line,
      beforeLines: "a\n",
      afterLines: "b\n",
    });
    const previewOf = (keys: string[]) => ({
      files: [
        {
          repoId: "repo-api",
          path: "src/main.rs",
          kind: "hunk",
          hunks: keys.length,
          bytes: null,
          changes: keys.map((key, index) => change(key, (index + 1) * 10)),
        },
      ],
      skipped: [],
      baseCommitVector: [],
      saved: { workspace: "live", committed: true },
    });
    writeReviewDrafts(REVIEW_ID, {
      changeNotes: { "k-new": "From the other tab." },
      excludedChanges: ["k-new"],
      fileNotes: {},
      excludedFiles: [],
    });
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, async () => {
        calls += 1;
        if (calls === 1) {
          return HttpResponse.json({ data: previewOf(["k-old"]) });
        }
        await new Promise<void>((resolve) => {
          releases.push(resolve);
        });
        return HttpResponse.json({ data: previewOf(["k-old", "k-new"]) });
      }),
      http.post(
        `*/api/v1/workspaces/${REVIEW_ID}/review/publish`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json(
            {
              data: {
                publication: publication(),
                findings: [],
                suggestions: [],
                skipped: [],
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    renderApp(
      <>
        <PreviewPrimer />
        <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />
      </>,
    );
    await waitFor(() =>
      expect(screen.getByTestId("primer")).toHaveTextContent("ok"),
    );
    act(() => requestReviewPush());
    const dialog = await screen.findByTestId("push-review-dialog");
    // Mounting the dialog's observer refetches (cancelled at once) and the
    // dialog then makes its own request: both are held here.
    await waitFor(() => expect(calls).toBe(3));
    // The cached preview (k-old only) shows, and the dialog waits.
    expect(within(dialog).getByTestId("push-review-confirm")).toBeDisabled();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(readReviewDrafts(REVIEW_ID)).toEqual({
      changeNotes: { "k-new": "From the other tab." },
      excludedChanges: ["k-new"],
      fileNotes: {},
      excludedFiles: [],
    });
    releases.forEach((release) => release());
    await waitFor(() =>
      expect(within(dialog).getByTestId("push-review-confirm")).toBeEnabled(),
    );
    expect(readReviewDrafts(REVIEW_ID)).toEqual({
      changeNotes: { "k-new": "From the other tab." },
      excludedChanges: ["k-new"],
      fileNotes: {},
      excludedFiles: [],
    });
    expect(
      within(dialog).getByTestId("push-review-file-kind-src/main.rs"),
    ).toHaveTextContent("1 left out");
    await userEvent.click(within(dialog).getByTestId("push-review-confirm"));
    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0]).toEqual({
      verdict: "comment_only",
      changeNotes: { "k-new": "From the other tab." },
      excludedChanges: ["k-new"],
    });
  });

  it("prunes a draft only once its own fresh preview no longer lists the change", async () => {
    let calls = 0;
    const releases: Array<() => void> = [];
    const previewOf = (keys: string[]) => ({
      files: [
        {
          repoId: "repo-api",
          path: "src/main.rs",
          kind: "hunk",
          hunks: keys.length,
          bytes: null,
          changes: keys.map((key, index) => ({
            key,
            kind: "hunk",
            anchorStart: (index + 1) * 10,
            beforeLines: "a\n",
            afterLines: "b\n",
          })),
        },
      ],
      skipped: [],
      baseCommitVector: [],
      saved: { workspace: "live", committed: true },
    });
    writeReviewDrafts(REVIEW_ID, {
      changeNotes: { "k-new": "From the other tab." },
      excludedChanges: ["k-new"],
      fileNotes: {},
      excludedFiles: [],
    });
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, async () => {
        calls += 1;
        if (calls === 1) {
          return HttpResponse.json({ data: previewOf(["k-old"]) });
        }
        await new Promise<void>((resolve) => {
          releases.push(resolve);
        });
        // The fresh preview settles it: the change is gone.
        return HttpResponse.json({ data: previewOf(["k-old"]) });
      }),
    );
    renderApp(
      <>
        <PreviewPrimer />
        <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />
      </>,
    );
    await waitFor(() =>
      expect(screen.getByTestId("primer")).toHaveTextContent("ok"),
    );
    act(() => requestReviewPush());
    const dialog = await screen.findByTestId("push-review-dialog");
    await waitFor(() => expect(calls).toBe(3));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(readReviewDrafts(REVIEW_ID).changeNotes).toHaveProperty("k-new");
    releases.forEach((release) => release());
    await waitFor(() =>
      expect(within(dialog).getByTestId("push-review-confirm")).toBeEnabled(),
    );
    await waitFor(() =>
      expect(readReviewDrafts(REVIEW_ID)).toEqual({
        changeNotes: {},
        excludedChanges: [],
        fileNotes: {},
        excludedFiles: [],
      }),
    );
  });

  it("identifies the review workspace and its source", async () => {
    renderApp(
      <ReviewWorkspaceBanner
        review={review}
        canPush={false}
        onPushed={() => {}}
      />,
    );
    const banner = screen.getByTestId("workspace-review-banner");
    expect(
      within(banner).getByTestId("workspace-review-stripe"),
    ).toBeInTheDocument();
    expect(
      within(banner).getByTestId("workspace-review-badge"),
    ).toBeInTheDocument();
    // A reader sees whose copy it is; the owner reads "Your copy of".
    expect(banner).toHaveTextContent(
      "Alex Chen's copy of kaihui-test3; only they can push it as a review.",
    );
    // Only the reviewer (owner of the copy) can push, update or run the review.
    // The push event opens nothing for a reader: the dialog is the owner's.
    act(() => requestReviewPush());
    expect(screen.queryByTestId("push-review-dialog")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Update from/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Run agent review" }),
    ).not.toBeInTheDocument();
  });

  it("pushes the review with a verdict and summary and reports what was published", async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(
        `*/api/v1/workspaces/${REVIEW_ID}/review/publish`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json(
            {
              data: {
                publication: publication(),
                findings: [],
                suggestions: [
                  {
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
                    note: "",
                    commentId: "c-1",
                    status: "open",
                    statusBy: null,
                    statusAt: null,
                    supersedes: null,
                    adoptedFrom: null,
                    createdAt: "2026-09-23T14:31:00.000Z",
                  },
                ],
                skipped: [],
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    const onPushed = vi.fn();
    const onSeed = vi.fn();
    renderApp(
      <ReviewWorkspaceBanner
        review={review}
        canPush
        onSeedReviewPrompt={onSeed}
        onPushed={onPushed}
      />,
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Run agent review" }),
    );
    expect(onSeed).toHaveBeenCalledOnce();

    act(() => requestReviewPush());
    const dialog = await screen.findByTestId("push-review-dialog");
    expect(dialog).toHaveTextContent("Push review to kaihui-test3");
    expect(dialog).toHaveTextContent("Changes become suggestions.");
    await userEvent.selectOptions(
      within(dialog).getByTestId("push-review-verdict"),
      "approve",
    );
    await userEvent.type(
      within(dialog).getByLabelText(/Summary for the owner/),
      "Fixed the early return.",
    );
    await userEvent.click(within(dialog).getByTestId("push-review-confirm"));

    await waitFor(() => expect(onPushed).toHaveBeenCalledOnce());
    expect(bodies).toEqual([
      {
        summary: "Fixed the early return.",
        verdict: "approve",
      },
    ]);
    expect(onPushed.mock.calls[0][0].suggestions).toHaveLength(1);
    expect(screen.queryByTestId("push-review-dialog")).not.toBeInTheDocument();
  });

  it("lists the changed files: new text files are proposals, binaries artifacts", async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () =>
        HttpResponse.json({
          data: {
            files: [
              {
                repoId: "repo-api",
                path: "src/main.rs",
                kind: "hunk",
                hunks: 2,
                bytes: null,
              },
              {
                repoId: "repo-api",
                path: "NOTES.md",
                kind: "new_file",
                hunks: 0,
                bytes: 120,
              },
              {
                repoId: "repo-api",
                path: "mock.png",
                kind: "artifact",
                hunks: 0,
                bytes: 1200,
              },
            ],
            skipped: [
              {
                repoId: "repo-api",
                path: "big.bin",
                reason: "artifact too large",
              },
            ],
          },
        }),
      ),
      http.post(
        `*/api/v1/workspaces/${REVIEW_ID}/review/publish`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json(
            {
              data: {
                publication: publication(),
                findings: [],
                suggestions: [],
                skipped: [],
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    renderApp(
      <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />,
    );
    act(() => requestReviewPush());
    const preview = await screen.findByTestId("push-review-preview");
    expect(preview).toHaveTextContent("3 changed files");
    expect(preview).toHaveTextContent("src/main.rs");
    expect(preview).toHaveTextContent("2 changes");
    expect(preview).toHaveTextContent("big.bin (artifact too large)");
    // A new text file is proposed as a file: the owner decides whether it
    // enters the workspace. A binary is an artifact. The reviewer chooses
    // nothing here.
    expect(
      within(preview).getByTestId("push-review-file-kind-NOTES.md"),
    ).toHaveTextContent("new file · the owner decides");
    expect(
      within(preview).getByTestId("push-review-file-kind-mock.png"),
    ).toHaveTextContent("artifact");
    expect(preview.querySelector('input[type="checkbox"]')).toBeNull();
    await userEvent.click(screen.getByTestId("push-review-confirm"));
    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0]).not.toHaveProperty("repoNewFiles");
  });

  it("sends the notes and holds written in My review with the push, then clears them", async () => {
    const bodies: unknown[] = [];
    // A stale key (a change edited since it was noted or held) is neither
    // sent, which the server would refuse, nor kept.
    writeReviewDrafts(REVIEW_ID, {
      changeNotes: { k1: "Early return should succeed.", k9: "Edited since." },
      excludedChanges: ["k2", "k8"],
      fileNotes: {
        "NOTES.md": "Reading notes.",
        "gone.md": "Not in the round.",
      },
      excludedFiles: [
        { repoId: "repo-api", path: "mock.png" },
        { repoId: "repo-api", path: "gone.md" },
        { repoId: "repo-other", path: "src/main.rs" },
      ],
    });
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () =>
        HttpResponse.json({
          data: {
            files: [
              {
                repoId: "repo-api",
                path: "src/main.rs",
                kind: "hunk",
                hunks: 2,
                bytes: null,
                changes: [
                  {
                    key: "k1",
                    kind: "hunk",
                    anchorStart: 3,
                    beforeLines: "return 1;\n",
                    afterLines: "return 0;\n",
                  },
                  {
                    key: "k2",
                    kind: "hunk",
                    anchorStart: 9,
                    beforeLines: "x\n",
                    afterLines: "y\n",
                  },
                ],
              },
              {
                repoId: "repo-api",
                path: "NOTES.md",
                kind: "new_file",
                hunks: 0,
                bytes: 12,
                changes: [
                  {
                    key: "k3",
                    kind: "new_file",
                    anchorStart: null,
                    beforeLines: null,
                    afterLines: "# Notes\n",
                  },
                ],
              },
              {
                repoId: "repo-api",
                path: "mock.png",
                kind: "artifact",
                hunks: 0,
                bytes: 1200,
                changes: [],
              },
            ],
            skipped: [],
            saved: { workspace: "live", committed: true },
          },
        }),
      ),
      http.post(
        `*/api/v1/workspaces/${REVIEW_ID}/review/publish`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json(
            {
              data: {
                publication: publication(),
                findings: [],
                suggestions: [],
                skipped: [],
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    renderApp(
      <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />,
    );
    act(() => requestReviewPush());
    const preview = await screen.findByTestId("push-review-preview");
    expect(within(preview).getByTestId("push-review-drafts")).toHaveTextContent(
      "2 of 3 changes included, 1 note written in My review",
    );
    expect(
      within(preview).getByTestId("push-review-file-kind-src/main.rs"),
    ).toHaveTextContent("1 left out");
    expect(
      within(preview).getByTestId("push-review-file-kind-mock.png"),
    ).toHaveTextContent("left out");
    await userEvent.click(screen.getByTestId("push-review-confirm"));
    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0]).toEqual({
      verdict: "comment_only",
      changeNotes: { k1: "Early return should succeed." },
      excludedChanges: ["k2"],
      notes: { "NOTES.md": "Reading notes." },
      excludedFiles: [{ repoId: "repo-api", path: "mock.png" }],
    });
    expect(readReviewDrafts(REVIEW_ID).changeNotes).not.toHaveProperty("k9");
    expect(readReviewDrafts(REVIEW_ID)).toEqual({
      changeNotes: {},
      excludedChanges: [],
      fileNotes: {},
      excludedFiles: [],
    });
  });

  it("says when the review workspace is stopped, so the push sends what it saved", async () => {
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () =>
        HttpResponse.json({
          data: {
            files: [],
            skipped: [],
            saved: { workspace: "stopped", committed: false },
          },
        }),
      ),
    );
    renderApp(
      <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />,
    );
    act(() => requestReviewPush());
    expect(
      await screen.findByTestId("push-review-workspace-stopped"),
    ).toHaveTextContent("Your review workspace is stopped.");
    // Push itself is not blocked: the saved state is a valid round.
    await waitFor(() =>
      expect(screen.getByTestId("push-review-confirm")).toBeEnabled(),
    );
  });

  it("does not push until the changed-file preview loaded, and retries a failed one", async () => {
    // The save fails until the test lets it succeed: whichever request
    // reaches the server first (the query's own initial fetch, cancelled by
    // the dialog, or the dialog's), the dialog's preview fails until Retry.
    let failing = true;
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () => {
        if (failing) {
          return HttpResponse.json(
            { error: { code: "INTERNAL_ERROR", message: "boom" } },
            { status: 500 },
          );
        }
        return HttpResponse.json({
          data: {
            files: [
              {
                repoId: "repo-api",
                path: "src/new.rs",
                kind: "artifact",
                hunks: 0,
                bytes: 40,
              },
            ],
            skipped: [],
          },
        });
      }),
    );
    renderApp(
      <ReviewWorkspaceBanner
        review={review}
        canPush
        onPushed={() => {}}
        onUpdated={() => {}}
      />,
    );
    act(() => requestReviewPush());
    const error = await screen.findByTestId("push-review-preview-error");
    expect(error).toHaveTextContent("could not be saved and previewed");
    expect(screen.getByTestId("push-review-confirm")).toBeDisabled();
    failing = false;
    await userEvent.click(screen.getByTestId("push-review-preview-retry"));
    await screen.findByTestId("push-review-preview");
    expect(screen.queryByTestId("push-review-preview-error")).toBeNull();
    await waitFor(() =>
      expect(screen.getByTestId("push-review-confirm")).toBeEnabled(),
    );
    expect(
      screen.getByTestId("push-review-file-kind-src/new.rs"),
    ).toHaveTextContent("artifact");
  });

  it("updates the copy from the source inside the reviewer's VM and refreshes the preview", async () => {
    const actions: string[] = [];
    let previews = 0;
    server.use(
      http.post(
        `*/api/v1/workspaces/${REVIEW_ID}/changes/:action`,
        ({ params }) => {
          actions.push(params.action as string);
          return HttpResponse.json({
            data: {
              workspace_id: REVIEW_ID,
              source: "live",
              attached: true,
              members: [],
            },
          });
        },
      ),
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/preview`, () => {
        previews += 1;
        return HttpResponse.json({
          data: {
            files: [],
            skipped: [],
            baseCommitVector: [],
            saved: { workspace: "live", committed: false },
          },
        });
      }),
    );
    const onUpdated = vi.fn();
    renderApp(
      <>
        <PreviewPrimer />
        <ReviewWorkspaceBanner
          review={review}
          canPush
          onPushed={() => {}}
          onUpdated={onUpdated}
        />
      </>,
    );
    // My review's preview (the primer) described the copy before the update.
    await waitFor(() => expect(previews).toBe(1));
    await userEvent.click(
      screen.getByRole("button", { name: "Update from kaihui-test3" }),
    );
    await waitFor(() => expect(onUpdated).toHaveBeenCalledOnce());
    expect(actions).toEqual(["update"]);
    expect(onUpdated.mock.calls[0][0].ok).toBe(true);
    // The rebased copy is previewed again: the moved-on hint, My review
    // and the changed-line marks follow the new tree.
    await waitFor(() => expect(previews).toBe(2));
  });

  it("reports an update that the report marks failed instead of claiming success", async () => {
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/changes/:action`, () =>
        HttpResponse.json({
          data: {
            workspace_id: REVIEW_ID,
            source: "live",
            attached: true,
            status: "failed",
            members: [
              {
                repo_id: "repo-api",
                root_path: "api",
                dirty: [{ status: "M", path: "src/main.rs" }],
                errors: ["working tree has uncommitted changes"],
              },
            ],
          },
        }),
      ),
    );
    const onUpdated = vi.fn();
    renderApp(
      <ReviewWorkspaceBanner
        review={review}
        canPush
        onPushed={() => {}}
        onUpdated={onUpdated}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Update from kaihui-test3" }),
    );
    await waitFor(() => expect(onUpdated).toHaveBeenCalledOnce());
    expect(onUpdated.mock.calls[0][0]).toEqual({
      ok: false,
      message: "api: working tree has uncommitted changes",
    });
  });

  it("says what stood in the way of an update: the unmerged paths and git's words", async () => {
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/changes/:action`, () =>
        HttpResponse.json({
          data: {
            workspace_id: REVIEW_ID,
            source: "live",
            attached: true,
            status: "failed",
            members: [
              {
                repo_id: "repo-api",
                root_path: "api",
                dirty: [],
                errors: ["UPDATE_FAILED"],
                conflicts: ["README.md", "src/lib.rs"],
                detail:
                  "  CONFLICT (content): Merge conflict in README.md\nerror: could not apply 1a2b3c4... Review round  ",
              },
            ],
          },
        }),
      ),
    );
    const onUpdated = vi.fn();
    renderApp(
      <ReviewWorkspaceBanner
        review={review}
        canPush
        onPushed={() => {}}
        onUpdated={onUpdated}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Update from kaihui-test3" }),
    );
    await waitFor(() => expect(onUpdated).toHaveBeenCalledOnce());
    expect(onUpdated.mock.calls[0][0]).toEqual({
      ok: false,
      message:
        "api: Could not bring kaihui-test3's latest into your copy (could not merge: README.md, src/lib.rs) — CONFLICT (content): Merge conflict in README.md\nerror: could not apply 1a2b3c4... Review round",
    });
  });

  it("names the paths where the source's version was kept over the copy's commit", async () => {
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/changes/:action`, () =>
        HttpResponse.json({
          data: {
            workspace_id: REVIEW_ID,
            source: "live",
            attached: true,
            status: "succeeded",
            members: [
              {
                repo_id: "repo-api",
                root_path: "api",
                dirty: [],
                errors: [],
                updated: true,
                base_branch: "kaihui-test3",
                resolved: ["README.md"],
              },
              {
                repo_id: "repo-docs",
                root_path: "docs",
                dirty: [],
                errors: [],
                updated: true,
                base_branch: "kaihui-test3",
              },
            ],
          },
        }),
      ),
    );
    const onUpdated = vi.fn();
    renderApp(
      <ReviewWorkspaceBanner
        review={review}
        canPush
        onPushed={() => {}}
        onUpdated={onUpdated}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Update from kaihui-test3" }),
    );
    await waitFor(() => expect(onUpdated).toHaveBeenCalledOnce());
    expect(onUpdated.mock.calls[0][0]).toEqual({
      ok: true,
      message:
        "Picked up kaihui-test3's latest; kaihui-test3's version kept in: api/README.md.",
    });
  });

  it("offers a way back to the workspace this copy reviews, first in the row", async () => {
    const onBackToSource = vi.fn();
    renderApp(
      <ReviewWorkspaceBanner
        review={review}
        canPush
        onPushed={() => {}}
        onBackToSource={onBackToSource}
      />,
    );
    const back = screen.getByTestId("workspace-review-back-to-source");
    expect(back).toHaveTextContent("Back to kaihui-test3");
    expect(back).toHaveAttribute(
      "title",
      "Open kaihui-test3, the workspace this copy reviews",
    );
    // First control of the banner row.
    const row = back.parentElement as HTMLElement;
    expect(row.firstElementChild).toBe(back);
    await userEvent.click(back);
    expect(onBackToSource).toHaveBeenCalledOnce();
  });

  it("carries a Push review of its own on mobile only, where the Review tab is out of reach", async () => {
    renderApp(
      <ReviewWorkspaceBanner
        review={review}
        canPush
        onPushed={() => {}}
        mobilePush
      />,
    );
    await userEvent.click(screen.getByTestId("workspace-review-push-mobile"));
    expect(await screen.findByTestId("push-review-dialog")).toBeInTheDocument();
  });

  it("carries no Push review on desktop: the Review tab has the one button", () => {
    renderApp(
      <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />,
    );
    expect(
      screen.queryByTestId("workspace-review-push-mobile"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Push review" }),
    ).not.toBeInTheDocument();
  });

  it("keeps Update off while the copy is not running, and says why", async () => {
    renderApp(
      <ReviewWorkspaceBanner
        review={{ ...review, vmStatus: "stopped" }}
        canPush
        onPushed={() => {}}
      />,
    );
    const update = screen.getByRole("button", {
      name: "Update from kaihui-test3",
    });
    expect(update).toBeDisabled();
    expect(update).toHaveAttribute(
      "title",
      "Start your review workspace to update.",
    );
  });

  it("keeps Run agent review off while the copy is not running, and offers Start", async () => {
    renderApp(
      <ReviewWorkspaceBanner
        review={{ ...review, vmStatus: "stopped" }}
        canPush
        onSeedReviewPrompt={() => {}}
        onPushed={() => {}}
      />,
    );
    const seed = await screen.findByTestId("workspace-review-seed-prompt");
    expect(seed).toBeDisabled();
    expect(seed).toHaveAttribute(
      "title",
      "Start your review workspace to run the agent review.",
    );
  });

  it("follows the page's VM state over the review context when they disagree (g5.workspace.ui.vm-state)", async () => {
    renderApp(
      <WorkspaceLiveGateProvider
        value={{
          kind: "stopped",
          canWrite: true,
          live: false,
          canStart: true,
          reviewCopy: true,
          reviewIdentityKnown: true,
          owner: true,
          workspaceNoun: "your review workspace",
        }}
      >
        <ReviewWorkspaceBanner
          review={{ ...review, vmStatus: "running" }}
          canPush
          onSeedReviewPrompt={() => {}}
          onPushed={() => {}}
        />
      </WorkspaceLiveGateProvider>,
    );
    const update = await screen.findByRole("button", {
      name: "Update from kaihui-test3",
    });
    expect(update).toBeDisabled();
    expect(update).toHaveAttribute(
      "title",
      "Start your review workspace to update.",
    );
    expect(screen.getByTestId("workspace-review-seed-prompt")).toBeDisabled();
    // The reason and Start appear once the blocked Update is tried.
    expect(screen.queryByTestId("workspace-review-start-hint")).toBeNull();
    fireEvent.click(screen.getByTestId("workspace-review-start-hint-trigger"));
    expect(screen.getByTestId("workspace-review-start-hint")).toBeVisible();
  });

  it("offers Finish review only when wired, and calls it", async () => {
    const onFinishReview = vi.fn();
    const { unmount } = renderApp(
      <ReviewWorkspaceBanner
        review={review}
        canPush
        onPushed={() => {}}
        onFinishReview={onFinishReview}
      />,
    );
    fireEvent.click(await screen.findByTestId("workspace-review-finish"));
    expect(onFinishReview).toHaveBeenCalledTimes(1);
    unmount();
    renderApp(
      <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />,
    );
    await screen.findByTestId("workspace-review-banner");
    expect(screen.queryByTestId("workspace-review-finish")).toBeNull();
  });

  it("offers no way back when nothing is wired", async () => {
    renderApp(
      <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />,
    );
    await screen.findByTestId("workspace-review-banner");
    expect(
      screen.queryByTestId("workspace-review-back-to-source"),
    ).not.toBeInTheDocument();
  });

  it("reports a status-only failure with the generic refusal message", async () => {
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/changes/:action`, () =>
        HttpResponse.json({
          data: {
            workspace_id: REVIEW_ID,
            source: "live",
            attached: true,
            status: "failed",
            members: [
              { repo_id: "repo-api", root_path: "api", dirty: [], errors: [] },
            ],
          },
        }),
      ),
    );
    const onUpdated = vi.fn();
    renderApp(
      <ReviewWorkspaceBanner
        review={review}
        canPush
        onPushed={() => {}}
        onUpdated={onUpdated}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Update from kaihui-test3" }),
    );
    await waitFor(() => expect(onUpdated).toHaveBeenCalledOnce());
    expect(onUpdated.mock.calls[0][0]).toEqual({
      ok: false,
      message:
        "The update was refused; the review continues on its previous base.",
    });
  });

  it("says the list was refreshed on a stale key, then shows a later failure's own message", async () => {
    let calls = 0;
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/publish`, () => {
        calls += 1;
        return HttpResponse.json(
          {
            error: {
              code: calls === 1 ? "CONFLICT" : "VALIDATION_ERROR",
              message:
                calls === 1
                  ? "a change is no longer in the round: open Push review again"
                  : "nothing to publish: the review matches the source",
            },
          },
          { status: calls === 1 ? 409 : 422 },
        );
      }),
    );
    renderApp(
      <ReviewWorkspaceBanner review={review} canPush onPushed={() => {}} />,
    );
    act(() => requestReviewPush());
    const dialog = await screen.findByTestId("push-review-dialog");
    await waitFor(() =>
      expect(within(dialog).getByTestId("push-review-confirm")).toBeEnabled(),
    );
    await userEvent.click(within(dialog).getByTestId("push-review-confirm"));
    expect(await screen.findByTestId("push-review-error")).toHaveTextContent(
      "Your changes moved; the list was refreshed, push again.",
    );
    await waitFor(() =>
      expect(within(dialog).getByTestId("push-review-confirm")).toBeEnabled(),
    );
    await userEvent.click(within(dialog).getByTestId("push-review-confirm"));
    await waitFor(() =>
      expect(screen.getByTestId("push-review-error")).toHaveTextContent(
        "nothing to publish",
      ),
    );
  });

  it("keeps the dialog open with the error when pushing is refused", async () => {
    server.use(
      http.post(`*/api/v1/workspaces/${REVIEW_ID}/review/publish`, () =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "nothing to publish: the review matches the source",
            },
          },
          { status: 422 },
        ),
      ),
    );
    const onPushed = vi.fn();
    renderApp(
      <ReviewWorkspaceBanner review={review} canPush onPushed={onPushed} />,
    );

    act(() => requestReviewPush());
    const dialog = await screen.findByTestId("push-review-dialog");
    await userEvent.click(within(dialog).getByTestId("push-review-confirm"));

    expect(await screen.findByTestId("push-review-error")).toHaveTextContent(
      "nothing to publish",
    );
    expect(onPushed).not.toHaveBeenCalled();
    expect(screen.getByTestId("push-review-dialog")).toBeInTheDocument();
  });
});
