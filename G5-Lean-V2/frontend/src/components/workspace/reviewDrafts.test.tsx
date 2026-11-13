/**
 * Review drafts are one store per review workspace: every mounted consumer
 * sees every write and clear, and a stale entry never outlives the preview
 * that no longer lists it.
 */

import React from "react";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ReviewPreview } from "../../lib/types";
import {
  clearReviewDrafts,
  draftPublishFields,
  hasReviewDrafts,
  pruneReviewDrafts,
  readReviewDrafts,
  useReviewDrafts,
  writeReviewDrafts,
} from "./reviewDrafts";

const ID = "ws-review-1";

const preview: ReviewPreview = {
  files: [
    {
      repoId: "repo-api",
      path: "src/main.rs",
      kind: "hunk",
      hunks: 1,
      bytes: null,
      changes: [
        {
          key: "k1",
          kind: "hunk",
          anchorStart: 3,
          beforeLines: "a\n",
          afterLines: "b\n",
        },
      ],
    },
  ],
  skipped: [],
};

function Consumer({ name }: { name: string }) {
  const drafts = useReviewDrafts(ID);
  return (
    <div>
      <span data-testid={`${name}-notes`}>
        {Object.keys(drafts.drafts.changeNotes).join(",")}
      </span>
      <span data-testid={`${name}-holds`}>
        {drafts.drafts.excludedChanges.join(",")}
      </span>
      <button type="button" onClick={() => drafts.setChangeNote("k1", "hi")}>
        {name} note
      </button>
      <button
        type="button"
        onClick={() => drafts.setChangeIncluded("k1", false)}
      >
        {name} hold
      </button>
      <button type="button" onClick={drafts.clear}>
        {name} clear
      </button>
      <button type="button" onClick={() => drafts.reconcile(preview)}>
        {name} reconcile
      </button>
    </div>
  );
}

describe("cleared notes", () => {
  const published = {
    publicationId: "p1",
    round: 1,
    suggestionId: "s1",
    status: "open",
  };
  const twoChanges = {
    ...preview,
    files: [
      {
        ...preview.files[0],
        changes: [
          {
            ...preview.files[0].changes![0],
            key: "k1",
            publishedIn: published,
          },
          { ...preview.files[0].changes![0], key: "k2", publishedIn: null },
        ],
      },
    ],
  } as ReviewPreview;

  it("sends an explicit empty note for every change the preview still lists", () => {
    const fields = draftPublishFields(
      {
        changeNotes: { k1: "", k2: "", k3: "" },
        excludedChanges: [],
        fileNotes: {},
        excludedFiles: [],
      },
      twoChanges,
    );
    // k2 is not "in a round" in this preview, yet a push may still carry a
    // note for it (the owner committed since): the clearing must reach it.
    expect(fields.changeNotes).toEqual({ k1: "", k2: "" });
  });

  it("keeps a cleared note through pruning, and it is a draft to clear", () => {
    const cleared = {
      changeNotes: { k1: "   " },
      excludedChanges: [],
      fileNotes: {},
      excludedFiles: [],
    };
    expect(pruneReviewDrafts(cleared, twoChanges).changeNotes).toEqual({
      k1: "",
    });
    // A pending deletion is discarded through Clear all drafts, so it must
    // count as a draft.
    expect(hasReviewDrafts(cleared)).toBe(true);
    expect(
      hasReviewDrafts({
        changeNotes: {},
        excludedChanges: [],
        fileNotes: {},
        excludedFiles: [],
      }),
    ).toBe(false);
  });
});

describe("review drafts store", () => {
  beforeEach(() => {
    localStorage.clear();
    clearReviewDrafts(ID);
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("keeps the memory copy authoritative when the browser refuses to store", async () => {
    localStorage.setItem(
      `g5.review-drafts:${ID}`,
      JSON.stringify({
        changeNotes: { k1: "old" },
        excludedChanges: [],
        fileNotes: {},
        excludedFiles: [],
      }),
    );
    render(<Consumer name="panel" />);
    expect(screen.getByTestId("panel-notes")).toHaveTextContent("k1");
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("QuotaExceededError");
    });
    await userEvent.click(screen.getByText("panel hold"));
    // The hold was refused by storage but is the state every consumer sees,
    // and the older stored value does not win it back on the next read.
    expect(screen.getByTestId("panel-holds")).toHaveTextContent("k1");
    expect(readReviewDrafts(ID).excludedChanges).toEqual(["k1"]);
    expect(readReviewDrafts(ID).changeNotes).toEqual({ k1: "old" });
    vi.restoreAllMocks();
    await userEvent.click(screen.getByText("panel note"));
    expect(readReviewDrafts(ID)).toEqual({
      changeNotes: { k1: "hi" },
      excludedChanges: ["k1"],
      fileNotes: {},
      excludedFiles: [],
    });
    expect(
      JSON.parse(localStorage.getItem(`g5.review-drafts:${ID}`) ?? "{}"),
    ).toEqual(readReviewDrafts(ID));
  });

  it("shares one state between every mounted consumer, including a clear", async () => {
    render(
      <>
        <Consumer name="panel" />
        <Consumer name="dialog" />
      </>,
    );
    await userEvent.click(screen.getByText("panel note"));
    await userEvent.click(screen.getByText("dialog hold"));
    expect(screen.getByTestId("panel-notes")).toHaveTextContent("k1");
    expect(screen.getByTestId("dialog-notes")).toHaveTextContent("k1");
    expect(screen.getByTestId("panel-holds")).toHaveTextContent("k1");
    expect(readReviewDrafts(ID).excludedChanges).toEqual(["k1"]);
    await userEvent.click(screen.getByText("dialog clear"));
    expect(screen.getByTestId("panel-notes")).toHaveTextContent("");
    expect(screen.getByTestId("panel-holds")).toHaveTextContent("");
    expect(screen.getByTestId("dialog-notes")).toHaveTextContent("");
    expect(readReviewDrafts(ID).changeNotes).toEqual({});
  });

  it("refreshes from a write made outside the hook and from another tab", async () => {
    render(<Consumer name="panel" />);
    act(() => {
      writeReviewDrafts(ID, {
        changeNotes: { k1: "from elsewhere" },
        excludedChanges: [],
        fileNotes: {},
        excludedFiles: [],
      });
    });
    expect(screen.getByTestId("panel-notes")).toHaveTextContent("k1");
    act(() => {
      localStorage.setItem(
        `g5.review-drafts:${ID}`,
        JSON.stringify({
          changeNotes: {},
          excludedChanges: ["k1"],
          fileNotes: {},
          excludedFiles: [],
        }),
      );
      window.dispatchEvent(
        new StorageEvent("storage", { key: `g5.review-drafts:${ID}` }),
      );
    });
    expect(screen.getByTestId("panel-notes")).toHaveTextContent("");
    expect(screen.getByTestId("panel-holds")).toHaveTextContent("k1");
  });

  it("prunes what the preview no longer lists, and never sends a stale key", async () => {
    writeReviewDrafts(ID, {
      changeNotes: { k1: "keep", k9: "stale" },
      excludedChanges: ["k8"],
      fileNotes: { "gone.md": "stale", "src/main.rs": "keep" },
      excludedFiles: [
        { repoId: "repo-api", path: "gone.md" },
        { repoId: "repo-other", path: "src/main.rs" },
      ],
    });
    expect(pruneReviewDrafts(readReviewDrafts(ID), preview)).toEqual({
      changeNotes: { k1: "keep" },
      excludedChanges: [],
      fileNotes: { "src/main.rs": "keep" },
      excludedFiles: [],
    });
    expect(draftPublishFields(readReviewDrafts(ID), preview)).toEqual({
      changeNotes: { k1: "keep" },
      excludedChanges: undefined,
      notes: { "src/main.rs": "keep" },
      excludedFiles: undefined,
    });
    render(<Consumer name="panel" />);
    await userEvent.click(screen.getByText("panel reconcile"));
    expect(readReviewDrafts(ID)).toEqual({
      changeNotes: { k1: "keep" },
      excludedChanges: [],
      fileNotes: { "src/main.rs": "keep" },
      excludedFiles: [],
    });
    expect(screen.getByTestId("panel-notes")).toHaveTextContent("k1");
  });
});
