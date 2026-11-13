#!/usr/bin/env python3
"""Weekly release-train approver for the gated Cloud Build release queues.

Executed by cloudbuild-release-train.yaml (cloudbuild_pipelines ontology
[D25]) on a Cloud Scheduler cadence. Since the 2026-07-27 gate move the
manual-approval queues are STAGING's: the script approves the pending builds
in both — the terraform apply queue first, then the app promote queue —
selecting per queue the newest artifact whose origin build was created at or
before one shared merge cutoff. The green staging runs auto-chain the
approval-free prod apply/promote, so approving staging IS the prod release
decision.

Decision rules ([D25]):

- Cutoff: the most recent start-of-day AFTER --cutoff-day in --cutoff-tz
  (default: Wednesday 00:00 America/Los_Angeles, i.e. merges through the end
  of Tuesday PT ship). A commit's "merged at" proxy is the createTime of its
  origin build (g5-deploy-qa / g5-terraform-apply-qa) in Cloud Build
  history — never a GitHub read. A build's origin is correlated at or before
  that build's own createTime, so a later re-run of an old commit's dev build
  cannot make an old pending prod build look fresh (the check_deploy_freshness
  artifact_origin_time rule).
- Advance predicate: a candidate is approved only when its origin time is
  strictly newer than the newest already-APPROVED build of the same queue
  (by that build's own <=-createTime origin; its createTime is the
  conservative fallback when the origin has aged out of the list window). A
  quiet week is a clean green no-op — no approval is spent.
- Fail-closed inputs: a full list window, an unclassifiable terraform queue
  (pending builds with unresolvable origins), or a queue plan that does not
  repeat identically across consecutive list fetches all halt the train red
  BEFORE any approval — acting on an incomplete or unstable picture is worse
  than skipping a week.
- Sequencing: the terraform staging candidate is approved first and polled
  to a terminal state. Its green run has already fired the approval-free
  chained prod apply (trigger-next is a step inside it), so the train then
  resolves that chained build — same COMMIT_SHA, created after the staging
  pending build, carrying no PENDING approval — and polls it to terminal
  too. A non-SUCCESS outcome of either apply, or an unresolvable chained
  build, halts the train red WITHOUT touching the app queue: the prod app
  deploy (auto-chained from the approved staging deploy) must not start
  before the prod infra apply has succeeded. The app plan is computed AFTER
  the terraform waits from a fresh listing (the pre-wait snapshot can be
  hours stale); the app candidate is approved without waiting for its build:
  the deploy pipelines' own gates own the deploy verdicts.
- Queue hygiene: after approving a candidate, pending builds it supersedes
  (strictly older origin, or a duplicate build of the same commit) are
  REJECTED so a stale promote cannot be approved later.

Approvals/rejections use the Cloud Build REST :approve endpoint because the
gcloud CLI cannot parse regional build resource names. The whole decision
attempt (token fetch included) retries on transient failures; a non-transient
4xx is cross-checked against the build's current approval state so a lost
response to a landed approval still counts as success.

Stdlib only; must stay compatible with the Python <= 3.10 shipped in
gcr.io/cloud-builders images.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from typing import Optional
from zoneinfo import ZoneInfo

from check_deploy_freshness import parse_time

WEEKDAYS = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
]
TERMINAL_STATUSES = {
    "SUCCESS",
    "FAILURE",
    "INTERNAL_ERROR",
    "TIMEOUT",
    "CANCELLED",
    "EXPIRED",
}
# The listing is server-side filtered to the train's five triggers (both
# staging queues, both origin triggers, and the chained prod apply); at
# current volume (~100 filtered builds/day) 2000 spans ~2-3 weeks of queue
# history. gcloud pages transparently. A full window fails closed only when
# it no longer reaches back to the decision horizon below — total filtered
# queue history exceeded the window in 2026-07, so "full" alone no longer
# implies "incomplete" ([D25]).
LIST_LIMIT = 2000
# [D25] decision horizon: pending builds created more than this long before
# the cutoff are FOSSILS — Cloud Build approval requests expire after ~30
# days, so a pending this old can never legitimately ship, and the 2026-07-29
# incident proved such pendings can be immortal (rejection 400s with
# Precondition-check-failed, cancellation silently no-ops). The planner skips
# them loudly instead of counting them as unclassifiable.
FOSSIL_PENDING_AGE = dt.timedelta(days=21)
# Stamped on every approval/rejection decision ([D25] attributable decisions):
# approval.result.approverAccount has been observed empty in the Pub/Sub
# payload notification surfaces render, so the comment is the durable identity
# marker (cloudbuild_pipelines [D26] / terraform_cloudbuild D18 fallback chain).
DECISION_COMMENTS = {
    "APPROVED": "Auto-approved by the g5-release-train weekly release train (g5-release-train service account)",
    "REJECTED": "Auto-rejected by the g5-release-train weekly release train (g5-release-train service account)",
}
LIST_ATTEMPTS = 3
LIST_RETRY_SLEEP_SECONDS = 15
API_BASE = "https://cloudbuild.googleapis.com/v1"
LIST_PAGE_SIZE = 1000
LIST_REQUEST_TIMEOUT_SECONDS = 120
# gcloud-minted tokens live ~1h; refresh well before expiry.
TOKEN_REFRESH_SECONDS = 45 * 60
# The mint subprocess is the one non-REST call left; unbounded, it would
# bypass every request timeout and retry budget.
TOKEN_MINT_TIMEOUT_SECONDS = 60
DECIDE_ATTEMPTS = 3
DECIDE_RETRY_SLEEP_SECONDS = 10
DECIDE_REQUEST_TIMEOUT_SECONDS = 60
# Consecutive unreadable polls before wait_terminal gives up (~20 min at the
# default 60s poll) instead of consuming the full multi-hour wait budget.
WAIT_UNKNOWN_BUDGET = 20
# A queue plan must repeat identically across this many consecutive fetches
# before it is executed — gcloud builds list is eventually consistent in both
# directions ([I4]), so a single read may hide a just-approved newer build.
PLAN_CONFIRM_ROUNDS = 3
PLAN_CONFIRM_SLEEP_SECONDS = 15
# The chained prod apply exists before the staging apply reports SUCCESS
# (trigger-next is a step inside it), so this bounded appearance loop only
# rides out list eventual consistency ([I4]); exhausting it fails the train
# red before the app queue is touched.
CHAINED_RESOLVE_ATTEMPTS = 10
CHAINED_RESOLVE_SLEEP_SECONDS = 30
RETRYABLE_HTTP_CODES = {408, 429}


def compute_cutoff(now: dt.datetime, tz_name: str, cutoff_day: str) -> dt.datetime:
    """Most recent boundary instant at or before `now`: 00:00 local on the day
    AFTER `cutoff_day`, so merges through the END of `cutoff_day` ship."""
    boundary_weekday = (WEEKDAYS.index(cutoff_day.lower()) + 1) % len(WEEKDAYS)
    tz = ZoneInfo(tz_name)
    local_now = now.astimezone(tz)
    today = local_now.date()
    for days_back in range(8):
        candidate_date = today - dt.timedelta(days=days_back)
        if candidate_date.weekday() != boundary_weekday:
            continue
        boundary = dt.datetime.combine(candidate_date, dt.time.min, tzinfo=tz)
        if boundary <= local_now:
            return boundary
    raise RuntimeError(f"no {cutoff_day} boundary found within a week of {now}")


def trigger_name(build: dict) -> str:
    return (build.get("substitutions") or {}).get("TRIGGER_NAME") or ""


def commit_sha(build: dict) -> str:
    return (build.get("substitutions") or {}).get("COMMIT_SHA") or ""


def approval_state(build: dict) -> str:
    return (build.get("approval") or {}).get("state") or ""


def build_create_time(build: dict):
    try:
        return parse_time(build.get("createTime") or "")
    except ValueError:
        return None


def collect_origin_times(builds: list, origin_trigger: str) -> dict:
    """commit sha -> list of origin-build create times for `origin_trigger`,
    skipping malformed entries (pattern of check_deploy_freshness)."""
    origins: dict = {}
    for build in builds:
        if trigger_name(build) != origin_trigger:
            continue
        sha = commit_sha(build)
        created = build_create_time(build)
        if not sha or created is None:
            continue
        origins.setdefault(sha, []).append(created)
    return origins


def origin_time_for(origins: dict, sha: str, at_or_before: dt.datetime):
    """Newest origin time for `sha` created at or before `at_or_before`.

    The bound is what stops a LATER re-run of an old commit's dev build from
    making an old pending prod build look fresh — only origins that could
    actually have produced this prod build qualify (mirrors
    check_deploy_freshness.artifact_origin_time)."""
    qualifying = [t for t in origins.get(sha, []) if t <= at_or_before]
    return max(qualifying) if qualifying else None


@dataclass
class PendingBuild:
    origin_time: dt.datetime
    build_id: str
    commit: str
    # The pending build's own createTime — the lower bound for resolving the
    # chained prod apply it fires (a chained build is always created after
    # the pending build that produced it). Optional so plan signatures and
    # existing constructors stay stable.
    create_time: Optional[dt.datetime] = None


@dataclass
class QueuePlan:
    queue: str
    reason: str
    candidate: Optional[PendingBuild] = None
    rejects: list = field(default_factory=list)
    unresolved_ids: list = field(default_factory=list)
    fossil_ids: list = field(default_factory=list)

    def signature(self) -> tuple:
        return (
            self.reason,
            self.candidate.build_id if self.candidate else "",
            tuple(sorted(entry.build_id for entry in self.rejects)),
            tuple(sorted(self.unresolved_ids)),
            tuple(sorted(self.fossil_ids)),
        )


def newest_approved_time(builds: list, prod_trigger: str, origins: dict):
    """Newest shipped point for the queue: max over approval.state==APPROVED
    builds of their commit's <=-createTime origin. When an approved build's
    origin has aged out of the list window, its own createTime is the
    fallback — a prod build is always created after its origin, so the
    fallback can only block, never admit, a borderline candidate."""
    best = None
    for build in builds:
        if trigger_name(build) != prod_trigger:
            continue
        if approval_state(build) != "APPROVED":
            continue
        created = build_create_time(build)
        if created is None:
            continue
        point = origin_time_for(origins, commit_sha(build), created) or created
        if best is None or point > best:
            best = point
    return best


def plan_queue(builds: list, prod_trigger: str, origin_trigger: str, cutoff: dt.datetime) -> QueuePlan:
    origins = collect_origin_times(builds, origin_trigger)
    pending = []
    unresolved_ids = []
    fossil_ids = []
    fossil_horizon = cutoff - FOSSIL_PENDING_AGE
    for build in builds:
        if trigger_name(build) != prod_trigger:
            continue
        if build.get("status") != "PENDING" or approval_state(build) != "PENDING":
            continue
        build_id = build.get("id") or ""
        created = build_create_time(build)
        if created is not None and created < fossil_horizon:
            # [D25] fossil: older than the decision horizon, so its approval
            # request has expired and it can never ship — skip it entirely
            # (candidacy, unresolved fail-closed, and reject hygiene alike).
            fossil_ids.append(build_id)
            continue
        origin = (
            origin_time_for(origins, commit_sha(build), created)
            if created is not None
            else None
        )
        if origin is None:
            unresolved_ids.append(build_id)
            continue
        pending.append(PendingBuild(origin, build_id, commit_sha(build), created))

    candidates = [entry for entry in pending if entry.origin_time <= cutoff]
    if not candidates:
        return QueuePlan(prod_trigger, "no_candidate", unresolved_ids=unresolved_ids,
                         fossil_ids=fossil_ids)

    candidate = max(candidates, key=lambda entry: (entry.origin_time, entry.build_id))
    shipped = newest_approved_time(builds, prod_trigger, origins)
    if shipped is not None and candidate.origin_time <= shipped:
        return QueuePlan(prod_trigger, "does_not_advance", unresolved_ids=unresolved_ids,
                         fossil_ids=fossil_ids)

    rejects = [
        entry
        for entry in pending
        if entry.build_id != candidate.build_id
        and (entry.origin_time < candidate.origin_time or entry.commit == candidate.commit)
    ]
    return QueuePlan(prod_trigger, "advance", candidate, rejects, unresolved_ids, fossil_ids)


class CloudBuildClient:
    """Thin effect wrapper over the Cloud Build REST API (reads + approval
    decisions). The gcloud CLI is used ONLY to mint the access token: for
    approvals it cannot parse regional build resource names, and for list
    reads its Python JSON pipeline took 10+ minutes per fetch at this window
    size on the default builder (2026-07-17 recovery run) versus seconds for
    a paginated partial-response REST read."""

    def __init__(self, project: str, region: str, *, trigger_names=None,
                 run=subprocess.run, opener=urllib.request.urlopen, sleep=time.sleep,
                 monotonic=time.monotonic) -> None:
        self.project = project
        self.region = region
        # Server-side scope: without it the window covers total project build
        # volume (proven full on day one) instead of the train's queues.
        self.trigger_names = list(trigger_names or [])
        self._run = run
        self._opener = opener
        self._sleep = sleep
        self._monotonic = monotonic
        self._token = ""
        self._token_born = float("-inf")

    def _access_token(self) -> str:
        if self._token and self._monotonic() - self._token_born < TOKEN_REFRESH_SECONDS:
            return self._token
        try:
            result = self._run(
                ["gcloud", "auth", "print-access-token"], capture_output=True, text=True,
                timeout=TOKEN_MINT_TIMEOUT_SECONDS,
            )
        except subprocess.TimeoutExpired as exc:
            raise RuntimeError(
                f"token minting timed out after {TOKEN_MINT_TIMEOUT_SECONDS}s"
            ) from exc
        if result.returncode != 0:
            raise RuntimeError(f"could not obtain access token: {result.stderr.strip()}")
        self._token = result.stdout.strip()
        self._token_born = self._monotonic()
        return self._token

    def _invalidate_token(self) -> None:
        self._token = ""
        self._token_born = float("-inf")

    def _get_json(self, path_and_query: str) -> dict:
        """GET with one in-place retry on 401: a cached token can die before
        its refresh horizon (revocation, short lifetime), and retrying with
        the same rejected token can never succeed."""
        for auth_attempt in (1, 2):
            request = urllib.request.Request(
                f"{API_BASE}/{path_and_query}",
                headers={"Authorization": f"Bearer {self._access_token()}"},
            )
            try:
                with self._opener(request, timeout=LIST_REQUEST_TIMEOUT_SECONDS) as response:
                    return json.loads(response.read().decode("utf-8"))
            except urllib.error.HTTPError as exc:
                if exc.code == 401 and auth_attempt == 1:
                    print("access token rejected (401); minting a fresh one", file=sys.stderr)
                    self._invalidate_token()
                    continue
                raise
        raise AssertionError("unreachable")

    def list_builds(self) -> list:
        """Paginated partial-response read of the train's build window.

        The whole paginated read retries as one unit: the list API is
        eventually consistent and can fail transiently ([I4])."""
        params = {
            "fields": "builds(id,createTime,status,substitutions,approval.state),nextPageToken",
        }
        if self.trigger_names:
            params["filter"] = " OR ".join(
                f'substitutions.TRIGGER_NAME="{name}"' for name in self.trigger_names
            )
        last_error: Optional[Exception] = None
        for attempt in range(1, LIST_ATTEMPTS + 1):
            try:
                builds: list = []
                page_token = ""
                while True:
                    query = dict(params)
                    query["pageSize"] = min(LIST_PAGE_SIZE, LIST_LIMIT - len(builds))
                    if page_token:
                        query["pageToken"] = page_token
                    payload = self._get_json(
                        f"projects/{self.project}/locations/{self.region}/builds?"
                        + urllib.parse.urlencode(query)
                    )
                    builds.extend(payload.get("builds") or [])
                    page_token = payload.get("nextPageToken") or ""
                    if not page_token or len(builds) >= LIST_LIMIT:
                        return builds[:LIST_LIMIT]
            except (urllib.error.URLError, ValueError, RuntimeError) as exc:
                last_error = exc
                if attempt < LIST_ATTEMPTS:
                    print(
                        f"builds list attempt {attempt}/{LIST_ATTEMPTS} failed: {exc}; retrying",
                        file=sys.stderr,
                    )
                    self._sleep(LIST_RETRY_SLEEP_SECONDS)
        raise RuntimeError(f"builds list failed after {LIST_ATTEMPTS} attempts: {last_error}")

    def _get_build_field(self, build_id: str, fields: str) -> dict:
        """One build's partial fields. Transient failures (network, 408/429,
        5xx) return {} so pollers retry; a permanent 4xx (403/404/...) raises —
        it can never heal, and returning {} would let wait_terminal read
        "still running" out of a dead endpoint for the full 5h wait budget."""
        try:
            return self._get_json(
                f"projects/{self.project}/locations/{self.region}/builds/{build_id}?"
                + urllib.parse.urlencode({"fields": fields})
            )
        except urllib.error.HTTPError as exc:
            if exc.code in RETRYABLE_HTTP_CODES or exc.code >= 500:
                print(f"get build {build_id} failed transiently: {exc}", file=sys.stderr)
                return {}
            raise RuntimeError(
                f"build {build_id} read rejected permanently ({exc.code})"
            ) from exc
        except (urllib.error.URLError, ValueError, RuntimeError) as exc:
            print(f"get build {build_id} failed transiently: {exc}", file=sys.stderr)
            return {}

    def describe_status(self, build_id: str) -> str:
        """Build status; empty string on a transient read failure (pollers
        retry). Permanent read rejections propagate."""
        return self._get_build_field(build_id, "status").get("status") or ""

    def describe_approval_state(self, build_id: str) -> str:
        """Current approval.state, or empty string when unreadable — bounded
        by the caller's DECIDE_ATTEMPTS, so even permanent read failures here
        only classify the state as unknown/retryable, never hang."""
        try:
            payload = self._get_build_field(build_id, "approval.state")
        except RuntimeError as exc:
            print(f"approval-state read failed: {exc}", file=sys.stderr)
            return ""
        return (payload.get("approval") or {}).get("state") or ""

    def decide(self, build_id: str, decision: str) -> None:
        """POST the approval decision via REST: `gcloud beta builds approve`
        cannot parse regional build resource names.

        Token acquisition sits inside the retry scope; 5xx/408/429/network
        errors retry; any other 4xx is cross-checked against the build's
        current approval state so a lost response to a landed decision counts
        as success instead of failing the train."""
        url = (
            "https://cloudbuild.googleapis.com/v1/"
            f"projects/{self.project}/locations/{self.region}/builds/{build_id}:approve"
        )
        body = json.dumps(
            {"approvalResult": {"decision": decision, "comment": DECISION_COMMENTS[decision]}}
        ).encode("utf-8")
        last_error: Optional[Exception] = None
        for attempt in range(1, DECIDE_ATTEMPTS + 1):
            try:
                request = urllib.request.Request(
                    url,
                    data=body,
                    headers={
                        "Authorization": f"Bearer {self._access_token()}",
                        "Content-Type": "application/json",
                    },
                    method="POST",
                )
                with self._opener(request, timeout=DECIDE_REQUEST_TIMEOUT_SECONDS) as response:
                    response.read()
                return
            except urllib.error.HTTPError as exc:
                if exc.code == 401:
                    # A rejected cached token can never succeed on retry;
                    # mint a fresh one for the next attempt.
                    self._invalidate_token()
                    last_error = exc
                elif exc.code >= 500 or exc.code in RETRYABLE_HTTP_CODES:
                    last_error = exc
                else:
                    current = self.describe_approval_state(build_id)
                    if current == decision:
                        print(
                            f"build {build_id} approval state is already {current}; "
                            f"treating the {decision} decision as applied"
                        )
                        return
                    if not current:
                        # The reconciliation read itself failed transiently:
                        # the state is UNKNOWN, not conflicting — retry the
                        # whole decision instead of failing mid-mutation.
                        last_error = exc
                    else:
                        detail = exc.read().decode("utf-8", "replace")
                        raise RuntimeError(
                            f"approval decision {decision} for {build_id} rejected "
                            f"({exc.code}, current state {current}): {detail}"
                        ) from exc
            except (urllib.error.URLError, RuntimeError) as exc:
                last_error = exc
            if attempt < DECIDE_ATTEMPTS:
                print(
                    f"decide {decision} {build_id} attempt {attempt}/{DECIDE_ATTEMPTS} "
                    f"failed: {last_error}; retrying",
                    file=sys.stderr,
                )
                self._sleep(DECIDE_RETRY_SLEEP_SECONDS)
        raise RuntimeError(
            f"approval decision {decision} for {build_id} failed after "
            f"{DECIDE_ATTEMPTS} attempts: {last_error}"
        )


def wait_terminal(client: CloudBuildClient, build_id: str, timeout_seconds: int,
                  poll_seconds: int, sleep=time.sleep, monotonic=time.monotonic) -> str:
    """Poll a build to a terminal status; empty string means the wait gave up
    (timeout, or WAIT_UNKNOWN_BUDGET consecutive unreadable polls — a poller
    that cannot read the build at all must not sit out the full wait budget).
    A permanent read rejection from describe_status propagates."""
    deadline = monotonic() + timeout_seconds
    consecutive_unknown = 0
    while True:
        status = client.describe_status(build_id)
        if status in TERMINAL_STATUSES:
            return status
        consecutive_unknown = consecutive_unknown + 1 if not status else 0
        if consecutive_unknown >= WAIT_UNKNOWN_BUDGET:
            # Raise rather than return "": the caller's timeout message
            # ("still running at wait timeout") would misreport a read outage.
            raise RuntimeError(
                f"build {build_id} unreadable for {consecutive_unknown} consecutive "
                f"polls; giving up the wait"
            )
        if monotonic() >= deadline:
            return ""
        sleep(poll_seconds)


def find_chained_build(builds: list, chained_trigger: str, commit: str,
                       after_time: Optional[dt.datetime]) -> str:
    """Newest build of `chained_trigger` for `commit` created after
    `after_time`, excluding approval-PENDING entries (pre-gate-move fossils
    still sitting in the old prod approval queue must never be mistaken for
    the freshly auto-chained apply). Returns "" when absent."""
    best_id = ""
    best_time = None
    for build in builds:
        if trigger_name(build) != chained_trigger or commit_sha(build) != commit:
            continue
        if approval_state(build) == "PENDING":
            continue
        created = build_create_time(build)
        if created is None:
            continue
        if after_time is not None and created <= after_time:
            continue
        if best_time is None or created > best_time:
            best_id, best_time = build.get("id") or "", created
    return best_id


def resolve_chained_build(client: CloudBuildClient, chained_trigger: str,
                          candidate: PendingBuild, sleep=time.sleep) -> str:
    """Resolve the approval-free chained prod apply the green staging apply
    fired. The build exists before the staging build reports SUCCESS
    (trigger-next is a step inside it), so the bounded retries only ride out
    list eventual consistency ([I4]). Raises when the build never appears —
    the train must halt red rather than approve app images whose prod infra
    state is unknown."""
    for attempt in range(1, CHAINED_RESOLVE_ATTEMPTS + 1):
        build_id = find_chained_build(
            client.list_builds(), chained_trigger, candidate.commit,
            candidate.create_time,
        )
        if build_id:
            return build_id
        if attempt < CHAINED_RESOLVE_ATTEMPTS:
            print(
                f"[{chained_trigger}] chained apply for commit {candidate.commit} not "
                f"visible yet (attempt {attempt}/{CHAINED_RESOLVE_ATTEMPTS}); the list "
                f"API is eventually consistent — retrying",
                flush=True,
            )
            sleep(CHAINED_RESOLVE_SLEEP_SECONDS)
    raise RuntimeError(
        f"[{chained_trigger}] no chained apply for commit {candidate.commit} appeared "
        f"after {CHAINED_RESOLVE_ATTEMPTS} attempts; the staging apply succeeded but "
        f"its prod promotion cannot be located"
    )


def log_url(project: str, region: str, build_id: str) -> str:
    return (
        f"https://console.cloud.google.com/cloud-build/builds;region={region}/"
        f"{build_id}?project={project}"
    )


def walk_apply(client: CloudBuildClient, queue: str, build_id: str, label: str,
               timeout_seconds: int, poll_seconds: int, sleep=time.sleep) -> bool:
    """Poll one terraform apply to terminal SUCCESS, reporting every
    non-success outcome (failure, wait timeout, unreadable build) loudly on
    stderr. False means the caller must halt the train with the app queue
    untouched — shared by the approved staging apply and the chained prod
    apply so their halt semantics cannot drift."""
    print(f"[{queue}] waiting up to {timeout_seconds}s for {label} {build_id}")
    try:
        status = wait_terminal(client, build_id, timeout_seconds, poll_seconds, sleep=sleep)
    except RuntimeError as exc:
        print(
            f"[{queue}] could not track the {label}: {exc}; "
            f"halting the train — the app queue was left untouched.",
            file=sys.stderr,
        )
        return False
    if status != "SUCCESS":
        print(
            f"[{queue}] {label} {build_id} finished "
            f"{status or 'still running at wait timeout'}; halting the train — "
            f"the app queue was left untouched. "
            f"{log_url(client.project, client.region, build_id)}",
            file=sys.stderr,
        )
        return False
    print(f"[{queue}] {label} {build_id} succeeded")
    return True


def describe_plan(plan: QueuePlan, cutoff: dt.datetime) -> None:
    print(f"[{plan.queue}] plan={plan.reason} (cutoff {cutoff.isoformat()})")
    if plan.candidate:
        print(
            f"[{plan.queue}] candidate build {plan.candidate.build_id} "
            f"commit {plan.candidate.commit} origin {plan.candidate.origin_time.isoformat()}"
        )
    for reject in plan.rejects:
        print(
            f"[{plan.queue}] superseded pending build {reject.build_id} "
            f"commit {reject.commit} origin {reject.origin_time.isoformat()} -> will reject"
        )
    for build_id in plan.unresolved_ids:
        print(
            f"[{plan.queue}] pending build {build_id} has no resolvable origin at or "
            f"before its creation within the list window"
        )
    for build_id in plan.fossil_ids:
        print(
            f"[{plan.queue}] ignoring fossil pending build {build_id}: created more "
            f"than {FOSSIL_PENDING_AGE.days} days before the cutoff, so its approval "
            f"request has expired and it can never ship ([D25] decision horizon)"
        )


def fetch_and_plan(client: CloudBuildClient, prod_trigger: str, origin_trigger: str,
                   cutoff: dt.datetime) -> QueuePlan:
    print(f"[{prod_trigger}] fetching builds window (limit {LIST_LIMIT})...", flush=True)
    builds = client.list_builds()
    if len(builds) >= LIST_LIMIT:
        # A full window is tolerable only while it still reaches back past the
        # [D25] decision horizon: everything older is fossil territory the
        # planner skips anyway. A full window that fails to cover the horizon
        # means decidable pendings could be hidden — fail closed.
        oldest = min(
            (t for t in (build_create_time(entry) for entry in builds) if t is not None),
            default=None,
        )
        horizon = cutoff - FOSSIL_PENDING_AGE
        if oldest is None or oldest > horizon:
            raise RuntimeError(
                f"builds list window is full ({len(builds)} >= {LIST_LIMIT}) and does "
                f"not reach the {FOSSIL_PENDING_AGE.days}-day decision horizon "
                f"(oldest {oldest.isoformat() if oldest else 'unparsable'} > horizon "
                f"{horizon.isoformat()}); refusing to plan against a truncated "
                f"history — raise the window or clean the queues"
            )
        print(
            f"[{prod_trigger}] builds list window is full ({len(builds)} >= "
            f"{LIST_LIMIT}) but covers the {FOSSIL_PENDING_AGE.days}-day decision "
            f"horizon (oldest {oldest.isoformat()}); older history is fossil "
            f"territory and planning proceeds"
        )
    return plan_queue(builds, prod_trigger, origin_trigger, cutoff)


def confirmed_plan(client: CloudBuildClient, prod_trigger: str, origin_trigger: str,
                   cutoff: dt.datetime, sleep=time.sleep) -> QueuePlan:
    """A plan is executed only after repeating identically across
    PLAN_CONFIRM_ROUNDS consecutive fetches ([I4] symmetric consistency: a
    just-approved newer build briefly hidden from one read must not let a
    stale candidate through). An action-free plan needs no confirmation."""
    plan = fetch_and_plan(client, prod_trigger, origin_trigger, cutoff)
    if plan.candidate is None:
        return plan
    for round_index in range(2, PLAN_CONFIRM_ROUNDS + 1):
        sleep(PLAN_CONFIRM_SLEEP_SECONDS)
        confirming = fetch_and_plan(client, prod_trigger, origin_trigger, cutoff)
        if confirming.signature() != plan.signature():
            raise RuntimeError(
                f"[{prod_trigger}] queue plan changed between consecutive fetches "
                f"(round {round_index}/{PLAN_CONFIRM_ROUNDS}); refusing to act on an "
                f"unstable queue picture"
            )
    return plan


def apply_plan(client: CloudBuildClient, plan: QueuePlan) -> None:
    """Approve the candidate (load-bearing — failures raise), then reject the
    superseded pendings BEST-EFFORT: Cloud Build approval requests expire
    after ~30 days, and such fossils return 400 Precondition-check-failed
    while still reading PENDING. An un-rejectable expired pending is also
    un-approvable, so skipping it poses no stale-ship hazard, while failing
    the train over hygiene would red every weekly run ([D25])."""
    assert plan.candidate is not None
    client.decide(plan.candidate.build_id, "APPROVED")
    print(f"[{plan.queue}] approved {plan.candidate.build_id} (commit {plan.candidate.commit})")
    failed_rejects = 0
    for reject in plan.rejects:
        try:
            client.decide(reject.build_id, "REJECTED")
        except RuntimeError as exc:
            failed_rejects += 1
            print(
                f"[{plan.queue}] could not reject superseded {reject.build_id} "
                f"(commit {reject.commit}): {exc}",
                file=sys.stderr,
            )
            continue
        print(f"[{plan.queue}] rejected superseded {reject.build_id} (commit {reject.commit})")
    if failed_rejects:
        print(
            f"[{plan.queue}] {failed_rejects}/{len(plan.rejects)} superseded rejection(s) "
            f"failed (likely expired approval requests); left pending",
            file=sys.stderr,
        )


def run_train(args, client: CloudBuildClient, now: dt.datetime, sleep=time.sleep) -> int:
    cutoff = compute_cutoff(now, args.cutoff_tz, args.cutoff_day)
    print(
        f"release train at {now.isoformat()}: shipping artifacts merged at or before "
        f"{cutoff.isoformat()} (end of {args.cutoff_day} in {args.cutoff_tz})"
    )

    if args.dry_run:
        builds = client.list_builds()
        print(f"inspecting {len(builds)} builds from the list window (limit {LIST_LIMIT})")
        if len(builds) >= LIST_LIMIT:
            print("warning: list window is full; a real run would fail closed", file=sys.stderr)
        describe_plan(plan_queue(builds, args.tf_trigger, args.tf_origin_trigger, cutoff), cutoff)
        describe_plan(plan_queue(builds, args.app_trigger, args.app_origin_trigger, cutoff), cutoff)
        print("dry run: no approval was created or rejected")
        return 0

    # Terraform queue first.
    try:
        tf_plan = confirmed_plan(client, args.tf_trigger, args.tf_origin_trigger, cutoff, sleep)
    except RuntimeError as exc:
        print(str(exc), file=sys.stderr)
        return 1
    describe_plan(tf_plan, cutoff)

    if tf_plan.unresolved_ids:
        print(
            f"[{tf_plan.queue}] {len(tf_plan.unresolved_ids)} pending build(s) could not "
            f"be classified; an unclassifiable terraform queue must not silently become "
            f"'no infra this week' while app code ships. Halting before any approval — "
            f"clean the queue or raise the list window.",
            file=sys.stderr,
        )
        return 1

    if tf_plan.candidate:
        apply_plan(client, tf_plan)
        if not walk_apply(client, tf_plan.queue, tf_plan.candidate.build_id,
                          "staging apply", args.tf_wait_timeout_seconds,
                          args.poll_seconds, sleep=sleep):
            return 1

        # The green staging apply auto-chained the approval-free prod apply;
        # walk it to success too. The prod app deploy (auto-chained from the
        # approved staging deploy) must not start before prod infra is green,
        # so any failure here halts the train before the app queue.
        if args.tf_chained_trigger:
            try:
                chained_id = resolve_chained_build(
                    client, args.tf_chained_trigger, tf_plan.candidate, sleep
                )
            except RuntimeError as exc:
                print(
                    f"{exc}; halting the train — the app queue was left untouched.",
                    file=sys.stderr,
                )
                return 1
            if not walk_apply(client, args.tf_chained_trigger, chained_id,
                              "chained prod apply", args.tf_wait_timeout_seconds,
                              args.poll_seconds, sleep=sleep):
                return 1

    # App queue from a FRESH listing: the pre-wait snapshot can be hours stale
    # and a human may have approved/rejected queue entries during the wait.
    try:
        app_plan = confirmed_plan(client, args.app_trigger, args.app_origin_trigger, cutoff, sleep)
    except RuntimeError as exc:
        print(str(exc), file=sys.stderr)
        return 1
    describe_plan(app_plan, cutoff)
    if app_plan.unresolved_ids:
        # Non-blocking for the app queue: its in-pipeline freshness gate
        # remains the final deploy authority.
        print(
            f"[{app_plan.queue}] warning: {len(app_plan.unresolved_ids)} pending "
            f"build(s) left untouched (unresolvable origin)",
            file=sys.stderr,
        )

    if app_plan.candidate:
        apply_plan(client, app_plan)
        print(
            f"[{app_plan.queue}] approved; the deploy pipeline's own gates own the "
            f"deploy verdict from here. "
            f"{log_url(client.project, client.region, app_plan.candidate.build_id)}"
        )

    if not tf_plan.candidate and not app_plan.candidate:
        print("nothing to ship this week; no approval was spent")
    return 0


def parse_bool(value: str) -> bool:
    return value.strip().lower() == "true"


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", required=True)
    parser.add_argument("--region", required=True)
    parser.add_argument("--cutoff-day", default="tuesday", choices=WEEKDAYS)
    parser.add_argument("--cutoff-tz", default="America/Los_Angeles")
    parser.add_argument("--tf-trigger", default="g5-terraform-apply-staging")
    parser.add_argument("--tf-origin-trigger", default="g5-terraform-apply-qa")
    # The approval-free prod apply the green staging apply auto-chains; the
    # train polls it to success before approving the app queue. Empty
    # disables the chained wait (single-gated-environment layouts).
    parser.add_argument("--tf-chained-trigger", default="g5-terraform-apply-prod")
    parser.add_argument("--app-trigger", default="g5-promote-staging")
    parser.add_argument("--app-origin-trigger", default="g5-deploy-qa")
    # 4h child pipeline cap + queue-delay headroom (the child's timeout only
    # starts at startTime).
    parser.add_argument("--tf-wait-timeout-seconds", type=int, default=18000)
    parser.add_argument("--poll-seconds", type=int, default=60)
    parser.add_argument("--dry-run", type=parse_bool, default=False)
    parser.add_argument("--now", default="", help="test hook: ISO timestamp overriding wall clock")
    args = parser.parse_args(argv)

    now = parse_time(args.now) if args.now else dt.datetime.now(dt.timezone.utc)
    client = CloudBuildClient(
        args.project,
        args.region,
        trigger_names=[
            name
            for name in (
                args.tf_trigger,
                args.tf_origin_trigger,
                args.tf_chained_trigger,
                args.app_trigger,
                args.app_origin_trigger,
            )
            if name
        ],
    )
    return run_train(args, client, now)


if __name__ == "__main__":
    sys.exit(main())
