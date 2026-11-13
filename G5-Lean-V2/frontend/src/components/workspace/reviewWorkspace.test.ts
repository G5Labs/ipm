import { describe, expect, it } from "vitest";

import type { ReviewPreview } from "../../lib/types";
import { reviewSuggestionStatuses } from "../../lib/reviewSuggestionEdits";
import { draftSummary, EMPTY_REVIEW_DRAFTS } from "./reviewDrafts";
import {
  changedLineRanges,
  copySideLine,
  draftNewness,
  nothingNewMessage,
  previewMarkersStale,
  previewNewness,
  removedFromRound,
  withoutStaleMarkers,
  publicationCommitLabel,
  quotedForAgent,
  sourceAdvanced,
  sourceHeadsLabel,
  inRoundChip,
  decisionOf,
  previewPathForKey,
  rejectedInRound,
  withLiveStatuses,
  finishReviewStep,
  finishReviewStopped,
} from "./reviewWorkspace";

describe("finishReviewStep (g5.governance.review-workspace.finish)", () => {
  it("stops a live or starting copy and offers removal for a stopped one", () => {
    expect(finishReviewStep("running")).toBe("stop");
    expect(finishReviewStep("starting")).toBe("stop");
    expect(finishReviewStep("queued_for_capacity")).toBe("stop");
    expect(finishReviewStep("stopped")).toBe("remove");
    expect(finishReviewStep("failed")).toBe("remove");
  });

  it("waits while the copy is stopping or its state is not known", () => {
    expect(finishReviewStep("stopping")).toBe("stopping");
    expect(finishReviewStep(null)).toBe("unknown");
    expect(finishReviewStep("something-new")).toBe("unknown");
  });

  it("treats only a stopped answer as complete: an accepted stop is still releasing", () => {
    expect(finishReviewStopped({ status: "stopped" })).toBe(true);
    expect(finishReviewStopped({ status: "stopping" })).toBe(false);
  });
});

describe("preview parent markers with durable child decisions", () => {
  it("counts a proven subset continuation without claiming a whole hunk marker or inherited note", () => {
    const partial: ReviewPreview = {
      files: [
        {
          repoId: "docs",
          path: "spec.md",
          kind: "hunk",
          hunks: 1,
          bytes: null,
          changes: [
            {
              key: "mixed",
              kind: "hunk",
              anchorStart: 1,
              beforeLines: "Alpha Omega",
              afterLines: "Beta Gamma new",
              editCount: 3,
              carriedEditCount: 1,
              carriedOpenEditCount: 1,
              publishedIn: null,
              carried: null,
            },
          ],
        },
      ],
      skipped: [],
    };
    expect(previewNewness(partial)).toEqual({
      total: 3,
      inRound: 1,
      fresh: 2,
      freshChanged: 2,
      round: null,
    });
    expect(
      draftNewness(partial, {
        ...EMPTY_REVIEW_DRAFTS,
        excludedChanges: ["mixed"],
      }),
    ).toEqual({ notesChanged: 0, heldBack: 1 });
    expect(partial.files[0].changes[0].publishedIn).toBeNull();
    expect(partial.files[0].changes[0].carried).toBeNull();
  });

  it("uses pending continuation proof for removals without subtracting settled carried decisions", () => {
    const mixed: ReviewPreview = {
      files: [
        {
          repoId: "docs",
          path: "spec.md",
          kind: "hunk",
          hunks: 1,
          bytes: null,
          changes: [
            {
              key: "mixed",
              kind: "hunk",
              anchorStart: 1,
              beforeLines: "before",
              afterLines: "after",
              editCount: 3,
              carriedEditCount: 2,
              carriedOpenEditCount: 1,
              publishedIn: null,
            },
          ],
        },
      ],
      skipped: [],
    };
    expect(
      removedFromRound(mixed, [
        { id: "kept", parentSuggestionId: "old", kind: "hunk", status: "open" },
        {
          id: "dropped",
          parentSuggestionId: "old",
          kind: "hunk",
          status: "open",
        },
        {
          id: "settled",
          parentSuggestionId: "old",
          kind: "hunk",
          status: "rejected",
        },
      ]),
    ).toBe(1);
    expect(
      draftNewness(mixed, {
        ...EMPTY_REVIEW_DRAFTS,
        excludedChanges: ["mixed"],
      }).heldBack,
    ).toBe(1);
  });

  const preview: ReviewPreview = {
    files: [
      {
        repoId: "docs",
        path: "spec.md",
        kind: "hunk",
        hunks: 1,
        bytes: null,
        changes: [
          {
            key: "parent-key",
            editCount: 2,
            kind: "hunk",
            anchorStart: 1,
            beforeLines: "Alpha Omega.\n",
            afterLines: "Beta Gamma.\n",
            publishedIn: {
              publicationId: "pub",
              round: 1,
              suggestionId: "parent",
              status: "open",
            },
          },
        ],
      },
    ],
    skipped: [],
    baseCommitVector: [{ repoId: "docs", sourceHead: "before-accept" }],
  };
  const children = [
    { id: "accepted-child", parentSuggestionId: "parent", status: "accepted" },
    { id: "pending-child", parentSuggestionId: "parent", status: "open" },
  ];

  it("counts edits rather than Git groups and retains pending child markers across checkpoints", () => {
    expect(previewNewness(preview)).toEqual({
      total: 2,
      inRound: 2,
      fresh: 0,
      freshChanged: 0,
      round: 1,
    });
    const shown = withLiveStatuses(
      withoutStaleMarkers(preview, { docs: "after-accept" }, children),
      children,
    );
    expect(shown.files[0].changes[0].publishedIn).toEqual({
      publicationId: "pub",
      round: 1,
      suggestionId: "parent",
      status: "open",
    });
    expect(
      removedFromRound(
        shown,
        children.map((row) => ({ ...row, kind: "hunk" })),
      ),
    ).toBe(0);
  });

  it("summarizes mixed terminal children as partial without altering their decisions", () => {
    const mixed = [{ ...children[0] }, { ...children[1], status: "rejected" }];
    expect(reviewSuggestionStatuses(mixed).get("parent")).toBe("partial");
    const shown = withLiveStatuses(preview, mixed);
    expect(shown.files[0].changes[0].publishedIn?.status).toBe("partial");
    expect(mixed.map((row) => row.status)).toEqual(["accepted", "rejected"]);
  });

  it("counts authoritative edit units in grouped preview exclusion while retaining one publication key", () => {
    expect(draftSummary(preview, EMPTY_REVIEW_DRAFTS)).toEqual({
      included: 2,
      total: 2,
      notes: 0,
    });
    const drafts = { ...EMPTY_REVIEW_DRAFTS, excludedChanges: ["parent-key"] };
    expect(draftSummary(preview, drafts)).toEqual({
      included: 0,
      total: 2,
      notes: 0,
    });
    expect(draftNewness(preview, drafts).heldBack).toBe(2);
  });
});

describe("commit labels", () => {
  it("names each head by its repository, falling back to the short id", () => {
    const heads = {
      "repo-docs-0123456789": "0ac5834" + "f".repeat(33),
      "repo-code-0123456789": "04603f4" + "e".repeat(33),
    };
    expect(
      sourceHeadsLabel(heads, {
        "repo-code-0123456789": "code",
        "repo-docs-0123456789": "docs",
      }),
    ).toBe("code 04603f4 · docs 0ac5834");
    expect(sourceHeadsLabel(heads)).toBe("repo-cod 04603f4 · repo-doc 0ac5834");
    expect(sourceHeadsLabel({})).toBe("unknown commit");
    expect(
      publicationCommitLabel(
        {
          baseCommitVector: [
            {
              repoId: "repo-code-0123456789",
              sourceHead: heads["repo-code-0123456789"],
            },
          ],
        },
        { "repo-code-0123456789": "code" },
      ),
    ).toBe("code 04603f4");
  });
});

describe("changedLineRanges", () => {
  function preview(
    changes: NonNullable<ReviewPreview["files"][number]["changes"]>,
    kind = "hunk",
  ): ReviewPreview {
    return {
      files: [
        {
          repoId: "repo-api",
          path: "src/main.rs",
          kind,
          hunks: 0,
          bytes: null,
          changes,
        },
      ],
      skipped: [],
    };
  }

  it("shifts later hunks by the lines earlier hunks inserted or removed", () => {
    const ten = Array.from({ length: 10 }, (_, i) => `l${i}`).join("\n") + "\n";
    const ranges = changedLineRanges(
      preview([
        {
          key: "a",
          kind: "hunk",
          anchorStart: 5,
          beforeLines: null,
          afterLines: ten,
        },
        {
          key: "b",
          kind: "hunk",
          anchorStart: 30,
          beforeLines: "x\n",
          afterLines: "y\nz\n",
        },
        {
          key: "c",
          kind: "hunk",
          anchorStart: 50,
          beforeLines: "p\nq\nr\n",
          afterLines: "s\n",
        },
      ]),
      "repo-api",
      "src/main.rs",
    );
    expect(ranges).toEqual([
      { start: 5, end: 14 },
      { start: 40, end: 41 },
      { start: 61, end: 61 },
    ]);
  });

  it("marks one line where a hunk only deleted, and sorts hunks by anchor", () => {
    const ranges = changedLineRanges(
      preview([
        {
          key: "b",
          kind: "hunk",
          anchorStart: 20,
          beforeLines: "x\n",
          afterLines: "y\n",
        },
        {
          key: "a",
          kind: "hunk",
          anchorStart: 3,
          beforeLines: "a\nb\n",
          afterLines: "",
        },
      ]),
      "repo-api",
      "src/main.rs",
    );
    expect(ranges).toEqual([
      { start: 3, end: 3 },
      { start: 18, end: 18 },
    ]);
  });

  it("spans a whole new file and ignores files of other repositories", () => {
    const p = preview(
      [
        {
          key: "n",
          kind: "new_file",
          anchorStart: null,
          beforeLines: null,
          afterLines: "a\nb\nc\n",
        },
      ],
      "new_file",
    );
    expect(changedLineRanges(p, "repo-api", "src/main.rs")).toEqual([
      { start: 1, end: 3 },
    ]);
    expect(changedLineRanges(p, "repo-docs", "src/main.rs")).toEqual([]);
    expect(changedLineRanges(p, "repo-api", "other.rs")).toEqual([]);
  });
});

describe("previewNewness", () => {
  const published = {
    publicationId: "p",
    round: 4,
    suggestionId: "s",
    status: "open",
  };
  const change = (key: string, inRound: boolean) => ({
    key,
    kind: "hunk",
    anchorStart: 1,
    beforeLines: "a\n",
    afterLines: "b\n",
    publishedIn: inRound ? published : null,
  });

  it("counts each change and artifact once and tells the new from the pushed", () => {
    const preview = {
      files: [
        {
          repoId: "r",
          path: "a.rs",
          kind: "hunk",
          hunks: 2,
          bytes: null,
          changes: [change("k1", true), change("k2", false)],
        },
        {
          repoId: "r",
          path: "n.md",
          kind: "new_file",
          hunks: 0,
          bytes: 3,
          changes: [{ ...change("k3", true), kind: "new_file" }],
          publishedIn: published,
        },
        {
          repoId: "r",
          path: "m.png",
          kind: "artifact",
          hunks: 0,
          bytes: 9,
          changes: [],
          publishedIn: null,
        },
      ],
      skipped: [],
      baseCommitVector: [],
    } as unknown as ReviewPreview;
    expect(previewNewness(preview)).toEqual({
      total: 4,
      inRound: 2,
      fresh: 2,
      freshChanged: 1,
      round: 4,
    });
  });

  it("counts draft notes that differ from the round and published changes held back", () => {
    const preview = {
      files: [
        {
          repoId: "r",
          path: "a.rs",
          kind: "hunk",
          hunks: 2,
          bytes: null,
          changes: [
            {
              ...change("k1", true),
              publishedIn: { ...published, suggestionId: "s1" },
              carried: { suggestionId: "s1", round: 4, note: " same " },
            },
            {
              ...change("k2", true),
              publishedIn: { ...published, suggestionId: "s2" },
              carried: { suggestionId: "s2", round: 4, note: "carried note" },
            },
            change("k3", false),
          ],
        },
        {
          repoId: "r",
          path: "m.png",
          kind: "artifact",
          hunks: 0,
          bytes: 9,
          changes: [],
          publishedIn: published,
        },
      ],
      skipped: [],
      baseCommitVector: [],
    } as unknown as ReviewPreview;
    const drafts = {
      changeNotes: { k1: "same", k2: "new note", k3: "ignored: not pushed" },
      excludedChanges: ["k2"],
      fileNotes: {},
      excludedFiles: [{ repoId: "r", path: "m.png" }],
    };
    // The excluded artifact does not count: a push never retires one.
    expect(draftNewness(preview, drafts)).toEqual({
      notesChanged: 1,
      heldBack: 1,
    });
    // A carried note with no draft stands; an explicit empty draft clears it.
    expect(
      draftNewness(preview, {
        changeNotes: { k1: "same" },
        excludedChanges: [],
        fileNotes: {},
        excludedFiles: [],
      }),
    ).toEqual({ notesChanged: 0, heldBack: 0 });
    expect(
      draftNewness(preview, {
        changeNotes: { k2: "" },
        excludedChanges: [],
        fileNotes: {},
        excludedFiles: [],
      }),
    ).toEqual({ notesChanged: 1, heldBack: 0 });
    expect(draftNewness(null, drafts)).toEqual({
      notesChanged: 0,
      heldBack: 0,
    });
  });

  it("reads markers as stale once the live tips moved past the preview's", () => {
    const preview = {
      baseCommitVector: [{ repoId: "r", sourceHead: "a".repeat(40) }],
      files: [
        {
          repoId: "r",
          path: "a.rs",
          kind: "hunk",
          hunks: 1,
          bytes: null,
          changes: [change("k1", true)],
          publishedIn: null,
        },
        {
          repoId: "r",
          path: "m.png",
          kind: "artifact",
          hunks: 0,
          bytes: 9,
          changes: [],
          publishedIn: published,
        },
      ],
      skipped: [],
    } as unknown as ReviewPreview;
    expect(previewMarkersStale(preview, { r: "a".repeat(40) })).toBe(false);
    expect(previewMarkersStale(preview, { r: "A".repeat(40) })).toBe(false);
    expect(previewMarkersStale(preview, { other: "b".repeat(40) })).toBe(false);
    expect(previewMarkersStale(preview, null)).toBe(false);
    expect(previewMarkersStale(preview, { r: "b".repeat(40) })).toBe(true);
    expect(withoutStaleMarkers(preview, { r: "a".repeat(40) })).toBe(preview);
    const stripped = withoutStaleMarkers(preview, { r: "b".repeat(40) });
    expect(stripped.files[0].changes?.[0].publishedIn).toBeNull();
    expect(stripped.files[1].publishedIn).toBeNull();
    expect(previewNewness(stripped).inRound).toBe(0);
  });

  it("counts the round's open proposals the preview no longer carries as removed", () => {
    const preview = {
      files: [
        {
          repoId: "r",
          path: "a.rs",
          kind: "hunk",
          hunks: 1,
          bytes: null,
          changes: [
            {
              ...change("k1", true),
              publishedIn: { ...published, suggestionId: "s1" },
            },
          ],
        },
        {
          repoId: "r",
          path: "m.png",
          kind: "artifact",
          hunks: 0,
          bytes: 9,
          changes: [],
          publishedIn: { ...published, suggestionId: "s-art" },
        },
      ],
      skipped: [],
      baseCommitVector: [],
    } as unknown as ReviewPreview;
    const round = [
      { id: "s1", kind: "hunk", status: "open" },
      { id: "s2", kind: "hunk", status: "open" },
      { id: "s-art", kind: "artifact", status: "open" },
      { id: "s-new", kind: "new_file", status: "open" },
      { id: "s-acc", kind: "hunk", status: "accepted" },
      { id: "s-c", kind: "comment", status: "open" },
      // An artifact the preview no longer lists is not retired by a push.
      { id: "s-art-gone", kind: "artifact", status: "open" },
    ];
    expect(removedFromRound(preview, round)).toBe(2);
    expect(removedFromRound(null, round)).toBe(0);
    expect(removedFromRound(preview, [])).toBe(0);
  });

  it("has no round when nothing is pushed, and nothing at all without a preview", () => {
    expect(
      previewNewness({
        files: [
          {
            repoId: "r",
            path: "a.rs",
            kind: "hunk",
            hunks: 1,
            bytes: null,
            changes: [change("k1", false)],
          },
        ],
        skipped: [],
        baseCommitVector: [],
      } as unknown as ReviewPreview),
    ).toEqual({ total: 1, inRound: 0, fresh: 1, freshChanged: 1, round: null });
    expect(previewNewness(null).total).toBe(0);
    expect(nothingNewMessage(2)).toBe(
      "Nothing new since round 2; pushing again only updates your verdict or summary.",
    );
  });
});

describe("sourceAdvanced", () => {
  const base = { files: [], skipped: [] };

  it("is true when the copy's merge base sits behind the source head", () => {
    expect(
      sourceAdvanced({
        ...base,
        baseCommitVector: [
          {
            repoId: "repo-api",
            sourceHead: "B".repeat(40),
            mergeBase: "a".repeat(40),
          },
        ],
      }),
    ).toBe(true);
  });

  it("is false while every repository's merge base is the source head", () => {
    expect(
      sourceAdvanced({
        ...base,
        baseCommitVector: [
          {
            repoId: "repo-api",
            sourceHead: "A".repeat(40),
            mergeBase: "a".repeat(40),
          },
        ],
      }),
    ).toBe(false);
    expect(sourceAdvanced({ ...base, baseCommitVector: [] })).toBe(false);
    expect(sourceAdvanced(base)).toBe(false);
    expect(sourceAdvanced(null)).toBe(false);
    // An entry that names no head says nothing.
    expect(
      sourceAdvanced({
        ...base,
        baseCommitVector: [{ repoId: "repo-api", sourceHead: "a".repeat(40) }],
      }),
    ).toBe(false);
  });

  it("is true when the source's current tip moved past the one the preview saw", () => {
    const preview = {
      ...base,
      baseCommitVector: [
        {
          repoId: "repo-api",
          sourceHead: "a".repeat(40),
          mergeBase: "a".repeat(40),
        },
      ],
    };
    // The preview predates the owner's commit; the rounds query knows.
    expect(sourceAdvanced(preview, { "repo-api": "b".repeat(40) })).toBe(true);
    // Same tip, spelled in another case: nothing moved.
    expect(sourceAdvanced(preview, { "repo-api": "A".repeat(40) })).toBe(false);
    // No tip known for the repository, or no tips at all: the preview rules.
    expect(sourceAdvanced(preview, { "repo-docs": "b".repeat(40) })).toBe(
      false,
    );
    expect(sourceAdvanced(preview, {})).toBe(false);
    expect(sourceAdvanced(preview, null)).toBe(false);
  });
});

describe("copySideLine", () => {
  it("opens a change where the decorations mark it: shifted by earlier hunks", () => {
    const file = {
      repoId: "repo-api",
      path: "src/main.rs",
      kind: "hunk",
      hunks: 2,
      bytes: null,
      changes: [
        {
          key: "later",
          kind: "hunk",
          anchorStart: 30,
          beforeLines: "old();\n",
          afterLines: "new();\n",
        },
        {
          key: "first",
          kind: "hunk",
          anchorStart: 5,
          beforeLines: "",
          afterLines:
            Array.from({ length: 10 }, () => "inserted();").join("\n") + "\n",
        },
      ],
    };
    expect(copySideLine(file, file.changes[1])).toBe(5);
    expect(copySideLine(file, file.changes[0])).toBe(40);
    // A change the file does not list falls back to its own anchor.
    expect(
      copySideLine(file, {
        key: "elsewhere",
        kind: "hunk",
        anchorStart: 7,
        beforeLines: "a\n",
        afterLines: "b\n",
      }),
    ).toBe(7);
  });
});

describe("rejectedInRound / inRoundChip", () => {
  const published = (status: string) => ({
    publicationId: "p",
    round: 3,
    suggestionId: "s",
    status,
  });
  const preview = {
    baseCommitVector: [],
    skipped: [],
    files: [
      {
        repoId: "r",
        path: "a.rs",
        kind: "hunk",
        hunks: 2,
        bytes: null,
        changes: [
          {
            key: "k1",
            kind: "hunk",
            anchorStart: 1,
            beforeLines: "a",
            afterLines: "b",
            publishedIn: published("rejected"),
          },
          {
            key: "k2",
            kind: "hunk",
            anchorStart: 9,
            beforeLines: "a",
            afterLines: "b",
            publishedIn: published("open"),
          },
        ],
      },
      {
        repoId: "r",
        path: "n.md",
        kind: "new_file",
        hunks: 0,
        bytes: 3,
        changes: [
          {
            key: "k3",
            kind: "new_file",
            anchorStart: null,
            beforeLines: null,
            afterLines: "x",
            publishedIn: published("rejected"),
          },
        ],
        publishedIn: published("rejected"),
      },
      {
        repoId: "r",
        path: "m.png",
        kind: "artifact",
        hunks: 0,
        bytes: 9,
        changes: [],
        publishedIn: published("rejected"),
        key: "k4",
      },
      {
        repoId: "r",
        path: "k.png",
        kind: "artifact",
        hunks: 0,
        bytes: 9,
        changes: [],
        publishedIn: published("accepted"),
        key: "k5",
      },
    ],
  } as unknown as ReviewPreview;

  it("names rejected hunks by their key and rejected new files and artifacts by the file's", () => {
    expect(rejectedInRound(preview)).toEqual({
      keys: ["k1", "k3", "k4"],
      count: 3,
    });
    expect(rejectedInRound(null)).toEqual({ keys: [], count: 0 });
    expect(previewPathForKey(preview, "k4")).toBe("m.png");
    expect(previewPathForKey(preview, "k2")).toBe("a.rs");
    expect(previewPathForKey(preview, "nope")).toBeNull();
  });

  it("prefers the polled row's status over the preview's marker", () => {
    const withMarkers = {
      ...preview,
      files: preview.files.map((file) => ({
        ...file,
        publishedIn: file.publishedIn
          ? { ...file.publishedIn, suggestionId: `row-${file.path}` }
          : file.publishedIn,
        changes: (file.changes ?? []).map((change) => ({
          ...change,
          publishedIn: change.publishedIn
            ? { ...change.publishedIn, suggestionId: `row-${change.key}` }
            : change.publishedIn,
        })),
      })),
    } as ReviewPreview;
    const live = withLiveStatuses(withMarkers, [
      { id: "row-k2", status: "rejected" },
      { id: "row-m.png", status: "accepted" },
    ]);
    expect(live.files[0].changes[1].publishedIn?.status).toBe("rejected");
    expect(live.files[0].changes[0].publishedIn?.status).toBe("rejected");
    expect(live.files[2].publishedIn?.status).toBe("accepted");
    // Nothing loaded: the preview stands.
    expect(withLiveStatuses(withMarkers, null)).toBe(withMarkers);
  });

  it("reads a rejection from the carried row once the marker is gone", () => {
    const carried = { suggestionId: "row-x", round: 4, note: "" };
    const live = new Map([["row-x", "rejected"]]);
    expect(decisionOf({ publishedIn: null, carried }, live)).toEqual({
      round: 4,
      status: "rejected",
    });
    expect(
      decisionOf({ publishedIn: null, carried }, new Map([["row-x", "open"]])),
    ).toBeNull();
    expect(decisionOf({ publishedIn: null, carried }, null)).toBeNull();
    // A marker wins, whatever is carried.
    expect(
      decisionOf({ publishedIn: published("open"), carried }, live),
    ).toEqual({ round: 3, status: "open" });
    // rejectedInRound sees it too.
    const stale = {
      ...preview,
      files: preview.files.map((file) => ({
        ...file,
        publishedIn: null,
        carried: file.kind === "artifact" ? carried : null,
        changes: (file.changes ?? []).map((change) => ({
          ...change,
          publishedIn: null,
          carried: change.key === "k1" ? carried : null,
        })),
      })),
    } as ReviewPreview;
    expect(rejectedInRound(stale, live)).toEqual({
      keys: ["k1", "k4", "k5"],
      count: 3,
    });
    expect(rejectedInRound(stale)).toEqual({ keys: [], count: 0 });
  });

  it("says the round and the decision", () => {
    expect(inRoundChip(published("rejected"))).toBe("In round 3 · rejected");
    expect(inRoundChip(published("open"))).toBe("In round 3 · open");
  });
});
