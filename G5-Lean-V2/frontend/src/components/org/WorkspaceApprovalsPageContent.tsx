import { ApprovalReviewReader } from "./ApprovalReviewReader";
import { RosterAvatar } from "../workspace/approvalRoster";
import {
  ApprovalRequestComposer,
  type ApprovalRequestPrefill,
} from "../workspace-review/ApprovalRequestComposer";
import { ApprovalVersionIndicator } from "../workspace-review/ApprovalVersionIndicator";
import { PersonalApprovalsBanner } from "../workspace-review/PersonalApprovalsBanner";
import { ApprovalReviewsSection } from "../workspace-review/ApprovalReviewsSection";
import { randomUuid } from "../../lib/randomUuid";
import { WorkspaceReviewActions } from "../workspace-review/WorkspaceReviewActions";
import React, { useEffect, useId, useMemo, useRef, useState } from "react";
import {
  Link,
  useLocation,
  useParams,
  useSearchParams,
} from "react-router-dom";
import {
  AlertCircle,
  Bot,
  FileText,
  GitMerge,
  Info,
  MoreHorizontal,
  Plus,
  Search,
  UserRound,
} from "lucide-react";

import { useQuery } from "@tanstack/react-query";
import { useRetryApprovalCleanup } from "../../hooks/useWorkspaceApprovals";
import { Button } from "../ui/UI";
import { AnchoredPopup } from "../ui/AnchoredPopup";
import { useAuth } from "../../hooks/useAuth";
import {
  useWorkspace,
  workspacePullRequestsQueryOptions,
} from "../../hooks/useWorkspace";
import { useWorkspaceReviewFlow } from "../../hooks/useWorkspaceReviewFlow";
import {
  AGENT_RECIPIENT,
  hasOutstandingNeeds,
  outstandingPicks,
  requestCandidates,
  type ApprovalPick,
} from "../../lib/approvalSelection";
import {
  approvalPathText,
  buildRequirementRoster,
  CURRENTNESS_CIRCLE,
  currentnessGlyph,
  currentnessMessage,
  currentnessTone,
  requestAgainTarget,
  requestLabel,
  requirementBadge,
  rosterDecisionLabel,
  type ApprovalBadgeTone,
  type ApprovalRequirementRosterRow,
} from "../../lib/approvalRequirementRoster";
import { approvalRecommendation } from "../../lib/approvalPhases";
import { useOrgApprovalDefinitions } from "../../hooks/useOrgApprovals";
import {
  useAttachRequirement,
  useCancelApprovalReview,
  useRequestApprovals,
  useRetireRequirement,
  useWorkspaceApprovals,
} from "../../hooks/useWorkspaceApprovals";
import type {
  AgentApprovalReviewDTO,
  ApprovalDecisionLogEntryDTO,
  ApprovalRequestDTO,
  WorkspaceApprovalsStateDTO,
  WorkspaceOutgoingRequirementDTO,
  WorkspaceOutgoingHistoryDTO,
  WorkspaceRequiredIntoDTO,
} from "../../lib/types";
import { formatApprovalDateTime, formatDateTime } from "../../lib/dateTime";
import {
  buildApprovalTimeline,
  resolveApprovalEvidence,
  type ApprovalEvidence,
  type ApprovalHistoryCard,
  type ApprovalHistoryDecisionFilter,
  type ApprovalHistoryRequestStatus,
  type ApprovalTimelineEntry,
} from "../../lib/approvalHistory";

/* ------------------------------------------------------------------ */
/*  Approval history                                                   */
/* ------------------------------------------------------------------ */

function agentApproverName(definitionName: string): string {
  return `${definitionName} agent`;
}

const REQUEST_STATUS_FILTERS: ApprovalHistoryRequestStatus[] = [
  "Active",
  "Settled",
  "Failed",
  "Cancelled",
  "Expired",
  "Superseded",
  "Bypassed",
  "Unknown",
];

function evidenceForCard(card: ApprovalHistoryCard): ApprovalEvidence {
  return resolveApprovalEvidence(card.review, card.request.state);
}

/** The roster's prominent Read report treatment (issue #1095, item 7). */
const PROMINENT_REPORT =
  "inline-flex min-h-8 items-center gap-[7px] rounded border border-[var(--g5-text0)]! bg-[var(--g5-text0)] px-3 py-1 text-[13px] font-semibold text-[var(--g5-bg0)] hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] focus-visible:ring-offset-1";

function EvidenceState({
  evidence,
  className,
  workspaceId,
  prominent = false,
}: {
  workspaceId: string;
  evidence: ApprovalEvidence;
  className: string;
  /** Show an available report as the roster's high-contrast button. */
  prominent?: boolean;
}) {
  const [open, setOpen] = useState(false);
  if (evidence.kind === "report" || evidence.kind === "inspection")
    return (
      <>
        <button
          type="button"
          onClick={() => setOpen(true)}
          className={
            prominent && evidence.kind === "report"
              ? PROMINENT_REPORT
              : className
          }
          data-testid="workspace-approvals-read-report"
        >
          {prominent && evidence.kind === "report" && (
            <FileText size={15} strokeWidth={2} aria-hidden="true" />
          )}
          {evidence.kind === "report" ? "Read report" : evidence.label}
        </button>
        {open && (
          <ApprovalReviewReader
            workspaceId={workspaceId}
            reviewId={evidence.reviewId}
            kind={evidence.kind}
            onClose={() => setOpen(false)}
          />
        )}
      </>
    );
  return evidence.kind === "conversation" ? (
    <Link
      to={`?session=${encodeURIComponent(evidence.sessionId)}`}
      className={className}
    >
      {evidence.label}
    </Link>
  ) : (
    <span className={className}>{evidence.label}</span>
  );
}

function TechnicalDetails({
  card,
  workspaceId,
  canRetryCleanup = false,
}: {
  card: ApprovalHistoryCard;
  workspaceId: string;
  canRetryCleanup?: boolean;
}) {
  const decision =
    card.decision?.outcome === "approved"
      ? "Approved"
      : card.decision?.outcome === "rejected"
        ? "Rejected"
        : card.decision
          ? "No decision"
          : "—";
  const metadata = [
    ["Request ID", card.request.requestId],
    ["Task ID", card.review?.taskId ?? "—"],
    [
      "Definition / version",
      `${card.request.definitionName} · v${card.request.definitionVersion}`,
    ],
    ["Request status", card.request.state],
    ["Decision", decision],
  ];

  return (
    <details
      data-testid="workspace-approvals-technical-details"
      className="mt-2 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2"
    >
      <summary className="cursor-pointer text-[11px] font-semibold text-[var(--g5-text1)]">
        Technical details
      </summary>
      <dl className="mt-3 grid gap-x-4 gap-y-2 sm:grid-cols-2">
        {metadata.map(([label, value]) => (
          <div key={label} className="min-w-0">
            <dt className="text-[9.5px] font-semibold uppercase tracking-[0.05em] text-[var(--g5-text2)]">
              {label}
            </dt>
            <dd className="break-all text-[11px] text-[var(--g5-text0)]">
              {value}
            </dd>
          </div>
        ))}
      </dl>
      {card.request.agentReviews.length > 0 && (
        <ul className="mt-3 space-y-2 text-xs" aria-label="Review attempts">
          {card.request.agentReviews.map((attempt) => (
            <li key={attempt.reviewId ?? attempt.taskId ?? attempt.createdAt}>
              <span className="mr-2">
                {attempt.state} · {formatApprovalDateTime(attempt.createdAt)}
              </span>
              <EvidenceState
                workspaceId={workspaceId}
                className="text-[var(--g5-info)]"
                evidence={resolveApprovalEvidence(attempt, card.request.state)}
              />
              {canRetryCleanup &&
                attempt.reviewId &&
                attempt.cleanupPending &&
                ["succeeded", "failed", "cancelled"].includes(
                  attempt.state,
                ) && (
                  <CleanupRetryButton
                    workspaceId={workspaceId}
                    reviewId={attempt.reviewId}
                  />
                )}
            </li>
          ))}
        </ul>
      )}
    </details>
  );
}

function ApprovalTimelineEvents({ card }: { card: ApprovalHistoryCard }) {
  return (
    <ol
      aria-label="Request timeline"
      className="mt-3 space-y-2 border-t border-[var(--g5-stroke)] pt-3"
    >
      {card.events.map((event) => (
        <li key={event.key} className="text-[11px] text-[var(--g5-text1)]">
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <b className="font-semibold text-[var(--g5-text0)]">
              {event.label}
            </b>
            <time
              dateTime={event.at}
              title={new Date(event.at).toISOString()}
              className="text-[10px] text-[var(--g5-text2)]"
            >
              {formatApprovalDateTime(event.at)}
            </time>
          </div>
          {event.attributes.length > 0 && (
            <div className="mt-0.5 flex flex-wrap gap-x-3 gap-y-1 text-[10px] text-[var(--g5-text2)]">
              {event.attributes.map((attribute) => (
                <span key={attribute.label}>
                  {attribute.label}:{" "}
                  <span className="break-all text-[var(--g5-text1)]">
                    {attribute.value}
                  </span>
                </span>
              ))}
            </div>
          )}
        </li>
      ))}
    </ol>
  );
}

function CleanupRetryButton({
  workspaceId,
  reviewId,
}: {
  workspaceId: string;
  reviewId: string;
}) {
  const cleanup = useRetryApprovalCleanup(workspaceId);
  return (
    <>
      <button
        type="button"
        data-testid="workspace-approvals-retry-cleanup"
        disabled={cleanup.isPending}
        onClick={() => cleanup.mutate(reviewId)}
      >
        Retry cleanup
      </button>
      {cleanup.error && <p role="alert">{cleanup.error.message}</p>}
    </>
  );
}

function RetainedApprovalDecision({
  entry,
  workspaceId,
}: {
  entry: Extract<ApprovalTimelineEntry, { kind: "decision" }>;
  workspaceId: string;
}) {
  const decision = entry.decision;
  return (
    <article
      data-testid="workspace-approvals-decision-record"
      className="rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-4"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
        <h4 className="font-semibold">
          {entry.definitionName} — {entry.approverName}
        </h4>
        <span
          className={
            decision.outcome === "approved"
              ? "text-[var(--g5-ok)]"
              : decision.outcome === "rejected"
                ? "text-[var(--g5-blocked)]"
                : "text-[var(--g5-text2)]"
          }
        >
          {entry.presentation.label}
        </span>
        {decision.superseded && (
          <span className="text-xs text-[var(--g5-text2)]">Historical</span>
        )}
      </div>
      <p className="mt-1 text-xs text-[var(--g5-text2)]">
        <time dateTime={decision.createdAt}>
          {formatApprovalDateTime(decision.createdAt)}
        </time>
        {" · "}Definition version {decision.definitionVersion}
      </p>
      <p className="mt-2 text-xs text-[var(--g5-text2)]">
        {entry.request
          ? "Earlier decision on this request."
          : "Request details unavailable for this retained decision."}
      </p>
      {decision.comment && (
        <p className="mt-2 whitespace-pre-wrap text-sm">{decision.comment}</p>
      )}
      {decision.summary && (
        <p className="mt-2 whitespace-pre-wrap text-sm">{decision.summary}</p>
      )}
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <ApprovalVersionIndicator
          workspaceId={workspaceId}
          reviewed={decision.reviewedGitHeads}
          reviewId={decision.agentReviewId}
          reviewer={entry.approverName}
          decidedAt={decision.createdAt}
        />
        {decision.actorType === "agent" &&
          (decision.agentReviewId ? (
            <EvidenceState
              workspaceId={workspaceId}
              evidence={{
                kind: "report",
                label: "Read review report",
                reviewId: decision.agentReviewId,
              }}
              className="text-xs text-[var(--g5-info)]"
            />
          ) : (
            <span className="text-xs text-[var(--g5-text2)]">
              Exact report unavailable
            </span>
          ))}
      </div>
    </article>
  );
}

function ApprovalTimeline({
  state,
  definitionId,
  definitionName,
  id,
  workspaceId,
  canRetryCleanup,
}: {
  canRetryCleanup: boolean;
  workspaceId: string;
  state: WorkspaceApprovalsStateDTO;
  definitionId: string;
  definitionName: string;
  id: string;
}) {
  const cards = useMemo(
    () => buildApprovalTimeline(state, definitionId),
    [state, definitionId],
  );
  const [approver, setApprover] = useState("all");
  const [requestStatus, setRequestStatus] = useState("all");
  const [decisions, setDecisions] = useState<
    Set<ApprovalHistoryDecisionFilter>
  >(new Set());

  const approvers = useMemo(
    () =>
      Array.from(
        new Map(cards.map((card) => [card.approverKey, card.approverName])),
      )
        .map(([key, label]) => ({ key, label }))
        .sort((left, right) => left.label.localeCompare(right.label)),
    [cards],
  );
  const effectiveApprover =
    approver !== "all" && approvers.some((option) => option.key === approver)
      ? approver
      : "all";
  const filtered = cards.filter(
    (card) =>
      (effectiveApprover === "all" || card.approverKey === effectiveApprover) &&
      (requestStatus === "all" ||
        card.presentation.requestStatus === requestStatus) &&
      (decisions.size === 0 || decisions.has(card.presentation.decisionFilter)),
  );
  const hasActiveFilters =
    effectiveApprover !== "all" ||
    requestStatus !== "all" ||
    decisions.size > 0;
  const entryLabel = cards.length === 1 ? "timeline entry" : "timeline entries";

  const toggleDecision = (decision: ApprovalHistoryDecisionFilter) => {
    setDecisions((current) => {
      const next = new Set(current);
      if (next.has(decision)) next.delete(decision);
      else next.add(decision);
      return next;
    });
  };

  return (
    <div
      id={id}
      role="region"
      aria-label={`Timeline for ${definitionName}`}
      data-testid="workspace-approvals-feed"
      className="mt-3 rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-5 py-4"
    >
      <div
        className="mb-4 flex flex-wrap items-end gap-3"
        data-testid="workspace-approvals-history-filters"
      >
        <label className="text-[10.5px] text-[var(--g5-text2)]">
          Approver
          <select
            aria-label="Filter approval history by approver"
            className="ml-1.5 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1 text-xs text-[var(--g5-text0)]"
            value={effectiveApprover}
            onChange={(event) => setApprover(event.target.value)}
          >
            <option value="all">All</option>
            {approvers.map((option) => (
              <option key={option.key} value={option.key}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <label className="text-[10.5px] text-[var(--g5-text2)]">
          Request status
          <select
            aria-label="Filter approval history by request status"
            className="ml-1.5 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1 text-xs text-[var(--g5-text0)]"
            value={requestStatus}
            onChange={(event) => setRequestStatus(event.target.value)}
          >
            <option value="all">All</option>
            {REQUEST_STATUS_FILTERS.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
        </label>
        <fieldset className="flex items-center gap-2 text-[10.5px] text-[var(--g5-text2)]">
          <legend className="float-left mr-1">Decision</legend>
          {[
            ["approved", "Approved"],
            ["rejected", "Rejected"],
            ["no_decision", "No decision"],
          ].map(([value, label]) => (
            <label key={value} className="inline-flex items-center gap-1">
              <input
                type="checkbox"
                checked={decisions.has(value as ApprovalHistoryDecisionFilter)}
                onChange={() =>
                  toggleDecision(value as ApprovalHistoryDecisionFilter)
                }
              />
              {label}
            </label>
          ))}
        </fieldset>
        <button
          type="button"
          data-testid="workspace-approvals-clear-history-filters"
          className="rounded-md border border-[var(--g5-stroke)] px-2.5 py-1 text-xs font-semibold text-[var(--g5-text1)] hover:bg-[var(--g5-bg0)] disabled:cursor-default disabled:opacity-50"
          disabled={!hasActiveFilters}
          onClick={() => {
            setApprover("all");
            setRequestStatus("all");
            setDecisions(new Set());
          }}
        >
          Clear filters
        </button>
        <span
          aria-live="polite"
          className="self-center text-[10.5px] text-[var(--g5-text2)]"
        >
          {hasActiveFilters
            ? `Showing ${filtered.length} of ${cards.length} ${entryLabel}`
            : `Showing all ${cards.length} ${entryLabel}`}
        </span>
      </div>
      {cards.length === 0 && (
        <p className="text-xs text-[var(--g5-text2)]">
          No approval requests yet.
        </p>
      )}
      {cards.length > 0 && filtered.length === 0 && (
        <p className="mb-3 text-xs text-[var(--g5-text2)]">
          No history matches these filters.
        </p>
      )}
      <div className="space-y-3">
        {approvers
          .filter((reviewer) =>
            filtered.some((card) => card.approverKey === reviewer.key),
          )
          .map((reviewer) => (
            <section
              key={reviewer.key}
              aria-label={`Timeline for reviewer ${reviewer.label}`}
              className="space-y-3"
              data-testid="approval-reviewer-timeline"
            >
              <h3 className="text-sm font-semibold">{reviewer.label}</h3>
              {filtered
                .filter((card) => card.approverKey === reviewer.key)
                .map((card) => {
                  if (card.kind === "decision")
                    return (
                      <RetainedApprovalDecision
                        key={card.key}
                        entry={card}
                        workspaceId={workspaceId}
                      />
                    );
                  const evidence =
                    card.approverType === "agent"
                      ? evidenceForCard(card)
                      : null;
                  const toneClass =
                    card.presentation.tone === "success"
                      ? "border-[rgb(113,201,136,0.35)] text-[var(--g5-ok)]"
                      : card.presentation.tone === "danger"
                        ? "border-[rgb(255,107,107,0.35)] text-[var(--g5-blocked)]"
                        : card.presentation.tone === "warning"
                          ? "border-[rgb(242,162,73,0.35)] text-[var(--g5-warn)]"
                          : card.presentation.tone === "info"
                            ? "border-[rgb(103,164,255,0.35)] text-[var(--g5-info)]"
                            : "border-[var(--g5-stroke)] text-[var(--g5-text1)]";
                  return (
                    <article
                      key={card.key}
                      data-testid="workspace-approvals-history-card"
                      className="rounded-xl border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-4"
                    >
                      <div className="flex flex-wrap items-start justify-between gap-3">
                        <div>
                          <div className="text-[12.5px] font-semibold text-[var(--g5-text0)]">
                            {card.definitionName} — {card.approverName}
                          </div>
                          <time
                            dateTime={card.request.requestedAt}
                            title={new Date(
                              card.request.requestedAt,
                            ).toISOString()}
                            className="mt-0.5 block text-[10.5px] text-[var(--g5-text2)]"
                          >
                            Requested{" "}
                            {formatApprovalDateTime(card.request.requestedAt)}
                          </time>
                        </div>
                        <span className="inline-flex items-center gap-1.5">
                          <span
                            className={`rounded-full border px-2.5 py-1 text-[10.5px] font-semibold ${toneClass}`}
                          >
                            {card.presentation.label}
                          </span>
                          {card.decision && (
                            <ApprovalVersionIndicator
                              workspaceId={workspaceId}
                              reviewed={card.decision.reviewedGitHeads}
                              reviewId={card.decision.agentReviewId}
                              reviewer={card.approverName}
                              decidedAt={card.decision.createdAt}
                            />
                          )}
                        </span>
                      </div>
                      {card.request.note && (
                        <p
                          data-testid="approval-history-request-note"
                          className="mt-3 whitespace-pre-wrap text-xs"
                        >
                          Note: {card.request.note}
                        </p>
                      )}
                      {card.decision?.comment &&
                        card.approverType === "human" && (
                          <div className="mt-3 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2 text-xs leading-relaxed text-[var(--g5-text1)]">
                            {card.decision.comment}
                          </div>
                        )}
                      {card.review?.terminalReason && (
                        <p role="status" className="mt-2 text-xs">
                          {card.review.terminalReason}
                        </p>
                      )}
                      {card.review?.cleanupError && (
                        <div className="mt-2 text-xs">
                          <p role="status">
                            {card.review.cleanupOverdue
                              ? "Cleanup has failed for over 24 hours"
                              : "Cleanup pending"}
                            : {card.review.cleanupError}
                          </p>
                          {canRetryCleanup && card.review.reviewId && (
                            <CleanupRetryButton
                              workspaceId={workspaceId}
                              reviewId={card.review.reviewId}
                            />
                          )}
                        </div>
                      )}
                      {evidence && (
                        <EvidenceState
                          workspaceId={workspaceId}
                          evidence={evidence}
                          className="mt-2 inline-flex text-[11px] font-semibold text-[var(--g5-info)]"
                        />
                      )}
                      <ApprovalTimelineEvents card={card} />
                      <TechnicalDetails
                        card={card}
                        workspaceId={workspaceId}
                        canRetryCleanup={canRetryCleanup}
                      />
                    </article>
                  );
                })}
            </section>
          ))}
      </div>
    </div>
  );
}

function ApprovalTimelineButton({
  name,
  open,
  panelId,
  onToggle,
}: {
  name: string;
  open: boolean;
  panelId: string;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={`Timeline for ${name}`}
      data-testid="workspace-approvals-timeline-toggle"
      aria-expanded={open}
      aria-controls={panelId}
      title="Approval timeline"
      className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded text-[var(--g5-text2)] hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-info)]"
      onClick={onToggle}
    >
      <Info className="h-4 w-4" aria-hidden="true" />
    </button>
  );
}

function HistoricalApproval({
  history,
  state,
  workspaceId,
  canRetryCleanup,
}: {
  history: WorkspaceOutgoingHistoryDTO;
  state: WorkspaceApprovalsStateDTO;
  workspaceId: string;
  canRetryCleanup: boolean;
}) {
  const [open, setOpen] = useState(false);
  const panelId = useId();
  return (
    <article
      className="py-3"
      data-testid="workspace-approvals-historical-requirement"
    >
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="text-sm font-semibold">{history.definitionName}</h3>
        <ApprovalTimelineButton
          name={history.definitionName}
          open={open}
          panelId={panelId}
          onToggle={() => setOpen((value) => !value)}
        />
        <span className="text-xs text-[var(--g5-text2)]">
          Not currently required
        </span>
      </div>
      {open && (
        <ApprovalTimeline
          state={state}
          definitionId={history.definitionId}
          definitionName={history.definitionName}
          id={panelId}
          workspaceId={workspaceId}
          canRetryCleanup={canRetryCleanup}
        />
      )}
    </article>
  );
}

/* ------------------------------------------------------------------ */
/*  Required-into row (attachment management)                          */
/* ------------------------------------------------------------------ */

function RequiredIntoRow({
  requirement,
  canWrite,
  workspaceId,
}: {
  requirement: WorkspaceRequiredIntoDTO;
  canWrite: boolean;
  workspaceId: string;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const menuRef = useRef<HTMLSpanElement>(null);
  const retire = useRetireRequirement(workspaceId);
  const def = requirement.definition;

  // Dismiss the menu on outside click or Escape (worker-picker pattern).
  useEffect(() => {
    if (!menuOpen) return;
    function handleMouseDown(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    }
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setMenuOpen(false);
    }
    document.addEventListener("mousedown", handleMouseDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleMouseDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [menuOpen]);

  return (
    <div
      data-testid="workspace-approvals-required-row"
      className="mt-2.5 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)]"
    >
      <div className="group flex flex-wrap items-center gap-2.5 px-3 py-2.5">
        <span className="text-[13px] font-semibold text-[var(--g5-text0)]">
          {def.name}
        </span>
        <span className="flex-1" />
        {/* Finalize-panel avatar cluster (shell D50); the incoming side has
            no decision state, so entries render in the pending style. */}
        <span className="flex items-center">
          {def.humans.map((h, i) => (
            <RosterAvatar
              key={h.userId}
              entry={{
                key: `human:${h.userId}`,
                kind: "human",
                displayName: h.displayName,
                userId: h.userId,
                state: "pending",
                definitionNames: [def.name],
                decidedAt: null,
              }}
              first={i === 0}
            />
          ))}
          {def.agentType === "simple" && (
            <RosterAvatar
              entry={{
                key: `agent:${def.id}`,
                kind: "agent",
                displayName: agentApproverName(def.name),
                userId: null,
                state: "pending",
                definitionNames: [def.name],
                decidedAt: null,
              }}
              first={def.humans.length === 0}
            />
          )}
        </span>
        {canWrite && (
          <span ref={menuRef} className="relative">
            <button
              type="button"
              data-testid="workspace-approvals-required-menu"
              className="cursor-pointer rounded px-1.5 py-0.5 text-[var(--g5-text2)] hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]"
              onClick={() => setMenuOpen((v) => !v)}
            >
              <MoreHorizontal className="h-4 w-4" />
            </button>
            {menuOpen && (
              <span className="absolute right-0 z-10 mt-1 block rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] p-1">
                <button
                  type="button"
                  data-testid="workspace-approvals-stop-requiring"
                  className="block w-full cursor-pointer whitespace-nowrap rounded px-3 py-1.5 text-left text-xs text-[var(--g5-blocked)] hover:bg-[rgb(255,107,107,0.1)]"
                  onClick={() => {
                    setMenuOpen(false);
                    setConfirming(true);
                  }}
                >
                  Stop requiring…
                </button>
              </span>
            )}
          </span>
        )}
      </div>
      <div className="px-3 pb-2 text-[11px] text-[var(--g5-text2)]">
        Required since {formatDateTime(requirement.attachedAt)} · attached by{" "}
        {requirement.attachedBy.displayName}
      </div>
      {confirming && (
        <div
          data-testid="workspace-approvals-retire-confirm"
          className="border-t border-[rgb(255,107,107,0.25)] bg-[rgb(255,107,107,0.06)] px-3 py-2.5 text-xs leading-relaxed text-[var(--g5-text1)]"
        >
          Stop requiring <b>{def.name}</b> for merges into this workspace? This
          is recorded as an append-only <b>retire event</b> — the history keeps
          both events and existing decisions are untouched.
          <div className="mt-2 flex gap-2">
            <button
              type="button"
              data-testid="workspace-approvals-confirm-retire"
              className="cursor-pointer rounded-md border border-[rgb(255,107,107,0.35)] px-2.5 py-1 text-[11.5px] font-semibold text-[var(--g5-blocked)] hover:bg-[rgb(255,107,107,0.1)]"
              disabled={retire.isLoading}
              onClick={() => {
                retire
                  .mutateAsync({ definitionId: def.id })
                  .then(() => setConfirming(false))
                  .catch(() => {
                    /* surfaced via retire.error */
                  });
              }}
            >
              Record retire event
            </button>
            <button
              type="button"
              data-testid="workspace-approvals-retire-cancel"
              className="cursor-pointer rounded-md border border-[var(--g5-stroke)] px-2.5 py-1 text-[11.5px] text-[var(--g5-text1)] hover:bg-[var(--g5-bg1)]"
              onClick={() => setConfirming(false)}
            >
              Cancel
            </button>
          </div>
          {retire.error && (
            <div className="mt-2 text-[11.5px] text-[var(--g5-blocked)]">
              {retire.error.message}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function ManagementSection({
  state,
  canWrite,
  workspaceId,
  workspaceName,
  onRequire,
}: {
  state: WorkspaceApprovalsStateDTO;
  canWrite: boolean;
  workspaceId: string;
  workspaceName: string;
  onRequire: () => void;
}) {
  return (
    <section data-testid="workspace-approvals-required-into" className="mt-3">
      {state.requiredInto.length === 0 && (
        <p className="text-sm text-[var(--g5-text2)]">
          No approvals are required for merges into this workspace.
        </p>
      )}
      {state.requiredInto.map((requirement) => (
        <RequiredIntoRow
          key={requirement.definition.id}
          requirement={requirement}
          canWrite={canWrite}
          workspaceId={workspaceId}
        />
      ))}
      {canWrite && (
        <button
          type="button"
          data-testid="workspace-approvals-require"
          className="mt-2.5 flex w-full cursor-pointer items-center justify-center gap-2 rounded-md border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-3 text-[12.5px] font-semibold text-[var(--g5-text1)] hover:border-[var(--g5-info)] hover:text-[var(--g5-text0)]"
          onClick={onRequire}
        >
          <Plus className="h-3.5 w-3.5" />
          Require an additional approval to merge into {workspaceName}
        </button>
      )}
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Main content component                                             */
/* ------------------------------------------------------------------ */

function ApprovalBadge({
  label,
  tone,
  testId,
}: {
  label: string;
  tone: ApprovalBadgeTone;
  testId?: string;
}) {
  // Status reads from the word; the tint and dot only reinforce it, and the
  // text keeps the theme foreground so light palettes stay readable.
  const tint =
    tone === "ok"
      ? "bg-[color-mix(in_srgb,var(--g5-ok)_16%,transparent)]"
      : tone === "blocked"
        ? "bg-[color-mix(in_srgb,var(--g5-blocked)_16%,transparent)]"
        : tone === "warn"
          ? "bg-[color-mix(in_srgb,var(--g5-warn)_18%,transparent)]"
          : "bg-[color-mix(in_srgb,var(--g5-text2)_14%,transparent)]";
  const dot =
    tone === "ok"
      ? "bg-[var(--g5-ok)]"
      : tone === "blocked"
        ? "bg-[var(--g5-blocked)]"
        : tone === "warn"
          ? "bg-[var(--g5-warn)]"
          : "bg-[var(--g5-text2)]";
  return (
    <span
      data-testid={testId}
      className={`inline-flex items-center gap-1.5 whitespace-nowrap rounded-full px-2 py-0.5 text-[11px] font-semibold leading-snug text-[var(--g5-text0)] ${tint}`}
    >
      <span aria-hidden="true" className={`h-1.5 w-1.5 rounded-full ${dot}`} />
      {label}
    </span>
  );
}

function ReviewerTypeIcon({ kind }: { kind: "human" | "agent" }) {
  const Icon = kind === "agent" ? Bot : UserRound;
  return (
    <span
      role="img"
      aria-label={kind === "agent" ? "Agent" : "Human"}
      className="mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center text-[var(--g5-text1)]"
    >
      <Icon size={15} strokeWidth={1.75} aria-hidden="true" />
    </span>
  );
}

function RequirementCurrentness({
  name,
  currentness,
}: {
  name: string;
  currentness: WorkspaceOutgoingRequirementDTO["currentness"];
}) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const copy = currentnessMessage(currentness);
  if (!copy) return null;
  return (
    <>
      <button
        type="button"
        data-testid="workspace-approvals-currentness"
        aria-haspopup="dialog"
        aria-label={`Overall commit status: ${copy.label}`}
        title={copy.label}
        className={`inline-flex h-5 min-w-5 items-center justify-center rounded px-0.5 ${currentnessTone(copy.status)}`}
        onClick={(event) => setAnchor(event.currentTarget)}
      >
        <span aria-hidden="true" className={CURRENTNESS_CIRCLE}>
          {currentnessGlyph(copy.status)}
        </span>
      </button>
      {anchor && (
        <AnchoredPopup
          anchor={anchor}
          title={`${name} · approval commit status`}
          ariaLabel={`${name} approval commit status`}
          testId="workspace-approvals-currentness-popup"
          onClose={() => setAnchor(null)}
        >
          <p>{copy.message}</p>
        </AnchoredPopup>
      )}
    </>
  );
}

function RosterDecisionDetails({
  entry,
  workspaceId,
  reviewer,
  isAgent,
  requestAgain,
  escalation = null,
}: {
  entry: ApprovalDecisionLogEntryDTO;
  workspaceId: string;
  reviewer: string;
  isAgent: boolean;
  requestAgain: React.ReactNode;
  /** The requirement's agent-escalation display, for the latest response only. */
  escalation?: WorkspaceOutgoingRequirementDTO["agentEscalation"];
}) {
  const { label, tone } = rosterDecisionLabel(entry, escalation);
  return (
    <>
      <ApprovalBadge label={label} tone={tone} />
      <time
        dateTime={entry.createdAt}
        className="text-xs text-[var(--g5-text2)]"
      >
        {formatApprovalDateTime(entry.createdAt)}
      </time>
      <ApprovalVersionIndicator
        workspaceId={workspaceId}
        reviewed={entry.reviewedGitHeads}
        reviewId={entry.agentReviewId}
        reviewer={reviewer}
        decidedAt={entry.createdAt}
      />
      {entry.superseded && (
        <span className="text-xs text-[var(--g5-text2)]">Historical</span>
      )}
      {isAgent &&
        (entry.agentReviewId ? (
          <EvidenceState
            workspaceId={workspaceId}
            prominent
            className="text-xs text-[var(--g5-info)] underline-offset-2 hover:underline"
            evidence={{
              kind: "report",
              label: "Read review report",
              reviewId: entry.agentReviewId,
            }}
          />
        ) : (
          <span className="text-xs text-[var(--g5-text2)]">
            Exact report unavailable
          </span>
        ))}
      {requestAgain && <span className="ml-auto">{requestAgain}</span>}
    </>
  );
}

/**
 * One request's own note (g5.governance.approval.request-status), labelled
 * with its requester so it never reads as the reviewer's response. It starts
 * open only for the request addressed to the signed-in viewer; the caller
 * keys it by viewer and request, so a refresh keeps the manual choice.
 */
function RequestNote({
  request,
  viewerUserId,
}: {
  request: ApprovalRequestDTO;
  viewerUserId: string;
}) {
  const [open, setOpen] = useState(
    request.actorType === "human" &&
      !!viewerUserId &&
      request.actorUserId === viewerUserId,
  );
  if (!request.note) return null;
  const requester = request.requestedBy?.displayName;
  return (
    <details
      data-testid="review-request-note"
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
      className="mt-1"
    >
      <summary className="w-fit cursor-pointer text-[11px] text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]">
        <strong className="font-semibold text-[var(--g5-text1)]">
          {requester
            ? `Request note from ${requester}`
            : "Request note · Requester unavailable"}
        </strong>
      </summary>
      <p
        data-testid="review-request-note-text"
        className="mt-1.5 max-w-[68ch] whitespace-pre-wrap border-l-2 border-[var(--g5-stroke)] pl-2.5 text-[13px] text-[var(--g5-text1)] [overflow-wrap:anywhere]"
      >
        {request.note}
      </p>
    </details>
  );
}

/**
 * A response line labelled by who wrote it: the reviewer or the agent. Its
 * Request again sits on the right, in the column of the requirement's own
 * request action, so the decision row above keeps its width.
 */
function ResponseSummary({
  text,
  isAgent,
  requestAgain = null,
}: {
  text: string;
  isAgent: boolean;
  requestAgain?: React.ReactNode;
}) {
  return (
    <div className="mt-0.5 flex items-baseline gap-x-3">
      <p className="min-w-0 max-w-[68ch] whitespace-pre-wrap text-[13px] text-[var(--g5-text1)]">
        <span className="mr-1 font-semibold text-[var(--g5-text0)]">
          {isAgent ? "Agent response:" : "Reviewer response:"}
        </span>
        <span data-testid="workspace-approvals-summary">{text}</span>
      </p>
      {requestAgain && (
        <span className="ml-auto flex-none">{requestAgain}</span>
      )}
    </div>
  );
}

/** The settled response's own summary line: agent summary or human comment. */
function decisionSummary(
  entry: ApprovalDecisionLogEntryDTO,
  isAgent: boolean,
): string | null {
  const text = isAgent ? entry.summary : entry.comment;
  return text?.trim() ? text : null;
}

function CompactRequirement({
  requirement,
  state,
  workspaceId,
  canRequest,
  requestsAllowed,
  actionsUnavailable,
  viewerUserId,
  onRequestPicks,
}: {
  requirement: WorkspaceOutgoingRequirementDTO;
  state: WorkspaceApprovalsStateDTO;
  workspaceId: string;
  canRequest: boolean;
  /** The request PR rule currently permits new requests. */
  requestsAllowed: boolean;
  actionsUnavailable: boolean;
  viewerUserId: string;
  /** Opens the shared Request approvals dialog in Pick mode, prechecked. */
  onRequestPicks: (picks: ApprovalPick[]) => void;
}) {
  const [definitionAnchor, setDefinitionAnchor] = useState<HTMLElement | null>(
    null,
  );
  const [timelineOpen, setTimelineOpen] = useState(false);
  const timelineId = useId();
  const popupId = useId();
  const [retryError, setRetryError] = useState<string | null>(null);
  const retry = useRequestApprovals(workspaceId);
  const cancel = useCancelApprovalReview(workspaceId);
  const retryIds = useRef(new Map<string, string>());
  const d = requirement.definition;
  const rows = buildRequirementRoster(requirement);
  const status = requirementBadge(requirement);
  const path = requirement.approvalPath
    ? approvalPathText(requirement.approvalPath)
    : "";
  const canAskAgain = canRequest && requestsAllowed;
  const candidates = canAskAgain
    ? requestCandidates(requirement, viewerUserId)
    : [];
  const showOutstanding =
    canAskAgain && hasOutstandingNeeds(requirement, viewerUserId);
  async function retryRun(
    request: ApprovalRequestDTO,
    review: AgentApprovalReviewDTO,
  ) {
    if (!review.reviewId || retry.isLoading) return;
    const id = retryIds.current.get(review.reviewId) ?? randomUuid();
    retryIds.current.set(review.reviewId, id);
    const result = await retry.mutateAsync([
      {
        key: review.reviewId,
        label: `${request.definitionName} agent`,
        kind: "agent",
        definitionId: d.id,
        definitionVersion: request.definitionVersion,
        phase: request.phase ?? "ie",
        retryReviewId: review.reviewId,
        clientRequestId: id,
      },
    ]);
    setRetryError(
      result[0]?.status === "rejected"
        ? (result[0].message ?? "Retry failed")
        : null,
    );
  }
  function requestAgainButton(row: ApprovalRequirementRosterRow) {
    const recipientId = row.kind === "agent" ? AGENT_RECIPIENT : row.userId;
    if (
      !recipientId ||
      !candidates.some((c) => c.id === recipientId && !c.active)
    )
      return null;
    return (
      <button
        type="button"
        data-testid="workspace-approvals-request-again"
        className="text-xs text-[var(--g5-info)] underline-offset-2 hover:underline"
        onClick={() => onRequestPicks([{ definitionId: d.id, recipientId }])}
      >
        Request again
      </button>
    );
  }
  return (
    <article data-testid="workspace-approvals-requirement" className="py-4">
      <div className="flex flex-wrap items-center gap-2">
        <h3>
          <button
            data-testid="workspace-approvals-definition"
            type="button"
            className="text-sm font-semibold text-[var(--g5-info)] underline decoration-1 underline-offset-4"
            aria-haspopup="dialog"
            onClick={(event) => setDefinitionAnchor(event.currentTarget)}
          >
            {d.name}
          </button>
        </h3>
        <ApprovalBadge
          label={status.label}
          tone={status.tone}
          testId="workspace-approvals-requirement-status"
        />
        <RequirementCurrentness
          name={d.name}
          currentness={requirement.currentness}
        />
        {showOutstanding && (
          <button
            type="button"
            data-testid="workspace-approvals-request-outstanding"
            className="ml-auto rounded border border-[var(--g5-stroke)] px-2 py-0.5 text-xs text-[var(--g5-text0)] hover:bg-[var(--g5-bg1)]"
            onClick={() =>
              onRequestPicks(outstandingPicks(requirement, viewerUserId))
            }
          >
            Request outstanding approvals
          </button>
        )}
      </div>
      {path && (
        <p
          data-testid="workspace-approvals-path"
          className="mt-1.5 max-w-[80ch] text-[13px] text-[var(--g5-text1)]"
        >
          {path}
        </p>
      )}
      {rows.length > 0 && (
        <ul
          aria-label={`Requested approvers for ${d.name}`}
          className="mt-2 space-y-1.5 text-sm"
        >
          {rows.map((row) => {
            const isAgent = row.kind === "agent";
            const againAt = canAskAgain ? requestAgainTarget(row) : null;
            if (row.primary.type === "decision") {
              const entry = row.primary.decision;
              const summary = decisionSummary(entry, isAgent);
              return (
                <li
                  data-testid="workspace-approvals-decision"
                  key={`${workspaceId}:${row.slot}:${entry.requestId}:${entry.agentReviewId ?? ""}`}
                  className="grid grid-cols-[16px_minmax(0,1fr)] gap-x-2"
                >
                  <ReviewerTypeIcon kind={row.kind} />
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                      <span className="font-semibold text-[var(--g5-text0)]">
                        {row.name}
                      </span>
                      <RosterDecisionDetails
                        entry={entry}
                        workspaceId={workspaceId}
                        reviewer={row.name}
                        isAgent={isAgent}
                        escalation={requirement.agentEscalation}
                        requestAgain={
                          againAt === "primary" && !summary
                            ? requestAgainButton(row)
                            : null
                        }
                      />
                    </div>
                    {row.primary.request && (
                      <RequestNote
                        key={`${viewerUserId}:${row.primary.request.requestId}`}
                        request={row.primary.request}
                        viewerUserId={viewerUserId}
                      />
                    )}
                    {summary && (
                      <ResponseSummary
                        text={summary}
                        isAgent={isAgent}
                        requestAgain={
                          againAt === "primary" ? requestAgainButton(row) : null
                        }
                      />
                    )}
                  </div>
                </li>
              );
            }
            const request = row.primary.request;
            const review = request.agentReviews.at(-1);
            const pending = request.state === "pending";
            const failure =
              review?.state === "failed" || review?.state === "cancelled";
            const label = requestLabel(request);
            const prior = row.prior;
            const priorSummary = prior ? decisionSummary(prior, isAgent) : null;
            return (
              <li
                data-testid="workspace-approvals-wait"
                key={`${workspaceId}:${row.slot}:${request.requestId}`}
                className="grid grid-cols-[16px_minmax(0,1fr)] gap-x-2"
              >
                <ReviewerTypeIcon kind={row.kind} />
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                    <span className="font-semibold text-[var(--g5-text0)]">
                      {row.name}
                    </span>
                    <span data-testid="review-request-status">
                      <ApprovalBadge label={label.label} tone={label.tone} />
                    </span>
                    <time
                      dateTime={request.requestedAt}
                      className="text-xs text-[var(--g5-text2)]"
                    >
                      {formatApprovalDateTime(request.requestedAt)}
                    </time>
                    {isAgent && (
                      <EvidenceState
                        workspaceId={workspaceId}
                        prominent
                        className="text-xs text-[var(--g5-info)]"
                        evidence={resolveApprovalEvidence(
                          review ?? null,
                          request.state,
                        )}
                      />
                    )}
                    {canRequest &&
                      pending &&
                      (request.actorType === "agent" ||
                        request.requestedBy?.userId === viewerUserId) && (
                        <button
                          type="button"
                          data-testid="workspace-approvals-cancel-request"
                          className="text-xs text-[var(--g5-info)] underline-offset-2 hover:underline"
                          disabled={cancel.isLoading}
                          onClick={() =>
                            void cancel
                              .mutateAsync({
                                requestId: request.requestId,
                                taskId: review?.taskId ?? null,
                              })
                              .catch(() => {})
                          }
                        >
                          Cancel request
                        </button>
                      )}
                    {canRequest &&
                      review?.retryAvailable &&
                      review.state === "failed" && (
                        <button
                          type="button"
                          data-testid="workspace-approvals-retry-review"
                          className="text-xs text-[var(--g5-info)] underline-offset-2 hover:underline"
                          disabled={retry.isLoading}
                          onClick={() => void retryRun(request, review)}
                        >
                          Retry
                        </button>
                      )}
                  </div>
                  {failure && (
                    <p className="mt-0.5 text-xs text-[var(--g5-text1)]">
                      Last attempt {review.state}
                      {review.terminalReason
                        ? `: ${review.terminalReason}`
                        : ""}
                    </p>
                  )}
                  <RequestNote
                    key={`${viewerUserId}:${request.requestId}`}
                    request={request}
                    viewerUserId={viewerUserId}
                  />
                  {prior && (
                    <div
                      data-testid="workspace-approvals-prior"
                      className="mt-1.5 border-l-2 border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-1.5"
                    >
                      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                        <span className="text-xs text-[var(--g5-text2)]">
                          Earlier settled response
                        </span>
                        <RosterDecisionDetails
                          entry={prior}
                          workspaceId={workspaceId}
                          reviewer={row.name}
                          isAgent={isAgent}
                          requestAgain={
                            againAt === "prior" && !priorSummary
                              ? requestAgainButton(row)
                              : null
                          }
                        />
                      </div>
                      {priorSummary && (
                        <ResponseSummary
                          text={priorSummary}
                          isAgent={isAgent}
                          requestAgain={
                            againAt === "prior" ? requestAgainButton(row) : null
                          }
                        />
                      )}
                    </div>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
      <WorkspaceReviewActions
        workspaceId={workspaceId}
        definitionId={d.id}
        disabled={actionsUnavailable}
      />
      {(retryError || cancel.error) && (
        <p role="alert" className="mt-2 text-xs">
          {retryError ?? cancel.error?.message}
        </p>
      )}
      {rows.length > 0 && (
        <div className="ml-6 mt-2">
          <button
            type="button"
            aria-label={`Timeline for ${d.name}`}
            data-testid="workspace-approvals-timeline-toggle"
            aria-expanded={timelineOpen}
            aria-controls={timelineId}
            className="inline-flex items-center gap-1.5 text-sm font-semibold text-[var(--g5-text2)] no-underline hover:text-[var(--g5-text0)]"
            onClick={() => setTimelineOpen((value) => !value)}
          >
            <span aria-hidden="true" className="text-[10px]">
              {timelineOpen ? "▼" : "▶"}
            </span>
            Timeline
          </button>
          {timelineOpen && (
            <ApprovalTimeline
              state={state}
              definitionId={d.id}
              definitionName={d.name}
              id={timelineId}
              workspaceId={workspaceId}
              canRetryCleanup={canRequest}
            />
          )}
        </div>
      )}
      {definitionAnchor && (
        <AnchoredPopup
          anchor={definitionAnchor}
          title={`${d.name} definition`}
          ariaLabel={`${d.name} definition`}
          testId="workspace-approvals-definition-popup"
          onClose={() => setDefinitionAnchor(null)}
        >
          <p>Version {d.latestVersion}</p>
          <section aria-labelledby={`${popupId}-description`} className="mt-3">
            <h3 id={`${popupId}-description`} className="font-semibold">
              Description:
            </h3>
            <p className="whitespace-pre-wrap">
              {d.description || "No description provided."}
            </p>
            {approvalRecommendation(d) && <p>{approvalRecommendation(d)}</p>}
          </section>
          <section aria-labelledby={`${popupId}-approvers`} className="mt-3">
            <h3 id={`${popupId}-approvers`} className="font-semibold">
              Approvers:
            </h3>
            <p>
              {d.requireAllReviewers === false
                ? "Any one eligible reviewer"
                : "All eligible reviewers"}
            </p>
            {d.agentType && (
              <p
                data-testid="workspace-approvals-definition-agent"
                className="flex items-start gap-1.5"
              >
                <ReviewerTypeIcon kind="agent" />
                <span>{d.name} agent</span>
              </p>
            )}
            {d.humans.length > 0 && (
              <p
                data-testid="workspace-approvals-definition-humans"
                className="flex items-start gap-1.5"
              >
                <ReviewerTypeIcon kind="human" />
                <span>{d.humans.map((h) => h.displayName).join(", ")}</span>
              </p>
            )}
            {!d.agentType && d.humans.length === 0 && <p>No approvers.</p>}
          </section>
          {d.agentPrompt && (
            <section aria-labelledby={`${popupId}-prompt`} className="mt-3">
              <h3 id={`${popupId}-prompt`} className="font-semibold">
                Prompt:
              </h3>
              <p className="whitespace-pre-wrap">{d.agentPrompt}</p>
            </section>
          )}
          <p className="mt-3 text-xs text-[var(--g5-text1)]">
            {d.allowSelfApproval
              ? "Self approval allowed."
              : "Self approval disallowed."}{" "}
            {d.humanApprovalsOverrideAgent !== false &&
              d.requireAllReviewers === false &&
              "Human approval may override an agent rejection. Human rejection always blocks."}
          </p>
        </AnchoredPopup>
      )}
    </article>
  );
}

export interface WorkspaceApprovalsPageContentProps {
  workspaceId: string;
  /** Dirty live workspace file count from the workspace shell. */
  dirtyCount?: number;
  /** Opens a workspace file in the editor (at a line when given). */
  onOpenWorkspaceFile?: (
    filePath: string,
    repoRef?: string,
    line?: number,
  ) => void;
}

type ApprovalsView = "page" | "picker";

/**
 * Merge when ready on this workspace's open PR into the section's parent
 * (g5.git.pr-auto-merge), as one line for every visitor beneath the heading.
 * It reads the Overview's pull-request list and says nothing while that read
 * is loading or failed: Prepare to Ship owns the setting.
 */
function MergeWhenReadyNote({
  workspaceId,
  targetWorkspaceId,
  targetName,
}: {
  workspaceId: string;
  targetWorkspaceId: string;
  targetName: string;
}) {
  const pulls = useQuery({
    ...workspacePullRequestsQueryOptions(workspaceId, "source", "all"),
    refetchInterval: 15_000,
  });
  const on = pulls.data?.some(
    (pull) =>
      pull.status === "open" &&
      pull.targetWorkspaceId === targetWorkspaceId &&
      pull.autoMergeRequest != null,
  );
  if (!on) return null;
  return (
    <p
      data-testid="workspace-approvals-merge-when-ready"
      className="-mt-1 mb-3 flex items-center gap-1.5 text-xs text-[var(--g5-text2)]"
    >
      <GitMerge
        size={14}
        aria-hidden="true"
        className="flex-none text-[var(--g5-info)]"
      />
      <span>
        <strong className="font-semibold text-[var(--g5-text1)]">
          Merge when ready is on.
        </strong>{" "}
        G5 merges into {targetName} once checks pass and approvals are met.
      </span>
    </p>
  );
}

export function WorkspaceApprovalsPageContent({
  workspaceId,
  onOpenWorkspaceFile,
}: WorkspaceApprovalsPageContentProps) {
  const { org, workspace } = useParams<{ org: string; workspace: string }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const { user, hasScope } = useAuth();

  const currentOrg = useMemo(
    () => user?.orgs.find((o) => o.orgSlug === org),
    [user, org],
  );
  const orgId = currentOrg?.orgId ?? "";
  const workspaceName = workspace ?? "this workspace";
  const isActiveOrg = !!currentOrg && currentOrg.orgId === user?.activeOrgId;
  const canWrite =
    currentOrg?.role === "admin" || (isActiveOrg && hasScope("org:admin"));
  // Human mutations use the captured-vector dialog above. The existing
  // requirement roster remains read-only; agent controls are unchanged.
  const callerUserId = user?.id ?? "";

  const [view, setView] = useState<ApprovalsView>("page");
  const [search, setSearch] = useState("");
  const [picked, setPicked] = useState<string[]>([]);

  const stateQuery = useWorkspaceApprovals(workspaceId);
  const workspaceQuery = useWorkspace(workspaceId);
  const reviewFlow = useWorkspaceReviewFlow(workspaceId);
  // Request again / Request outstanding approvals open the shared dialog
  // prechecked; the page-level Request opens it fresh.
  const [requestPrefill, setRequestPrefill] =
    useState<ApprovalRequestPrefill | null>(null);
  const prefillTokens = useRef(0);
  const openRequestPicks = (picks: ApprovalPick[]) => {
    prefillTokens.current += 1;
    setRequestPrefill({ token: prefillTokens.current, picks });
  };
  // Ownership is the only write authority. A detail without an owner (older
  // servers, cached payloads) keeps today's in-place review for everyone;
  // until the detail has loaded, neither review action is offered.
  const workspaceDetailLoaded = workspaceQuery.status === "ok";
  const viewerOwnsWorkspace =
    workspaceDetailLoaded && workspaceQuery.data.ownerUserId === callerUserId;
  // Agent requests follow the ordinary task's owner admission on main.
  // A human reviewer still decides their addressed request in the dialog.
  const canRequestReviews =
    isActiveOrg &&
    hasScope("workspaces:write") &&
    workspaceQuery.status === "ok" &&
    workspaceQuery.data.ownerUserId === callerUserId &&
    !workspaceQuery.data.isBase &&
    !workspaceQuery.data.readOnly &&
    workspaceQuery.data.mutationPolicyTransition !== "freezing";
  // `?view=reviews` (review notifications, Push review) lands on this tab:
  // scroll the Reviews panel into view once per arrival. Each navigation
  // has its own location key, so a notification clicked while this tab is
  // already open (no remount) scrolls too, and a re-render does not.
  const reviewsRef = useRef<HTMLElement | null>(null);
  const location = useLocation();
  const wantReviews = searchParams.get("view") === "reviews";
  const scrolledForNavigation = useRef<string | null>(null);
  useEffect(() => {
    if (!wantReviews || !reviewsRef.current) return;
    if (scrolledForNavigation.current === location.key) return;
    scrolledForNavigation.current = location.key;
    reviewsRef.current.scrollIntoView?.({ block: "start" });
    // The section mounts only once both the approvals state and the
    // workspace detail have loaded and the page view is showing.
  }, [
    wantReviews,
    location.key,
    workspaceDetailLoaded,
    stateQuery.status,
    view,
  ]);
  const library = useOrgApprovalDefinitions(orgId, view === "picker");
  const attach = useAttachRequirement(workspaceId);

  const openPicker = () => setView("picker");

  if (stateQuery.status === "loading") {
    return (
      <p
        data-testid="workspace-approvals-loading"
        className="px-8 py-8 text-sm text-[var(--g5-text2)]"
      >
        Loading approvals…
      </p>
    );
  }
  if (stateQuery.status === "error") {
    return (
      <div
        data-testid="workspace-approvals-error"
        className="flex items-center gap-2 px-8 py-8 text-sm text-[var(--g5-blocked)]"
      >
        <AlertCircle className="h-4 w-4" />
        Failed to load workspace approvals.
      </div>
    );
  }
  const state = stateQuery.data;
  // Approval actions wait for current workspace details and approvals: not
  // while either is loading, failed, or could not be refreshed.
  const approvalActionsUnavailableCopy =
    workspaceQuery.status !== "ok" || workspaceQuery.refreshError
      ? "Workspace details are unavailable, so approval actions are temporarily disabled."
      : stateQuery.refreshError
        ? "Approvals could not be refreshed, so approval actions are temporarily disabled."
        : null;
  const activeIds = new Set(state.requiredInto.map((r) => r.definition.id));

  /* ---- picker subview (full page) ---- */
  if (view === "picker") {
    const definitions = library.status === "ok" ? library.data : [];
    return (
      <div className="mx-auto w-full max-w-4xl px-6 py-8">
        <nav className="mb-3 text-xs text-[var(--g5-text2)]">
          <button
            type="button"
            data-testid="workspace-approvals-breadcrumb-page"
            className="cursor-pointer text-[var(--g5-info)] hover:underline"
            onClick={() => setView("page")}
          >
            Approvals — {workspaceName}
          </button>{" "}
          / <span className="text-[var(--g5-text0)]">Require approvals</span>
        </nav>
        <h1 className="text-xl font-semibold text-[var(--g5-text0)]">
          Require approvals for merges into {workspaceName}
        </h1>
        <p className="mb-5 mt-1 max-w-2xl text-xs leading-relaxed text-[var(--g5-text2)]">
          Pick from the org approval library. Requiring is mandatory by
          definition; each pick is recorded as an append-only attach event, and
          requirements always evaluate the definition's latest version — no
          pinning.
        </p>
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div className="flex w-72 items-center gap-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-1.5">
            <Search className="h-3.5 w-3.5 text-[var(--g5-text2)]" />
            <input
              data-testid="workspace-approvals-picker-search"
              className="w-full bg-transparent text-xs text-[var(--g5-text0)]"
              placeholder="Search the library…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
          {/* Escape hatch: the definition you need may not exist yet — jump to
              the org library's tab, where new definitions are created (D3). */}
          <button
            type="button"
            data-testid="workspace-approvals-picker-new-definition"
            title="Create a new definition in the org approval library"
            className="inline-flex cursor-pointer items-center gap-1.5 rounded-md border border-[var(--g5-stroke)] px-2.5 py-1.5 text-[11.5px] font-semibold text-[var(--g5-text1)] hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]"
            onClick={() => {
              const next = new URLSearchParams(searchParams);
              next.set("view", "approval-definitions");
              setSearchParams(next);
            }}
          >
            <Plus className="h-3.5 w-3.5" />
            New definition
          </button>
        </div>
        {library.status === "loading" && (
          <p
            data-testid="workspace-approvals-picker-loading"
            className="text-sm text-[var(--g5-text2)]"
          >
            Loading the approval library…
          </p>
        )}
        {library.status === "error" && (
          <div
            data-testid="workspace-approvals-picker-error"
            className="flex items-center gap-2 text-sm text-[var(--g5-blocked)]"
          >
            <AlertCircle className="h-4 w-4" />
            Failed to load the approval library.
          </div>
        )}
        <div className="grid gap-3 sm:grid-cols-2">
          {definitions
            .filter(
              (d) =>
                !search.trim() ||
                d.name.toLowerCase().includes(search.trim().toLowerCase()),
            )
            .map((d) => {
              const already = activeIds.has(d.id);
              const on = picked.includes(d.id);
              return (
                <button
                  key={d.id}
                  type="button"
                  data-testid="workspace-approvals-pick-card"
                  disabled={already}
                  className={`cursor-pointer rounded-xl border p-3.5 text-left ${
                    already
                      ? "cursor-default border-[var(--g5-stroke)] bg-[var(--g5-bg1)] opacity-50"
                      : on
                        ? "border-[var(--g5-info)] bg-[rgb(103,164,255,0.07)]"
                        : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
                  }`}
                  onClick={() =>
                    setPicked((prev) =>
                      prev.includes(d.id)
                        ? prev.filter((id) => id !== d.id)
                        : [...prev, d.id],
                    )
                  }
                >
                  <span className="flex items-center justify-between text-[13px] font-semibold text-[var(--g5-text0)]">
                    {d.name}
                    <span
                      className={`inline-flex h-4 w-4 items-center justify-center rounded border text-[11px] text-[rgb(8,12,24)] ${
                        already || on
                          ? "border-[var(--g5-info)] bg-[var(--g5-info)]"
                          : "border-[var(--g5-stroke)]"
                      }`}
                    >
                      {(already || on) && "✓"}
                    </span>
                  </span>
                  <span className="mt-1 block min-h-[26px] text-[11.5px] text-[var(--g5-text2)]">
                    {d.description ?? ""}
                    {already && " (already required)"}
                  </span>
                </button>
              );
            })}
        </div>
        {attach.error && (
          <div className="mt-3 flex items-center gap-2 text-xs text-[var(--g5-blocked)]">
            <AlertCircle className="h-3.5 w-3.5" />
            {attach.error.message}
          </div>
        )}
        <div className="mt-5 flex items-center justify-between gap-4 border-t border-[var(--g5-stroke)] pt-3.5">
          <span className="max-w-md text-[11px] leading-relaxed text-[var(--g5-text2)]">
            To require different content, publish a new definition in the
            library instead of pinning a version.
          </span>
          <div className="flex gap-2.5">
            <Button
              kind="secondary"
              onClick={() => {
                setPicked([]);
                setView("page");
              }}
            >
              Cancel
            </Button>
            <Button
              kind="primary"
              data-testid="workspace-approvals-picker-submit"
              disabled={picked.length === 0 || attach.isLoading}
              onClick={() => {
                // Sequential attaches; each response replaces the cache.
                (async () => {
                  for (const definitionId of picked) {
                    await attach.mutateAsync({ definitionId });
                  }
                })()
                  .then(() => {
                    setPicked([]);
                    setView("page");
                  })
                  .catch(() => {
                    /* surfaced via attach.error */
                  });
              }}
            >
              Require {picked.length} selected
            </Button>
          </div>
        </div>
      </div>
    );
  }

  const outgoing = state.outgoing;
  const outgoingRequirementIds = new Set(
    outgoing?.requirements.map((requirement) => requirement.definition.id),
  );
  const historicalApprovals = (outgoing?.history ?? [])
    .filter(
      (history) =>
        !outgoingRequirementIds.has(history.definitionId) &&
        (history.requests.length > 0 || history.decisionLog.length > 0),
    )
    .sort((a, b) => a.definitionName.localeCompare(b.definitionName));
  const headingClass = "text-xl font-semibold text-[var(--g5-text0)]";
  // The request PR rule (g5.governance.approval.request-pr) applies to both
  // kinds; a missing policy is the default, which needs no PR.
  const requestsAllowed =
    !outgoing?.requestPolicy?.prRequired ||
    (!!reviewFlow.data?.pr && !reviewFlow.error);

  return (
    <div className="mx-auto w-full max-w-4xl px-6 py-8">
      {/* Approvals requested from this viewer, as plain text: they are
          already here (g5.governance.approval.personal-requests). */}
      <PersonalApprovalsBanner
        workspaceId={workspaceId}
        viewerUserId={callerUserId}
        navigation={false}
      />
      {stateQuery.refreshError && (
        <p
          role="status"
          data-testid="workspace-approvals-refresh-error"
          className="mb-4 rounded border border-[var(--g5-warn)]/50 px-3 py-2 text-xs text-[var(--g5-warn)]"
        >
          Could not refresh approvals: showing them as last loaded.
        </p>
      )}
      <section aria-labelledby="approvals-required-heading">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <h2
            id="approvals-required-heading"
            className={`${headingClass} min-w-0 flex-1 break-words`}
          >
            Approvals Required to merge into{" "}
            {outgoing ? (
              <Link
                className="text-[var(--g5-info)] hover:underline"
                to={`/${org}/${encodeURIComponent(outgoing.targetWorkspaceName)}`}
              >
                {outgoing.targetWorkspaceName}
              </Link>
            ) : (
              "a parent workspace"
            )}
          </h2>
          {canRequestReviews && outgoing && (
            <ApprovalRequestComposer
              workspaceId={workspaceId}
              ownerId={callerUserId}
              requirements={outgoing.requirements}
              requestPolicy={outgoing.requestPolicy}
              disabled={!!approvalActionsUnavailableCopy}
              prefill={requestPrefill}
              onPrefillClose={() => setRequestPrefill(null)}
            />
          )}
        </div>
        {outgoing && (
          <MergeWhenReadyNote
            workspaceId={workspaceId}
            targetWorkspaceId={outgoing.targetWorkspaceId}
            targetName={outgoing.targetWorkspaceName}
          />
        )}
        {outgoing ? (
          <div
            data-testid="workspace-approvals-outgoing"
            className="divide-y divide-[var(--g5-stroke)]"
          >
            {outgoing.requirements.length === 0 && (
              <p
                data-testid="workspace-approvals-empty"
                className="py-3 text-sm"
              >
                {outgoing.targetWorkspaceName} requires no approvals.
              </p>
            )}
            {[...outgoing.requirements]
              .sort((a, b) =>
                a.definition.name.localeCompare(b.definition.name),
              )
              .map((requirement) => (
                <CompactRequirement
                  key={`${workspaceId}:${requirement.definition.id}`}
                  requirement={requirement}
                  state={state}
                  workspaceId={workspaceId}
                  canRequest={
                    canRequestReviews && !approvalActionsUnavailableCopy
                  }
                  requestsAllowed={requestsAllowed}
                  actionsUnavailable={!!approvalActionsUnavailableCopy}
                  viewerUserId={callerUserId}
                  onRequestPicks={openRequestPicks}
                />
              ))}
            {historicalApprovals.length > 0 && (
              <details
                key={`${workspaceId}:${outgoing.targetWorkspaceId}`}
                data-testid="workspace-approvals-legacy"
                className="pt-4"
              >
                <summary
                  data-testid="workspace-approvals-legacy-toggle"
                  className="cursor-pointer text-sm font-semibold text-[var(--g5-text2)]"
                >
                  Legacy approvals ({historicalApprovals.length})
                </summary>
                <p className="mt-2 text-xs text-[var(--g5-text2)]">
                  Earlier requests and decisions for approvals that{" "}
                  {outgoing.targetWorkspaceName} does not currently require.
                </p>
                {historicalApprovals.map((history) => (
                  <HistoricalApproval
                    key={`${workspaceId}:${history.definitionId}`}
                    history={history}
                    state={state}
                    workspaceId={workspaceId}
                    canRetryCleanup={
                      canRequestReviews && !approvalActionsUnavailableCopy
                    }
                  />
                ))}
              </details>
            )}
          </div>
        ) : (
          <p className="text-sm text-[var(--g5-text2)]">
            This workspace has no parent merge target.
          </p>
        )}
      </section>
      <section
        ref={reviewsRef}
        data-testid="workspace-approvals-reviews"
        className="mt-8"
      >
        {workspaceDetailLoaded ? (
          <ApprovalReviewsSection
            workspaceId={workspaceId}
            workspaceName={workspaceName}
            orgSlug={org ?? ""}
            viewerUserId={callerUserId}
            viewerOwnsWorkspace={viewerOwnsWorkspace}
            onOpenFile={onOpenWorkspaceFile}
          />
        ) : (
          <>
            <h2 className={headingClass}>Reviews</h2>
            <p role="status" className="mt-3 text-sm text-[var(--g5-text2)]">
              {workspaceQuery.status === "error"
                ? "Could not load review details."
                : "Loading review details…"}
            </p>
          </>
        )}
      </section>
      <section aria-labelledby="approvals-configure-heading" className="mt-8">
        <h2 id="approvals-configure-heading" className={headingClass}>
          Configure Approvals Required to merge into this workspace
        </h2>
        <ManagementSection
          state={state}
          canWrite={canWrite}
          workspaceId={workspaceId}
          workspaceName={workspaceName}
          onRequire={openPicker}
        />
      </section>
    </div>
  );
}
