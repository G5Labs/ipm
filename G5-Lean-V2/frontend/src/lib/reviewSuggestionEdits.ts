import type { ReviewSuggestion } from "./types";

export const REVIEW_EDIT_SOURCE_CONFLICT_NOTE =
  "This edit’s source needs attention after other accepted changes.";

export interface ReviewSourceDigest {
  content: string;
  digest: string;
}

/** A digest is usable only for the exact currently displayed raw text. */
export function boundReviewSourceDigest(
  selected: ReviewSourceDigest | undefined,
  content: string | null,
): string | undefined {
  return selected?.content === content ? selected.digest : undefined;
}

/** Select display coordinates without changing the owner's recorded outcome. */
export function reviewEditSourceBasis(
  suggestion: ReviewSuggestion,
  selectedRawDigest?: string,
) {
  const edit = suggestion.reviewEdit;
  if (!edit) return undefined;
  const captured = suggestion.parentSuggestionId
    ? suggestion.parentBeforeLines
    : suggestion.beforeLines;
  if (
    suggestion.status === "open" &&
    /^[0-9a-f]{64}$/.test(selectedRawDigest ?? "") &&
    selectedRawDigest === suggestion.contentHash &&
    captured !== null &&
    captured !== undefined
  ) {
    const start = utf8OffsetToUtf16(captured, edit.start);
    const end = utf8OffsetToUtf16(captured, edit.end);
    if (
      start !== undefined &&
      end !== undefined &&
      start <= end &&
      captured.slice(start, end) === edit.beforeText
    )
      return {
        source: captured,
        start: edit.start,
        end: edit.end,
        captured: true,
        sourceConflict: false,
      };
    return {
      source: captured,
      start: edit.start,
      end: edit.end,
      captured: false,
      sourceConflict: true,
    };
  }
  return {
    source: suggestion.beforeLines,
    start: edit.currentStart,
    end: edit.currentEnd,
    captured: false,
    sourceConflict: !!edit.sourceConflict,
  };
}

/** A carried unit can occur in several publication instances simultaneously. */
export const reviewEditDisplayId = (parentId: string, editId: string) =>
  `${parentId}:${editId}`;

/** Present the API's durable children without creating any decision identity. */
export function expandReviewSuggestionEdits(
  suggestions: readonly ReviewSuggestion[],
): ReviewSuggestion[] {
  return suggestions.flatMap((parent) => {
    if (parent.editId || parent.kind !== "hunk" || !parent.edits?.length)
      return [parent];
    const retired = ["superseded", "withdrawn", "obsolete"].includes(
      parent.status,
    );
    const parentOpenEditCount = retired
      ? 0
      : parent.edits.filter((edit) => edit.status === "open").length;
    return parent.edits.map((edit) => ({
      ...parent,
      id: reviewEditDisplayId(parent.id, edit.id),
      parentSuggestionId: parent.id,
      editId: edit.id,
      reviewEdit: edit,
      edits: undefined,
      beforeLines:
        parent.expectedBeforeLines ??
        edit.expectedBeforeLines ??
        parent.beforeLines,
      // The parent source is shared. A child's localized preview uses its raw
      // replacement, rather than materializing a whole source copy per edit.
      afterLines: edit.afterText,
      parentBeforeLines: parent.beforeLines,
      parentAfterLines: parent.afterLines,
      parentCommentId: parent.commentId,
      parentOpenEditCount,
      commentId: edit.commentId,
      status: retired && edit.status === "open" ? parent.status : edit.status,
      statusBy:
        retired && edit.status === "open" ? parent.statusBy : edit.statusBy,
      statusAt:
        retired && edit.status === "open" ? parent.statusAt : edit.statusAt,
    }));
  });
}

/** A decision response contains the parent ledger; retain the chosen card. */
export function selectedReviewSuggestion(
  parent: ReviewSuggestion,
  editId?: string,
): ReviewSuggestion {
  if (!editId) return parent;
  const selected = expandReviewSuggestionEdits([parent]).find(
    (row) => row.editId === editId,
  );
  if (!selected) throw new Error("The decision response omitted this edit.");
  return selected;
}

/** Parent preview markers summarize the durable children, never undo them. */
export function aggregateReviewEditStatus(statuses: readonly string[]): string {
  if (statuses.some((status) => status === "open")) return "open";
  if (statuses.length && statuses.every((status) => status === statuses[0]))
    return statuses[0];
  return "partial";
}

export function reviewSuggestionStatuses(
  suggestions: readonly Pick<
    ReviewSuggestion,
    "id" | "status" | "parentSuggestionId"
  >[],
): Map<string, string> {
  const statuses = new Map(suggestions.map((row) => [row.id, row.status]));
  const parents = new Map<string, string[]>();
  for (const row of suggestions) {
    if (row.parentSuggestionId)
      parents.set(row.parentSuggestionId, [
        ...(parents.get(row.parentSuggestionId) ?? []),
        row.status,
      ]);
  }
  for (const [parent, children] of parents)
    statuses.set(parent, aggregateReviewEditStatus(children));
  return statuses;
}

/** Convert only exact UTF-8 boundaries; never round a byte offset into a glyph. */
export function utf8OffsetToUtf16(
  text: string,
  offset: number,
): number | undefined {
  if (!Number.isInteger(offset) || offset < 0) return undefined;
  let bytes = 0,
    index = 0;
  for (const character of text) {
    if (bytes === offset) return index;
    const point = character.codePointAt(0)!;
    bytes += point < 0x80 ? 1 : point < 0x800 ? 2 : point < 0x10000 ? 3 : 4;
    index += character.length;
    if (bytes > offset) return undefined;
  }
  return bytes === offset ? index : undefined;
}
