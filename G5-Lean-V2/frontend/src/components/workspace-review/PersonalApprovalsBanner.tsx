// =============================================================================
// Approvals requested from you — the personal banner
// (g5.governance.approval.personal-requests)
// =============================================================================
// One banner for the workspace Overview (with navigation to Approvals) and the
// Approvals page (plain text, no navigation). It reads the same approvals
// query as the landing summary and the Approvals roster, so its loading and
// unavailable states are that read's, never a separate counter.
// =============================================================================

import { useQueryClient } from "@tanstack/react-query";
import { ArrowRight, CircleHelp, Inbox } from "lucide-react";
import { Link } from "react-router-dom";
import {
  useWorkspaceApprovals,
  workspaceApprovalsQueryOptions,
} from "../../hooks/useWorkspaceApprovals";
import {
  personalApprovals,
  personalApprovalsTitle,
} from "../../lib/personalApprovals";

const APPROVALS = "?view=approvals";
const SHOWN_NAMES = 3;
const FOCUS =
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] focus-visible:ring-offset-1";
// Border colours are important: index.css gives every element the stroke colour.
const BANNER =
  "mb-5 flex flex-wrap items-center gap-3.5 rounded-lg border border-[color-mix(in_srgb,var(--g5-info)_35%,var(--g5-stroke))]! bg-[color-mix(in_srgb,var(--g5-info)_9%,var(--g5-bg0))] px-5 py-4 max-sm:items-start max-sm:gap-3 max-sm:px-3.5";
const ICON =
  "flex h-9 w-9 flex-none items-center justify-center rounded-lg bg-[color-mix(in_srgb,var(--g5-info)_12%,transparent)] text-[var(--g5-info)]";

export function PersonalApprovalsBanner({
  workspaceId,
  viewerUserId,
  navigation,
}: {
  workspaceId: string;
  viewerUserId: string;
  /** The Overview links to Approvals; the Approvals page shows plain text. */
  navigation: boolean;
}) {
  const queryClient = useQueryClient();
  const state = personalApprovals(
    useWorkspaceApprovals(workspaceId),
    viewerUserId,
  );
  if (state.kind === "none") return null;
  if (state.kind === "loading")
    return (
      <section
        role="status"
        aria-label="Loading your approval requests"
        data-testid="personal-approvals-banner"
        data-state="loading"
        className={BANNER}
      >
        <p className="text-sm text-[var(--g5-text1)]">
          Loading your approval requests…
        </p>
      </section>
    );
  if (state.kind === "unavailable")
    return (
      <section
        role="status"
        data-testid="personal-approvals-banner"
        data-state="unavailable"
        className={BANNER}
      >
        <span className={ICON} aria-hidden="true">
          <CircleHelp size={18} />
        </span>
        <p className="min-w-0 flex-1 text-sm text-[var(--g5-text1)]">
          Couldn’t load your approval requests.
        </p>
        {/* The Approvals page already reports its failed refresh, so only
            the Overview offers Retry and the way to Approvals. */}
        {navigation && (
          <>
            <button
              type="button"
              data-testid="personal-approvals-retry"
              className={`text-xs text-[var(--g5-info)] hover:underline ${FOCUS}`}
              onClick={() =>
                void queryClient.refetchQueries({
                  queryKey:
                    workspaceApprovalsQueryOptions(workspaceId).queryKey,
                })
              }
            >
              Retry
            </button>
            <Link
              to={APPROVALS}
              className={`text-xs text-[var(--g5-info)] hover:underline ${FOCUS}`}
            >
              Approvals
            </Link>
          </>
        )}
      </section>
    );

  const { names, requester } = state;
  const shown = navigation ? names.slice(0, SHOWN_NAMES) : names;
  const chip = `rounded-[5px] border border-[color-mix(in_srgb,var(--g5-info)_35%,var(--g5-stroke))]! bg-[var(--g5-bg0)] px-2 py-0.5 text-xs text-[var(--g5-info)] hover:bg-[color-mix(in_srgb,var(--g5-info)_10%,var(--g5-bg0))] ${FOCUS}`;
  return (
    <section
      aria-label="Your requested approvals"
      data-testid="personal-approvals-banner"
      data-state="requested"
      className={BANNER}
    >
      <span className={ICON} aria-hidden="true">
        <Inbox size={18} />
      </span>
      <div className="min-w-0 flex-1 max-sm:basis-[calc(100%-3rem)]">
        <h2 className="text-[17px] font-semibold leading-snug text-[var(--g5-text0)] max-sm:text-base">
          {personalApprovalsTitle(names.length)}
        </h2>
        <p className="mt-1 text-xs text-[var(--g5-text1)]">
          {requester
            ? `${requester} requested your review.`
            : "Your review has been requested for these approvals."}
        </p>
        {navigation ? (
          <div
            data-testid="personal-approvals-names"
            className="mt-2 flex flex-wrap items-center gap-2"
          >
            {shown.map((name) => (
              <Link key={name} to={APPROVALS} className={chip}>
                {name}
              </Link>
            ))}
            {names.length > SHOWN_NAMES && (
              <Link to={APPROVALS} className={chip}>
                +{names.length - SHOWN_NAMES} more
              </Link>
            )}
          </div>
        ) : (
          <p
            data-testid="personal-approvals-names"
            className="mt-1.5 text-[13px] text-[var(--g5-text1)] [overflow-wrap:anywhere]"
          >
            {names.join(" · ")}
          </p>
        )}
      </div>
      {navigation && (
        <Link
          to={APPROVALS}
          data-testid="personal-approvals-go"
          className={`inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md bg-[var(--g5-info)] px-3.5 py-2 text-xs font-semibold text-white hover:brightness-110 max-sm:w-full ${FOCUS}`}
        >
          Go to Approvals
          <ArrowRight size={15} aria-hidden="true" />
        </Link>
      )}
    </section>
  );
}
