// =============================================================================
// Home — the Re-run verification dialog (ui g5.ui.home-reverification.dialog)
// =============================================================================
// Explains a Base re-verification run, shows the status's counts with the
// pull requests merged since the last completed run and every run (both
// paged like Home's tiles), and starts an incremental or a full run: a fork
// of Base admitted with its run (g5.reverification.run). A start lands on
// the new workspace's chat, pre-typed with the run's prompt under the cost
// notice and never sent (g5.reverification.workspace); a refusal stays here
// with the server's reason.
// =============================================================================

import { useQueryClient } from "@tanstack/react-query";
import { Loader2, X } from "lucide-react";
import React, { useRef, useState } from "react";
import { Link, useNavigate } from "react-router-dom";

import { baseReverificationQueryKey } from "../../hooks/useBaseReverification";
import { useCloneWorkspace } from "../../hooks/useWorkspace";
import { ApiError, api } from "../../lib/api";
import {
  REVERIFICATION_INCREMENTAL_UNAVAILABLE_REASON,
  REVERIFICATION_KIND_LABELS,
  baseReverificationWorkspaceName,
  formatReverificationDate,
  reverificationLinesText,
  reverificationRunStateText,
  reverificationStartBlockedReason,
  type EnabledBaseReverificationStatus,
} from "../../lib/baseReverification";
import { randomUuid } from "../../lib/randomUuid";
import {
  BASE_REVERIFICATION_NOTICE,
  baseReverificationSeed,
} from "../../lib/reviewPrompts";
import { routes } from "../../lib/routes";
import type {
  BaseReverificationStatus,
  OrgMemberDTO,
  ReverificationKind,
  ReverificationRun,
} from "../../lib/types";
import { findOwner, memberLabel } from "../../lib/workspaceOwner";
import {
  forkLandingQuery,
  forkLandingState,
  type ForkLanding,
} from "../../lib/workspaceLanding";
import { workspacePagePath } from "../../lib/workspacePagePath";
import { Modal, classNames } from "../ui/UI";
import { HomeShowMore } from "./HomeTileParts";

export interface HomeReverificationDialogProps {
  orgId: string;
  orgSlug: string;
  status: EnabledBaseReverificationStatus;
  /** The organization roster, for starters' and mergers' names. */
  members: OrgMemberDTO[];
  /** Home's per-person page size (g5.ui.home-paging). */
  pageSize: number;
  onClose: () => void;
}

const CHOICE_CLASSES =
  "w-full rounded-[var(--g5-radius-sm)] border px-4 py-2.5 text-left text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] disabled:cursor-not-allowed disabled:opacity-50";

function memberName(members: OrgMemberDTO[], userId: string | null): string {
  const member = findOwner(members, userId);
  return (member && memberLabel(member)) || "Unknown member";
}

/**
 * The server's reason for a refusal, as a person reads it: a validation's own
 * message rather than its JSON, and a conflict without its code prefixes.
 */
export function reverificationRefusalMessage(error: unknown): string {
  if (error instanceof ApiError) {
    const detail = error.details?.message;
    if (error.code === "VALIDATION_ERROR" && typeof detail === "string") {
      return detail;
    }
    // A fork's own refusal can carry the prefix twice, once from the reason
    // it recorded and once from the response.
    return error.message.replace(/^(?:Conflict: )+/, "");
  }
  return (
    (error instanceof Error && error.message) ||
    "Could not start the re-verification."
  );
}

/** A definitive refusal: nothing was admitted under the attempt's key. */
function isRefusal(error: unknown): boolean {
  return (
    error instanceof ApiError &&
    error.isClientError &&
    error.status !== 408 &&
    error.status !== 429
  );
}

export function HomeReverificationDialog({
  orgId,
  orgSlug,
  status,
  members,
  pageSize,
  onClose,
}: HomeReverificationDialogProps) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const start = useCloneWorkspace(status.baseWorkspaceId);
  const [startingKind, setStartingKind] = useState<ReverificationKind | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [pullRequestsShown, setPullRequestsShown] = useState(pageSize);
  const [runsShown, setRunsShown] = useState(pageSize);
  // One fork request per kind: a retry after an unknown outcome replays the
  // same request (key and name), so it cannot fork twice; a refusal admitted
  // nothing, so the next attempt starts a new one.
  const attempts = useRef<
    Partial<Record<ReverificationKind, { key: string; name: string }>>
  >({});
  // The kind whose last start had an unknown outcome. Its fork may have been
  // admitted, so the status now shows its run in progress; the kept request
  // stays offered, since a replay finishes that start and cannot fork twice.
  const [retryKind, setRetryKind] = useState<ReverificationKind | null>(null);

  const starting = startingKind !== null;
  /** The in-progress run of the workspace just forked, from a fresh status. */
  const readStartedRun = async (
    workspaceId: string,
  ): Promise<ReverificationRun | null> => {
    try {
      const fresh = await queryClient.fetchQuery({
        queryKey: baseReverificationQueryKey(orgId),
        queryFn: () =>
          api.get<BaseReverificationStatus>(
            routes.orgs.homeBaseReverification(orgId),
          ),
        staleTime: 0,
      });
      return fresh.enabled && fresh.inProgress?.workspaceId === workspaceId
        ? fresh.inProgress
        : null;
    } catch {
      return null;
    }
  };
  const blockedReason = reverificationStartBlockedReason(status);
  const blockedFor = (kind: ReverificationKind) =>
    !!blockedReason && kind !== retryKind;
  const incrementalUnavailable = status.lastCompleted === null;
  const lines = reverificationLinesText(status);

  const begin = async (kind: ReverificationKind) => {
    if (starting || blockedFor(kind)) return;
    if (kind === "incremental" && incrementalUnavailable) return;
    const attempt = (attempts.current[kind] ??= {
      key: randomUuid(),
      name: baseReverificationWorkspaceName(),
    });
    setStartingKind(kind);
    setError(null);
    try {
      const result = await start.mutateAsync({
        name: attempt.name,
        idempotencyKey: attempt.key,
        uniquifyName: true,
        reverification: { kind },
      });
      delete attempts.current[kind];
      void queryClient.invalidateQueries({
        queryKey: baseReverificationQueryKey(orgId),
      });
      // The response names the run; if it ever does not, the status does.
      const run =
        result.reverificationRun ?? (await readStartedRun(result.workspace.id));
      const seed = run ? baseReverificationSeed(run) : null;
      const landing: ForkLanding = seed
        ? { chatSessionId: null, draft: seed.text, header: seed.header }
        : { chatSessionId: null, draft: null };
      onClose();
      navigate(
        `${workspacePagePath(orgSlug, result.workspace.name)}${forkLandingQuery(landing)}`,
        { state: forkLandingState(landing) },
      );
    } catch (caught) {
      const refused = isRefusal(caught);
      if (refused) delete attempts.current[kind];
      setRetryKind(refused ? null : kind);
      // A refusal may mean another run started meanwhile: re-read the status
      // so the dialog and the line show it.
      void queryClient.invalidateQueries({
        queryKey: baseReverificationQueryKey(orgId),
      });
      const message = reverificationRefusalMessage(caught);
      setError(
        refused
          ? message
          : `${message.replace(/\.?$/, ".")} Choose it again to retry.`,
      );
      setStartingKind(null);
    }
  };

  const lastCompleted = status.lastCompleted;
  const pullRequests = status.pullRequests;
  const runs = status.runs;

  return (
    <Modal
      onClose={onClose}
      labelledBy="home-reverification-dialog-title"
      size="lg"
      closeDisabled={starting}
      className="flex max-h-[90vh] flex-col p-0"
      testId="home-reverification-dialog"
    >
      <div className="flex items-center justify-between border-b border-[var(--g5-stroke)] px-5 py-4">
        <h2
          id="home-reverification-dialog-title"
          className="text-base font-semibold text-[var(--g5-text0)]"
        >
          Re-run verification of Base
        </h2>
        <button
          type="button"
          data-testid="home-reverification-dialog-close"
          onClick={onClose}
          disabled={starting}
          title="Close"
          aria-label="Close"
          className="inline-flex h-7 w-7 items-center justify-center rounded-[var(--g5-radius-sm)] text-[var(--g5-text2)] transition-colors hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] disabled:opacity-50"
        >
          <X size={16} aria-hidden="true" />
        </button>
      </div>

      <div className="min-h-0 flex-1 space-y-5 overflow-y-auto px-5 py-4 text-sm text-[var(--g5-text1)]">
        <section aria-label="About re-verification" className="space-y-3">
          <p
            data-testid="home-reverification-notice"
            role="note"
            className="rounded-md border border-[var(--g5-warn)]/60 bg-[var(--g5-warn)]/15 px-4 py-3 font-semibold leading-6 text-[var(--g5-text0)]"
          >
            {BASE_REVERIFICATION_NOTICE}
          </p>
          <p className="leading-6">
            A run happens in a new workspace forked from Base. People then merge
            that workspace into Base like any pull request, with every merge
            gate and approval, and Base counts as re-verified up to the commits
            the run forked from. Its findings, bugs and design simplifications,
            are reported rather than fixed, and triaged in that workspace before
            it merges.
          </p>
        </section>

        <dl
          data-testid="home-reverification-counts"
          className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1"
        >
          <dt className="text-[var(--g5-text2)]">Last re-verified</dt>
          <dd className="text-[var(--g5-text0)]">
            {lastCompleted
              ? formatReverificationDate(
                  lastCompleted.completedAt ?? lastCompleted.startedAt,
                )
              : "Never"}
          </dd>
          {lastCompleted && (
            <>
              <dt className="text-[var(--g5-text2)]">Merged since</dt>
              <dd className="text-[var(--g5-text0)]">
                {status.pullRequestCount === 1
                  ? "1 pull request"
                  : `${status.pullRequestCount.toLocaleString("en-US")} pull requests`}
              </dd>
              <dt className="text-[var(--g5-text2)]">Lines since</dt>
              <dd className="text-[var(--g5-text0)]">{lines ?? "None"}</dd>
            </>
          )}
        </dl>

        <section aria-labelledby="home-reverification-prs-title">
          <h3
            id="home-reverification-prs-title"
            className="mb-2 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--g5-text1)]"
          >
            Pull requests merged since the last re-verification
          </h3>
          {!lastCompleted ? (
            <p className="text-[var(--g5-text2)]">
              Listed once a re-verification has completed.
            </p>
          ) : pullRequests.length === 0 ? (
            <p className="text-[var(--g5-text2)]">None.</p>
          ) : (
            <div className="overflow-hidden rounded-md border border-[var(--g5-stroke)]">
              <ul data-testid="home-reverification-prs">
                {pullRequests.slice(0, pullRequestsShown).map((pr) => (
                  <li
                    key={pr.id}
                    data-testid="home-reverification-pr"
                    className="flex items-baseline gap-3 border-b border-[var(--g5-stroke)] px-3 py-2 last:border-b-0"
                  >
                    <span className="min-w-0 flex-1 text-[var(--g5-text0)]">
                      {pr.title}
                    </span>
                    <span className="flex-none font-[var(--g5-font-mono)] text-xs text-[var(--g5-text2)]">
                      #{pr.number}
                    </span>
                    <span className="flex-none text-xs text-[var(--g5-text2)]">
                      {formatReverificationDate(pr.mergedAt)}
                      {pr.mergedBy && ` by ${memberName(members, pr.mergedBy)}`}
                    </span>
                  </li>
                ))}
              </ul>
              <HomeShowMore
                remaining={pullRequests.length - pullRequestsShown}
                pageSize={pageSize}
                noun="pull request"
                testId="home-reverification-prs-more"
                onShowMore={() =>
                  setPullRequestsShown((shown) => shown + pageSize)
                }
              />
            </div>
          )}
          {status.pullRequestCount > pullRequests.length &&
            pullRequests.length > 0 && (
              <p
                data-testid="home-reverification-prs-capped"
                className="mt-1 text-xs text-[var(--g5-text2)]"
              >
                Showing the newest {pullRequests.length} of{" "}
                {status.pullRequestCount.toLocaleString("en-US")}.
              </p>
            )}
        </section>

        <section aria-labelledby="home-reverification-runs-title">
          <h3
            id="home-reverification-runs-title"
            className="mb-2 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--g5-text1)]"
          >
            Runs
          </h3>
          {runs.length === 0 ? (
            <p className="text-[var(--g5-text2)]">No runs yet.</p>
          ) : (
            <div className="overflow-hidden rounded-md border border-[var(--g5-stroke)]">
              <ul data-testid="home-reverification-runs">
                {runs.slice(0, runsShown).map((run) => (
                  <RunRow
                    key={run.id}
                    run={run}
                    orgSlug={orgSlug}
                    starter={memberName(members, run.startedBy)}
                  />
                ))}
              </ul>
              <HomeShowMore
                remaining={runs.length - runsShown}
                pageSize={pageSize}
                noun="run"
                testId="home-reverification-runs-more"
                onShowMore={() => setRunsShown((shown) => shown + pageSize)}
              />
            </div>
          )}
          {status.runCount > runs.length && runs.length > 0 && (
            <p
              data-testid="home-reverification-runs-capped"
              className="mt-1 text-xs text-[var(--g5-text2)]"
            >
              Showing the newest {runs.length} of{" "}
              {status.runCount.toLocaleString("en-US")} runs.
            </p>
          )}
        </section>
      </div>

      <div className="space-y-2 border-t border-[var(--g5-stroke)] px-5 py-4">
        {blockedReason && (
          <p
            data-testid="home-reverification-blocked"
            className="text-sm text-[var(--g5-text2)]"
          >
            {blockedReason}.
          </p>
        )}
        <div>
          <button
            type="button"
            data-testid="home-reverification-incremental"
            disabled={
              starting || blockedFor("incremental") || incrementalUnavailable
            }
            aria-describedby={
              incrementalUnavailable
                ? "home-reverification-incremental-reason"
                : undefined
            }
            onClick={() => void begin("incremental")}
            className={classNames(
              CHOICE_CLASSES,
              "border-[var(--g5-stroke)] text-[var(--g5-text0)] enabled:hover:bg-[var(--g5-bg2)]",
            )}
          >
            Incrementally re-verify the changes since the last re-verification
          </button>
          {incrementalUnavailable && (
            <p
              id="home-reverification-incremental-reason"
              data-testid="home-reverification-incremental-reason"
              className="mt-1 text-xs text-[var(--g5-text2)]"
            >
              {REVERIFICATION_INCREMENTAL_UNAVAILABLE_REASON}
            </p>
          )}
        </div>
        <button
          type="button"
          data-testid="home-reverification-full"
          disabled={starting || blockedFor("full")}
          onClick={() => void begin("full")}
          className={classNames(
            CHOICE_CLASSES,
            "border-[var(--g5-stroke)] text-[var(--g5-text0)] enabled:hover:bg-[var(--g5-bg2)]",
          )}
        >
          Re-verify the entire codebase
        </button>
        {starting && (
          <p
            role="status"
            data-testid="home-reverification-starting"
            className="flex items-center gap-2 text-sm text-[var(--g5-text1)]"
          >
            <Loader2 size={14} className="animate-spin" aria-hidden="true" />
            Forking Base for the{" "}
            {startingKind === "incremental" ? "incremental" : "full"}{" "}
            re-verification…
          </p>
        )}
        {error && (
          <p
            role="alert"
            data-testid="home-reverification-error"
            className="text-sm text-[var(--g5-blocked)]"
          >
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}

function RunRow({
  run,
  orgSlug,
  starter,
}: {
  run: ReverificationRun;
  orgSlug: string;
  starter: string;
}) {
  return (
    <li
      data-testid="home-reverification-run"
      data-run-id={run.id}
      className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 border-b border-[var(--g5-stroke)] px-3 py-2 last:border-b-0"
    >
      <span className="font-medium text-[var(--g5-text0)]">
        {REVERIFICATION_KIND_LABELS[run.kind]}
      </span>
      <span className="min-w-0 flex-1">
        {run.workspaceName && run.workspaceDeleted ? (
          <span className="text-[var(--g5-text1)]">
            {run.workspaceName}{" "}
            <span className="text-xs text-[var(--g5-text2)]">(deleted)</span>
          </span>
        ) : run.workspaceName ? (
          <Link
            to={workspacePagePath(orgSlug, run.workspaceName)}
            className="text-[var(--g5-info)] underline-offset-2 hover:underline"
          >
            {run.workspaceName}
          </Link>
        ) : (
          <span className="text-[var(--g5-text2)]">Workspace unavailable</span>
        )}
      </span>
      <span className="text-xs text-[var(--g5-text2)]">
        by {starter}, {formatReverificationDate(run.startedAt)}
      </span>
      <span
        data-testid="home-reverification-run-state"
        className="text-xs text-[var(--g5-text1)]"
      >
        {reverificationRunStateText(run)}
      </span>
    </li>
  );
}
