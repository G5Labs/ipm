import type { ReviewSuggestionEdit } from "../lib/types";

/** A server-shaped captured delta; test callers explicitly choose its scope. */
export function reviewEditFixture(
  id: string,
  source: string,
  start: number,
  end: number,
  replacement: string,
  overrides: Partial<ReviewSuggestionEdit> = {},
): ReviewSuggestionEdit {
  const bytes = (text: string) => new TextEncoder().encode(text).length;
  return {
    id,
    start: bytes(source.slice(0, start)),
    end: bytes(source.slice(0, end)),
    beforeText: source.slice(start, end),
    afterText: replacement,
    acceptedAfterText: null,
    status: "open",
    statusBy: null,
    statusAt: null,
    commentId: null,
    checkpointPending: false,
    writtenHash: null,
    currentStart: bytes(source.slice(0, start)),
    currentEnd: bytes(source.slice(0, end)),
    expectedBeforeLines: source,
    expectedAfterLines:
      source.slice(0, start) + replacement + source.slice(end),
    ...overrides,
  };
}
