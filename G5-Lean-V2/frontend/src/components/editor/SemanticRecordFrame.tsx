// =============================================================================
// SemanticRecordFrame — the ONE semantic record frame (D69)
// =============================================================================
// Shared read-only statement layout, authored metadata and question controls.

import React, { useId, useState, type ReactNode } from "react";
import { classNames } from "../ui/UI";
import { renderInlineMarkdown } from "../../lib/utils";
import type { BlockTag, RecordSpan } from "../../lib/utils";
import { SEVERITY_LABEL, SEVERITY_TOKEN } from "../../lib/severity";
import type { QuestionSeverity } from "../../lib/questionBlock";
import { Info } from "lucide-react";
import {
  MarkdownPreview,
  type MarkdownPreviewProps,
} from "../files/MarkdownPreview";

export const RECORD_KIND_META: Record<RecordSpan["kind"], { label: string }> = {
  I: { label: "Intent" },
  D: { label: "Design" },
  Q: { label: "Question" },
};

export interface RecordQuestionAccent {
  severity: QuestionSeverity | null;
  /** The payload's own `status: answered` — what the status TAG reports. */
  answered: boolean;
  /**
   * Answered AND carrying a `user` provenance. Only this is a settled human
   * decision (D10), so only this takes the green a `human` tag gives any
   * record; an agent-proposed answer keeps the open treatment (D69).
   */
  humanAnswered: boolean;
  /**
   * The record's fence exists but its payload does not parse. Validity is a
   * precondition of authority (D69/I999): stored label tags may still REPORT
   * what the document last recorded, but they must not confer the
   * human-decision green over a payload nobody can read.
   */
  invalidPayload?: boolean;
}

/** One read-only badge for authored provenance or derived question state. */
const RECORD_TAG_BADGE_BASE =
  "shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-semibold";

interface RecordTagBadgeProps {
  testId: string;
  label: string;
  ariaLabel?: string;
  /** Literal Tailwind colour classes, kept per tag so appearance is unchanged. */
  className?: string;
  /** Colour carried as tokens instead, for the severity palette. */
  style?: React.CSSProperties;
  authored?: boolean;
}

function RecordTagBadge({
  testId,
  label,
  ariaLabel,
  className,
  style,
  authored = false,
}: RecordTagBadgeProps) {
  return (
    <span
      data-testid={testId}
      data-tag-source={authored ? "authored" : "system"}
      // Not statement text, so change highlights skip it.
      data-change-highlight-ignore
      aria-label={ariaLabel}
      style={style}
      className={classNames(RECORD_TAG_BADGE_BASE, className)}
    >
      {label}
    </span>
  );
}

/**
 * Preview layout of a statement in an ontology file
 * (`g5.documents.ontology.disclosure`): a marker column, the lead as the
 * frame's content, then the statement's own details, its verification panel and
 * its child statements. A fold hides the details and children together.
 */
export interface StatementLayout {
  /** `fold`: a top-level statement with something to open; `dot`: nothing to
   * open; `child`: a sub-bullet whose details and children always show. */
  marker: "fold" | "dot" | "child";
  /** Whether the fold is open; folds start closed. */
  open?: boolean;
  onToggle?: () => void;
  /** The start of the lead, naming the fold's button. */
  leadText: string;
  /** The rest of the statement's own content. */
  details?: ReactNode;
  /** Child statements, rendered as sub-bullets. */
  childStatements?: ReactNode;
  /** The verification status control, at the end of the actions rail. */
  status?: ReactNode;
  /** Verification details, below the statement's own content. */
  panel?: ReactNode;
}

export interface SemanticRecordFrameProps {
  /** Historical display must not expose current-document edit/navigation selectors. */
  historical?: boolean;
  /** Display-only bullet; never inserted into authored Markdown. */
  presentationBullet?: boolean;
  record: RecordSpan;
  tags?: readonly BlockTag[];
  /** Present only for a Q record whose fence parsed. */
  questionAccent?: RecordQuestionAccent | null;
  /** Controls left of the details toggle (the chat card's Edit). */
  navigationControl?: ReactNode;
  metadata?: string;
  metadataContext?: Pick<
    MarkdownPreviewProps,
    "filePath" | "workspaceId" | "repoId" | "onOpenWorkspaceFile" | "plugins"
  >;
  /** Open details, when the owner keeps them across remounts. */
  detailsOpen?: boolean;
  onDetailsOpenChange?: (open: boolean) => void;
  /** Ontology Preview layout; absent for every other statement display. */
  statement?: StatementLayout;
  children: ReactNode;
}

export const SemanticRecordFrame = React.memo<SemanticRecordFrameProps>(
  function SemanticRecordFrame({
    historical = false,
    presentationBullet = false,
    record,
    tags = record.metadataTags ?? [],
    questionAccent,
    navigationControl,
    metadata,
    metadataContext,
    detailsOpen: keptOpen,
    onDetailsOpenChange,
    statement,
    children,
  }) {
    const kind = RECORD_KIND_META[record.kind];
    const [localOpen, setLocalOpen] = useState(false);
    const detailsOpen = keptOpen ?? localOpen;
    const setDetailsOpen = (change: (open: boolean) => boolean) => {
      const next = change(detailsOpen);
      setLocalOpen(next);
      onDetailsOpenChange?.(next);
    };
    const detailsId = useId();
    const foldId = useId();
    const folded = statement?.marker === "fold" && !statement.open;
    const contentChildren = React.Children.toArray(children);
    const humanTags = tags.filter((tag) => tag.kind === "human");
    const uncertaintyTags = tags.filter((tag) => tag.kind === "uncertainty");
    // Question state is STORED in the label suffix so consumers see it without
    // parsing the fence — but the payload is what the user actually answered,
    // so it is trusted for display and the label is the index corrected on the
    // next apply (D69). A label alone must never make the card claim an answer
    // the payload does not hold, which is the whole reason these tags are not
    // hand-editable. The stored tags render only when there is no parseable
    // payload to read: a corrupted fence still reports what the document last
    // recorded about it.
    const storedAnswered = tags.some((tag) => tag.kind === "answered");
    const storedSeverity =
      tags.find((tag) => tag.kind === "severity")?.severity ?? null;
    const questionState: RecordQuestionAccent | null =
      questionAccent?.invalidPayload
        ? // Stored tags still REPORT what the document last recorded, but a
          // payload nobody can read verifies nothing: `Answered` renders in
          // the warn colour and the accent never goes green (D69/I999). With
          // no stored tags there is nothing to report either — rendering an
          // `Open` tag over an unreadable payload would claim a status the
          // document does not hold; the fence's invalid notice covers it.
          storedAnswered || storedSeverity
          ? {
              severity: storedSeverity,
              answered: storedAnswered,
              humanAnswered: false,
            }
          : null
        : (questionAccent ??
          (storedAnswered || storedSeverity
            ? {
                severity: storedSeverity,
                answered: storedAnswered,
                // The writer mints `answered` only for a USER provenance, so
                // the stored tag already means a settled human decision.
                humanAnswered: storedAnswered,
              }
            : null));
    const recordAccent = uncertaintyTags.length
      ? { name: "uncertain", color: "var(--g5-blocked)" }
      : humanTags.length
        ? { name: "human", color: "var(--g5-ok)" }
        : questionState
          ? // A question answered BY A USER is a settled human decision, so it
            // reads like one — the same green a `human` tag gives any record.
            // An agent-proposed answer is not a decision (D10), so it keeps the
            // open treatment, as does a still-open question: its severity,
            // defaulting to the warn colour when the author gave none, so an
            // undecided question is never silent.
            questionState.humanAnswered
            ? { name: "human", color: "var(--g5-ok)" }
            : {
                name: `question-${questionState.severity ?? "open"}`,
                color: questionState.severity
                  ? SEVERITY_TOKEN[questionState.severity]
                  : "var(--g5-warn)",
              }
          : { name: "neutral", color: "var(--g5-stroke)" };
    const Content = statement ? "div" : React.Fragment;
    const tagRow = (
      <div
        data-testid="semantic-record-tags"
        className="flex min-w-0 flex-wrap items-center gap-1 empty:hidden"
      >
        {/* Question badges report the payload; authored provenance stays distinct. */}
        {questionState && (
          <RecordTagBadge
            testId="semantic-record-question-status"
            label={questionState.answered ? "Answered" : "Open"}
            // Reports the payload's status, but takes the green
            // reserved for a settled human decision only under the
            // same `user`-provenance rule as the accent (D10).
            className={
              questionState.humanAnswered
                ? "border-[var(--g5-ok)]/45 bg-[var(--g5-ok)]/12 text-[var(--g5-ok)]"
                : "border-[var(--g5-warn)]/45 bg-[var(--g5-warn)]/12 text-[var(--g5-warn)]"
            }
          />
        )}
        {questionState?.severity && (
          <RecordTagBadge
            testId="semantic-record-question-severity"
            label={SEVERITY_LABEL[questionState.severity]}
            // Keeps its colour after answering: on a tag row this is
            // block metadata beside `Uncertain`, which does not
            // grey out either, and the urgency is already spent by
            // the accent (D69).
            style={{
              borderColor: `color-mix(in srgb, ${SEVERITY_TOKEN[questionState.severity]} 45%, transparent)`,
              backgroundColor: `color-mix(in srgb, ${SEVERITY_TOKEN[questionState.severity]} 12%, transparent)`,
              color: SEVERITY_TOKEN[questionState.severity],
            }}
          />
        )}
        {/* The blocking tier's non-colour signal: `critical` and
              `high` are the same red, so red alone cannot separate
              them. Dropped once a human has decided (D69). */}
        {questionState?.severity === "critical" &&
          !questionState.humanAnswered && (
            <RecordTagBadge
              testId="semantic-record-question-blocking"
              label="Blocking"
              className="border-[var(--g5-blocked)]/45 bg-[var(--g5-blocked)]/12 text-[var(--g5-blocked)]"
            />
          )}
        {/* Human provenance shows in details and the Human filter, not
            as a permanent badge (g5.documents.ontology.presentation). */}
        {uncertaintyTags.map((_, index) => (
          <RecordTagBadge
            key={`uncertainty:${index}`}
            testId="semantic-record-uncertain-badge"
            label="Uncertain"
            ariaLabel="Uncertain"
            className="border-[var(--g5-blocked)]/45 bg-[var(--g5-blocked)]/12 text-[var(--g5-blocked)]"
            authored
          />
        ))}
      </div>
    );
    return (
      <div
        data-testid="semantic-record-shell"
        className={`group/record relative min-w-0${statement ? ` ontology-statement ontology-statement-${statement.marker}` : presentationBullet ? " ontology-statement-bullet" : ""}`}
        style={{ gridColumn: "1 / -1" }}
      >
        {statement &&
          (statement.marker === "fold" ? (
            <button
              type="button"
              className="ontology-statement-marker ontology-statement-triangle"
              data-testid="ontology-statement-toggle"
              aria-expanded={!!statement.open}
              aria-controls={[
                statement.details ? `${foldId}-details` : "",
                statement.childStatements ? `${foldId}-children` : "",
              ]
                .filter(Boolean)
                .join(" ")}
              aria-label={`${statement.open ? "Hide" : "Show"} details: ${statement.leadText}`}
              onClick={statement.onToggle}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                <path d="M8 5.5 17 12l-9 6.5z" />
              </svg>
            </button>
          ) : (
            <span
              className="ontology-statement-marker ontology-statement-dot"
              data-testid="ontology-statement-dot"
              aria-hidden="true"
            />
          ))}
        {/* Only an ontology statement wraps its content beside the marker. */}
        <Content
          {...(statement
            ? { className: "ontology-statement-content min-w-0" }
            : {})}
        >
          <section
            data-testid="semantic-record"
            data-record-span={historical ? undefined : record.id}
            data-record-kind={record.kind}
            data-record-accent={recordAccent.name}
            data-intent-record={historical ? undefined : record.id}
            data-block-index={historical ? undefined : record.spanStart}
            aria-label={`${kind.label} ${record.id}: ${record.title}`}
            className="g5-editor-prose semantic-record-frame relative min-w-0 py-2 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--g5-info)]"
          >
            <div
              data-testid="semantic-record-identity"
              className="semantic-record-actions flex items-center gap-1 text-xs leading-6 text-[var(--g5-text2)]"
            >
              {navigationControl}
              {/* The status stays last; the details toggle keeps its place. */}
              <button
                type="button"
                data-testid="semantic-record-details-toggle"
                aria-label={`Statement details for ${record.id}`}
                aria-expanded={detailsOpen}
                aria-controls={detailsId}
                onClick={() => setDetailsOpen((open) => !open)}
                className="semantic-record-details-toggle rounded p-1 hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text1)]"
              >
                <Info className="h-4 w-4" aria-hidden="true" />
              </button>
              {statement?.status}
            </div>
            <div
              className="semantic-record-body min-w-0"
              data-testid="semantic-record-body"
            >
              {!record.current && (
                <div className="flex min-w-0 items-start leading-6">
                  <strong
                    data-testid="semantic-record-title"
                    className="min-w-0 break-words text-lg font-semibold leading-6 text-[var(--g5-text0)]"
                  >
                    {renderInlineMarkdown(record.title)}
                  </strong>
                </div>
              )}
              <div
                data-testid="semantic-record-blocks"
                className="semantic-record-blocks-grid min-w-0"
              >
                {/* A statement's kind is not a tag: a Q shows no badge for it
                    (`g5.documents.ontology.presentation`). */}
                {contentChildren}
                {/* Badges stay beside the lead, above a statement's details. */}
                {statement && tagRow}
                {statement?.details && (
                  <div
                    id={`${foldId}-details`}
                    className="ontology-statement-details min-w-0"
                    data-testid="ontology-statement-details"
                    data-fold-region={record.id}
                    hidden={folded}
                  >
                    {statement.details}
                  </div>
                )}
              </div>
              {!statement && tagRow}
            </div>
            <div
              id={detailsId}
              hidden={!detailsOpen}
              className="semantic-record-metadata min-w-0 text-xs text-[var(--g5-text1)]"
              data-testid="semantic-record-metadata"
            >
              <div className="space-y-2">
                <div className="flex min-w-0 flex-wrap gap-x-6 gap-y-1">
                  <div className="font-semibold text-[var(--g5-text0)]">
                    Metadata
                  </div>
                  <div>
                    Type:{" "}
                    <span data-testid="semantic-record-kind">{kind.label}</span>
                  </div>
                  <div className="min-w-0 break-words">
                    ID:{" "}
                    <code data-testid="semantic-record-id">{record.id}</code>
                  </div>
                </div>
                {metadata && (
                  <MarkdownPreview
                    {...metadataContext}
                    content={metadata}
                    historical={historical}
                  />
                )}
              </div>
            </div>
          </section>
          {statement?.panel}
          {statement?.childStatements && (
            <div
              id={`${foldId}-children`}
              className="ontology-statement-children min-w-0"
              data-testid="ontology-statement-children"
              data-fold-region={record.id}
              hidden={folded}
            >
              {statement.childStatements}
            </div>
          )}
        </Content>
      </div>
    );
  },
);
