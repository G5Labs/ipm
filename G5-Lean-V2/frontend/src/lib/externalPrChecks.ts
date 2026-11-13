/**
 * Typed projection + verdict over the external PR check snapshot returned by
 * `GET /workspaces/:id/pull-requests/info?checks=true` (the same read that
 * backs `g5d pr checks`). Surfaced in Prepare to Ship
 * (governance g5.governance.pr.checks.surface) and mirrored by the server-side
 * merge policy (g5.governance.merge.external-policy).
 *
 * The verdict here is advisory display + pre-emptive button gating; the server
 * re-reads and is authoritative at merge.
 */

/** Normalized check-state bucket from the checks reader. */
export type ExternalCheckBucket =
  | "pass"
  | "skipping"
  | "fail"
  | "cancel"
  | "pending"
  | (string & {});

export interface ExternalCheckItem {
  /** "check_run" | "status" | "expected" (required but not reported yet). */
  kind?: string;
  name?: string | null;
  state?: string | null;
  bucket?: ExternalCheckBucket | null;
  link?: string | null;
  description?: string | null;
  /** The PR's base branch requires this check (g5.governance.pr.checks.required). */
  required: boolean;
}

export interface ExternalRepoChecks {
  repository?: string | null;
  /** GitHub's PR number; null for a local repository pull request. */
  number?: number | null;
  /** `github`, or `g5` for a local repository pull request. */
  tracker?: string | null;
  /**
   * The G5 PR's name (`PR3fa9c1`) that names a local repository pull request
   * (g5.git.pr-local-number); GitHub's rows have none.
   */
  g5Number?: string | null;
  /** "ready" | "pending" | "unavailable". */
  availability?: string | null;
  /** Present when the repo's checks could not be read. */
  message?: string | null;
  /** The base branch whose required checks were read. */
  baseRefName?: string | null;
  /** GitHub rate-limited this repo's read. */
  rateLimited: boolean;
  /** GitHub's reported rate-limit reset (RFC 3339), when it gave one. */
  retryAt?: string | null;
  /**
   * A local repository pull request (g5.git.pr-repository): not on GitHub, so
   * no external checks apply to it.
   */
  noExternalChecks?: boolean;
  checks: ExternalCheckItem[];
}

export interface ExternalChecksSnapshot {
  source?: string;
  repositories: ExternalRepoChecks[];
}

/** Aggregate merge-policy verdict; mirrors the backend `external_checks_verdict`. */
export type ExternalChecksVerdict = "passing" | "pending" | "failing";

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

/** Parse the untyped `info?checks=true` body into the typed snapshot, or null. */
export function parseChecksSnapshot(
  value: unknown,
): ExternalChecksSnapshot | null {
  const root = asRecord(value);
  if (!root) return null;
  const repos = Array.isArray(root.repositories) ? root.repositories : null;
  if (!repos) return null;
  const repositories: ExternalRepoChecks[] = repos.map((repo) => {
    const r = asRecord(repo) ?? {};
    const rawChecks = Array.isArray(r.checks) ? r.checks : [];
    const checks: ExternalCheckItem[] = rawChecks.map((check) => {
      const c = asRecord(check) ?? {};
      return {
        kind: typeof c.kind === "string" ? c.kind : undefined,
        name: typeof c.name === "string" ? c.name : null,
        state: typeof c.state === "string" ? c.state : null,
        bucket: typeof c.bucket === "string" ? c.bucket : null,
        link: typeof c.link === "string" ? c.link : null,
        description: typeof c.description === "string" ? c.description : null,
        required: c.required === true,
      };
    });
    return {
      repository: typeof r.repository === "string" ? r.repository : null,
      number: typeof r.number === "number" ? r.number : null,
      tracker: typeof r.tracker === "string" ? r.tracker : null,
      g5Number: typeof r.g5Number === "string" ? r.g5Number : null,
      availability: typeof r.availability === "string" ? r.availability : null,
      message: typeof r.message === "string" ? r.message : null,
      baseRefName: typeof r.baseRefName === "string" ? r.baseRefName : null,
      rateLimited: r.rateLimited === true,
      retryAt: typeof r.retryAt === "string" ? r.retryAt : null,
      noExternalChecks: r.noExternalChecks === true,
      checks,
    };
  });
  return {
    source: typeof root.source === "string" ? root.source : undefined,
    repositories,
  };
}

/**
 * Whether any reported check is still pending, required or not. The display
 * keeps refreshing until it is not, so a check that later fails turns the
 * verdict to failing.
 */
export function hasPendingChecks(
  snapshot: ExternalChecksSnapshot | null,
): boolean {
  return !!snapshot?.repositories.some((repo) =>
    repo.checks.some((check) => check.bucket === "pending"),
  );
}

/** Whether any provider repo was present (i.e. the PR has an external PR). */
export function hasExternalRepos(
  snapshot: ExternalChecksSnapshot | null,
): boolean {
  return (
    !!snapshot && snapshot.repositories.some((repo) => !repo.noExternalChecks)
  );
}

/**
 * Worst-wins aggregate verdict across repos, mirroring the backend:
 *   - any check/status bucket `fail`/`cancel`, required or not ⇒ failing
 *   - else any required check `pending` (including expected but not yet
 *     reported), or a repo not `ready` ⇒ pending
 *   - else passing: checks the base branch does not require are advisory
 *     until they fail, so a repo requiring none passes unless one fails
 * An empty repo set is passing (no external PR to gate); a null snapshot is
 * pending (not yet known — never a pass).
 */
export function aggregateVerdict(
  snapshot: ExternalChecksSnapshot | null,
): ExternalChecksVerdict {
  if (!snapshot) return "pending";
  if (snapshot.repositories.length === 0) return "passing";
  let pending = false;
  for (const repo of snapshot.repositories) {
    for (const check of repo.checks) {
      if (check.bucket === "fail" || check.bucket === "cancel") {
        return "failing";
      }
      if (check.bucket === "pending" && check.required) pending = true;
    }
    if (repo.availability !== "ready") pending = true;
  }
  return pending ? "pending" : "passing";
}

/** Prepare to Ship's check poll while checks are pending. */
export const CHECKS_PENDING_POLL_MS = 30_000;
/** Poll after a read that was unavailable for a reason other than a rate limit. */
export const CHECKS_UNAVAILABLE_POLL_MS = 60_000;
/** Bounds on waiting for GitHub's reported rate-limit reset. */
export const CHECKS_RATE_LIMIT_MIN_MS = 60_000;
export const CHECKS_RATE_LIMIT_MAX_MS = 300_000;

/**
 * When Prepare to Ship next reads the checks (g5.git.check-waits), or
 * false once nothing is pending. A rate-limited read waits until the latest
 * reported reset, clamped to one to five minutes (one when none is given); a
 * read unavailable for another reason waits a minute; pending checks poll
 * every 30 seconds.
 */
export function checksRefetchInterval(
  snapshot: ExternalChecksSnapshot | null | undefined,
  now: number = Date.now(),
): number | false {
  if (!snapshot) return false;
  const limited = snapshot.repositories.filter((repo) => repo.rateLimited);
  if (limited.length > 0) {
    const resets = limited
      .map((repo) => (repo.retryAt ? Date.parse(repo.retryAt) : NaN))
      .filter((at) => Number.isFinite(at));
    if (resets.length === 0) return CHECKS_RATE_LIMIT_MIN_MS;
    const wait = Math.max(...resets) - now;
    return Math.min(
      CHECKS_RATE_LIMIT_MAX_MS,
      Math.max(CHECKS_RATE_LIMIT_MIN_MS, wait),
    );
  }
  if (
    snapshot.repositories.some((repo) => repo.availability === "unavailable")
  ) {
    return CHECKS_UNAVAILABLE_POLL_MS;
  }
  return aggregateVerdict(snapshot) === "pending" || hasPendingChecks(snapshot)
    ? CHECKS_PENDING_POLL_MS
    : false;
}
