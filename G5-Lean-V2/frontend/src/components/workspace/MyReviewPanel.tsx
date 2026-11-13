// =============================================================================
// G5 Workspace — MyReviewPanel
// =============================================================================
// The reviewer's own review tool, on their review copy: every change the
// copy carries against the source (with the text, a note to write and a
// switch to hold a change back), the new files and artifacts the next push
// would send, the comments they left on the source, and how the owner
// handled their last round. The copy has no git step: a preview saves the
// copy and compares it; opening the tab, an Update, a push or a discard
// takes a fresh one. A proposal the owner rejected stays in the copy until
// the reviewer discards it from here. It is the body of the copy's Review
// tab and renders that page's header too.
// =============================================================================

import React, { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { AlertTriangle, Loader2 } from "lucide-react";
import { Button, Chip, classNames } from "../ui/UI";
import type { State } from "../ui/UI";
import {
  useDiscardReviewChanges,
  useReviewPreview,
  useReviewPublications,
  useReviewSuggestions,
} from "../../hooks/useWorkspaceReview";
import type {
  ReviewPreviewChange,
  ReviewPreviewFile,
  ReviewPublicationSummary,
  ReviewSuggestion,
  WorkspaceReview,
} from "../../lib/types";
import {
  draftSummary,
  hasReviewDrafts,
  isReviewFileExcluded,
  useReviewDrafts,
} from "./reviewDrafts";
import { dropDirtyEdits, useDirtyEditorPaths } from "../editor/dirtyEditors";
import { ReviewHunkDiff } from "./ReviewHunkDiff";
import { reviewSuggestionStatuses } from "../../lib/reviewSuggestionEdits";
import {
  liveGateReason,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import {
  copySideLine,
  draftNewness,
  decisionOf,
  type ItemDecision,
  inRoundChip,
  isPublicationCurrentWithPendingEdits,
  nothingNewMessage,
  previewNewness,
  previewFileEditCount,
  previewPathForKey,
  rejectedInRound,
  removedFromRound,
  requestReviewPush,
  withLiveStatuses,
  withoutStaleMarkers,
  sourceAdvanced,
  sourceAdvancedMessage,
  sourceReviewsPath,
} from "./reviewWorkspace";

export interface MyReviewPanelProps {
  review: WorkspaceReview;
  viewerUserId: string;
  /** Org slug, for links to the source's Reviews panel. */
  org: string;
  /** Only the reviewer (the copy's owner) previews and pushes. */
  canPush: boolean;
  /** Opens a file of this copy in the editor, at a line. */
  onOpenFile?: (filePath: string, repoRef?: string, line?: number) => void;
  /**
   * Kept for callers that rendered the panel under the banner; the panel is
   * the Review tab's page now and always renders in full.
   */
  embedded?: boolean;
}

function statusChip(status: string): { state: State; text: string } {
  switch (status) {
    case "accepted":
      return { state: "ok", text: "Accepted" };
    case "rejected":
      return { state: "blocked", text: "Rejected" };
    case "open":
      return { state: "warn", text: "Open" };
    case "obsolete":
      return { state: "muted", text: "Obsolete" };
    case "withdrawn":
      return { state: "muted", text: "Withdrawn" };
    case "superseded":
      return { state: "muted", text: "Superseded" };
    case "resolved":
      return { state: "ok", text: "Resolved" };
    default:
      return { state: "info", text: status };
  }
}

function verdictChip(verdict: string | null): { state: State; text: string } {
  switch (verdict) {
    case "approve":
      return { state: "ok", text: "Approved" };
    case "request_changes":
      return { state: "warn", text: "Changes requested" };
    case "comment_only":
      return { state: "muted", text: "Comments" };
    default:
      return { state: "muted", text: "No verdict" };
  }
}

function kindLabel(kind: string): string {
  switch (kind) {
    case "hunk":
      return "change";
    case "new_file":
      return "new file";
    case "artifact":
      return "artifact";
    case "comment":
      return "comment";
    default:
      return kind;
  }
}

function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** The viewer's own current human round on the source, if any. */
export function ownLastRound(
  publications: ReviewPublicationSummary[],
  viewerUserId: string,
): ReviewPublicationSummary | null {
  let latest: ReviewPublicationSummary | null = null;
  for (const publication of publications) {
    if (publication.reviewerUserId !== viewerUserId) continue;
    if (publication.authorKind !== "human") continue;
    if ((publication.kind ?? "round") !== "round") continue;
    if (!latest || publication.round > latest.round) latest = publication;
  }
  return latest;
}

// ---------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------

const muted = "text-xs text-[var(--g5-text2)]";
const quietLink = "text-xs text-[var(--g5-info)] hover:underline";
const card =
  "overflow-hidden rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]";

const SectionHeading: React.FC<{
  title: string;
  count?: number;
  aside?: React.ReactNode;
}> = ({ title, count, aside }) => (
  <div className="flex items-baseline gap-2">
    <h2 className="text-sm font-semibold text-[var(--g5-text0)]">{title}</h2>
    {count !== undefined && (
      <span className="text-xs font-medium tabular-nums text-[var(--g5-text2)]">
        · {count}
      </span>
    )}
    {aside && <div className="ml-auto">{aside}</div>}
  </div>
);

const EmptyState: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <p
    className={classNames(
      "rounded-[var(--g5-radius-md)] border border-dashed border-[var(--g5-stroke)]",
      "px-4 py-6 text-center text-xs text-[var(--g5-text2)]",
    )}
  >
    {children}
  </p>
);

const IncludeSwitch: React.FC<{
  checked: boolean;
  disabled?: boolean;
  label: string;
  testId: string;
  onChange: (checked: boolean) => void;
}> = ({ checked, disabled, label, testId, onChange }) => (
  <label
    className={classNames(
      "inline-flex select-none items-center gap-1.5 text-xs",
      disabled ? "text-[var(--g5-text2)]" : "text-[var(--g5-text1)]",
    )}
  >
    <input
      type="checkbox"
      className="h-3.5 w-3.5 accent-[var(--g5-info)]"
      data-testid={testId}
      checked={checked}
      disabled={disabled}
      onChange={(event) => onChange(event.target.checked)}
    />
    {label}
  </label>
);

const ChangeBlock: React.FC<{
  file: ReviewPreviewFile;
  change: ReviewPreviewChange;
  note: string;
  /** The round the shown note was pushed with, when it is not a draft. */
  carriedFromRound?: number | null;
  included: boolean;
  fileIncluded: boolean;
  onNote: (note: string) => void;
  onIncluded: (included: boolean) => void;
  onOpen?: () => void;
  /** Take this (rejected) change out of the copy's working tree. */
  onDiscard?: () => void;
  /** Why discarding is not possible right now (the copy is not running). */
  discardDisabledReason?: string | null;
  /** The round the change is in and how it fared, if known. */
  decision?: ItemDecision | null;
}> = ({
  file,
  change,
  note,
  carriedFromRound = null,
  included,
  fileIncluded,
  onNote,
  onIncluded,
  onOpen,
  onDiscard,
  discardDisabledReason = null,
  decision = null,
}) => {
  const active = included && fileIncluded;
  const hasNote = note.trim().length > 0;
  return (
    <div
      data-testid={`my-review-change-${change.key}`}
      className={classNames("space-y-2 px-4 py-3", !active && "opacity-60")}
    >
      <div className="flex flex-wrap items-center gap-3 text-xs">
        <span className="font-medium text-[var(--g5-text1)]">
          {change.kind === "new_file"
            ? "whole file"
            : change.anchorStart !== null
              ? `line ${change.anchorStart}`
              : "change"}
        </span>
        {(change.editCount ?? 1) > 1 && (
          <span className="text-[var(--g5-text2)]">
            {change.editCount} independently reviewable edits
          </span>
        )}
        {decision && (
          <Chip
            state={decision.status === "rejected" ? "warn" : "muted"}
            text={inRoundChip(decision)}
            title={
              decision.status === "rejected"
                ? "The owner rejected this; it stays in your copy until you discard it"
                : "Already in your review; a push carries it over unchanged"
            }
            data-testid={`my-review-in-round-${change.key}`}
          />
        )}
        {onOpen && (
          <button
            type="button"
            className={quietLink}
            data-testid={`my-review-open-${change.key}`}
            onClick={onOpen}
          >
            Open in editor
          </button>
        )}
        {onDiscard && (
          <button
            type="button"
            className={classNames(quietLink, "disabled:opacity-50")}
            data-testid={`my-review-discard-${change.key}`}
            title={
              discardDisabledReason ??
              "Reverts this change in your copy; nothing on the source changes"
            }
            disabled={discardDisabledReason !== null}
            onClick={onDiscard}
          >
            Discard from my copy
          </button>
        )}
        <span className="ml-auto">
          <IncludeSwitch
            checked={included}
            disabled={!fileIncluded}
            label="Include in next push"
            testId={`my-review-include-${change.key}`}
            onChange={onIncluded}
          />
        </span>
      </div>
      {change.kind === "hunk" && (
        <ReviewHunkDiff
          className="max-h-64 border-[var(--g5-stroke)]/70"
          before={change.beforeLines}
          after={change.afterLines}
          oldStart={change.anchorStart}
          newStart={copySideLine(file, change)}
        />
      )}
      {change.kind === "new_file" && (
        <p className={muted}>
          {formatBytes(file.bytes)}
          {file.bytes !== null && file.bytes !== undefined ? " · " : ""}
          the owner decides whether it enters the workspace
        </p>
      )}
      <textarea
        rows={hasNote ? 2 : 1}
        className={classNames(
          "w-full resize-y rounded-[var(--g5-radius-sm)] px-2.5 py-1.5 text-xs text-[var(--g5-text0)]",
          "placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
          hasNote
            ? "border border-[var(--g5-stroke)] border-l-2 border-l-[var(--g5-info)] bg-[var(--g5-bg0)]"
            : "border border-dashed border-[var(--g5-stroke)]/70 bg-transparent hover:border-[var(--g5-stroke)] focus:bg-[var(--g5-bg0)]",
        )}
        data-testid={`my-review-note-${change.key}`}
        value={note}
        onChange={(event) => onNote(event.target.value)}
        placeholder="Add a note for the owner (it opens the suggestion's discussion)"
      />
      {carriedFromRound !== null && (
        <p
          className={classNames(muted, "text-[11px]")}
          data-testid={`my-review-note-carried-${change.key}`}
        >
          Carried from round {carriedFromRound}; edit or empty it to change what
          the next push says.
        </p>
      )}
    </div>
  );
};

const FileCard: React.FC<{
  file: ReviewPreviewFile;
  kind: "Changed" | "New file";
  meta: string;
  included: boolean;
  onIncluded: (included: boolean) => void;
  children: React.ReactNode;
}> = ({ file, kind, meta, included, onIncluded, children }) => (
  <div
    data-testid={`my-review-file-${file.path}`}
    className={classNames(card, !included && "opacity-75")}
  >
    <div className="flex flex-wrap items-center gap-3 border-b border-[var(--g5-stroke)]/60 px-4 py-2.5">
      <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-[var(--g5-text0)]">
        {file.path}
      </span>
      <Chip state={kind === "Changed" ? "info" : "active"} text={kind} />
      {meta && <span className={muted}>{meta}</span>}
      <IncludeSwitch
        checked={included}
        label="Include file"
        testId={`my-review-include-file-${file.repoId}-${file.path}`}
        onChange={onIncluded}
      />
    </div>
    <div className="divide-y divide-[var(--g5-stroke)]/60">{children}</div>
  </div>
);

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

export const MyReviewPanel: React.FC<MyReviewPanelProps> = ({
  review,
  viewerUserId,
  org,
  canPush,
  onOpenFile,
}) => {
  const preview = useReviewPreview(review.reviewWorkspaceId, canPush);
  const publications = useReviewPublications(review.sourceWorkspaceId);
  const suggestions = useReviewSuggestions(review.sourceWorkspaceId);
  const drafts = useReviewDrafts(review.reviewWorkspaceId);
  const discard = useDiscardReviewChanges(
    review.reviewWorkspaceId,
    review.sourceWorkspaceId,
  );
  const [discardOutcome, setDiscardOutcome] = useState<{
    discarded: number;
    skipped: { key: string; path: string; reason: string }[];
    error: string | null;
  } | null>(null);
  const [confirmDiscardRejected, setConfirmDiscardRejected] = useState(false);
  // Discarding writes into the copy's running tree, like Push saves it.
  // The page's gate, when present, reads the same VM status as the strip;
  // the review context is a separate read (`g5.workspace.ui.vm-state`).
  const gate = useWorkspaceLiveGate();
  const copyRunning = gate ? gate.live : review.vmStatus === "running";
  const discardDisabledReason = !copyRunning
    ? ((gate
        ? liveGateReason(gate.kind, "discard changes", {
            workspaceNoun: "your review workspace",
          })
        : null) ?? "Start your review workspace to discard changes.")
    : discard.isLoading
      ? "Discarding…"
      : null;
  const retryPreview = preview.retry;
  // A discard rewrites files in the copy. An editor holding unsaved edits
  // on one of them would keep those edits over the rewritten text and save
  // them with a stale hash: the reviewer is asked first, and on "Discard
  // and reload" those editors drop their edits and take the refetched
  // document.
  const dirtyEditors = useDirtyEditorPaths();
  const [pendingDiscard, setPendingDiscard] = useState<{
    keys: string[];
    paths: string[];
  } | null>(null);
  const runDiscard = async (keys: string[]) => {
    // The copy may have stopped since the confirmation opened.
    if (!copyRunning) return;
    const before = preview.status === "ok" ? preview.data : null;
    const affected = keys
      .map((key) => previewPathForKey(before, key))
      .filter((path): path is string => path !== null)
      .filter((path) => dirtyEditors.includes(path));
    if (affected.length > 0) {
      setPendingDiscard({ keys, paths: Array.from(new Set(affected)) });
      return;
    }
    await performDiscard(keys, []);
  };
  const performDiscard = async (keys: string[], dropPaths: string[]) => {
    if (!copyRunning) return;
    setPendingDiscard(null);
    setDiscardOutcome(null);
    const before = preview.status === "ok" ? preview.data : null;
    try {
      const result = await discard.mutateAsync({ keys });
      dropDirtyEdits(dropPaths);
      setDiscardOutcome({
        discarded: result.discarded.length,
        skipped: result.skipped.map((item) => ({
          key: item.key,
          path: previewPathForKey(before, item.key) ?? item.key,
          reason: item.reason,
        })),
        error: null,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      // A key the copy no longer has: the preview was out of date. It is
      // taken again; the reviewer decides once more on what is there now.
      const stale = /push review again/i.test(message);
      if (stale) retryPreview();
      setDiscardOutcome({
        discarded: 0,
        skipped: [],
        error: stale
          ? "Your copy changed since this list was made; it was refreshed, try again."
          : message,
      });
    }
  };
  // A note or hold on a change the reviewer edited since (its key changed)
  // would block the push; the latest preview decides what still exists.
  const reconcileDrafts = drafts.reconcile;
  // Only a preview fetched since this panel mounted prunes: a cached one
  // may predate notes another tab wrote for newer changes.
  const [mountedAt] = useState(() => Date.now());
  const previewData =
    preview.status === "ok" &&
    !preview.fetching &&
    preview.updatedAt >= mountedAt
      ? preview.data
      : null;
  useEffect(() => {
    if (previewData) reconcileDrafts(previewData);
  }, [previewData, reconcileDrafts]);
  const sourceLink = sourceReviewsPath(org, review);
  // The preview names where the copy stands against the source's branch:
  // a merge base behind the source head means the source moved on, and so
  // does a source tip (refreshed with the rounds) past the preview's.
  const advanced = previewData
    ? sourceAdvanced(
        previewData,
        publications.status === "ok"
          ? publications.data.sourceHeads
          : undefined,
      )
    : false;

  // The preview as read here: once the source's live tips moved past the
  // ones the preview compared against, its "in round N" markers are stale
  // (a push publishes against the new commit and carries nothing over), so
  // every change reads as new until the next preview.
  const shown =
    preview.status === "ok"
      ? withLiveStatuses(
          withoutStaleMarkers(
            preview.data,
            publications.status === "ok" ? publications.data.sourceHeads : null,
            suggestions.status === "ok" ? suggestions.data.suggestions : null,
          ),
          suggestions.status === "ok" ? suggestions.data.suggestions : null,
        )
      : null;
  // The polled rows' statuses, by id: the owner's decisions since the
  // preview, and the decision on a carried row once the markers are gone.
  const liveStatuses = useMemo(() => {
    return reviewSuggestionStatuses(
      suggestions.status === "ok" ? suggestions.data.suggestions : [],
    );
  }, [suggestions]);
  const changedFiles = shown
    ? shown.files.filter((file) => file.kind === "hunk")
    : [];
  const newFiles = shown
    ? shown.files.filter((file) => file.kind === "new_file")
    : [];
  const artifacts = shown
    ? shown.files.filter((file) => file.kind === "artifact")
    : [];
  const skipped = shown ? shown.skipped : [];
  const summary = shown ? draftSummary(shown, drafts.drafts) : null;
  // What the current round already carries is not new: the heading counts
  // only new changes. A copy with nothing new says so, unless the drafts
  // change what the round says (a note that differs from the pushed one,
  // a published change held back) or the copy dropped a proposal. Push
  // stays available either way: the dialog can still change the verdict
  // or the summary.
  const newness = shown ? previewNewness(shown) : null;
  const draftsNewness = draftNewness(shown, drafts.drafts);
  const publicationsById = useMemo(() => {
    const map = new Map<string, ReviewPublicationSummary>();
    if (publications.status === "ok") {
      for (const publication of publications.data.publications) {
        map.set(publication.id, publication);
      }
    }
    return map;
  }, [publications]);
  // The sections are about the copy's reviewer: for the owner of the copy
  // that is the viewer; a reader sees the reviewer's rounds and comments.
  const reviewerUserId = review.reviewerUserId;
  const myComments: ReviewSuggestion[] = useMemo(() => {
    if (suggestions.status !== "ok") return [];
    return suggestions.data.suggestions.filter(
      (suggestion) =>
        suggestion.kind === "comment" &&
        publicationsById.get(suggestion.publicationId)?.reviewerUserId ===
          reviewerUserId,
    );
  }, [suggestions, publicationsById, reviewerUserId]);
  const lastRound = useMemo(
    () =>
      publications.status === "ok"
        ? ownLastRound(publications.data.publications, reviewerUserId)
        : null,
    [publications, reviewerUserId],
  );
  const lastRoundSuggestions: ReviewSuggestion[] = useMemo(() => {
    if (!lastRound || suggestions.status !== "ok") return [];
    return suggestions.data.suggestions.filter(
      (suggestion) => suggestion.publicationId === lastRound.id,
    );
  }, [lastRound, suggestions]);
  // Proposals of the round the copy reverted since: pushing retires them.
  // Once the source moved past the round, the preview says nothing is "in
  // round N" on purpose, and a push retires nothing: the round is outdated,
  // not reduced.
  const lastRoundIsCurrent =
    lastRound !== null &&
    publications.status === "ok" &&
    isPublicationCurrentWithPendingEdits(
      lastRound,
      publications.data.sourceHeads ?? {},
      lastRoundSuggestions,
    );
  const removed = lastRoundIsCurrent
    ? removedFromRound(shown, lastRoundSuggestions)
    : 0;
  // Said only once the round's rows are known (so "removed" is real) and
  // the preview speaks of the same round as the publications do.
  const nothingNew =
    newness !== null &&
    newness.total > 0 &&
    newness.fresh === 0 &&
    newness.round !== null &&
    suggestions.status === "ok" &&
    lastRound !== null &&
    lastRound.round === newness.round &&
    draftsNewness.notesChanged === 0 &&
    draftsNewness.heldBack === 0 &&
    removed === 0;
  const lastRoundOutdated = lastRound !== null && !lastRoundIsCurrent;
  const lastRoundCounts = useMemo(() => {
    const counts = { accepted: 0, rejected: 0, open: 0 };
    for (const suggestion of lastRoundSuggestions) {
      if (suggestion.status === "accepted") counts.accepted += 1;
      else if (suggestion.status === "rejected") counts.rejected += 1;
      else if (suggestion.status === "open") counts.open += 1;
    }
    return counts;
  }, [lastRoundSuggestions]);

  const excludedChanges = new Set(drafts.drafts.excludedChanges);
  const fileIncluded = (file: ReviewPreviewFile) =>
    !isReviewFileExcluded(drafts.drafts, file.repoId, file.path);

  // A change a push would continue shows the note the push would carry
  // (whatever the source's commit) until the reviewer writes here; emptying
  // the box is how it is cleared (an explicit empty draft).
  const renderChange = (
    file: ReviewPreviewFile,
    change: ReviewPreviewChange,
  ) => (
    <ChangeBlock
      key={change.key}
      file={file}
      change={change}
      note={drafts.drafts.changeNotes[change.key] ?? change.carried?.note ?? ""}
      carriedFromRound={
        drafts.drafts.changeNotes[change.key] === undefined &&
        change.carried &&
        change.carried.note.trim() !== ""
          ? change.carried.round
          : null
      }
      included={!excludedChanges.has(change.key)}
      fileIncluded={fileIncluded(file)}
      onNote={(note) => drafts.setChangeNote(change.key, note)}
      onIncluded={(included) => drafts.setChangeIncluded(change.key, included)}
      onOpen={
        onOpenFile
          ? () =>
              onOpenFile(
                file.path,
                file.repoId ?? undefined,
                copySideLine(file, change),
              )
          : undefined
      }
      onDiscard={
        canPush && decisionOf(change, liveStatuses)?.status === "rejected"
          ? () => void runDiscard([change.key])
          : undefined
      }
      discardDisabledReason={discardDisabledReason}
      decision={decisionOf(change, liveStatuses)}
    />
  );
  const rejected = rejectedInRound(shown, liveStatuses);

  const savedNote =
    preview.status === "ok"
      ? preview.data.saved?.workspace === "stopped"
        ? "Your copy is stopped: this is what it saved last."
        : "Saved from your running copy."
      : null;
  const leftOut = summary ? summary.total - summary.included : 0;
  const nothingToPush =
    preview.status === "ok" &&
    changedFiles.length === 0 &&
    newFiles.length === 0 &&
    artifacts.length === 0;

  return (
    <section data-testid="my-review-panel" className="space-y-8 text-sm">
      {/* Page header: what this page is, and where the copy stands. */}
      <header className="flex flex-wrap items-start gap-x-6 gap-y-3">
        <div className="min-w-0 flex-1">
          <h1 className="text-lg font-semibold text-[var(--g5-text0)]">
            Review of {review.sourceWorkspaceName}
          </h1>
          <p className="mt-1 text-xs text-[var(--g5-text2)]">
            {canPush
              ? `What this copy changed against ${review.sourceWorkspaceName}, your notes, and how its owner handled your rounds.`
              : `${review.reviewerDisplayName}'s rounds on ${review.sourceWorkspaceName} and how its owner handled them.`}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {savedNote && (
            <span className={muted} data-testid="my-review-saved">
              {savedNote}
            </span>
          )}
          {canPush && preview.status === "loading" && (
            <span
              className={classNames(muted, "inline-flex items-center gap-1")}
            >
              <Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />
              Saving your copy and comparing it…
            </span>
          )}
          {summary && (
            <>
              {summary.notes > 0 && (
                <Chip
                  state="active"
                  text={`${summary.notes} note${summary.notes === 1 ? "" : "s"}`}
                />
              )}
              {leftOut > 0 && (
                <Chip state="muted" text={`${leftOut} left out`} />
              )}
            </>
          )}
          {canPush && (
            <Button
              kind="primary"
              className="!px-3 !py-1 !text-xs"
              data-testid="my-review-push"
              onClick={requestReviewPush}
            >
              Push review
            </Button>
          )}
        </div>
      </header>

      {advanced && (
        <div
          role="status"
          data-testid="my-review-source-advanced"
          className={classNames(
            "flex gap-2 rounded-[var(--g5-radius-md)] border border-[var(--g5-warn)]/50",
            "bg-[color-mix(in_srgb,var(--g5-warn)_10%,transparent)] px-4 py-3 text-xs text-[var(--g5-text0)]",
          )}
        >
          <AlertTriangle
            className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[var(--g5-warn)]"
            aria-hidden="true"
          />
          <span>{sourceAdvancedMessage(review.sourceWorkspaceName)}</span>
        </div>
      )}
      {!canPush && (
        <p className={muted} data-testid="my-review-not-owner">
          This is {review.reviewerDisplayName}&apos;s copy; only they can push.
        </p>
      )}
      {preview.status === "error" && (
        <div
          data-testid="my-review-error"
          className={classNames(
            "flex flex-wrap items-center gap-3 rounded-[var(--g5-radius-md)] border border-[var(--g5-warn)]/50",
            "bg-[color-mix(in_srgb,var(--g5-warn)_10%,transparent)] px-4 py-3 text-xs text-[var(--g5-text0)]",
          )}
        >
          <span className="min-w-0 flex-1">
            Your copy could not be saved and compared yet:{" "}
            {preview.error.message}
          </span>
          <Button
            kind="secondary"
            className="!px-2.5 !py-1 !text-xs"
            data-testid="my-review-retry"
            onClick={preview.retry}
          >
            Try again
          </Button>
        </div>
      )}

      {preview.status === "ok" && (
        <div className="space-y-8" data-testid="my-review-body">
          <div className="space-y-3">
            <SectionHeading
              title="Changes to push"
              count={newness ? newness.freshChanged : changedFiles.length}
              aside={
                canPush && rejected.count > 0 && !confirmDiscardRejected ? (
                  <Button
                    kind="ghost"
                    className="!px-2 !py-0.5 !text-xs"
                    data-testid="my-review-discard-rejected"
                    title={
                      discardDisabledReason ??
                      "Reverts every change the owner rejected in your copy; nothing on the source changes"
                    }
                    onClick={() => setConfirmDiscardRejected(true)}
                    disabled={discardDisabledReason !== null}
                  >
                    Discard rejected changes
                  </Button>
                ) : undefined
              }
            />
            {confirmDiscardRejected && (
              <div
                role="alertdialog"
                data-testid="my-review-discard-rejected-confirm"
                className={classNames(
                  "flex flex-wrap items-center gap-3 rounded-[var(--g5-radius-md)] border border-[var(--g5-warn)]/50",
                  "bg-[color-mix(in_srgb,var(--g5-warn)_10%,transparent)] px-4 py-3 text-xs text-[var(--g5-text0)]",
                )}
              >
                <span className="min-w-0 flex-1">
                  Reverts {rejected.count} rejected change
                  {rejected.count === 1 ? "" : "s"} in your copy; nothing on{" "}
                  {review.sourceWorkspaceName} changes.
                </span>
                <Button
                  kind="ghost"
                  className="!px-2.5 !py-1 !text-xs"
                  onClick={() => setConfirmDiscardRejected(false)}
                  disabled={discard.isLoading}
                >
                  Cancel
                </Button>
                <Button
                  kind="secondary"
                  className="!px-2.5 !py-1 !text-xs"
                  data-testid="my-review-discard-rejected-go"
                  disabled={discardDisabledReason !== null}
                  title={discardDisabledReason ?? undefined}
                  onClick={() => {
                    setConfirmDiscardRejected(false);
                    void runDiscard(rejected.keys);
                  }}
                >
                  {discard.isLoading ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    "Discard"
                  )}
                </Button>
              </div>
            )}
            {pendingDiscard && (
              <div
                role="alertdialog"
                data-testid="my-review-discard-unsaved-confirm"
                className={classNames(
                  "flex flex-wrap items-center gap-3 rounded-[var(--g5-radius-md)] border border-[var(--g5-warn)]/50",
                  "bg-[color-mix(in_srgb,var(--g5-warn)_10%,transparent)] px-4 py-3 text-xs text-[var(--g5-text0)]",
                )}
              >
                <span className="min-w-0 flex-1">
                  You have unsaved edits in {pendingDiscard.paths.join(", ")};
                  discard them too?
                </span>
                <Button
                  kind="ghost"
                  className="!px-2.5 !py-1 !text-xs"
                  onClick={() => setPendingDiscard(null)}
                  disabled={discard.isLoading}
                >
                  Cancel
                </Button>
                <Button
                  kind="secondary"
                  className="!px-2.5 !py-1 !text-xs"
                  data-testid="my-review-discard-unsaved-go"
                  disabled={discardDisabledReason !== null}
                  title={discardDisabledReason ?? undefined}
                  onClick={() =>
                    void performDiscard(
                      pendingDiscard.keys,
                      pendingDiscard.paths,
                    )
                  }
                >
                  Discard and reload
                </Button>
              </div>
            )}
            {discardOutcome && (
              <div
                role="status"
                data-testid="my-review-discard-outcome"
                className={classNames(muted, "space-y-0.5")}
              >
                {discardOutcome.error ? (
                  <p className="text-[var(--g5-blocked)]">
                    Could not discard: {discardOutcome.error}
                  </p>
                ) : (
                  <p>
                    Discarded {discardOutcome.discarded} change
                    {discardOutcome.discarded === 1 ? "" : "s"}
                  </p>
                )}
                {discardOutcome.skipped.map((item) => (
                  <p key={item.key}>
                    Skipped: {item.path}: {item.reason}
                  </p>
                ))}
              </div>
            )}
            {nothingNew &&
              newness?.round !== null &&
              newness?.round !== undefined && (
                <p
                  className={muted}
                  role="status"
                  data-testid="my-review-nothing-new"
                >
                  {nothingNewMessage(newness.round)}
                </p>
              )}
            {nothingToPush ? (
              <EmptyState>
                Nothing changed in this copy yet. Edit files or ask the agent,
                then come back here.
              </EmptyState>
            ) : changedFiles.length === 0 ? (
              <p className={muted}>No changed files.</p>
            ) : (
              changedFiles.map((file) => (
                <FileCard
                  key={`${file.repoId}:${file.path}`}
                  file={file}
                  kind="Changed"
                  meta={`${previewFileEditCount(file)} change${previewFileEditCount(file) === 1 ? "" : "s"}`}
                  included={fileIncluded(file)}
                  onIncluded={(included) =>
                    drafts.setFileIncluded(file.repoId, file.path, included)
                  }
                >
                  {(file.changes ?? []).map((change) =>
                    renderChange(file, change),
                  )}
                </FileCard>
              ))
            )}
          </div>
          {newFiles.length > 0 && (
            <div className="space-y-3">
              <SectionHeading title="New files" count={newFiles.length} />
              {newFiles.map((file) => (
                <FileCard
                  key={`${file.repoId}:${file.path}`}
                  file={file}
                  kind="New file"
                  meta={formatBytes(file.bytes)}
                  included={fileIncluded(file)}
                  onIncluded={(included) =>
                    drafts.setFileIncluded(file.repoId, file.path, included)
                  }
                >
                  {(file.changes ?? []).map((change) =>
                    renderChange(file, change),
                  )}
                </FileCard>
              ))}
            </div>
          )}
          {artifacts.length > 0 && (
            <div className="space-y-3">
              <SectionHeading title="Artifacts" count={artifacts.length} />
              <ul
                className={classNames(
                  card,
                  "divide-y divide-[var(--g5-stroke)]/60",
                )}
              >
                {artifacts.map((file) => (
                  <li
                    key={`${file.repoId}:${file.path}`}
                    data-testid={`my-review-artifact-${file.path}`}
                    className="flex flex-wrap items-center gap-3 px-4 py-2.5"
                  >
                    <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-[var(--g5-text0)]">
                      {file.path}
                    </span>
                    <Chip state="active" text="Artifact" />
                    {decisionOf(file, liveStatuses) && (
                      <Chip
                        state={
                          decisionOf(file, liveStatuses)?.status === "rejected"
                            ? "warn"
                            : "muted"
                        }
                        text={inRoundChip(
                          decisionOf(file, liveStatuses) as ItemDecision,
                        )}
                        title={
                          decisionOf(file, liveStatuses)?.status === "rejected"
                            ? "The owner rejected this; it stays in your copy until you discard it"
                            : "Already in your review; a push keeps the same row"
                        }
                        data-testid={`my-review-in-round-artifact-${file.path}`}
                      />
                    )}
                    {canPush &&
                      decisionOf(file, liveStatuses)?.status === "rejected" &&
                      file.key && (
                        <button
                          type="button"
                          className={classNames(
                            quietLink,
                            "disabled:opacity-50",
                          )}
                          data-testid={`my-review-discard-artifact-${file.path}`}
                          title={
                            discardDisabledReason ??
                            "Deletes this file from your copy; nothing on the source changes"
                          }
                          disabled={discardDisabledReason !== null}
                          onClick={() => void runDiscard([file.key as string])}
                        >
                          Discard from my copy
                        </button>
                      )}
                    <span className={muted}>
                      {formatBytes(file.bytes)} · sent as an artifact, opened or
                      downloaded, never written into the workspace
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {skipped.length > 0 && (
            <div className="space-y-3">
              <SectionHeading title="Skipped" count={skipped.length} />
              <ul
                className={classNames(
                  card,
                  "divide-y divide-[var(--g5-stroke)]/60",
                )}
              >
                {skipped.map((item) => (
                  <li
                    key={`${item.repoId}:${item.path}`}
                    data-testid={`my-review-skipped-${item.path}`}
                    className="flex flex-wrap items-center gap-3 px-4 py-2.5"
                  >
                    <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-[var(--g5-text0)]">
                      {item.path}
                    </span>
                    <span className={muted}>{item.reason}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {(summary || hasReviewDrafts(drafts.drafts)) && (
            <div className="flex flex-wrap items-center gap-3 border-t border-[var(--g5-stroke)]/60 pt-3">
              {summary && (
                <p className={muted} data-testid="my-review-summary">
                  {summary.included} of {summary.total} change
                  {summary.total === 1 ? "" : "s"} will be pushed ·{" "}
                  {newness && newness.inRound > 0
                    ? `${newness.fresh} new · ${newness.inRound} already in ${newness.round !== null ? `round ${newness.round}` : "review"} · `
                    : ""}
                  {draftsNewness.notesChanged > 0
                    ? `${draftsNewness.notesChanged} note${draftsNewness.notesChanged === 1 ? "" : "s"} changed · `
                    : ""}
                  {draftsNewness.heldBack > 0
                    ? `${draftsNewness.heldBack} held back · `
                    : ""}
                  {summary.notes} note{summary.notes === 1 ? "" : "s"}
                  {removed > 0 ? ` · ${removed} removed` : ""}
                </p>
              )}
              {hasReviewDrafts(drafts.drafts) && (
                <Button
                  kind="ghost"
                  className="ml-auto !px-2 !py-0.5 !text-xs"
                  data-testid="my-review-clear-drafts"
                  title="Forget every note and hold written here; nothing was pushed"
                  onClick={drafts.clear}
                >
                  Clear all drafts
                </Button>
              )}
            </div>
          )}
        </div>
      )}

      <div className="space-y-3">
        <SectionHeading
          title={`${canPush ? "Your" : "Their"} comments on ${review.sourceWorkspaceName}`}
          count={myComments.length}
        />
        {myComments.length === 0 ? (
          <p className={muted}>No comments left yet.</p>
        ) : (
          <ul
            className={classNames(
              card,
              "divide-y divide-[var(--g5-stroke)]/60",
            )}
          >
            {myComments.map((comment) => {
              const chip = statusChip(comment.status);
              return (
                <li
                  key={comment.id}
                  data-testid={`my-review-comment-${comment.id}`}
                  className="flex flex-wrap items-center gap-3 px-4 py-2.5"
                >
                  <span className="font-mono text-[13px] text-[var(--g5-text0)]">
                    {comment.path}
                    {comment.anchorStart !== null
                      ? `:${comment.anchorStart}`
                      : ""}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-xs text-[var(--g5-text1)]">
                    {comment.note}
                  </span>
                  <Chip state={chip.state} text={chip.text} />
                  <Link to={sourceLink} className={quietLink}>
                    Open on {review.sourceWorkspaceName}
                  </Link>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <div className="space-y-3" data-testid="my-review-last-round">
        <SectionHeading
          title={canPush ? "Your last round" : "Their last round"}
        />
        {!lastRound ? (
          <p className={muted}>Nothing pushed yet.</p>
        ) : (
          <div className={card}>
            <div className="flex flex-wrap items-center gap-3 border-b border-[var(--g5-stroke)]/60 px-4 py-2.5">
              <span className="inline-flex items-center rounded-full bg-[var(--g5-bg0)] px-2 py-0.5 text-[11px] font-semibold text-[var(--g5-text0)]">
                Round {lastRound.round}
              </span>
              <Chip
                state={verdictChip(lastRound.verdict).state}
                text={verdictChip(lastRound.verdict).text}
              />
              {lastRoundOutdated && <Chip state="muted" text="Outdated" />}
              <span className={muted}>
                {new Date(lastRound.createdAt).toLocaleString()}
              </span>
              <span
                className={classNames(muted, "ml-auto tabular-nums")}
                data-testid="my-review-last-round-counts"
              >
                Round {lastRound.round} · {lastRoundCounts.accepted} accepted ·{" "}
                {lastRoundCounts.rejected} rejected · {lastRoundCounts.open}{" "}
                open
              </span>
            </div>
            {lastRoundOutdated && (
              <p
                className={classNames(
                  muted,
                  "border-b border-[var(--g5-stroke)]/60 px-4 py-2",
                )}
                data-testid="my-review-last-round-outdated"
              >
                Outdated: it reviewed an earlier commit of{" "}
                {review.sourceWorkspaceName}. Its cards left the file pages;
                your next push starts a new group under the current commit.
              </p>
            )}
            <ul className="divide-y divide-[var(--g5-stroke)]/60">
              {lastRoundSuggestions.map((suggestion) => {
                const chip =
                  suggestion.kind === "artifact" &&
                  suggestion.status === "accepted"
                    ? { state: "ok" as State, text: "Kept" }
                    : statusChip(suggestion.status);
                return (
                  <li
                    key={suggestion.id}
                    data-testid={`my-review-round-suggestion-${suggestion.id}`}
                    className="flex flex-wrap items-center gap-3 px-4 py-2"
                  >
                    <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-[var(--g5-text0)]">
                      {suggestion.path ?? "(no file)"}
                    </span>
                    <span className={muted}>{kindLabel(suggestion.kind)}</span>
                    <Chip state={chip.state} text={chip.text} />
                    <Link to={sourceLink} className={quietLink}>
                      Open on {review.sourceWorkspaceName}
                    </Link>
                  </li>
                );
              })}
            </ul>
          </div>
        )}
      </div>
    </section>
  );
};
