// =============================================================================
// G5 Workspace — ReviewsPanel
// =============================================================================
// The Approvals page's "Reviews" section of a source workspace: every
// publication pushed from a review workspace (and every comment left without
// one), newest round first, with the suggestions each carries. The owner
// accepts a suggestion into the running workspace, edits it before accepting,
// rejects it or resolves a comment; a reviewer withdraws their own; anyone
// who can read the workspace replies under a suggestion or leaves a comment
// on a file. Accepting writes through the workspace VM, so it is only offered
// while that VM runs.
// =============================================================================

import React, {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  holdsValue,
  holdsWords,
  isSet,
  useActiveComposerSubjects,
  useComposerField,
  useComposerRecords,
  type ComposerSubject,
} from "../../hooks/useComposerStore";
import { ComposerOwnerIfNone } from "../comments/ComposerOwner";
import { Loader2, MessageSquare } from "lucide-react";
import { Button, Chip, classNames } from "../ui/UI";
import { useWorkspaceChanges } from "../../hooks/useFiles";
import { requestWorkspaceStart } from "./reviewWorkspace";
import { ReviewHunkDiff, splitHunkLines } from "./ReviewHunkDiff";
import { ReviewFileSection, reviewFileSections } from "./ReviewFileSections";
import { useWorkspace, useWorkspaceVm } from "../../hooks/useWorkspace";
import { useRenderedPrincipal } from "../../hooks/useAuth";
import { isLaterSuggestion } from "../../lib/suggestionRecords";
import {
  selectedReviewSuggestion,
  utf8OffsetToUtf16,
} from "../../lib/reviewSuggestionEdits";
import {
  useSuggestionCardNodes,
  type SuggestionCardNodes,
} from "../../hooks/useSuggestionCardNodes";
import {
  SuggestionCardPortals,
  SuggestionCardSlot,
} from "./SuggestionCardPlaces";
import {
  useAcceptReviewSuggestion,
  useAddReviewSuggestionReply,
  useRejectReviewSuggestion,
  useResolveReviewSuggestion,
  useReviewPublications,
  useReviewSuggestionReplies,
  useReviewSuggestions,
  useWithdrawReviewSuggestion,
} from "../../hooks/useWorkspaceReview";
import { sourceThreadWrites } from "../../lib/threadWrites";
import { formatDateTime } from "../../lib/dateTime";
import type {
  ReviewFinding,
  ReviewPublication,
  ReviewPublicationSummary,
  ReviewSuggestion,
  ReviewSuggestionResult,
} from "../../lib/types";
import { api } from "../../lib/api";
import { routes } from "../../lib/routes";
import { Modal } from "../ui/Modal";
import { withScriptlessPreviewCsp } from "../shared/sandboxedHtml";
import {
  commitGroupKey,
  isPublicationCurrentWithPendingEdits,
  publicationCommitLabel,
  sourceHeadsLabel,
} from "./reviewWorkspace";
import {
  liveGateCanStart,
  liveGateKind,
  liveGateReason,
  useLiveGateReason,
} from "../../lib/workspaceLiveGate";

export interface ReviewsPanelProps {
  showCounts?: boolean;
  revealTarget?: string;
  /** The workspace under review (the source). */
  workspaceId: string;
  workspaceName: string;
  viewerUserId: string;
  /** Only the owner accepts, rejects and resolves. */
  viewerOwnsWorkspace: boolean;
  /**
   * Opens a file of this workspace in the editor (at a line when given):
   * every change in the panel links to the file it is about.
   */
  onOpenFile?: (filePath: string, repoRef?: string, line?: number) => void;
}

/** How a round differs from the round it replaced, by actionable suggestions. */
export interface RoundDelta {
  previousRound: number;
  /** Suggestions with no counterpart in the previous round. */
  added: number;
  /** Suggestions that continue a previous round's thread (same change). */
  carried: number;
  /** Previous-round suggestions the new round no longer proposes. */
  dropped: number;
}

/**
 * The increment a round brings over the round it superseded (the same
 * reviewer and kind), or `null` for a first round or an annotation. A
 * suggestion that carries a `supersedes` link continues a thread from the
 * previous round; a previous-round suggestion retired by the new round
 * without a continuation was dropped by the reviewer.
 */
export function roundDelta(
  publication: ReviewPublication,
  publications: ReviewPublication[],
  suggestions: ReviewSuggestion[],
): RoundDelta | null {
  if ((publication.kind ?? "round") !== "round") return null;
  const previous = publications.find(
    (candidate) =>
      candidate.supersededBy === publication.id &&
      (candidate.kind ?? "round") === "round" &&
      candidate.reviewerUserId === publication.reviewerUserId &&
      candidate.authorKind === publication.authorKind,
  );
  if (!previous) return null;
  const actionable = (suggestion: ReviewSuggestion) =>
    suggestion.kind === "hunk" || suggestion.kind === "new_file";
  const own = suggestions.filter(
    (suggestion) =>
      suggestion.publicationId === publication.id && actionable(suggestion),
  );
  const prior = suggestions.filter(
    (suggestion) =>
      suggestion.publicationId === previous.id && actionable(suggestion),
  );
  const carried = new Set(
    own.map((suggestion) => suggestion.supersedes).filter(Boolean),
  );
  const priorEdits = new Set(
    prior.flatMap((suggestion) =>
      suggestion.editId ? [suggestion.editId] : [],
    ),
  );
  const ownEdits = new Set(
    own.flatMap((suggestion) => (suggestion.editId ? [suggestion.editId] : [])),
  );
  const isCarried = (suggestion: ReviewSuggestion) =>
    suggestion.editId
      ? priorEdits.has(suggestion.editId)
      : !!suggestion.supersedes;
  return {
    previousRound: previous.round,
    added: own.filter((suggestion) => !isCarried(suggestion)).length,
    carried: own.filter(isCarried).length,
    dropped: prior.filter(
      (suggestion) =>
        suggestion.status === "superseded" &&
        !(suggestion.editId
          ? ownEdits.has(suggestion.editId)
          : carried.has(suggestion.id)),
    ).length,
  };
}

/** The delta as one short line, or `null` when there is nothing to say. */
export function roundDeltaText(delta: RoundDelta | null): string | null {
  if (!delta) return null;
  const parts = [
    `${delta.added} new`,
    `${delta.carried} carried over`,
    `${delta.dropped} dropped`,
  ];
  return `since round ${delta.previousRound}: ${parts.join(" · ")}`;
}

type ChipState = "active" | "ok" | "info" | "warn" | "blocked" | "muted";

function verdictChip(verdict: string | null): {
  state: ChipState;
  text: string;
} {
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

function statusChip(status: string): { state: ChipState; text: string } {
  switch (status) {
    case "open":
      return { state: "info", text: "Open" };
    case "accepted":
      return { state: "ok", text: "Accepted" };
    case "resolved":
      return { state: "ok", text: "Resolved" };
    case "rejected":
      return { state: "blocked", text: "Rejected" };
    case "withdrawn":
      return { state: "muted", text: "Withdrawn" };
    case "superseded":
      return { state: "muted", text: "Superseded" };
    case "obsolete":
      return { state: "muted", text: "Obsolete" };
    default:
      return { state: "muted", text: status };
  }
}

function severityChip(severity: string): ChipState {
  switch (severity) {
    case "P0":
      return "blocked";
    case "P1":
      return "warn";
    case "P2":
      return "info";
    default:
      return "muted";
  }
}

function kindLabel(kind: string): string {
  switch (kind) {
    case "hunk":
      return "Change";
    case "new_file":
      return "New file";
    case "comment":
      return "Comment";
    case "artifact":
      return "Artifact";
    default:
      return kind;
  }
}

function authorName(
  publication: ReviewPublicationSummary,
  names: Record<string, string>,
): string {
  const person = names[publication.reviewerUserId];
  return person ?? publication.authorLabel;
}

const inputClass = classNames(
  "w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
  "bg-[var(--g5-bg0)] px-2.5 py-1.5 text-sm text-[var(--g5-text0)]",
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
);

/**
 * The Reviews panel. Its composers' state belongs to the page's owner, or to
 * the panel's own when it is shown without one.
 */
export const ReviewsPanel: React.FC<ReviewsPanelProps> = (props) => (
  <ComposerOwnerIfNone>
    <ReviewsPanelContent {...props} />
  </ComposerOwnerIfNone>
);

const ReviewsPanelContent: React.FC<ReviewsPanelProps> = ({
  showCounts = true,
  revealTarget,
  workspaceId,
  workspaceName,
  viewerUserId,
  viewerOwnsWorkspace,
  onOpenFile,
}) => {
  const publications = useReviewPublications(workspaceId);
  // Each head of a commit vector is named by its repository: the
  // workspace's repo list names it, the Changes report's mount is the
  // fallback, and the short id the last resort.
  const detail = useWorkspace(workspaceId);
  const changesReport = useWorkspaceChanges(workspaceId);
  const repoNames = useMemo(() => {
    const map: Record<string, string> = {};
    if (changesReport.status === "ok") {
      for (const member of changesReport.data.members) {
        if (member.root_path) map[member.repo_id] = member.root_path;
      }
    }
    if (detail.status === "ok") {
      for (const branch of detail.data.branches) {
        if (branch.repoName) map[branch.repoId] = branch.repoName;
      }
    }
    return map;
  }, [detail, changesReport]);
  const openSuggestionFile = useMemo(
    () =>
      onOpenFile
        ? (suggestion: ReviewSuggestion) => {
            if (!suggestion.path) return;
            onOpenFile(
              suggestion.path,
              suggestion.repoId ?? undefined,
              suggestion.anchorStart ?? undefined,
            );
          }
        : undefined,
    [onOpenFile],
  );
  const suggestions = useReviewSuggestions(workspaceId);
  const vm = useWorkspaceVm(workspaceId);
  const ownerVmRunning = vm.status === "ok" && vm.data.status === "running";
  // Applying a text suggestion is a save into the running tree: the state's
  // own reason and only the valid lifecycle action are offered
  // (`g5.workspace.ui.vm-state`, `g5.workspace.ui.unavailable`).
  const sourceDetail = detail.status === "ok" ? detail.data : null;
  const acceptGateKind = liveGateKind({
    isOwner: viewerOwnsWorkspace,
    isBase: sourceDetail?.isBase === true,
    readOnly: sourceDetail?.readOnly === true,
    freezing: sourceDetail?.mutationPolicyTransition === "freezing",
    vmStatus: vm.status === "ok" ? vm.data.status : null,
  });
  const acceptBlockedReason = liveGateReason(
    acceptGateKind,
    "accept suggestions",
  );
  const canStartForAccept = liveGateCanStart(acceptGateKind);
  // A read-only, freezing or Base source takes no thread message or decision.
  const sourceWrites = sourceThreadWrites(detail);
  const sourceWritable = sourceWrites.writable;
  const sourceClosedNote = sourceWrites.closedNote;
  const [showEarlier, setShowEarlier] = useState(false);
  // Outdated commit groups the person opened.
  const [openedGroups, setOpenedGroups] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  // Each suggestion's card is rendered once, into its node, and the place
  // showing it adopts the node: hiding, grouping and regrouping move a card
  // without unmounting what is typed, sent or decided in it. A card stays
  // mounted, hidden, once it has been shown (`g5.documents.comments.failure`).
  const cardNodes = useSuggestionCardNodes();
  const [shownSuggestionIds, setShownSuggestionIds] = useState<
    ReadonlySet<string>
  >(() => new Set());
  // Cards whose composers hold words, a send or a decision.
  const activeSuggestionIds = useActiveComposerSubjects("suggestion");
  // The rounds shown as current, and those in the current commit's group,
  // at the last render: a round being answered keeps its place when only
  // the data would move it.
  const [placedBefore, setPlacedBefore] = useState<{
    current: ReadonlySet<string>;
    currentGroup: ReadonlySet<string>;
  }>(() => ({ current: new Set(), currentGroup: new Set() }));

  const suggestionsByPublication = useMemo(() => {
    const map = new Map<string, ReviewSuggestion[]>();
    if (suggestions.status !== "ok") return map;
    for (const suggestion of suggestions.data.suggestions) {
      const list = map.get(suggestion.publicationId) ?? [];
      list.push(suggestion);
      map.set(suggestion.publicationId, list);
    }
    return map;
  }, [suggestions]);

  const findingsByPublication = useMemo(() => {
    const map = new Map<string, ReviewFinding[]>();
    if (publications.status !== "ok") return map;
    for (const finding of publications.data.findings) {
      const list = map.get(finding.publicationId) ?? [];
      list.push(finding);
      map.set(finding.publicationId, list);
    }
    for (const list of map.values()) {
      list.sort((a, b) => a.position - b.position);
    }
    return map;
  }, [publications]);

  const [revealed, setRevealed] = useState<string | undefined>();
  // The paragraph sections report when they have settled (their file read,
  // or its read failed): a deep link's card is revealed where it will stay,
  // not where the fallback card was while the file loaded.
  const [sectionStates, setSectionStates] = useState<
    ReadonlyMap<string, string>
  >(() => new Map());
  const markSectionSettled = useCallback(
    (key: string, state: string | null) => {
      setSectionStates((current) => {
        if (state === null ? !current.has(key) : current.get(key) === state)
          return current;
        const next = new Map(current);
        if (state === null) next.delete(key);
        else next.set(key, state);
        return next;
      });
    },
    [],
  );
  const allSectionsSettled = [...sectionStates.values()].every(
    (state) => state !== "loading",
  );
  // Changes whenever a section's cards may have moved: the reveal follows.
  const sectionsVersion = [...sectionStates.entries()]
    .map(([key, state]) => `${key}=${state}`)
    .join("\n");
  if (
    revealTarget &&
    revealTarget !== revealed &&
    publications.status === "ok" &&
    suggestions.status === "ok"
  ) {
    setRevealed(revealTarget);
    const targetId = revealTarget.replace(
      /^review-(?:suggestion|publication)-/,
      "",
    );
    const suggestion = suggestions.data.suggestions.find(
      (s) => s.id === targetId,
    );
    const publication = publications.data.publications.find(
      (p) => p.id === (suggestion?.publicationId ?? targetId),
    );
    if (publication) {
      setShowEarlier(true);
      setOpenedGroups(
        (old) => new Set([...old, commitGroupKey(publication) || "unknown"]),
      );
    }
  }
  // A reveal is done once its card was scrolled to with every section's
  // changes placed (their digests computed); later section changes do not
  // replay it. Until then it follows the sections as they settle.
  const revealDone = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (!revealTarget || !revealed || !allSectionsSettled) return;
    if (revealDone.current === revealed) return;
    const placed = [...sectionStates.values()].every(
      (state) => !state.endsWith(":hashing"),
    );
    const targetId = revealTarget.replace(
      /^review-(?:suggestion|publication)-/,
      "",
    );
    const frame = requestAnimationFrame(() => {
      if (placed) revealDone.current = revealed;
      const target = document.querySelector(
        `[data-testid="review-suggestion-${CSS.escape(targetId)}"], [data-testid="review-publication-${CSS.escape(targetId)}"]`,
      );
      target?.scrollIntoView?.({ block: "center" });
    });
    return () => cancelAnimationFrame(frame);
  }, [
    revealTarget,
    revealed,
    allSectionsSettled,
    sectionsVersion,
    sectionStates,
  ]);

  if (publications.status === "loading" || suggestions.status === "loading") {
    return (
      <p
        data-testid="reviews-panel-loading"
        className="text-sm text-[var(--g5-text2)]"
      >
        Loading reviews…
      </p>
    );
  }
  if (publications.status === "error") {
    return (
      <p className="text-sm text-[var(--g5-blocked)]">
        Could not load reviews: {publications.error.message}
      </p>
    );
  }
  if (suggestions.status === "error") {
    return (
      <p className="text-sm text-[var(--g5-blocked)]">
        Could not load suggestions: {suggestions.error.message}
      </p>
    );
  }

  const refreshError = publications.refreshError ?? suggestions.refreshError;
  const all = [...publications.data.publications].sort((a, b) =>
    b.createdAt.localeCompare(a.createdAt),
  );
  // Rounds holding a card whose composer has words, a send or a decision
  // in flight.
  const activePublicationIds = new Set(
    suggestions.data.suggestions
      .filter((suggestion) => activeSuggestionIds.has(suggestion.id))
      .map((suggestion) => suggestion.publicationId),
  );
  // A superseded round stays in view while it still carries open
  // suggestions: comments survive a new round, and hiding them would drop a
  // live discussion into history while the open count still includes it. A
  // round shown as current also stays while a card in it is being answered
  // or decided; one the person shows among the earlier rounds stays there.
  const inHistory = (publication: ReviewPublicationSummary) =>
    publication.supersededBy !== null &&
    publication.openCount === 0 &&
    !(
      activePublicationIds.has(publication.id) &&
      placedBefore.current.has(publication.id)
    );
  const current = all.filter((publication) => !inHistory(publication));
  const earlier = all.filter(inHistory);
  const visible = showEarlier ? all : current;
  const openCount = suggestions.data.suggestions.filter(
    (suggestion) => suggestion.status === "open",
  ).length;
  // Written proposals wait for the VM; a comment or an artifact does not.
  const openNeedingVm = suggestions.data.suggestions.filter(
    (suggestion) =>
      suggestion.status === "open" &&
      (suggestion.kind === "hunk" || suggestion.kind === "new_file"),
  ).length;
  const names = publications.data.reviewerDisplayNames;
  // Rounds belong to the commit of the source they reviewed. The current
  // commit's rounds are the review as it stands; a round about an earlier
  // commit is outdated: its cards left the file pages, and it stays here
  // under its commit, collapsed. Competing proposals and the deliverables
  // list are about the source as it is now.
  const sourceHeads = publications.data.sourceHeads ?? {};
  const currentPublicationIds = new Set(
    publications.data.publications
      .filter((publication) =>
        isPublicationCurrentWithPendingEdits(
          publication,
          sourceHeads,
          suggestions.data.suggestions,
        ),
      )
      .map((publication) => publication.id),
  );
  const currentSuggestions = suggestions.data.suggestions.filter((suggestion) =>
    currentPublicationIds.has(suggestion.publicationId),
  );
  const openCurrent = currentSuggestions.filter(
    (suggestion) => suggestion.status === "open",
  ).length;
  const openOutdated = openCount - openCurrent;
  // What the live changes report proves: which files still hold
  // uncommitted edits. An accepted written proposal in such a file is
  // flagged, whatever its round; whether the acceptance itself is in a
  // commit is not claimed (an acceptance the file already contained wrote
  // nothing). Without a live report (workspace stopped, a reader) nothing
  // is said.
  const uncommittedPaths = new Set<string>();
  if (
    changesReport.status === "ok" &&
    changesReport.data.source === "live" &&
    changesReport.data.attached
  ) {
    for (const member of changesReport.data.members) {
      for (const file of member.dirty ?? []) {
        uncommittedPaths.add(`${member.repo_id}:${file.path}`);
      }
    }
  }
  const isUncommitted = (suggestion: ReviewSuggestion) =>
    suggestion.status === "accepted" &&
    (suggestion.kind === "hunk" || suggestion.kind === "new_file") &&
    suggestion.repoId !== null &&
    suggestion.path !== null &&
    uncommittedPaths.has(`${suggestion.repoId}:${suggestion.path}`);
  const acceptedInDirtyFiles =
    suggestions.data.suggestions.filter(isUncommitted).length;
  const currentGroup = visible.filter((publication) =>
    currentPublicationIds.has(publication.id),
  );
  // The current commit's open text changes to Markdown files are shown by
  // paragraph, above the review cards (`g5.governance.review.items`).
  const sectionFiles = reviewFileSections(
    suggestions.data.suggestions,
    new Set(currentGroup.map((publication) => publication.id)),
  );
  const sectionIds = new Set(
    sectionFiles.flatMap((file) =>
      file.suggestions.map((suggestion) => suggestion.id),
    ),
  );
  // Proposals competing within a paragraph section share a box there.
  const competing = competingClusters(currentSuggestions).filter((cluster) =>
    cluster.suggestions.some((suggestion) => !sectionIds.has(suggestion.id)),
  );
  const outdatedGroups: {
    key: string;
    label: string;
    publications: ReviewPublicationSummary[];
  }[] = [];
  for (const publication of visible) {
    if (currentPublicationIds.has(publication.id)) continue;
    const key = commitGroupKey(publication) || "unknown";
    let group = outdatedGroups.find((candidate) => candidate.key === key);
    if (!group) {
      group = {
        key,
        label: publicationCommitLabel(publication, repoNames),
        publications: [],
      };
      outdatedGroups.push(group);
    }
    group.publications.push(publication);
  }
  // A round being answered or decided that the source's next commit moves
  // out of the current group opens the group it joins, as if the person
  // had; they can close it again.
  const joining = outdatedGroups.filter(
    (group) =>
      !openedGroups.has(group.key) &&
      group.publications.some(
        (publication) =>
          activePublicationIds.has(publication.id) &&
          placedBefore.currentGroup.has(publication.id),
      ),
  );
  if (joining.length > 0) {
    setOpenedGroups(
      new Set([...openedGroups, ...joining.map((group) => group.key)]),
    );
  }
  const currentIds = current.map((publication) => publication.id);
  const currentGroupIds = currentGroup.map((publication) => publication.id);
  if (
    !sameIds(placedBefore.current, currentIds) ||
    !sameIds(placedBefore.currentGroup, currentGroupIds)
  ) {
    setPlacedBefore({
      current: new Set(currentIds),
      currentGroup: new Set(currentGroupIds),
    });
  }
  const groupOpened = (group: (typeof outdatedGroups)[number]) =>
    openedGroups.has(group.key) || joining.includes(group);
  // A suggestion competing for the same lines is shown in its cluster; its
  // round points there.
  const clusteredIds = new Set(
    competing.flatMap((cluster) =>
      cluster.suggestions.map((suggestion) => suggestion.id),
    ),
  );
  const displayedPublicationIds = new Set([
    ...currentGroup.map((publication) => publication.id),
    ...outdatedGroups
      .filter(groupOpened)
      .flatMap((group) => group.publications.map((p) => p.id)),
  ]);
  const displayedSuggestionIds = suggestions.data.suggestions
    .filter(
      (suggestion) =>
        displayedPublicationIds.has(suggestion.publicationId) ||
        clusteredIds.has(suggestion.id),
    )
    .map((suggestion) => suggestion.id);
  if (displayedSuggestionIds.some((id) => !shownSuggestionIds.has(id))) {
    setShownSuggestionIds(
      new Set([...shownSuggestionIds, ...displayedSuggestionIds]),
    );
  }
  const cardSuggestions = suggestions.data.suggestions.filter(
    (suggestion) =>
      shownSuggestionIds.has(suggestion.id) ||
      displayedSuggestionIds.includes(suggestion.id),
  );
  const publicationsById = new Map(
    publications.data.publications.map((publication) => [
      publication.id,
      publication,
    ]),
  );
  const renderCard = (suggestion: ReviewSuggestion) => (
    <SuggestionCard
      workspaceId={workspaceId}
      suggestion={suggestion}
      viewerOwnsWorkspace={viewerOwnsWorkspace}
      viewerIsReviewer={
        publicationsById.get(suggestion.publicationId)?.reviewerUserId ===
        viewerUserId
      }
      ownerVmRunning={ownerVmRunning}
      sourceWritable={sourceWritable}
      sourceClosedNote={sourceClosedNote}
      onOpenFile={openSuggestionFile}
      outdatedOf={
        currentPublicationIds.has(suggestion.publicationId)
          ? null
          : workspaceName
      }
      uncommitted={isUncommitted(suggestion)}
    />
  );
  const renderCards = (
    list: ReviewPublicationSummary[],
    outdatedOf: string | null,
  ) => (
    <ul className="space-y-3">
      {list.map((publication) => (
        <li key={publication.id}>
          <PublicationCard
            workspaceId={workspaceId}
            publication={publication}
            author={authorName(publication, names)}
            findings={findingsByPublication.get(publication.id) ?? []}
            suggestions={suggestionsByPublication.get(publication.id) ?? []}
            viewerOwnsWorkspace={viewerOwnsWorkspace}
            ownerVmRunning={ownerVmRunning}
            sourceWritable={sourceWritable}
            outdatedOf={outdatedOf}
            cardNodes={cardNodes}
            clusteredIds={clusteredIds}
            sectionIds={sectionIds}
            delta={roundDelta(
              publication,
              publications.data.publications,
              suggestions.data.suggestions,
            )}
          />
        </li>
      ))}
    </ul>
  );

  return (
    <>
      <div data-testid="reviews-panel" className="space-y-4">
        {refreshError && (
          // A failed refresh keeps the cards, and what is typed in them.
          <p
            role="status"
            data-testid="reviews-panel-refresh-error"
            className="text-xs text-[var(--g5-blocked)]"
          >
            Could not refresh reviews: {refreshError.message}. Showing what was
            loaded earlier.
          </p>
        )}
        <div className="flex flex-wrap items-center gap-2 text-xs text-[var(--g5-text2)]">
          <span hidden={!showCounts} data-testid="reviews-panel-open-count">
            {openOutdated > 0
              ? `${openCurrent} open · ${openOutdated} outdated`
              : openCount === 1
                ? "1 open suggestion"
                : `${openCount} open suggestions`}
            {" · "}
            {current.length === 1 ? "1 review" : `${current.length} reviews`}
          </span>
          {viewerOwnsWorkspace &&
            !ownerVmRunning &&
            openNeedingVm > 0 &&
            acceptBlockedReason && (
              <span
                className="inline-flex items-center gap-1.5"
                data-testid="reviews-panel-vm-stopped"
              >
                <Chip state="warn" text={acceptBlockedReason} />
                {canStartForAccept && (
                  <Button
                    kind="secondary"
                    className="!px-2 !py-0.5 !text-xs"
                    data-testid="reviews-panel-start-workspace"
                    title="Starts this workspace's VM, the same as Start in the header"
                    onClick={requestWorkspaceStart}
                  >
                    Start
                  </Button>
                )}
              </span>
            )}
          {viewerOwnsWorkspace && (
            <span className="inline-flex items-center gap-2">
              <span
                role="status"
                className="text-xs text-[var(--g5-text2)]"
                data-testid="reviews-panel-accepted-uncommitted"
              >
                {acceptedInDirtyFiles > 0
                  ? `${acceptedInDirtyFiles} accepted change${
                      acceptedInDirtyFiles === 1 ? "" : "s"
                    } in files with uncommitted edits`
                  : ""}
              </span>
            </span>
          )}
          {earlier.length > 0 && (
            <button
              type="button"
              className="ml-auto text-xs text-[var(--g5-info)] hover:underline"
              data-testid="reviews-panel-toggle-earlier"
              onClick={() => setShowEarlier((value) => !value)}
            >
              {showEarlier
                ? "Hide earlier rounds"
                : `Show ${earlier.length} earlier round${earlier.length === 1 ? "" : "s"}`}
            </button>
          )}
        </div>

        {competing.length > 0 && (
          <section
            data-testid="reviews-panel-competing"
            className="space-y-3 rounded-[var(--g5-radius-md)] border border-[var(--g5-warn)]/60 bg-[var(--g5-warn)]/5 p-4"
          >
            <div>
              <p className="text-sm font-semibold text-[var(--g5-text0)]">
                Competing proposals
              </p>
              <p className="text-xs text-[var(--g5-text2)]">
                Several reviewers changed the same lines. Accepting one retires
                the others for those lines.
              </p>
            </div>
            {competing.map((cluster) => (
              <div
                key={cluster.key}
                data-testid={`reviews-panel-competing-${cluster.key}`}
                className="space-y-2"
              >
                <p className="font-mono text-xs text-[var(--g5-text1)]">
                  {cluster.path}:{cluster.start}
                  {cluster.end - cluster.start > 1 ? `-${cluster.end - 1}` : ""}
                </p>
                <div className="grid gap-2 md:grid-cols-2">
                  {cluster.suggestions.map((suggestion) => (
                    <div
                      key={suggestion.id}
                      className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
                    >
                      <p className="border-b border-[var(--g5-stroke)] px-3 py-1.5 text-xs font-medium text-[var(--g5-text0)]">
                        {suggestion.authorLabel}
                      </p>
                      <SuggestionCardSlot
                        nodes={cardNodes}
                        suggestionId={suggestion.id}
                      />
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </section>
        )}

        {visible.length === 0 ? (
          <p
            data-testid="reviews-panel-empty"
            className="rounded-[var(--g5-radius-md)] border border-dashed border-[var(--g5-stroke)] px-4 py-6 text-center text-sm text-[var(--g5-text2)]"
          >
            No reviews yet. A reviewer pushes suggestions from their review
            workspace.
          </p>
        ) : (
          <div className="space-y-4">
            {currentGroup.length > 0 && (
              <section
                data-testid="reviews-commit-group-current"
                className="space-y-2"
              >
                <h3 className="text-xs font-semibold text-[var(--g5-text1)]">
                  Reviewed at {sourceHeadsLabel(sourceHeads, repoNames)} ·
                  current commit
                </h3>
                {sectionFiles.map((file) => (
                  <ReviewFileSection
                    key={`${file.repoId}:${file.path}`}
                    workspaceId={workspaceId}
                    file={file}
                    reviewerName={(publicationId) => {
                      const publication = publicationsById.get(publicationId);
                      return publication
                        ? authorName(publication, names)
                        : "Reviewer";
                    }}
                    publishedAt={(publicationId) =>
                      publicationsById.get(publicationId)?.createdAt ?? null
                    }
                    overlay={{
                      workspaceId,
                      viewerOwnsWorkspace,
                      viewerUserId,
                      ownerVmRunning,
                      sourceWritable,
                      sourceClosedNote,
                    }}
                    cardNodes={cardNodes}
                    onOpenFile={onOpenFile}
                    onSettled={markSectionSettled}
                  />
                ))}
                {renderCards(currentGroup, null)}
              </section>
            )}
            {outdatedGroups.map((group) => (
              <OutdatedCommitGroup
                key={group.key}
                groupKey={group.key}
                opened={groupOpened(group)}
                onToggle={() =>
                  setOpenedGroups((opened) => {
                    const next = new Set(opened);
                    if (groupOpened(group)) next.delete(group.key);
                    else next.add(group.key);
                    return next;
                  })
                }
                label={group.label}
                openCount={group.publications.reduce(
                  (sum, publication) => sum + publication.openCount,
                  0,
                )}
                acceptedCount={group.publications.reduce(
                  (sum, publication) => sum + publication.acceptedCount,
                  0,
                )}
              >
                {renderCards(group.publications, workspaceName)}
              </OutdatedCommitGroup>
            ))}
          </div>
        )}
      </div>
      <SuggestionCardPortals
        nodes={cardNodes}
        suggestions={cardSuggestions}
        renderCard={renderCard}
      />
    </>
  );
};

/** Whether `set` holds exactly `ids`. */
function sameIds(set: ReadonlySet<string>, ids: readonly string[]): boolean {
  return set.size === ids.length && ids.every((id) => set.has(id));
}

// ---------------------------------------------------------------------------
// OutdatedCommitGroup
// ---------------------------------------------------------------------------

/**
 * Rounds about an earlier commit of the source, collapsed under that
 * commit: what the owner already moved past, kept for the record and for
 * a suggestion that still fits.
 */
const OutdatedCommitGroup: React.FC<{
  groupKey: string;
  /** Open: chosen by the person, or holding a card being answered. */
  opened: boolean;
  onToggle: () => void;
  label: string;
  openCount: number;
  acceptedCount: number;
  children: React.ReactNode;
}> = ({
  groupKey,
  opened: expanded,
  onToggle,
  label,
  openCount,
  acceptedCount,
  children,
}) => {
  return (
    <section
      data-testid={`reviews-commit-group-${groupKey}`}
      className="space-y-2"
    >
      <button
        type="button"
        className="inline-flex items-center gap-1 text-xs font-semibold text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"
        data-testid={`reviews-commit-toggle-${groupKey}`}
        aria-expanded={expanded}
        onClick={onToggle}
      >
        {expanded ? "▾" : "▸"} Reviewed at {label} · outdated: {openCount} open,{" "}
        {acceptedCount} accepted
      </button>
      {expanded && children}
    </section>
  );
};

// ---------------------------------------------------------------------------
// PublicationCard
// ---------------------------------------------------------------------------

interface PublicationCardProps {
  workspaceId: string;
  publication: ReviewPublicationSummary;
  author: string;
  findings: ReviewFinding[];
  suggestions: ReviewSuggestion[];
  viewerOwnsWorkspace: boolean;
  ownerVmRunning: boolean;
  /** The source accepts thread messages and decisions. */
  sourceWritable: boolean;
  /** How this round differs from the one it replaced. */
  delta?: RoundDelta | null;
  /** Set (to the source's name) when the round is about an earlier commit. */
  outdatedOf?: string | null;
  /** The nodes the panel renders each suggestion's card into. */
  cardNodes: SuggestionCardNodes;
  /** Suggestions shown with their competing proposals, not in their round. */
  clusteredIds: ReadonlySet<string>;
  /** Changes shown above, with their paragraphs (`ReviewFileSection`). */
  sectionIds: ReadonlySet<string>;
}

const PublicationCard: React.FC<PublicationCardProps> = ({
  workspaceId,
  publication,
  author,
  findings,
  suggestions,
  viewerOwnsWorkspace,
  ownerVmRunning,
  sourceWritable,
  delta = null,
  outdatedOf = null,
  cardNodes,
  clusteredIds,
  sectionIds,
}) => {
  const verdict = verdictChip(publication.verdict);
  const inSections = suggestions.filter((suggestion) =>
    sectionIds.has(suggestion.id),
  ).length;
  const findingAcceptBlockedReason = useLiveGateReason("accept suggestions");
  const superseded = publication.supersededBy !== null;
  const ungrouped = suggestions.filter(
    (suggestion) =>
      suggestion.findingId === null && !sectionIds.has(suggestion.id),
  );
  const accept = useAcceptReviewSuggestion(workspaceId);
  // Decisions are sent only for the principal this was rendered for
  // (`g5.security.login.principal`).
  const principal = useRenderedPrincipal();
  const [acceptAllError, setAcceptAllError] = useState<string | null>(null);

  const acceptFinding = async (finding: ReviewFinding) => {
    if (!viewerOwnsWorkspace || !sourceWritable) return;
    setAcceptAllError(null);
    const open = suggestions.filter(
      (suggestion) =>
        suggestion.findingId === finding.id &&
        suggestion.status === "open" &&
        (suggestion.kind === "hunk" || suggestion.kind === "new_file"),
    );
    for (const suggestion of open) {
      try {
        await accept.mutateAsync({
          suggestionId: suggestion.parentSuggestionId ?? suggestion.id,
          editId: suggestion.editId,
          principal,
        });
      } catch (error) {
        setAcceptAllError(
          `${suggestion.path ?? "file"}: ${error instanceof Error ? error.message : "could not accept"}`,
        );
        return;
      }
    }
  };

  return (
    <article
      data-testid={`review-publication-${publication.id}`}
      className={classNames(
        "overflow-hidden rounded-[var(--g5-radius-md)] border bg-[var(--g5-bg1)]",
        superseded
          ? "border-[var(--g5-stroke)] opacity-70"
          : "border-[var(--g5-active)]/40",
      )}
    >
      <header className="flex flex-wrap items-center gap-2 border-b border-[var(--g5-stroke)] px-4 py-3">
        <span className="text-sm font-semibold text-[var(--g5-text0)]">
          {author}
        </span>
        <Chip state={verdict.state} text={verdict.text} />
        <span className="text-xs text-[var(--g5-text2)]">
          {/* Only a review round has a round number to show; an annotation
              (a reader's file suggestions, stand-alone comments) is not one
              (`g5.governance.file-suggestions.record`). */}
          {(publication.kind ?? "round") === "round"
            ? `Round ${publication.round}`
            : "Suggestions"}{" "}
          · {formatDateTime(publication.createdAt)}
        </span>
        {roundDeltaText(delta) && (
          <span
            className="text-xs text-[var(--g5-text2)]"
            data-testid={`review-round-delta-${publication.id}`}
          >
            {roundDeltaText(delta)}
          </span>
        )}
        {superseded && <Chip state="muted" text="Earlier round" />}
        {outdatedOf && (
          <Chip
            state="muted"
            text="Outdated"
            data-testid={`review-round-outdated-${publication.id}`}
          />
        )}
        <span className="ml-auto text-xs text-[var(--g5-text2)]">
          {publication.openCount} open · {publication.acceptedCount} accepted ·{" "}
          {publication.rejectedCount} rejected
        </span>
      </header>
      {publication.summary && (
        <p className="whitespace-pre-wrap px-4 py-3 text-sm text-[var(--g5-text1)]">
          {publication.summary}
        </p>
      )}
      <div className="space-y-3 px-4 pb-4">
        {inSections > 0 && (
          <p
            className="text-xs text-[var(--g5-text2)]"
            data-testid={`review-publication-in-sections-${publication.id}`}
          >
            {inSections === 1
              ? "1 change is shown above, with its paragraph."
              : `${inSections} changes are shown above, with their paragraphs.`}
          </p>
        )}
        {findings.map((finding) => {
          // Accept finding takes every open change of the finding, shown
          // here or with its paragraph above; only the cards listed here
          // leave out the ones shown above.
          const acceptable = suggestions.some(
            (suggestion) =>
              suggestion.findingId === finding.id &&
              suggestion.status === "open" &&
              (suggestion.kind === "hunk" || suggestion.kind === "new_file"),
          );
          const own = suggestions.filter(
            (suggestion) =>
              suggestion.findingId === finding.id &&
              !sectionIds.has(suggestion.id),
          );
          return (
            <section
              key={finding.id}
              data-testid={`review-finding-${finding.id}`}
              className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]"
            >
              <div className="flex flex-wrap items-center gap-2 px-3 py-2">
                <Chip
                  state={severityChip(finding.severity)}
                  text={finding.severity}
                />
                <span className="text-sm font-medium text-[var(--g5-text0)]">
                  {finding.title}
                </span>
                {finding.dimension && (
                  <span className="text-xs text-[var(--g5-text2)]">
                    {finding.dimension}
                  </span>
                )}
                {finding.paths.length > 0 && (
                  <span
                    className="font-mono text-xs text-[var(--g5-text2)]"
                    data-testid={`review-finding-paths-${finding.id}`}
                  >
                    {finding.paths.join(", ")}
                  </span>
                )}
                {viewerOwnsWorkspace && sourceWritable && acceptable && (
                  <Button
                    kind="secondary"
                    className="ml-auto !px-2.5 !py-1 !text-xs"
                    data-testid={`review-finding-accept-${finding.id}`}
                    disabled={!ownerVmRunning || accept.isLoading}
                    title={
                      ownerVmRunning
                        ? undefined
                        : (findingAcceptBlockedReason ??
                          "Start the workspace to accept suggestions")
                    }
                    onClick={() => void acceptFinding(finding)}
                  >
                    Accept finding
                  </Button>
                )}
              </div>
              {finding.evidence && (
                <p className="whitespace-pre-wrap border-t border-[var(--g5-stroke)] px-3 py-2 text-xs text-[var(--g5-text1)]">
                  {finding.evidence}
                </p>
              )}
              {own.length > 0 && (
                <ul className="divide-y divide-[var(--g5-stroke)] border-t border-[var(--g5-stroke)]">
                  {own.map((suggestion) => (
                    <li key={suggestion.id}>
                      <SuggestionPlace
                        cardNodes={cardNodes}
                        suggestionId={suggestion.id}
                        clustered={clusteredIds.has(suggestion.id)}
                      />
                    </li>
                  ))}
                </ul>
              )}
            </section>
          );
        })}
        {acceptAllError && (
          <p className="text-xs text-[var(--g5-blocked)]">{acceptAllError}</p>
        )}
        {ungrouped.length > 0 && (
          <ul className="divide-y divide-[var(--g5-stroke)] rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]">
            {ungrouped.map((suggestion) => (
              <li key={suggestion.id}>
                <SuggestionPlace
                  cardNodes={cardNodes}
                  suggestionId={suggestion.id}
                  clustered={clusteredIds.has(suggestion.id)}
                />
              </li>
            ))}
          </ul>
        )}
        {findings.length === 0 &&
          ungrouped.length === 0 &&
          inSections === 0 && (
            <p className="text-xs text-[var(--g5-text2)]">
              No suggestions in this round.
            </p>
          )}
      </div>
    </article>
  );
};

/**
 * A suggestion's place in its round: its card, or, while it competes with
 * other rounds' proposals for the same lines, a pointer to where it is shown.
 */
const SuggestionPlace: React.FC<{
  cardNodes: SuggestionCardNodes;
  suggestionId: string;
  clustered: boolean;
}> = ({ cardNodes, suggestionId, clustered }) =>
  clustered ? (
    <p
      data-testid={`review-suggestion-competing-${suggestionId}`}
      className="px-3 py-2 text-xs text-[var(--g5-text2)]"
    >
      Shown above, with the proposals competing for the same lines.
    </p>
  ) : (
    <SuggestionCardSlot nodes={cardNodes} suggestionId={suggestionId} />
  );

// ---------------------------------------------------------------------------
// SuggestionCard
// ---------------------------------------------------------------------------

export interface SuggestionCardProps {
  workspaceId: string;
  suggestion: ReviewSuggestion;
  viewerOwnsWorkspace: boolean;
  viewerIsReviewer: boolean;
  ownerVmRunning: boolean;
  /** Tighter layout for in-file overlays (rail, view zone, doc block). */
  compact?: boolean;
  /** DocumentPreview owns its rich summary; source-editor cards retain the Git diff. */
  proposalPresentation?: React.ReactNode;
  /** Placement/context that must remain visible when the draft replaces the summary. */
  proposalEditContext?: React.ReactNode;
  /** Opens the file the suggestion is about (panel cards; in-file cards are already there). */
  onOpenFile?: (suggestion: ReviewSuggestion) => void;
  /**
   * Set (to the source's name) when the suggestion's round is about an
   * earlier commit: it may no longer apply, though the owner may still
   * accept one that does.
   */
  outdatedOf?: string | null;
  /**
   * The accepted written proposal's file still holds uncommitted edits
   * (the owner's live changes report lists it).
   */
  uncommitted?: boolean;
  /**
   * The source accepts thread messages and decisions. A read-only, freezing
   * or Base source offers neither, and neither does one whose state is not
   * known: the discussion stays readable
   * (`g5.governance.review-suggestions.display` follows the comment
   * controls rule). Callers decide it with `sourceThreadWrites`.
   */
  sourceWritable?: boolean;
  /** Why the source takes no replies, shown where the reply box would be. */
  sourceClosedNote?: string;
}

/**
 * Open hunk suggestions on the same file whose replaced lines overlap,
 * from more than one publication: competing proposals the owner chooses
 * between. Accepting one retires the others (server side).
 */
export interface CompetingCluster {
  key: string;
  repoId: string;
  path: string;
  start: number;
  end: number;
  suggestions: ReviewSuggestion[];
}

/**
 * The base lines a hunk actually edits, from a line diff of the replaced
 * block against the replacement (the server's `hunk_edited_spans`):
 * replaced lines as `[start, end)` spans and insertion points as empty
 * spans `[p, p]` (the insertion goes before line `p`). Lines the
 * replacement keeps are not edited. Beyond the exact-diff bound the hunk
 * has no spans and joins no competing group, as on the server.
 */
export function hunkSpans(suggestion: ReviewSuggestion): [number, number][] {
  if (suggestion.kind !== "hunk" || suggestion.anchorStart === null) {
    return [];
  }
  const anchor = suggestion.anchorStart;
  const before = splitHunkLines(suggestion.beforeLines);
  const after = splitHunkLines(suggestion.afterLines);
  const n = before.length;
  const m = after.length;
  if (n * m > MAX_EDIT_DIFF_CELLS) return [];
  const spans: [number, number][] = [];
  const replace = (at: number) => {
    const last = spans[spans.length - 1];
    if (last && last[0] !== last[1] && last[1] >= at) {
      last[1] = Math.max(last[1], at + 1);
      return;
    }
    // A replacement is walked as its inserted lines first, then the removed
    // line: that insertion point becomes the interval.
    if (last && last[0] === at && last[1] === at) {
      last[1] = at + 1;
      return;
    }
    spans.push([at, at + 1]);
  };
  const insert = (at: number) => {
    const last = spans[spans.length - 1];
    if (last && last[0] !== last[1] && last[1] === at) return;
    if (last && last[0] === at && last[1] === at) return;
    spans.push([at, at]);
  };
  const table: number[][] = Array.from({ length: n + 1 }, () =>
    new Array<number>(m + 1).fill(0),
  );
  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      table[i][j] =
        before[i] === after[j]
          ? table[i + 1][j + 1] + 1
          : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }
  let i = 0;
  let j = 0;
  while (i < n || j < m) {
    if (i < n && j < m && before[i] === after[j]) {
      i += 1;
      j += 1;
    } else if (j < m && (i >= n || table[i][j + 1] >= table[i + 1][j])) {
      insert(i);
      j += 1;
    } else {
      replace(i);
      i += 1;
    }
  }
  return spans.map(([s, e]) => [anchor + s, anchor + e]);
}

/** The largest before × after line table diffed exactly (mirrors the server). */
const MAX_EDIT_DIFF_CELLS = 1_000_000;

type SourcePoint = [number, number];
type SourceSpan = [SourcePoint, SourcePoint];
const compareSourcePoint = (a: SourcePoint, b: SourcePoint) =>
  a[0] - b[0] || a[1] - b[1];

function suggestionSourceSpans(suggestion: ReviewSuggestion): SourceSpan[] {
  const edit = suggestion.reviewEdit;
  if (!edit)
    return hunkSpans(suggestion).map(([start, end]) => [
      [start, 0],
      [end, 0],
    ]);
  if (suggestion.anchorStart === null) return [];
  // Competition is keyed by the immutable captured contentHash. Accepted
  // siblings may shift the current display frame, but never these bounds.
  const source = suggestion.parentSuggestionId
    ? suggestion.parentBeforeLines
    : suggestion.beforeLines;
  if (source === null || source === undefined) return [];
  const point = (offset: number): SourcePoint | null => {
    const index = utf8OffsetToUtf16(source, offset);
    if (index === undefined) return null;
    const prefix = source.slice(0, index);
    const lines = prefix.split("\n");
    return [
      suggestion.anchorStart! + lines.length - 1,
      new TextEncoder().encode(lines.at(-1)!).length,
    ];
  };
  const start = point(edit.start),
    end = point(edit.end);
  return start && end ? [[start, end]] : [];
}

function spansOverlap(a: SourceSpan[], b: SourceSpan[]): boolean {
  // Two replaced intervals that intersect, two insertions at one point, or
  // an insertion strictly inside a replaced interval (the server's rule).
  const touches = ([s1, e1]: SourceSpan, [s2, e2]: SourceSpan) => {
    const p1 = compareSourcePoint(s1, e1) === 0;
    const p2 = compareSourcePoint(s2, e2) === 0;
    if (!p1 && !p2)
      return compareSourcePoint(s1, e2) < 0 && compareSourcePoint(s2, e1) < 0;
    if (p1 && p2) return compareSourcePoint(s1, s2) === 0;
    if (p1)
      return compareSourcePoint(s2, s1) < 0 && compareSourcePoint(s1, e2) < 0;
    return compareSourcePoint(s1, s2) < 0 && compareSourcePoint(s2, e1) < 0;
  };
  return a.some((x) => b.some((y) => touches(x, y)));
}

export function competingClusters(
  suggestions: ReviewSuggestion[],
): CompetingCluster[] {
  const open = suggestions.filter(
    (suggestion) =>
      suggestion.status === "open" &&
      suggestion.kind === "hunk" &&
      suggestion.repoId !== null &&
      suggestion.path !== null &&
      suggestion.anchorStart !== null,
  );
  // Line numbers only compare within one published version of a file, so
  // proposals computed against different source versions are never grouped.
  const byFile = new Map<string, ReviewSuggestion[]>();
  for (const suggestion of open) {
    const key = `${suggestion.repoId}\u0000${suggestion.path}\u0000${suggestion.contentHash ?? ""}`;
    const list = byFile.get(key) ?? [];
    list.push(suggestion);
    byFile.set(key, list);
  }
  const clusters: CompetingCluster[] = [];
  for (const list of byFile.values()) {
    const entries = list
      .map((suggestion) => ({
        suggestion,
        spans: suggestionSourceSpans(suggestion),
      }))
      .filter((entry) => entry.spans.length > 0)
      .sort((a, b) => compareSourcePoint(a.spans[0][0], b.spans[0][0]));
    // Proposals compete when the lines they edit overlap, transitively.
    const groups: { spans: SourceSpan[]; entries: typeof entries }[] = [];
    for (const entry of entries) {
      // A hunk bridging several groups joins them into one: accepting it
      // retires every proposal it overlaps, so the choice shows them all.
      const touching = groups.filter((candidate) =>
        candidate.entries.some(
          (other) =>
            (other.suggestion.parentSuggestionId ?? other.suggestion.id) !==
              (entry.suggestion.parentSuggestionId ?? entry.suggestion.id) &&
            spansOverlap(other.spans, entry.spans),
        ),
      );
      if (touching.length === 0) {
        groups.push({ spans: [...entry.spans], entries: [entry] });
        continue;
      }
      const [first, ...rest] = touching;
      first.entries.push(entry);
      first.spans.push(...entry.spans);
      for (const other of rest) {
        first.entries.push(...other.entries);
        first.spans.push(...other.spans);
        groups.splice(groups.indexOf(other), 1);
      }
      first.entries.sort((a, b) =>
        compareSourcePoint(a.spans[0][0], b.spans[0][0]),
      );
    }
    for (const group of groups) {
      const first = group.entries[0].suggestion;
      clusters.push({
        // Two versions of one file (a rebased review) can cluster at the
        // same place: the hash keeps the sections apart.
        key: `${first.repoId}:${first.path}:${first.contentHash ?? ""}:${group.spans[0][0]}`,
        repoId: first.repoId ?? "",
        path: first.path ?? "",
        start: Math.min(...group.spans.map(([s]) => s[0])),
        end: Math.max(...group.spans.map(([, e]) => e[0])),
        suggestions: group.entries.map((entry) => entry.suggestion),
      });
    }
  }
  return clusters.filter(
    (cluster) =>
      new Set(cluster.suggestions.map((s) => s.publicationId)).size > 1,
  );
}

/**
 * A card's view flag, handed over through the composer store between the
 * instances of the same suggestion's card: one shown in its paragraph, one
 * kept for its round or the fallback list. An instance starts from the
 * flag handed over and adopts each later hand-over. The flag itself is the
 * instance's: it is view state, not draft words, so neither a continuation
 * of the proposal nor a store replaced for another rendered principal (which
 * holds no hand-over) changes what this card shows.
 */
function useHandedOverFlag(
  subject: ComposerSubject,
  field: string,
): [boolean, (next: boolean | ((current: boolean) => boolean)) => void] {
  const [handed, hand] = useComposerField<boolean | null>(subject, field, null);
  const [value, setValue] = useState(handed ?? false);
  const [seen, setSeen] = useState(handed);
  if (handed !== seen) {
    setSeen(handed);
    if (handed !== null && handed !== value) setValue(handed);
  }
  const set = useCallback(
    (next: boolean | ((current: boolean) => boolean)) => {
      setValue((current) => {
        const flag = typeof next === "function" ? next(current) : next;
        hand(flag);
        return flag;
      });
    },
    [hand],
  );
  return [value, set];
}

export const SuggestionCard: React.FC<SuggestionCardProps> = ({
  workspaceId,
  suggestion,
  viewerOwnsWorkspace,
  viewerIsReviewer,
  ownerVmRunning,
  compact = false,
  proposalPresentation,
  proposalEditContext,
  onOpenFile,
  outdatedOf = null,
  uncommitted = false,
  sourceWritable = false,
  sourceClosedNote,
}) => {
  const accept = useAcceptReviewSuggestion(workspaceId);
  // Decisions are sent only for the principal this was rendered for
  // (`g5.security.login.principal`).
  const principal = useRenderedPrincipal();
  const [withdrawing, setWithdrawing] = useState(false);
  // Withdraw's confirm takes focus when it opens; Keep gives it back to
  // Withdraw. (The kit's Button forwards no ref: the row is queried.)
  const actionsRef = useRef<HTMLDivElement | null>(null);
  const [restoreWithdrawFocus, setRestoreWithdrawFocus] = useState(false);
  useEffect(() => {
    const row = actionsRef.current;
    if (!row) return;
    if (withdrawing) {
      row
        .querySelector<HTMLButtonElement>(
          `[data-testid="review-suggestion-withdraw-go-${suggestion.id}"]`,
        )
        ?.focus();
    } else if (restoreWithdrawFocus) {
      row
        .querySelector<HTMLButtonElement>(
          `[data-testid="review-suggestion-withdraw-${suggestion.id}"]`,
        )
        ?.focus();
      setRestoreWithdrawFocus(false);
    }
  }, [withdrawing, restoreWithdrawFocus, suggestion.id]);
  const reject = useRejectReviewSuggestion(workspaceId);
  const resolve = useResolveReviewSuggestion(workspaceId);
  const withdraw = useWithdrawReviewSuggestion(workspaceId);
  // An edited replacement, a rejection reason and a decision in flight or
  // refused belong to the page's owner: they outlive this card, and keep it
  // shown, read-only, if the suggestion is decided elsewhere meanwhile
  // (`g5.documents.comments.failure`).
  const subject = { kind: "suggestion" as const, id: suggestion.id };
  const targetId = suggestion.parentSuggestionId ?? suggestion.id;
  const withdrawEdits = !!suggestion.editId || !!suggestion.edits?.length;
  const withdrawLabel = withdrawEdits ? "Withdraw all open edits" : "Withdraw";
  const remainingOpenEdits =
    suggestion.parentOpenEditCount ??
    suggestion.edits?.filter((edit) => edit.status === "open").length;
  const withdrawConfirmation = withdrawEdits
    ? `Withdraw all ${remainingOpenEdits === undefined ? "" : `${remainingOpenEdits} `}remaining open ${remainingOpenEdits === 1 ? "edit" : "edits"} in this change?`
    : "Take this proposal back?";
  const proposed =
    suggestion.reviewEdit?.afterText ?? suggestion.afterLines ?? "";
  const [editing, setEditing] = useComposerField<boolean>(
    subject,
    "editing",
    false,
    isSet,
    suggestion.editId,
  );
  const [edited, setEdited] = useComposerField<string>(
    subject,
    "edited",
    proposed,
    undefined,
    suggestion.editId,
  );
  const [rejecting, setRejecting] = useComposerField<boolean>(
    subject,
    "rejecting",
    false,
    undefined,
    suggestion.editId,
  );
  const [reason, setReason] = useComposerField<string>(
    subject,
    "reason",
    "",
    holdsWords,
    suggestion.editId,
  );
  const [deciding, setDeciding] = useComposerField<string | null>(
    subject,
    "deciding",
    null,
    holdsValue,
  );
  const [decisionError, setDecisionError] = useComposerField<string | null>(
    subject,
    "decisionError",
    null,
    holdsValue,
  );
  const { remember: rememberSuggestion } = useComposerRecords<ReviewSuggestion>(
    "suggestion",
    isLaterSuggestion,
  );
  // Whether the discussion is open is handed to the next instance of this
  // suggestion's card (in its paragraph, its round, or the fallback list),
  // which shows the discussion as it was left. Once opened, the discussion
  // stays mounted while hidden: a typed reply, a send in flight and its
  // error are there when it is shown again.
  const [repliesOpen, setRepliesOpen] = useHandedOverFlag(
    subject,
    "repliesOpen",
  );
  const [repliesMounted, setRepliesMounted] = useHandedOverFlag(
    subject,
    "repliesMounted",
  );
  const [parentRepliesOpen, setParentRepliesOpen] = useHandedOverFlag(
    subject,
    "parentRepliesOpen",
  );
  // What a decision reported (publication unfinished, nothing written)
  // belongs to the page's owner too: the card that showed it may give way
  // to another instance of the same suggestion, in its round, once decided.
  // Kept, not active: a notice is read, it keeps no round in view.
  const [notice, setNotice] = useComposerField<string | null>(
    subject,
    "notice",
    null,
    undefined,
    suggestion.editId,
  );

  // A kept artifact is one the owner added to the workspace's review
  // artifacts; nothing was written, so "Accepted" would mislead.
  const status =
    suggestion.kind === "artifact" && suggestion.status === "accepted"
      ? { state: "ok" as const, text: "Kept" }
      : uncommitted
        ? {
            state: "ok" as const,
            text: "Accepted · file has uncommitted changes",
          }
        : statusChip(suggestion.status);
  const open = suggestion.status === "open";
  // Every decision writes to the source's threads.
  // Applying a text suggestion is a save into the running tree: the page's
  // gate names the state while it is off (`g5.workspace.ui.vm-state`).
  const cardAcceptBlockedReason = useLiveGateReason("accept suggestions");
  const decidable = open && sourceWritable;
  // Checked when sent too: a reason typed while the source was writable is
  // kept, but not sent once the source or its owner changes.
  const canReject = decidable && viewerOwnsWorkspace;
  const writable = suggestion.kind === "hunk" || suggestion.kind === "new_file";
  const busy = deciding !== null;
  const error = decisionError;
  // Once the suggestion is decided elsewhere, a reason stays readable and
  // nothing more can be sent.
  // A reason stays readable, read-only, whenever Reject is not offered: the
  // suggestion decided elsewhere, the source read-only, the ownership moved.
  const rejectFormShown =
    rejecting && (canReject || reason.trim().length > 0 || !!decisionError);
  // An edit can be accepted only where Accept is offered.
  const canAcceptHere = decidable && viewerOwnsWorkspace && writable;
  const dismissDecision = () => {
    setDecisionError(null);
    setRejecting(false);
    setReason("");
  };
  /** One decision at a time; its refusal is shown here. */
  const decide = async (
    action: "accept" | "reject" | "resolve" | "withdraw",
    run: () => Promise<ReviewSuggestionResult>,
  ): Promise<boolean> => {
    if (deciding !== null) return false;
    setDeciding(action);
    setDecisionError(null);
    try {
      const result = await run();
      // A card kept from an earlier commit shows the decision at once.
      const chosen = selectedReviewSuggestion(
        result.suggestion,
        suggestion.editId,
      );
      rememberSuggestion(chosen.id, chosen);
      return true;
    } catch (failure) {
      setDecisionError(
        failure instanceof Error && failure.message
          ? failure.message
          : "The decision failed.",
      );
      return false;
    } finally {
      setDeciding(null);
    }
  };

  const handleAccept = async () => {
    setNotice(null);
    const accepted = await decide("accept", async () => {
      const result = await accept.mutateAsync({
        suggestionId: targetId,
        editId: suggestion.editId,
        afterLines: editing ? edited : undefined,
        principal,
      });
      if (result.checkpointPending) {
        setNotice(
          "Accepted in the workspace, but commit/push did not finish. Review Changes and any unrelated edits before retrying publication.",
        );
      } else if (result.alreadyApplied) {
        setNotice("The file already contained this change; nothing written.");
      }
      return result;
    });
    if (accepted) {
      setEditing(false);
      setEdited(proposed);
    }
  };

  const handleReject = async () => {
    if (!canReject) return;
    const rejected = await decide("reject", () =>
      reject.mutateAsync({
        suggestionId: targetId,
        editId: suggestion.editId,
        reason: reason.trim() || undefined,
        principal,
      }),
    );
    if (rejected) {
      setRejecting(false);
      setReason("");
    }
  };

  return (
    <div
      data-testid={`review-suggestion-${suggestion.id}`}
      className={classNames("space-y-2 px-3 py-2", compact && "text-xs")}
    >
      <div className="flex flex-wrap items-center gap-2">
        {!compact && <Chip state="muted" text={kindLabel(suggestion.kind)} />}
        {compact && (
          <span
            className={
              proposalPresentation
                ? "text-sm font-semibold text-[var(--g5-text0)]"
                : "text-[11px] font-medium text-[var(--g5-warn)]"
            }
          >
            {proposalPresentation
              ? suggestion.authorLabel
              : `${kindLabel(suggestion.kind)} · ${suggestion.authorLabel}`}
          </span>
        )}
        {suggestion.supersedes && (
          <span
            className="text-[11px] text-[var(--g5-text2)]"
            title="Replaces the proposal from an earlier round; the discussion carries over."
          >
            updated round
          </span>
        )}
        {!proposalPresentation && suggestion.path && (
          <span className="font-mono text-xs text-[var(--g5-text0)]">
            {suggestion.path}
            {suggestion.anchorStart !== null &&
            (suggestion.kind === "hunk" || suggestion.kind === "comment")
              ? `:${suggestion.anchorStart}`
              : ""}
          </span>
        )}
        {suggestion.path &&
          onOpenFile &&
          !compact &&
          suggestion.kind !== "artifact" &&
          // A proposed new file exists in the workspace only once accepted;
          // until then its content is the card itself.
          (suggestion.kind !== "new_file" ||
            suggestion.status === "accepted") && (
            <button
              type="button"
              className="text-xs text-[var(--g5-info)] hover:underline"
              data-testid={`review-suggestion-open-file-${suggestion.id}`}
              title="Open the file in the editor, at these lines"
              onClick={() => onOpenFile(suggestion)}
            >
              Open file
            </button>
          )}
        <Chip
          state={status.state}
          text={status.text}
          data-testid={`review-suggestion-status-${suggestion.id}`}
        />
        {proposalPresentation && (
          <span className="text-[11px] text-[var(--g5-text2)]">
            {formatDateTime(suggestion.createdAt)}
          </span>
        )}
        {suggestion.statusAt && suggestion.status !== "open" && (
          <span className="ml-auto text-xs text-[var(--g5-text2)]">
            {formatDateTime(suggestion.statusAt)}
          </span>
        )}
      </div>
      {outdatedOf && (
        <p
          className="text-xs text-[var(--g5-text2)]"
          data-testid={`review-suggestion-outdated-${suggestion.id}`}
        >
          From an earlier commit of {outdatedOf}; it may no longer apply.
        </p>
      )}

      {editing && proposalEditContext}
      {suggestion.kind === "hunk" &&
        (!editing && proposalPresentation ? (
          proposalPresentation
        ) : (
          // While the owner edits, the diff is of what accepting would write.
          <ReviewHunkDiff
            before={
              suggestion.reviewEdit
                ? suggestion.reviewEdit.beforeText
                : suggestion.beforeLines
            }
            after={
              editing
                ? edited
                : (suggestion.reviewEdit?.afterText ?? suggestion.afterLines)
            }
            oldStart={
              suggestion.reviewEdit
                ? (suggestionSourceSpans(suggestion)[0]?.[0][0] ??
                  suggestion.anchorStart)
                : suggestion.anchorStart
            }
            newStart={
              suggestion.reviewEdit
                ? (suggestionSourceSpans(suggestion)[0]?.[0][0] ??
                  suggestion.anchorStart)
                : suggestion.anchorStart
            }
          />
        ))}
      {suggestion.kind === "artifact" && (
        <ArtifactLinks workspaceId={workspaceId} suggestion={suggestion} />
      )}
      {suggestion.kind === "new_file" && !editing && (
        <pre
          className={classNames(
            "max-h-64 overflow-auto rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
            "bg-[var(--g5-ok)]/5 px-3 py-2 font-mono text-xs leading-5 text-[var(--g5-text0)]",
          )}
        >
          {suggestion.afterLines}
        </pre>
      )}
      {editing && proposalPresentation && (
        <p className="text-xs text-[var(--g5-text2)]">
          The document view is unchanged. This diff previews the replacement
          Accept will submit.
        </p>
      )}
      {editing && (
        <textarea
          aria-label="Edited replacement"
          data-testid={`review-suggestion-edit-${suggestion.id}`}
          rows={Math.min(12, Math.max(3, edited.split("\n").length))}
          value={edited}
          // Locked while a decision is sent: success closes the edit, and
          // words typed meanwhile would close with it. Read-only, too, where
          // it can no longer be accepted.
          readOnly={!canAcceptHere || busy}
          onChange={(event) => setEdited(event.target.value)}
          className={classNames(inputClass, "font-mono text-xs")}
        />
      )}
      {editing && !canAcceptHere && (
        <p
          className="text-xs text-[var(--g5-text2)]"
          data-testid={`review-suggestion-edit-closed-${suggestion.id}`}
        >
          {open
            ? "Your edited replacement can no longer be accepted here."
            : "This suggestion was decided meanwhile, so your edited replacement can no longer be accepted."}
          <button
            type="button"
            data-testid={`review-suggestion-edit-dismiss-${suggestion.id}`}
            className="ml-2 text-[var(--g5-text1)] underline"
            onClick={() => {
              setEditing(false);
              setEdited(proposed);
            }}
          >
            Dismiss
          </button>
        </p>
      )}
      {suggestion.kind === "comment" && suggestion.beforeLines && (
        <pre
          data-testid={`review-suggestion-anchor-${suggestion.id}`}
          className="overflow-x-auto rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-1.5 font-mono text-xs text-[var(--g5-text2)]"
        >
          {suggestion.beforeLines}
        </pre>
      )}
      {suggestion.note && (
        <p className="whitespace-pre-wrap text-sm text-[var(--g5-text1)]">
          {suggestion.note}
        </p>
      )}
      {notice && <p className="text-xs text-[var(--g5-text2)]">{notice}</p>}
      {error && (
        <p
          data-testid={`review-suggestion-error-${suggestion.id}`}
          className="text-xs text-[var(--g5-blocked)]"
        >
          {error}
          {!open && (
            <button
              type="button"
              data-testid={`review-suggestion-error-dismiss-${suggestion.id}`}
              className="ml-2 text-[var(--g5-text1)] underline"
              onClick={dismissDecision}
            >
              Dismiss
            </button>
          )}
        </p>
      )}

      {rejectFormShown && (
        <div className="flex flex-wrap items-center gap-2">
          <input
            aria-label="Reason"
            data-testid={`review-suggestion-reject-reason-${suggestion.id}`}
            value={reason}
            // Locked while a decision is sent, like the edit above.
            readOnly={!canReject || busy}
            onChange={(event) => setReason(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !busy) {
                event.preventDefault();
                void handleReject();
              }
            }}
            placeholder="Why (optional)"
            className={classNames(inputClass, "flex-1")}
          />
          <Button
            kind="primary"
            className="!px-2.5 !py-1 !text-xs"
            data-testid={`review-suggestion-reject-confirm-${suggestion.id}`}
            disabled={busy || !canReject}
            title={canReject ? undefined : "This suggestion is no longer open"}
            onClick={() => void handleReject()}
          >
            Reject
          </Button>
          <Button
            kind="ghost"
            className="!px-2.5 !py-1 !text-xs"
            data-testid={`review-suggestion-reject-cancel-${suggestion.id}`}
            disabled={busy}
            // Cancelled, the reason goes with the form, so nothing keeps
            // the card shown that the card no longer offers.
            onClick={dismissDecision}
          >
            Cancel
          </Button>
        </div>
      )}

      <div ref={actionsRef} className="flex flex-wrap items-center gap-2">
        {decidable && viewerOwnsWorkspace && writable && (
          <>
            <Button
              kind="primary"
              className="!px-2.5 !py-1 !text-xs"
              data-testid={`review-suggestion-accept-${suggestion.id}`}
              disabled={busy || !ownerVmRunning}
              title={
                ownerVmRunning
                  ? undefined
                  : (cardAcceptBlockedReason ??
                    "Start the workspace to accept suggestions")
              }
              onClick={() => void handleAccept()}
            >
              {deciding === "accept" ? (
                <>
                  <Loader2
                    className="h-3.5 w-3.5 animate-spin"
                    aria-hidden="true"
                  />
                  Accepting…
                </>
              ) : editing ? (
                "Accept edited"
              ) : (
                "Accept"
              )}
            </Button>
            <Button
              kind="ghost"
              className="!px-2.5 !py-1 !text-xs"
              data-testid={`review-suggestion-edit-toggle-${suggestion.id}`}
              disabled={busy}
              onClick={() => {
                setEdited(proposed);
                setEditing((value) => !value);
              }}
            >
              {editing ? "Discard edits" : "Edit"}
            </Button>
          </>
        )}
        {decidable && viewerOwnsWorkspace && suggestion.kind === "comment" && (
          <Button
            kind="secondary"
            className="!px-2.5 !py-1 !text-xs"
            data-testid={`review-suggestion-resolve-${suggestion.id}`}
            disabled={busy}
            onClick={() =>
              void decide("resolve", () =>
                resolve.mutateAsync({
                  suggestionId: targetId,
                  editId: suggestion.editId,
                  principal,
                }),
              )
            }
          >
            Resolve
          </Button>
        )}
        {decidable && viewerOwnsWorkspace && suggestion.kind === "artifact" && (
          // Keeping needs no VM: the artifact joins the workspace's review
          // artifacts, it is never written into the tree.
          <Button
            kind="primary"
            className="!px-2.5 !py-1 !text-xs"
            data-testid={`review-artifact-keep-${suggestion.id}`}
            disabled={busy}
            title="Add to the workspace's review artifacts"
            onClick={() =>
              void decide("accept", () =>
                accept.mutateAsync({
                  suggestionId: targetId,
                  editId: suggestion.editId,
                  principal,
                }),
              )
            }
          >
            {deciding === "accept" ? "Keeping…" : "Keep"}
          </Button>
        )}
        {canReject && !rejecting && (
          <Button
            kind="ghost"
            className="!px-2.5 !py-1 !text-xs"
            data-testid={`review-suggestion-reject-${suggestion.id}`}
            disabled={busy}
            onClick={() => setRejecting(true)}
          >
            Reject
          </Button>
        )}
        {decidable && viewerIsReviewer && !withdrawing && (
          <Button
            kind="ghost"
            className="!px-2.5 !py-1 !text-xs"
            data-testid={`review-suggestion-withdraw-${suggestion.id}`}
            disabled={busy}
            onClick={() => setWithdrawing(true)}
          >
            {withdrawLabel}
          </Button>
        )}
        {decidable && viewerIsReviewer && withdrawing && (
          <span
            className="inline-flex items-center gap-2 text-xs text-[var(--g5-text2)]"
            data-testid={`review-suggestion-withdraw-confirm-${suggestion.id}`}
          >
            {withdrawConfirmation}
            <Button
              kind="secondary"
              className="!px-2.5 !py-1 !text-xs"
              data-testid={`review-suggestion-withdraw-go-${suggestion.id}`}
              disabled={busy}
              onClick={() => {
                setWithdrawing(false);
                void decide("withdraw", () =>
                  withdraw.mutateAsync({
                    suggestionId: targetId,
                    principal,
                  }),
                );
              }}
            >
              {withdrawLabel}
            </Button>
            <Button
              kind="ghost"
              className="!px-2.5 !py-1 !text-xs"
              data-testid={`review-suggestion-withdraw-keep-${suggestion.id}`}
              disabled={busy}
              onClick={() => {
                setWithdrawing(false);
                setRestoreWithdrawFocus(true);
              }}
            >
              Keep
            </Button>
          </span>
        )}
        {(suggestion.commentId || suggestion.editId) && (
          <button
            type="button"
            className="ml-auto inline-flex items-center gap-1 text-xs text-[var(--g5-text2)] hover:underline"
            data-testid={`review-suggestion-replies-toggle-${suggestion.id}`}
            onClick={() => {
              setRepliesMounted(true);
              setRepliesOpen((value) => !value);
            }}
          >
            <MessageSquare size={12} aria-hidden="true" />
            {repliesOpen ? "Hide discussion" : "Discussion"}
          </button>
        )}
      </div>
      {repliesMounted && (
        <div hidden={!repliesOpen}>
          <RepliesThread
            workspaceId={workspaceId}
            suggestionId={suggestion.id}
            parentSuggestionId={suggestion.parentSuggestionId}
            editId={suggestion.editId}
            canReply={sourceWritable}
            closedNote={sourceClosedNote}
          />
        </div>
      )}
      {suggestion.parentSuggestionId && suggestion.parentCommentId && (
        <div>
          <button
            type="button"
            className="text-xs text-[var(--g5-text2)] underline"
            data-testid={`review-parent-discussion-${suggestion.id}`}
            onClick={() => setParentRepliesOpen((value) => !value)}
          >
            Original review discussion
          </button>
          {parentRepliesOpen && (
            <RepliesThread
              workspaceId={workspaceId}
              suggestionId={suggestion.parentSuggestionId}
              canReply={false}
              closedNote="Original review context. Reply in this edit’s discussion."
            />
          )}
        </div>
      )}
    </div>
  );
};

// ---------------------------------------------------------------------------
// RepliesThread
// ---------------------------------------------------------------------------

const RepliesThread: React.FC<{
  workspaceId: string;
  suggestionId: string;
  parentSuggestionId?: string;
  editId?: string;
  /** Anyone who can read replies, unless the source is read-only. */
  canReply: boolean;
  /** Why replies are closed; the read-only reason by default. */
  closedNote?: string;
}> = ({
  workspaceId,
  suggestionId,
  parentSuggestionId,
  editId,
  canReply,
  closedNote,
}) => {
  const targetId = parentSuggestionId ?? suggestionId;
  const replies = useReviewSuggestionReplies(
    workspaceId,
    targetId,
    true,
    editId,
  );
  const add = useAddReviewSuggestionReply(workspaceId);
  // Replies are sent only for the principal this was rendered for
  // (`g5.security.login.principal`).
  const principal = useRenderedPrincipal();
  // The words, a send in flight and its error belong to the page's owner:
  // they outlive this discussion, and unsent words or a send keep the card
  // shown if its suggestion is decided meanwhile.
  const subject = { kind: "suggestion" as const, id: suggestionId };
  const [content, setContent] = useComposerField<string>(
    subject,
    "reply",
    "",
    holdsWords,
    editId,
  );
  // One reply per send: a second Enter while the first is in flight is
  // ignored rather than sent again.
  const [sending, setSending] = useComposerField<boolean>(
    subject,
    "replySending",
    false,
    isSet,
  );
  const [sendError, setSendError] = useComposerField<string | null>(
    subject,
    "replyError",
    null,
  );

  const handleSend = async () => {
    const text = content.trim();
    if (!text || sending) return;
    setSending(true);
    setSendError(null);
    try {
      await add.mutateAsync({
        suggestionId: targetId,
        editId,
        content: text,
        principal,
      });
      // Clear only what was sent, never newer typing.
      setContent((current) => (current.trim() === text ? "" : current));
    } catch (error) {
      // Shown here; the text stays.
      setSendError(
        error instanceof Error ? error.message : "The reply was not sent.",
      );
    } finally {
      setSending(false);
    }
  };

  return (
    <div
      data-testid={`review-suggestion-replies-${suggestionId}`}
      className="space-y-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2"
    >
      {replies.status === "ok" && replies.refreshError && (
        <p
          role="status"
          data-testid={`review-suggestion-replies-refresh-error-${suggestionId}`}
          className="text-xs text-[var(--g5-blocked)]"
        >
          Could not refresh the discussion: {replies.refreshError.message}
        </p>
      )}
      {replies.status === "loading" ? (
        <p className="text-xs text-[var(--g5-text2)]">Loading discussion…</p>
      ) : replies.status === "error" ? (
        <p className="text-xs text-[var(--g5-blocked)]">
          {replies.error.message}
        </p>
      ) : replies.data.replies.length === 0 ? (
        <p className="text-xs text-[var(--g5-text2)]">No replies yet.</p>
      ) : (
        <ul className="space-y-1.5">
          {replies.data.replies.map((reply) => (
            <li key={reply.id} className="text-xs">
              <span className="font-medium text-[var(--g5-text0)]">
                {reply.authorName}
              </span>{" "}
              <span className="text-[var(--g5-text2)]">
                {formatDateTime(reply.createdAt)}
              </span>
              <p className="whitespace-pre-wrap text-[var(--g5-text1)]">
                {reply.content}
              </p>
            </li>
          ))}
        </ul>
      )}
      {canReply ? (
        <div className="flex gap-2">
          <input
            aria-label="Reply"
            data-testid={`review-suggestion-reply-input-${suggestionId}`}
            value={content}
            onChange={(event) => setContent(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                void handleSend();
              }
            }}
            placeholder="Reply…"
            className={classNames(inputClass, "flex-1")}
          />
          <Button
            kind="secondary"
            className="!px-2.5 !py-1 !text-xs"
            data-testid={`review-suggestion-reply-send-${suggestionId}`}
            disabled={sending || !content.trim()}
            onClick={() => void handleSend()}
          >
            Reply
          </Button>
        </div>
      ) : (
        <>
          <p
            className="text-xs text-[var(--g5-text2)]"
            data-testid={`review-suggestion-replies-closed-${suggestionId}`}
          >
            {closedNote ||
              "The workspace is read-only, so its discussions accept no replies."}
          </p>
          {/* Words typed before replies closed stay readable until
              dismissed; nothing more is sent. */}
          {content.trim().length > 0 && (
            <div className="flex gap-2">
              <input
                aria-label="Reply not sent"
                data-testid={`review-suggestion-kept-reply-${suggestionId}`}
                value={content}
                readOnly
                className={classNames(inputClass, "flex-1")}
              />
              <Button
                kind="ghost"
                className="!px-2.5 !py-1 !text-xs"
                data-testid={`review-suggestion-reply-dismiss-${suggestionId}`}
                onClick={() => {
                  setContent("");
                  setSendError(null);
                }}
              >
                Dismiss
              </Button>
            </div>
          )}
        </>
      )}
      {sendError && (
        <p className="text-xs text-[var(--g5-blocked)]">{sendError}</p>
      )}
    </div>
  );
};

// ---------------------------------------------------------------------------
// Artifacts — documents made for the review, never written to the repository
// ---------------------------------------------------------------------------

function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * Fetch an artifact through the authenticated client and hand it to the
 * browser as an object URL. Anchors cannot carry the bearer token and the
 * artifact route accepts no cookies, so both Open and Download go through
 * `api`. Opened HTML is shown in a sandboxed frame (no same-origin), never
 * as a page of this app.
 */
async function fetchArtifactObjectUrl(
  workspaceId: string,
  suggestion: ReviewSuggestion,
  purpose: "preview" | "download",
): Promise<string> {
  const response = await api.getRawResponse(
    routes.workspaceReview.artifact(workspaceId, suggestion.id),
  );
  if (!response.ok) {
    throw new Error(
      response.status === 403
        ? "You can't open this artifact."
        : `Could not load the artifact (${response.status}).`,
    );
  }
  // A blob URL carries none of the artifact endpoint's response headers
  // and is a page of this origin: opened top-level (a viewer can), the
  // frame's `sandbox` no longer applies. So the previewed document carries
  // its own policy forbidding scripts and any load beyond itself; the
  // frame's `sandbox` is the second layer. A download keeps the uploaded
  // bytes untouched.
  const contentType = (
    response.headers.get("content-type") ?? ""
  ).toLowerCase();
  if (purpose === "preview" && contentType.startsWith("text/html")) {
    return URL.createObjectURL(
      new Blob([withScriptlessPreviewCsp(await response.text())], {
        type: "text/html",
      }),
    );
  }
  return URL.createObjectURL(await response.blob());
}

/** Raster image types a plain <img> shows without any active content. */
export function isInertImage(contentType: string | null | undefined): boolean {
  const type = (contentType ?? "").split(";")[0].trim().toLowerCase();
  return ["image/png", "image/jpeg", "image/gif", "image/webp"].includes(type);
}

const ArtifactLinks: React.FC<{
  workspaceId: string;
  suggestion: ReviewSuggestion;
}> = ({ workspaceId, suggestion }) => {
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const previewTitleId = useId();
  // HTML opens in a sandboxed frame, a raster image as a plain <img>; a
  // PDF is download-only, since browsers refuse their PDF viewer inside a
  // sandboxed frame and an unsandboxed one is not offered here. An SVG is
  // download-only too: it is a document that can carry scripts, and an
  // <img> preview's blob URL, opened in a new tab, would run them on this
  // origin with no sandbox and no policy.
  const isHtml = (suggestion.contentType ?? "").startsWith("text/html");
  const isImage = isInertImage(suggestion.contentType);
  const inline = isHtml || isImage;

  const handleDownload = async () => {
    setBusy(true);
    setError(null);
    try {
      const url = await fetchArtifactObjectUrl(
        workspaceId,
        suggestion,
        "download",
      );
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = suggestion.path ?? "artifact";
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      setTimeout(() => URL.revokeObjectURL(url), 60_000);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Download failed.");
    } finally {
      setBusy(false);
    }
  };

  const handleOpen = async () => {
    setBusy(true);
    setError(null);
    try {
      setPreviewUrl(
        await fetchArtifactObjectUrl(workspaceId, suggestion, "preview"),
      );
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Could not open.");
    } finally {
      setBusy(false);
    }
  };

  const closePreview = () => {
    if (previewUrl) URL.revokeObjectURL(previewUrl);
    setPreviewUrl(null);
  };

  return (
    <div
      data-testid={`review-artifact-${suggestion.id}`}
      className="flex flex-wrap items-center gap-3 text-xs"
    >
      <span className="text-[var(--g5-text2)]">
        {suggestion.contentType ?? "file"}
        {suggestion.bytes ? ` · ${formatBytes(suggestion.bytes)}` : ""}
      </span>
      {inline && (
        <button
          type="button"
          className="text-[var(--g5-info)] hover:underline disabled:opacity-60"
          data-testid={`review-artifact-open-${suggestion.id}`}
          disabled={busy}
          onClick={() => void handleOpen()}
        >
          Open
        </button>
      )}
      <button
        type="button"
        className="text-[var(--g5-info)] hover:underline disabled:opacity-60"
        data-testid={`review-artifact-download-${suggestion.id}`}
        disabled={busy}
        onClick={() => void handleDownload()}
      >
        Download
      </button>
      {error && <span className="text-[var(--g5-blocked)]">{error}</span>}
      {previewUrl && (
        <Modal
          labelledBy={previewTitleId}
          onClose={closePreview}
          size="xl"
          className="p-0"
          testId={`review-artifact-preview-${suggestion.id}`}
        >
          <div className="flex items-center justify-between border-b border-[var(--g5-stroke)] px-4 py-2">
            <h2
              id={previewTitleId}
              className="truncate text-sm font-semibold text-[var(--g5-text0)]"
            >
              {suggestion.path}
            </h2>
            <Button kind="ghost" onClick={closePreview}>
              Close
            </Button>
          </div>
          {isImage ? (
            <img
              alt={suggestion.path ?? "artifact"}
              src={previewUrl}
              className="max-h-[75vh] w-full object-contain bg-white"
            />
          ) : (
            <iframe
              title={suggestion.path ?? "artifact"}
              src={previewUrl}
              sandbox=""
              className="h-[75vh] w-full bg-white"
            />
          )}
        </Modal>
      )}
    </div>
  );
};
