import { AlertTriangle, Loader2 } from "lucide-react";
import React from "react";

import type {
  AsyncState,
  ChangeGraphState,
  RebaseChangeGraphNodeDetail,
  SourceControlChangeGraphDiffEvidence,
  SourceControlChangeGraphNodeDetails,
} from "../../lib/types";
import { DiffHunksView } from "../editor/DiffHunksView";
import { classNames } from "../ui/UI";
import { ChangeGraphHumanBadge } from "./ChangeGraphPrimitives";

export function SourceControlChangeGraphWhatChanged({
  state,
  onRetry,
}: {
  state: AsyncState<SourceControlChangeGraphNodeDetails>;
  onRetry?: () => void;
}) {
  if (state.status === "loading") {
    return (
      <div
        role="status"
        className="flex items-center gap-2 py-3 text-[var(--g5-text2)]"
      >
        <Loader2
          size={13}
          className="animate-spin motion-reduce:animate-none"
        />
        Loading exact change evidence…
      </div>
    );
  }
  if (state.status === "error") {
    return (
      <div role="alert" className="space-y-2 py-3 text-[var(--g5-blocked)]">
        <p>Exact change evidence could not be loaded: {state.error.message}</p>
        {onRetry && (
          <button
            type="button"
            data-testid="change-graph-what-changed-retry"
            onClick={onRetry}
            className="rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1 text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          >
            Retry
          </button>
        )}
      </div>
    );
  }

  return <ChangeGraphDetailEvidence details={state.data} />;
}

export function ChangeGraphDetailEvidence({
  details,
  overlapIds = [],
  presentation = "standalone",
}: {
  details: SourceControlChangeGraphNodeDetails | RebaseChangeGraphNodeDetail;
  overlapIds?: readonly string[];
  presentation?: "standalone" | "paired";
}) {
  const paired = presentation === "paired";
  if (details.kind === "unchanged_context") {
    return (
      <p className="py-3 text-[var(--g5-text2)]">
        {paired
          ? "No authored change on this side."
          : "This contextual artifact has no change in the current diff."}
      </p>
    );
  }
  if (details.kind === "file_diff") {
    return (
      <DiffEvidenceView
        diff={details.diff}
        testId={paired ? "rebase-file-diff" : "change-graph-file-diff"}
        scrollLabel="File diff"
      />
    );
  }

  return (
    <div
      data-testid={
        paired ? "rebase-semantic-diff" : "change-graph-semantic-diff"
      }
      className={paired ? "space-y-3" : "space-y-4"}
    >
      {details.records.length === 0 && !details.otherFileChanges ? (
        <p className="py-3 text-[var(--g5-text2)]">
          {paired
            ? "No changed statements."
            : "No statements changed in this diff."}
        </p>
      ) : (
        details.records.map((record) => (
          <section
            key={record.id}
            data-testid={paired ? "rebase-changed-block" : undefined}
            data-block-id={paired ? record.id : undefined}
            data-overlap={paired ? overlapIds.includes(record.id) : undefined}
            className={classNames(
              "overflow-hidden rounded border bg-[var(--g5-bg0)]",
              paired && overlapIds.includes(record.id)
                ? "border-[var(--g5-blocked)]/70"
                : "border-[var(--g5-stroke)]",
            )}
          >
            <header
              className={classNames(
                "flex items-center gap-2 border-b px-3 py-2",
                paired && overlapIds.includes(record.id)
                  ? "border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/5"
                  : "border-[var(--g5-stroke)]",
              )}
            >
              <span
                className={classNames(
                  "rounded px-1.5 py-0.5 font-mono text-[10px]",
                  paired && overlapIds.includes(record.id)
                    ? "bg-[var(--g5-blocked)]/15 text-[var(--g5-blocked)]"
                    : "bg-[var(--g5-bg2)] text-[var(--g5-text1)]",
                )}
              >
                {record.id}
              </span>
              <div className="flex min-w-0 flex-1 items-center gap-1.5">
                <span className="min-w-0 truncate font-medium">
                  {record.title || statementKind(record.recordKind)}
                </span>
                {record.isHuman && <ChangeGraphHumanBadge />}
              </div>
              {paired && overlapIds.includes(record.id) ? (
                <span className="rounded border border-[var(--g5-blocked)]/60 bg-[var(--g5-blocked)]/10 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-[var(--g5-blocked)]">
                  Overlap
                </span>
              ) : (
                <StateBadge state={record.changeState} />
              )}
            </header>
            <DiffEvidenceView
              diff={record.diff}
              testId={`${paired ? "rebase" : "change-graph"}-semantic-diff-${record.id}`}
              scrollLabel={`${record.id} diff`}
            />
          </section>
        ))
      )}
      {details.otherFileChanges && (
        <section className="overflow-hidden rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)]">
          <h5 className="border-b border-[var(--g5-stroke)] px-3 py-2 text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
            Other changes (metadata, headings or prose)
          </h5>
          <DiffEvidenceView
            diff={details.otherFileChanges}
            testId={
              paired
                ? "rebase-semantic-other-file-diff"
                : "change-graph-semantic-other-file-diff"
            }
            scrollLabel="Other file changes diff"
          />
        </section>
      )}
      {details.truncated && (
        <TruncationNotice text="Some statement change evidence was truncated at the response limit." />
      )}
    </div>
  );
}

/* eslint-disable jsx-a11y/no-noninteractive-tabindex -- Native horizontal overflow regions must be keyboard focusable. */
function DiffEvidenceView({
  diff,
  testId,
  scrollLabel,
}: {
  diff: SourceControlChangeGraphDiffEvidence;
  testId: string;
  scrollLabel: string;
}) {
  if (diff.availability !== "available") {
    const message = {
      binary: "This binary change has no textual diff.",
      oversized:
        "This diff exceeds the 10 MiB input limit and was not expanded.",
      unavailable:
        "Exact diff evidence is unavailable for this retained graph snapshot.",
    }[diff.availability];
    return <p className="p-3 text-[var(--g5-text2)]">{message}</p>;
  }
  if (diff.hunks.length === 0) {
    return (
      <p className="p-3 text-[var(--g5-text2)]">
        No textual diff hunks were returned for this change.
      </p>
    );
  }
  return (
    <div className="min-w-0">
      <div
        role="region"
        aria-label={scrollLabel}
        tabIndex={0}
        data-testid={`${testId}-scroll`}
        className="min-w-0 overflow-x-auto overscroll-x-contain focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--g5-info)]"
      >
        <DiffHunksView
          hunks={diff.hunks}
          additions={diff.additions}
          deletions={diff.deletions}
          partialFinalHunk={diff.partialFinalHunk}
          testId={testId}
        />
      </div>
      {diff.truncated && !diff.partialFinalHunk && (
        <TruncationNotice text="Additional diff hunks were omitted at the response limit." />
      )}
    </div>
  );
}
/* eslint-enable jsx-a11y/no-noninteractive-tabindex */

function TruncationNotice({ text }: { text: string }) {
  return (
    <p className="flex items-start gap-1.5 border-t border-[var(--g5-warn)]/35 bg-[var(--g5-warn)]/10 px-3 py-2 text-[10px] text-[var(--g5-text1)]">
      <AlertTriangle size={12} className="shrink-0 text-[var(--g5-warn)]" />
      {text}
    </p>
  );
}

function StateBadge({ state }: { state: ChangeGraphState }) {
  const labels: Record<ChangeGraphState, string> = {
    added: "Added",
    modified: "Modified",
    deleted: "Deleted",
    unchanged: "Unchanged",
  };
  return (
    <span className="rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-[var(--g5-text1)]">
      {labels[state]}
    </span>
  );
}

function statementKind(kind: string): string {
  return kind === "I"
    ? "Intent"
    : kind === "D"
      ? "Decision"
      : kind === "Q"
        ? "Question"
        : "Statement";
}
