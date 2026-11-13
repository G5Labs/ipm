// Which of two views of one suggestion a kept card shows
// (`g5.governance.review-suggestions.display`).

import { describe, expect, it } from "vitest";
import { isLaterSuggestion } from "./suggestionRecords";
import type { ReviewSuggestion } from "./types";

const view = (
  status: ReviewSuggestion["status"],
  statusAt: string | null,
): ReviewSuggestion => ({
  id: "sug-1",
  publicationId: "pub-1",
  findingId: null,
  repoId: "repo-1",
  path: "src/app.ts",
  kind: "comment",
  beforeLines: "beta();\n",
  afterLines: null,
  anchorStart: 2,
  contentHash: null,
  anchorContext: null,
  authorKind: "human",
  authorLabel: "rita",
  note: "Is beta needed?",
  commentId: "thread-1",
  status,
  statusBy: null,
  statusAt,
  supersedes: null,
  adoptedFrom: null,
  createdAt: "2026-09-30T10:00:00Z",
});

describe("isLaterSuggestion", () => {
  it("lets any view replace an open one", () => {
    expect(isLaterSuggestion(view("open", null), view("open", null))).toBe(
      true,
    );
    expect(
      isLaterSuggestion(
        view("open", null),
        view("rejected", "2026-09-30T11:00:00Z"),
      ),
    ).toBe(true);
  });

  it("never lets an open view replace a decision, whatever time it carries", () => {
    const decided = view("rejected", "2026-09-30T11:00:00Z");
    expect(isLaterSuggestion(decided, view("open", null))).toBe(false);
    expect(
      isLaterSuggestion(decided, view("open", "2026-09-30T12:00:00Z")),
    ).toBe(false);
  });

  it("keeps the later of two decisions", () => {
    const rejected = view("rejected", "2026-09-30T11:00:00Z");
    const withdrawn = view("withdrawn", "2026-09-30T12:00:00Z");
    expect(isLaterSuggestion(rejected, withdrawn)).toBe(true);
    expect(isLaterSuggestion(withdrawn, rejected)).toBe(false);
  });

  it("compares decision times as instants, whatever their precision", () => {
    const later = view("withdrawn", "2026-09-30T11:00:00.5Z");
    const earlier = view("rejected", "2026-09-30T11:00:00Z");
    expect(isLaterSuggestion(later, earlier)).toBe(false);
    expect(isLaterSuggestion(earlier, later)).toBe(true);
  });
});
