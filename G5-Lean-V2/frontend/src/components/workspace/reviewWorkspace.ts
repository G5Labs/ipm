import type {
  ReviewBaseCommitEntry,
  ReviewCarried,
  ReviewPreview,
  ReviewPublishedIn,
  ReviewPreviewChange,
  ReviewPreviewFile,
  ReviewPublication,
  ReviewSuggestion,
  WorkspaceReview,
} from "../../lib/types";
import { VM_STATUS_LABEL } from "../../lib/workspaceVmControls";
import { workspacePagePath } from "../../lib/workspacePagePath";
import { isReviewFileExcluded, type ReviewDrafts } from "./reviewDrafts";
import { reviewSuggestionStatuses } from "../../lib/reviewSuggestionEdits";

// ---------------------------------------------------------------------------
// Which commit a publication is about
// ---------------------------------------------------------------------------

/**
 * The source heads a publication's base vector names, per repository:
 * the commit it reviewed (a round) or was made against (an annotation).
 * Anything that is not a well-formed vector reads as no heads at all.
 */
export function publicationSourceHeads(
  publication: Pick<ReviewPublication, "baseCommitVector">,
): Record<string, string> {
  const heads: Record<string, string> = {};
  const vector = publication.baseCommitVector;
  if (!Array.isArray(vector)) return heads;
  for (const entry of vector as Partial<ReviewBaseCommitEntry>[]) {
    if (
      entry &&
      typeof entry === "object" &&
      typeof entry.repoId === "string" &&
      typeof entry.sourceHead === "string" &&
      entry.sourceHead !== ""
    ) {
      heads[entry.repoId] = entry.sourceHead;
    }
  }
  return heads;
}

/**
 * Whether a publication is about the source's current commit: every
 * repository its vector names is still at that head. A vector naming no
 * repository (comments made before rounds recorded heads) counts as
 * current, and so does a repository the source no longer reports.
 */
export function isPublicationCurrent(
  publication: Pick<ReviewPublication, "baseCommitVector">,
  sourceHeads: Record<string, string> | null | undefined,
): boolean {
  const heads = publicationSourceHeads(publication);
  for (const [repoId, head] of Object.entries(heads)) {
    // Like the server: a repository the source no longer reports a head
    // for makes the publication outdated, so the Reviews page never shows
    // as current what the file pages hide.
    const current = sourceHeads?.[repoId];
    if (!current || current.toLowerCase() !== head.toLowerCase()) return false;
  }
  return true;
}

/** Accepted checkpoints do not retire the remaining durable child decisions. */
export function isPublicationCurrentWithPendingEdits(
  publication: Pick<
    ReviewPublication,
    "id" | "baseCommitVector" | "supersededBy"
  >,
  sourceHeads: Record<string, string> | null | undefined,
  suggestions: readonly Pick<
    ReviewSuggestion,
    "publicationId" | "editId" | "status"
  >[],
): boolean {
  return (
    isPublicationCurrent(publication, sourceHeads) ||
    (!publication.supersededBy &&
      suggestions.some(
        (suggestion) =>
          suggestion.publicationId === publication.id &&
          suggestion.editId &&
          suggestion.status === "open",
      ))
  );
}

/** A commit oid the way people read it: its first seven characters. */
export function shortOid(oid: string): string {
  return oid.slice(0, 7);
}

/**
 * A repository the way people read it: its name when the workspace's repo
 * list (or the Changes report's mount) knows one, else the short id.
 */
export function repoLabel(
  repoId: string,
  repoNames?: Record<string, string> | null,
): string {
  return repoNames?.[repoId] || repoId.slice(0, 8);
}

/** "code 04603f4 · docs 0ac5834": each head named by its repository. */
function headsLabel(
  heads: Record<string, string>,
  repoNames?: Record<string, string> | null,
): string {
  const parts = Object.entries(heads)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(
      ([repoId, head]) => `${repoLabel(repoId, repoNames)} ${shortOid(head)}`,
    );
  return parts.length === 0 ? "unknown commit" : parts.join(" · ");
}

/** The commit(s) a publication is about, short, for a heading. */
export function publicationCommitLabel(
  publication: Pick<ReviewPublication, "baseCommitVector">,
  repoNames?: Record<string, string> | null,
): string {
  return headsLabel(publicationSourceHeads(publication), repoNames);
}

/** The source's current commit(s), short, for the current group's heading. */
export function sourceHeadsLabel(
  sourceHeads: Record<string, string> | null | undefined,
  repoNames?: Record<string, string> | null,
): string {
  return headsLabel(sourceHeads ?? {}, repoNames);
}

/**
 * Groups publications by the commit they are about: the same heads, in
 * repository order. Empty for a publication that names no head.
 */
export function commitGroupKey(
  publication: Pick<ReviewPublication, "baseCommitVector">,
): string {
  return Object.entries(publicationSourceHeads(publication))
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([, head]) => shortOid(head))
    .join("-");
}

/**
 * Whether the source's branch moved past the copy's base: the preview names,
 * per repository, the source branch's tip in the mirror and where the copy
 * branched from it; a merge base behind the tip means the source moved on,
 * and so does a source tip (from the rounds query) past the one the
 * preview saw. (The copy's own Changes report cannot tell: its
 * remote-tracking ref is refreshed by Update, not while the VM runs.)
 */
export function sourceAdvanced(
  preview: ReviewPreview | null,
  sourceHeads?: Record<string, string> | null,
): boolean {
  if (!preview) return false;
  return (preview.baseCommitVector ?? []).some((entry) => {
    if (
      !!entry.mergeBase &&
      !!entry.sourceHead &&
      entry.mergeBase.toLowerCase() !== entry.sourceHead.toLowerCase()
    ) {
      return true;
    }
    // The preview is taken only when the reviewer acts (computing it saves
    // the copy), so it can predate the source's latest commit. The source's
    // current tips, read cheaply with its rounds, tell when it moved since.
    const current = sourceHeads?.[entry.repoId];
    return (
      !!current &&
      !!entry.sourceHead &&
      current.toLowerCase() !== entry.sourceHead.toLowerCase()
    );
  });
}

/**
 * How much of a preview is new: every change (each hunk, each new file) and
 * each artifact counts once; those the reviewer's current round already
 * carries (`publishedIn`) are not new. `round` is that round, or null when
 * nothing is in one.
 */
export interface PreviewNewness {
  total: number;
  inRound: number;
  fresh: number;
  /** New changes among changed files only (the "Changes to push" count). */
  freshChanged: number;
  round: number | null;
}

export function previewNewness(preview: ReviewPreview | null): PreviewNewness {
  const newness: PreviewNewness = {
    total: 0,
    inRound: 0,
    fresh: 0,
    freshChanged: 0,
    round: null,
  };
  if (!preview) return newness;
  const count = (
    published: ReviewPublishedIn | null | undefined,
    changed: boolean,
    units = 1,
  ) => {
    newness.total += units;
    if (published) {
      newness.inRound += units;
      newness.round = Math.max(newness.round ?? 0, published.round);
    } else {
      newness.fresh += units;
      if (changed) newness.freshChanged += units;
    }
  };
  for (const file of preview.files) {
    if (file.kind === "artifact") {
      count(file.publishedIn, false);
      continue;
    }
    for (const change of file.changes ?? []) {
      const units = change.editCount ?? 1;
      if (change.publishedIn)
        count(change.publishedIn, file.kind === "hunk", units);
      else {
        const carried = Math.min(
          units,
          Math.max(0, change.carriedEditCount ?? 0),
        );
        newness.total += carried;
        newness.inRound += carried;
        if (carried && change.carried)
          newness.round = Math.max(newness.round ?? 0, change.carried.round);
        count(null, file.kind === "hunk", units - carried);
      }
    }
  }
  return newness;
}

export function previewFileEditCount(file: ReviewPreviewFile): number {
  return file.changes?.length
    ? file.changes.reduce((count, change) => count + (change.editCount ?? 1), 0)
    : file.hunks;
}

/**
 * What the drafts would push beyond the changes themselves: a note that
 * differs from the one a push would carry (`change.carried.note`; a change
 * without a draft note is unchanged), and a published change or file the
 * drafts hold back (a withdrawal).
 * Artifacts count for neither: a push never retires an artifact row (an
 * unchanged one is reused as it is) and carries no note for it.
 */
export interface DraftNewness {
  notesChanged: number;
  heldBack: number;
}

export function draftNewness(
  preview: ReviewPreview | null,
  drafts: ReviewDrafts,
): DraftNewness {
  const newness: DraftNewness = { notesChanged: 0, heldBack: 0 };
  if (!preview) return newness;
  const excludedChanges = new Set(drafts.excludedChanges);
  for (const file of preview.files) {
    const fileExcluded = isReviewFileExcluded(drafts, file.repoId, file.path);
    if (file.kind === "artifact") continue;
    for (const change of file.changes ?? []) {
      if (fileExcluded || excludedChanges.has(change.key)) {
        newness.heldBack +=
          change.kind === "hunk" && change.carriedOpenEditCount !== undefined
            ? change.carriedOpenEditCount
            : change.publishedIn
              ? (change.editCount ?? 1)
              : 0;
      }
      // Only a note the reviewer wrote here is compared, against what a
      // push would carry (also after the source moved on): no draft means
      // the carried note stands; an explicit empty draft clears it.
      if (!change.carried) continue;
      const draft = drafts.changeNotes[change.key];
      if (draft === undefined) continue;
      if (draft.trim() !== change.carried.note.trim()) {
        newness.notesChanged += 1;
      }
    }
  }
  return newness;
}

/**
 * Open proposals of the reviewer's current round the copy no longer
 * carries (reverted since): a push of the reduced snapshot retires them,
 * which is a change of its own. `roundSuggestions` are the round's rows;
 * only open hunks and new files count: a push never retires an artifact.
 */
/**
 * The previewed items the owner rejected, as discard keys: a hunk's own
 * key, a new file's or an artifact's file-level key. Read from the preview
 * as shown, so stale markers name nothing.
 */
export function rejectedInRound(
  preview: ReviewPreview | null,
  liveStatuses: ReadonlyMap<string, string> | null = null,
): {
  keys: string[];
  count: number;
} {
  const keys: string[] = [];
  if (!preview) return { keys, count: 0 };
  for (const file of preview.files) {
    if (file.kind === "artifact") {
      if (decisionOf(file, liveStatuses)?.status === "rejected" && file.key) {
        keys.push(file.key);
      }
      continue;
    }
    for (const change of file.changes ?? []) {
      if (decisionOf(change, liveStatuses)?.status === "rejected") {
        keys.push(change.key);
      }
    }
  }
  return { keys, count: keys.length };
}

/** The round and decision an item shows: its marker, else what it carries. */
export interface ItemDecision {
  round: number;
  status: string;
}

/**
 * What the owner decided about a previewed item. The "in round N" marker
 * says so while the source sits on the commit the round reviewed; once the
 * source moved on the marker is gone, but a rejected row is still carried
 * (a push would inherit the rejection), so the row's live status, read from
 * the polled suggestions, still decides the chip and the discard.
 */
export function decisionOf(
  item: {
    publishedIn?: ReviewPublishedIn | null;
    carried?: ReviewCarried | null;
  },
  liveStatuses: ReadonlyMap<string, string> | null,
): ItemDecision | null {
  if (item.publishedIn) {
    return { round: item.publishedIn.round, status: item.publishedIn.status };
  }
  const carried = item.carried;
  if (!carried) return null;
  const live = liveStatuses?.get(carried.suggestionId);
  return live === "rejected" ? { round: carried.round, status: live } : null;
}

/** The path a preview key names, for reporting a skipped discard. */
export function previewPathForKey(
  preview: ReviewPreview | null,
  key: string,
): string | null {
  if (!preview) return null;
  for (const file of preview.files) {
    if (file.key === key) return file.path;
    if ((file.changes ?? []).some((change) => change.key === key)) {
      return file.path;
    }
  }
  return null;
}

/** "In round 3 · rejected": where a change already is, and how it fared. */
export function inRoundChip(published: {
  round: number;
  status: string;
}): string {
  return `In round ${published.round} · ${published.status}`;
}

export function removedFromRound(
  preview: ReviewPreview | null,
  roundSuggestions: readonly Pick<
    ReviewSuggestion,
    "id" | "kind" | "status" | "parentSuggestionId"
  >[],
): number {
  if (!preview) return 0;
  const referenced = new Set<string>();
  for (const file of preview.files) {
    if (file.publishedIn) referenced.add(file.publishedIn.suggestionId);
    for (const change of file.changes ?? []) {
      if (change.publishedIn) referenced.add(change.publishedIn.suggestionId);
    }
  }
  const absent = roundSuggestions.filter(
    (suggestion) =>
      suggestion.status === "open" &&
      (suggestion.kind === "hunk" || suggestion.kind === "new_file") &&
      !referenced.has(suggestion.parentSuggestionId ?? suggestion.id),
  ).length;
  const carriedPendingWithoutMarker = preview.files
    .flatMap((file) => file.changes ?? [])
    .reduce(
      (count, change) =>
        count +
        (!change.publishedIn
          ? Math.max(0, change.carriedOpenEditCount ?? 0)
          : 0),
      0,
    );
  return Math.max(0, absent - carriedPendingWithoutMarker);
}

/**
 * What the panel says when every previewed change is already in the round:
 * information, not a block, since a push can still change the verdict or
 * the summary.
 */
export function nothingNewMessage(round: number): string {
  return `Nothing new since round ${round}; pushing again only updates your verdict or summary.`;
}

/**
 * Whether the preview's "in round N" markers predate the source's current
 * commit: the preview records the source tips it compared against, and the
 * live tips (read with the rounds) can move past them while the preview,
 * which saves the copy, is not refetched. A push then publishes against the
 * new commit and carries nothing over, so the markers say nothing.
 */
export function previewMarkersStale(
  preview: ReviewPreview | null,
  sourceHeads?: Record<string, string> | null,
): boolean {
  if (!preview || !sourceHeads) return false;
  return (preview.baseCommitVector ?? []).some((entry) => {
    const current = sourceHeads[entry.repoId];
    return (
      !!current &&
      !!entry.sourceHead &&
      current.toLowerCase() !== entry.sourceHead.toLowerCase()
    );
  });
}

/**
 * The preview as the panel should read it: without stale markers. What a
 * push would carry (`carried`) does not depend on the source's commit and
 * stays.
 */
export function withoutStaleMarkers(
  preview: ReviewPreview,
  sourceHeads?: Record<string, string> | null,
  suggestions?:
    | readonly Pick<ReviewSuggestion, "parentSuggestionId" | "status">[]
    | null,
): ReviewPreview {
  if (!previewMarkersStale(preview, sourceHeads)) return preview;
  return {
    ...preview,
    files: preview.files.map((file) => ({
      ...file,
      publishedIn: null,
      changes: (file.changes ?? []).map((change) => ({
        ...change,
        publishedIn: suggestions?.some(
          (row) =>
            row.parentSuggestionId === change.publishedIn?.suggestionId &&
            row.status === "open",
        )
          ? change.publishedIn
          : null,
      })),
    })),
  };
}

/**
 * The preview is taken on an explicit action and then stands still, while
 * the source's suggestions are polled: the owner's decisions since the
 * preview show up there first. Every marker's status is replaced by the
 * polled row's when that row is loaded.
 */
export function withLiveStatuses(
  preview: ReviewPreview,
  suggestions:
    | readonly Pick<ReviewSuggestion, "id" | "status" | "parentSuggestionId">[]
    | null,
): ReviewPreview {
  if (!suggestions || suggestions.length === 0) return preview;
  const byId = reviewSuggestionStatuses(suggestions);
  const refresh = <T extends { publishedIn?: ReviewPublishedIn | null }>(
    item: T,
  ): T => {
    const marker = item.publishedIn;
    if (!marker) return item;
    const live = byId.get(marker.suggestionId);
    if (live === undefined || live === marker.status) return item;
    return { ...item, publishedIn: { ...marker, status: live } };
  };
  return {
    ...preview,
    files: preview.files.map((file) => ({
      ...refresh(file),
      changes: (file.changes ?? []).map(refresh),
    })),
  };
}

/** What a reviewer reads when the source moved on since their copy. */
export function sourceAdvancedMessage(sourceWorkspaceName: string): string {
  return `${sourceWorkspaceName} has a new commit since your copy; Update from ${sourceWorkspaceName} to review it. Your next push will start a new group under that commit.`;
}

/**
 * One-shot hand-off from the Approvals page to the review workspace page:
 * Start review navigates to the review workspace, and the workspace page
 * seeds the review prompt into a fresh chat composer exactly once. Session
 * storage survives the navigation and nothing else; a missing or blocked
 * store just means the reviewer seeds the prompt from the banner instead.
 */
const REVIEW_SEED_KEY_PREFIX = "g5.review-workspace.seed:";

export function markReviewPromptSeed(reviewWorkspaceId: string): void {
  try {
    sessionStorage.setItem(REVIEW_SEED_KEY_PREFIX + reviewWorkspaceId, "1");
  } catch {
    // Storage unavailable: the banner's "Run agent review" still works.
  }
}

/** Consume the seed marker for `reviewWorkspaceId`; true exactly once. */
export function takeReviewPromptSeed(reviewWorkspaceId: string): boolean {
  try {
    const key = REVIEW_SEED_KEY_PREFIX + reviewWorkspaceId;
    if (sessionStorage.getItem(key) === null) return false;
    sessionStorage.removeItem(key);
    return true;
  } catch {
    return false;
  }
}

/**
 * Start succeeds even when the review workspace's VM could not be resumed
 * (the fork exists; the API reports `resumeError`). The Approvals card stores
 * the reason so the review workspace page can surface it on arrival.
 */
const REVIEW_RESUME_ERROR_KEY_PREFIX = "g5.review-workspace.resume-error:";

export function markReviewResumeError(
  reviewWorkspaceId: string,
  message: string,
): void {
  try {
    sessionStorage.setItem(
      REVIEW_RESUME_ERROR_KEY_PREFIX + reviewWorkspaceId,
      message,
    );
  } catch {
    // Storage unavailable: the VM strip still shows the stopped state.
  }
}

/** Consume the stored resume error for `reviewWorkspaceId`, once. */
export function takeReviewResumeError(
  reviewWorkspaceId: string,
): string | null {
  try {
    const key = REVIEW_RESUME_ERROR_KEY_PREFIX + reviewWorkspaceId;
    const message = sessionStorage.getItem(key);
    if (message === null) return null;
    sessionStorage.removeItem(key);
    return message;
  } catch {
    return null;
  }
}

/** A 1-based inclusive line range of a file of the review copy. */
export interface ChangedLineRange {
  start: number;
  end: number;
}

function lineCount(text: string): number {
  if (text === "") return 0;
  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines.length;
}

/**
 * Where each change of a previewed file sits in the review copy. A hunk's
 * `anchorStart` is a line of the source's side of the diff; the copy's
 * side is what the editor shows, so each hunk is shifted by the lines the
 * hunks before it inserted or removed. A hunk spans its replacement (at
 * least one line, where a pure deletion happened); a new file spans all
 * of it.
 */
function fileChangeRanges(
  file: ReviewPreviewFile,
): Array<ChangedLineRange & { key: string }> {
  const ranges: Array<ChangedLineRange & { key: string }> = [];
  const hunks = [...(file.changes ?? [])].sort(
    (a, b) => (a.anchorStart ?? 1) - (b.anchorStart ?? 1),
  );
  let delta = 0;
  for (const change of hunks) {
    if (change.kind === "new_file") {
      ranges.push({
        key: change.key,
        start: 1,
        end: Math.max(1, lineCount(change.afterLines)),
      });
      continue;
    }
    const after = lineCount(change.afterLines);
    const before = lineCount(change.beforeLines ?? "");
    const start = Math.max(1, (change.anchorStart ?? 1) + delta);
    ranges.push({
      key: change.key,
      start,
      end: start + Math.max(1, after) - 1,
    });
    delta += after - before;
  }
  return ranges;
}

/**
 * The lines of one file of the review copy that the previewed round would
 * carry (see `fileChangeRanges`). Empty when the file is not in the preview.
 */
export function changedLineRanges(
  preview: ReviewPreview,
  repoId: string,
  path: string,
): ChangedLineRange[] {
  const file = preview.files.find(
    (entry) =>
      entry.path === path &&
      (!repoId || !entry.repoId || entry.repoId === repoId),
  );
  if (!file) return [];
  return fileChangeRanges(file).map(({ start, end }) => ({ start, end }));
}

/**
 * The line of the review copy where one previewed change starts: what
 * "Open in editor" opens, in the same coordinates the decorations mark.
 */
export function copySideLine(
  file: ReviewPreviewFile,
  change: ReviewPreviewChange,
): number {
  return (
    fileChangeRanges(file).find((range) => range.key === change.key)?.start ??
    change.anchorStart ??
    1
  );
}

/** Route of a workspace page; see `lib/workspacePagePath.ts`. */
export { workspacePagePath };

/** The source's Reviews panel (on its Approvals tab), where pushed reviews land. */
export function sourceReviewsPath(
  orgSlug: string,
  review: WorkspaceReview,
): string {
  return `${workspacePagePath(orgSlug, review.sourceWorkspaceName)}?view=reviews`;
}

/** Map a review workspace VM status onto the Chip state vocabulary. */
export function reviewVmChipState(
  vmStatus: string,
): "active" | "ok" | "info" | "warn" | "blocked" | "muted" {
  switch (vmStatus) {
    case "running":
      return "ok";
    case "starting":
    case "queued_for_capacity":
      return "info";
    case "stopping":
      return "warn";
    case "failed":
      return "blocked";
    default:
      return "muted";
  }
}

export function reviewVmStatusLabel(vmStatus: string): string {
  // A stopped or unknown VM reads as detached from the review's point of view.
  if (vmStatus === "stopped") return "VM detached";
  return VM_STATUS_LABEL[vmStatus] ?? "VM detached";
}

/**
 * What Finish review does for the copy's VM state
 * (`g5.governance.review-workspace.finish`): stop a live or starting copy,
 * go straight to the Remove offer for a stopped or failed one, and wait
 * while the state is unknown or the copy is already stopping.
 */
export function finishReviewStep(
  vmStatus: string | null,
): "stop" | "remove" | "stopping" | "unknown" {
  switch (vmStatus) {
    case "running":
    case "starting":
    case "queued_for_capacity":
      return "stop";
    case "stopped":
    case "failed":
      return "remove";
    case "stopping":
      return "stopping";
    default:
      return "unknown";
  }
}

/**
 * Whether a Stop response reports the copy as stopped now. The server
 * answers `stopped` for a copy that had already stopped and `stopping`
 * while the VM is still releasing; removal needs a stopped workspace.
 */
export function finishReviewStopped(outcome: { status: string }): boolean {
  return outcome.status === "stopped";
}

/**
 * Reviewer-written text quoted for the owner's agent: a named tag marks it
 * as data and a fence longer than any backtick run inside keeps it from
 * closing early, so nothing a reviewer wrote reads as an instruction.
 */
export function quotedForAgent(tag: string, text: string): string {
  let longest = 0;
  for (const run of text.matchAll(/`+/g)) {
    longest = Math.max(longest, run[0].length);
  }
  const fence = "`".repeat(Math.max(3, longest + 1));
  return `<${tag}>\n${fence}\n${text}\n${fence}\n</${tag}>`;
}

/**
 * The Review tab's own "Push review" button asks the banner (which owns the
 * push dialog and always sits above the editor of a copy) to open it: one
 * dialog, one place that handles the result, no prop thread through the
 * page's tab machinery.
 */
export const REVIEW_PUSH_EVENT = "g5:review-push";

/**
 * The owner's Reviews page asks the workspace page to start the VM (its
 * header's Start), the way the Review tab asks the banner to open Push.
 */
export const WORKSPACE_START_EVENT = "g5:workspace-start";

export function requestWorkspaceStart(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(WORKSPACE_START_EVENT));
}

export function requestReviewPush(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(REVIEW_PUSH_EVENT));
}
