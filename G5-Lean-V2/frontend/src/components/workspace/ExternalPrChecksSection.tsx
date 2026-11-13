// =============================================================================
// External PR checks section — Prepare to Ship
// =============================================================================
// Displays the open PR's external provider commit statuses and check runs per
// repository, split into the checks its base branch requires and the rest, with
// an aggregate verdict (governance g5.governance.pr.checks.surface and
// g5.governance.pr.checks.required).
// Advisory by itself; when the org requires passing checks to merge
// (g5.governance.merge.external-policy) the verdict also pre-gates the Ship
// button (the server re-reads and is authoritative at merge).
// =============================================================================

import {
  CheckCircle2,
  Clock,
  ExternalLink,
  Loader2,
  XCircle,
} from "lucide-react";

import type { AsyncState } from "../../lib/types";
import { classNames } from "../ui/UI";
import type {
  ExternalChecksSnapshot,
  ExternalCheckBucket,
  ExternalCheckItem,
  ExternalChecksVerdict,
  ExternalRepoChecks,
} from "../../lib/externalPrChecks";

interface Props {
  state: AsyncState<ExternalChecksSnapshot | null>;
  verdict: ExternalChecksVerdict | null;
  /** The org requires passing external checks before merge. */
  policyOn: boolean;
}

const VERDICT_LABEL: Record<ExternalChecksVerdict, string> = {
  passing: "Passing",
  pending: "Pending",
  failing: "Failing",
};

const VERDICT_TONE: Record<ExternalChecksVerdict, string> = {
  passing: "text-[var(--g5-ok)]",
  pending: "text-[var(--g5-warn)]",
  failing: "text-[var(--g5-blocked)]",
};

function BucketIcon({
  bucket,
}: {
  bucket: ExternalCheckBucket | null | undefined;
}) {
  switch (bucket) {
    case "pass":
      return (
        <CheckCircle2 size={12} className="text-[var(--g5-ok)]" aria-hidden />
      );
    case "fail":
    case "cancel":
      return (
        <XCircle size={12} className="text-[var(--g5-blocked)]" aria-hidden />
      );
    case "pending":
      return <Clock size={12} className="text-[var(--g5-warn)]" aria-hidden />;
    default:
      // skipping / neutral / unknown
      return (
        <span
          className="inline-block h-2 w-2 rounded-full bg-[var(--g5-text2)]"
          aria-hidden
        />
      );
  }
}

function CheckRow({ check }: { check: ExternalCheckItem }) {
  return (
    <li className="flex items-center gap-1.5 text-[11px] text-[var(--g5-text1)]">
      <BucketIcon bucket={check.bucket} />
      <span className="truncate">{check.name ?? check.kind}</span>
      {check.kind === "expected" ? (
        <span className="shrink-0 text-[10px] text-[var(--g5-text2)]">
          Expected, not reported yet
        </span>
      ) : null}
      {check.link ? (
        <a
          href={check.link}
          target="_blank"
          rel="noreferrer"
          className="text-[var(--g5-info)] hover:underline"
          aria-label={`Open ${check.name ?? "check"} details`}
        >
          <ExternalLink size={10} aria-hidden />
        </a>
      ) : null}
    </li>
  );
}

function CheckGroup({
  label,
  testId,
  checks,
  empty,
}: {
  label: string;
  testId: string;
  checks: ExternalCheckItem[];
  empty: string;
}) {
  return (
    <div data-testid={testId} className="mt-1">
      <p className="text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
        {label}
      </p>
      {checks.length === 0 ? (
        <p className="text-[10.5px] text-[var(--g5-text2)]">{empty}</p>
      ) : (
        <ul className="mt-0.5 flex flex-col gap-0.5">
          {checks.map((check, checkIdx) => (
            <CheckRow
              key={`${check.name ?? "check"}-${checkIdx}`}
              check={check}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function RepoChecks({ repo }: { repo: ExternalRepoChecks }) {
  // A local repository pull request has only its message: no external
  // checks apply. An unreadable repository has only its message too; listing
  // empty groups would read as "nothing required".
  if (repo.noExternalChecks) return null;
  if (
    repo.availability &&
    repo.availability !== "ready" &&
    repo.checks.length === 0
  ) {
    return null;
  }
  const branch = repo.baseRefName ?? "The base branch";
  return (
    <>
      <CheckGroup
        label="Required"
        testId="external-pr-checks-required"
        checks={repo.checks.filter((check) => check.required)}
        empty={`${branch} requires no checks; the checks below are advisory unless they fail.`}
      />
      <CheckGroup
        label="Not required"
        testId="external-pr-checks-not-required"
        checks={repo.checks.filter((check) => !check.required)}
        empty="No other checks reported."
      />
    </>
  );
}

export function ExternalPrChecksSection({ state, verdict, policyOn }: Props) {
  return (
    <div
      data-testid="external-pr-checks"
      data-verdict={verdict ?? "unknown"}
      className="mt-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2.5 py-2"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] font-semibold text-[var(--g5-text1)]">
          External checks
        </span>
        <div className="flex items-center gap-2">
          {verdict ? (
            <span
              data-testid="external-pr-checks-verdict"
              className={classNames(
                "text-[11px] font-medium",
                VERDICT_TONE[verdict],
              )}
            >
              {VERDICT_LABEL[verdict]}
            </span>
          ) : null}
          <span
            className={classNames(
              "rounded px-1.5 py-0.5 text-[10px]",
              policyOn
                ? "bg-[var(--g5-info)]/10 text-[var(--g5-info)]"
                : "text-[var(--g5-text2)]",
            )}
          >
            {policyOn ? "Required to merge" : "Advisory"}
          </span>
        </div>
      </div>

      {state.status === "loading" ? (
        <p className="mt-1 flex items-center gap-1 text-[11px] text-[var(--g5-text2)]">
          <Loader2 size={11} className="animate-spin" aria-hidden /> Reading
          external checks…
        </p>
      ) : state.status === "error" ? (
        <p className="mt-1 text-[11px] text-[var(--g5-text2)]">
          External checks are unavailable right now.
        </p>
      ) : (
        <ul className="mt-1.5 flex flex-col gap-1.5">
          {(state.data?.repositories ?? []).map((repo, repoIdx) => (
            <li key={repo.repository ?? repoIdx}>
              <div className="flex items-center justify-between gap-2">
                <span className="text-[11px] font-medium text-[var(--g5-text0)]">
                  {repo.repository ?? "repository"}
                  {repo.noExternalChecks
                    ? repo.g5Number
                      ? ` (${repo.g5Number})`
                      : ""
                    : repo.number != null
                      ? ` #${repo.number}`
                      : ""}
                </span>
                {repo.availability && repo.availability !== "ready" ? (
                  <span className="text-[10px] text-[var(--g5-warn)]">
                    {repo.availability}
                  </span>
                ) : null}
              </div>
              {repo.message ? (
                <p className="text-[10.5px] text-[var(--g5-text2)]">
                  {repo.message}
                </p>
              ) : null}
              <RepoChecks repo={repo} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
