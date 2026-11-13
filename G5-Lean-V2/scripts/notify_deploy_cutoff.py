#!/usr/bin/env python3
"""Post a snapshot of the expected weekly release at the merge cutoff.

Ontology: g5.quality.delivery.cutoff-summary. Ported from G5's
generation/scripts/notify_deploy_cutoff.py.
Reuse the train planner, prefer its advancing app candidate, and compare against
the newest inferred source of successful production app builds. Explain incomplete predictions and
final selection at approval time. Notification errors are fail-soft.
Stdlib only, compatible with Cloud Build Python 3.10.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
from typing import Optional
from zoneinfo import ZoneInfo

from approve_release_train import (
    FOSSIL_PENDING_AGE,
    LIST_LIMIT,
    WEEKDAYS,
    CloudBuildClient,
    PendingBuild,
    QueuePlan,
    build_create_time,
    collect_origin_times,
    compute_cutoff,
    newest_approved_time,
    origin_time_for,
    plan_queue,
    trigger_name,
)
from check_deploy_freshness import parse_time
from release_notification_helpers import (
    MAX_ROW_SECTIONS,
    WEBHOOK_PLACEHOLDER_MARKER,
    GcloudClient,
    chunk_rows,
    escape_mrkdwn,
    fetch_commit_pulls,
    fetch_compare,
    format_row,
    post_webhook,
    pr_rows,
    secret_absent,
)

# Origin builds in these states will not produce a pending prod build, so they
# never affect the late-merge honesty note.
FAILED_STATUSES = {"FAILURE", "INTERNAL_ERROR", "TIMEOUT", "CANCELLED", "EXPIRED"}

LATE_MERGE_NOTE = (
    "later pre-cutoff merge(s) are still in the QA pipeline and not yet "
    "queued for promotion"
)


FINAL_SELECTION_NOTE = (
    "Snapshot at notification time; later deployments can change what remains. "
    "The release train makes the final selection at approval time."
)


def log(message: str) -> None:
    print(message, flush=True)


def fail_soft(message: str) -> int:
    """Loud, non-fatal failure: a broken notification must stay a green build."""
    print(f"NOTIFY-DEPLOY-CUTOFF-FAILED: {message}", file=sys.stderr, flush=True)
    return 0


def next_deploy_time(now: dt.datetime, deploy_day: str, deploy_time: str,
                     tz_name: str) -> dt.datetime:
    """Next `deploy_day` at `deploy_time` (HH:MM) in `tz_name`, strictly after
    `now` — the release-train firing this cutoff feeds. Rejects a nonexistent
    local wall time (spring-forward gap) and resolves an ambiguous fall-back
    time to the earlier fold."""
    try:
        hour, minute = (int(part) for part in deploy_time.split(":", 1))
        parsed = dt.time(hour, minute)
    except (ValueError, TypeError) as exc:
        raise RuntimeError(f"invalid --deploy-time {deploy_time!r} (want HH:MM)") from exc
    target_weekday = WEEKDAYS.index(deploy_day.lower())
    tz = ZoneInfo(tz_name)
    local_now = now.astimezone(tz)
    for days_ahead in range(8):
        candidate_date = local_now.date() + dt.timedelta(days=days_ahead)
        if candidate_date.weekday() != target_weekday:
            continue
        candidate = dt.datetime.combine(candidate_date, parsed, tzinfo=tz)
        # zoneinfo happily builds nonexistent wall times; a UTC round-trip
        # moves them, which is how we detect the DST gap.
        if candidate.astimezone(dt.timezone.utc).astimezone(tz).time() != candidate.time():
            raise RuntimeError(
                f"deploy slot {candidate_date} {deploy_time} does not exist in "
                f"{tz_name} (DST spring-forward gap)"
            )
        if candidate.replace(fold=1).utcoffset() != candidate.utcoffset():
            log(
                f"deploy slot {candidate_date} {deploy_time} is ambiguous in "
                f"{tz_name} (DST fall-back); using the earlier instant"
            )
        if candidate > local_now:
            return candidate
    raise RuntimeError(f"no {deploy_day} deploy slot found within a week of {now}")


def format_deploy_time(deploy_at: dt.datetime) -> str:
    return deploy_at.strftime("%A %b %d, %H:%M %Z")


def predicted_head(app_plan: QueuePlan, tf_plan: QueuePlan) -> Optional[PendingBuild]:
    """App candidate preferred — the PR table is defined against the app
    deploy; terraform candidate only on a terraform-only week. Never a
    cross-queue newest-timestamp pick: origin createTime order is not
    git-ancestry order, and a re-run of an older commit's dev build must not
    shrink or invert the table."""
    if app_plan.reason == "advance" and app_plan.candidate is not None:
        return app_plan.candidate
    if tf_plan.reason == "advance" and tf_plan.candidate is not None:
        return tf_plan.candidate
    return None


def production_baseline(builds: list, deployed_trigger: str,
                        origin_trigger: str) -> tuple[Optional[dict], list]:
    """Infer a successful prod baseline by bounded QA origin, not rerun time.

    This orders recorded sources; it does not prove current runtime state.
    An unresolved origin can compete only if its prod build was created after
    the best known origin. Older records cannot move that baseline forward.
    """
    origins = collect_origin_times(builds, origin_trigger)
    best = None
    best_origin = None
    best_commits = set()
    unresolved = []
    malformed = 0
    for entry in builds:
        if trigger_name(entry) != deployed_trigger or entry.get("status") != "SUCCESS":
            continue
        sha = (entry.get("substitutions") or {}).get("COMMIT_SHA") or ""
        created = build_create_time(entry)
        if not sha or created is None:
            malformed += 1
            continue
        origin = origin_time_for(origins, sha, created)
        if origin is None:
            unresolved.append(created)
        elif best_origin is None or origin > best_origin:
            best, best_origin, best_commits = entry, origin, {sha}
        elif origin == best_origin:
            best_commits.add(sha)

    notes = []
    if malformed:
        notes.append(f"{malformed} malformed successful prod build record(s) ignored")
    if unresolved and (best_origin is None or any(t > best_origin for t in unresolved)):
        notes.append("production baseline uncertain — a successful prod build has an "
                     "unresolved source origin that could be newer")
        return None, notes
    if len(best_commits) > 1:
        notes.append("production baseline uncertain — different commits share the "
                     "newest successful source origin time")
        return None, notes
    return best, notes


def queue_unrepresented_tail(builds: list, prod_trigger: str, origin_trigger: str,
                             cutoff: dt.datetime,
                             candidate: Optional[PendingBuild]) -> str:
    """Commit of this queue's newest non-failed origin build at or before the
    cutoff that is strictly newer than BOTH the queue's shipped point (newest
    approved build's origin, the rule) and its own candidate's origin —
    i.e. a pre-cutoff merge the queue does not represent yet. Empty string
    when the queue is fully represented, so an already-shipped commit can
    never be reported as "still in the QA pipeline"."""
    best_sha = ""
    best_time = None
    for build in builds:
        if trigger_name(build) != origin_trigger:
            continue
        if build.get("status") in FAILED_STATUSES:
            continue
        sha = (build.get("substitutions") or {}).get("COMMIT_SHA") or ""
        created = build_create_time(build)
        if not sha or created is None or created > cutoff:
            continue
        if best_time is None or created > best_time:
            best_sha, best_time = sha, created
    if best_time is None:
        return ""
    if candidate is not None and best_time <= candidate.origin_time:
        return ""
    shipped = newest_approved_time(
        builds, prod_trigger, collect_origin_times(builds, origin_trigger)
    )
    if shipped is not None and best_time <= shipped:
        return ""
    return best_sha


def build_message(*, deploy_time_label: str, head: Optional[PendingBuild],
                  base_sha: str, rows: Optional[list], notes: list,
                  still_forming: bool, halt_reason: str,
                  owner: str, repo: str) -> dict:
    """One Block Kit payload in exactly one of the states: prediction
    unavailable (halt_reason), a real table (head), selection-still-forming,
    or confirmed-empty. The top-level fallback text always states the same
    verdict as the blocks."""
    blocks = [
        {
            "type": "header",
            "text": {"type": "plain_text", "text": ":train2: Deployment cutoff now"},
        },
    ]

    if halt_reason:
        text = f"Deployment cutoff now — deployment prediction unavailable ({halt_reason})"
        blocks.append({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": f"_Deployment prediction unavailable: {escape_mrkdwn(halt_reason)}._",
            },
        })
    elif head is not None:
        range_label = (
            f" (`{base_sha[:9]}` → `{head.commit[:9]}`)" if base_sha else ""
        )
        blocks.append({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": f"PRs expected in the deployment on "
                        f"*{escape_mrkdwn(deploy_time_label)}*{range_label}:",
            },
        })
        if rows:
            text = (
                f"Deployment cutoff now — {len(rows)} PR(s) expected in the deployment on "
                f"{deploy_time_label}"
            )
            lines = [format_row(row, owner, repo) for row in rows]
            row_sections = chunk_rows(lines)
            if len(row_sections) > MAX_ROW_SECTIONS:
                dropped = sum(
                    section.count("\n") + 1 for section in row_sections[MAX_ROW_SECTIONS:]
                )
                row_sections = row_sections[:MAX_ROW_SECTIONS]
                notes = notes + [f"{dropped} more PR row(s) omitted (Slack block limit)"]
            for section in row_sections:
                blocks.append({"type": "section", "text": {"type": "mrkdwn", "text": section}})
        elif rows is not None:
            text = (
                f"Deployment cutoff now — no merged PRs found for the planned deployment on "
                f"{deploy_time_label}"
            )
            blocks.append({
                "type": "section",
                "text": {"type": "mrkdwn", "text": "_No merged PRs found in this range._"},
            })
        else:
            text = (
                f"Deployment cutoff now — merged-PR list unavailable "
                f"(deployment planned for {deploy_time_label})"
            )
            blocks.append({
                "type": "section",
                "text": {"type": "mrkdwn", "text": "_Merged-PR list unavailable — see notes._"},
            })
    elif still_forming:
        text = (
            f"Deployment cutoff now — selection still forming; pre-cutoff "
            f"merge(s) are still in the QA pipeline (deployment planned for {deploy_time_label})"
        )
        blocks.append({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": f"_No candidate is queued yet — pre-cutoff merge(s) are still "
                        f"in the QA pipeline (deployment planned for "
                        f"*{escape_mrkdwn(deploy_time_label)}*)._",
            },
        })
    else:
        text = (
            f"Deployment cutoff now — nothing new to ship; no PRs are expected in the "
            f"deployment on {deploy_time_label}"
        )
        blocks.append({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": f"_Nothing new to ship — no PRs are expected in the deployment on "
                        f"*{escape_mrkdwn(deploy_time_label)}*._",
            },
        })

    blocks.append({
        "type": "context",
        "elements": [{
            "type": "mrkdwn",
            "text": escape_mrkdwn(" · ".join(notes + [FINAL_SELECTION_NOTE])),
        }],
    })

    return {"text": text, "blocks": blocks}


def resolve_webhook(gcloud_client: GcloudClient, secret_name: str,
                    version: str = "1") -> Optional[str]:
    """Webhook URL, or None for the two clean-skip cases ([I3]): explicitly
    absent secret, or operator placeholder. Any other read failure raises so
    the caller's catch-all marks the outage loudly."""
    try:
        webhook_url = gcloud_client.access_secret(secret_name, version)
    except RuntimeError as exc:
        if not secret_absent(exc):
            raise
        log(f"webhook secret {secret_name} absent ({exc}); skipping notification")
        return None
    if not webhook_url or WEBHOOK_PLACEHOLDER_MARKER in webhook_url:
        log(f"webhook secret {secret_name} holds the operator placeholder; skipping")
        return None
    try:
        parsed = urllib.parse.urlsplit(webhook_url)
        valid = (parsed.scheme == "https" and bool(parsed.hostname)
                 and parsed.username is None and parsed.password is None
                 and (parsed.port is None or parsed.port > 0)
                 and not any(char.isspace() or ord(char) < 32 or ord(char) == 127
                             for char in webhook_url))
    except ValueError:
        valid = False
    if not valid:
        raise RuntimeError(
            f"webhook secret {secret_name} version {version} is not a valid HTTPS URL"
        ) from None
    return webhook_url


def resolve_rows(args, gcloud_client: GcloudClient, base_sha: str,
                 head: PendingBuild, notes: list, opener) -> Optional[list]:
    """PR rows for base…head via the machinery; None (with a note) when
    the lookup fails or no base exists."""
    if not base_sha:
        notes.append("production baseline unavailable — PR list unavailable")
        return None
    if base_sha == head.commit:
        notes.append("candidate matches the currently deployed commit")
        return []
    try:
        token = gcloud_client.github_read_token(
            args.github_connection, args.github_connection_repository
        )
        compare = fetch_compare(
            token, args.github_owner, args.github_repo,
            base_sha, head.commit, opener=opener,
        )

        def get_pulls(sha):
            return fetch_commit_pulls(
                token, args.github_owner, args.github_repo, sha, opener=opener
            )

        status = compare.get("status") or ""
        if status and status != "ahead":
            notes.append(
                f"compare status is '{status}' — non-linear history; the table "
                f"may be incomplete"
            )
        rows, without_pr, truncated, association_failures = pr_rows(
            compare, get_pulls, base_ref=args.github_base_branch
        )
        log(f"PRs included since {base_sha[:9]}: {len(rows)}")
        if without_pr:
            notes.append(f"{without_pr} commit(s) with no resolvable PR not listed")
        if truncated:
            notes.append(f"{truncated} older commit(s) beyond the compare window not inspected")
        if association_failures:
            notes.append(
                f"{association_failures} PR-association lookup(s) failed; "
                "fell back to commit subjects"
            )
        return rows
    except (RuntimeError, ValueError, urllib.error.URLError) as exc:
        notes.append(f"merged-PR lookup failed: {exc}")
        return None


def run_notify(args, cb_client: CloudBuildClient, gcloud_client: GcloudClient,
               now: dt.datetime, opener=urllib.request.urlopen) -> int:
    webhook_url = resolve_webhook(gcloud_client, args.webhook_secret, args.webhook_version)
    if webhook_url is None:
        return 0

    cutoff = compute_cutoff(now, args.cutoff_tz, args.cutoff_day)
    deploy_at = next_deploy_time(now, args.deploy_day, args.deploy_time, args.deploy_tz)
    deploy_time_label = format_deploy_time(deploy_at)
    log(
        f"cutoff notification at {now.isoformat()}: cutoff {cutoff.isoformat()}, "
        f"predicted deploy {deploy_at.isoformat()}"
    )

    builds = cb_client.list_builds()
    log(f"inspecting {len(builds)} builds from the list window (limit {LIST_LIMIT})")
    notes: list = []
    if len(builds) >= LIST_LIMIT:
        # a prediction surface with no approval authority notes the
        # inaccuracy loudly instead of adopting 's fail-closed refusal —
        # but a full window that still reaches back past the decision
        # horizon is complete for prediction purposes (older is fossil
        # territory the planner skips) and adds no note.
        oldest = min(
            (t for t in (build_create_time(entry) for entry in builds) if t is not None),
            default=None,
        )
        if oldest is None or oldest > cutoff - FOSSIL_PENDING_AGE:
            notes.append("build-list window full — candidate prediction may be incomplete")

    app_plan = plan_queue(builds, args.app_trigger, args.app_origin_trigger, cutoff)
    tf_plan = plan_queue(builds, args.tf_trigger, args.tf_origin_trigger, cutoff)
    for plan in (app_plan, tf_plan):
        log(
            f"[{plan.queue}] plan={plan.reason} unresolved={len(plan.unresolved_ids)} "
            f"fossils={len(plan.fossil_ids)}"
        )
    fossil_count = len(app_plan.fossil_ids) + len(tf_plan.fossil_ids)
    if fossil_count:
        # fossils are skipped exactly as the train skips them and
        # surface as a count note, never as an unavailable prediction.
        notes.append(
            f"{fossil_count} fossil pending build(s) older than the "
            f"{FOSSIL_PENDING_AGE.days}-day decision horizon ignored "
            f"(Cloud Build approval requests expire after ~30 days)"
        )

    head = None
    halt_reason = ""
    rows: Optional[list] = None
    if tf_plan.unresolved_ids:
        # an unclassifiable terraform queue halts the real train
        # BEFORE any approval — promising a table here would be a lie.
        halt_reason = (
            f"{len(tf_plan.unresolved_ids)} pending terraform build(s) cannot be "
            f"classified, so the release train will halt red at this cutoff "
            f"without approving anything"
        )
    else:
        head = predicted_head(app_plan, tf_plan)
        if app_plan.unresolved_ids:
            notes.append(
                f"{len(app_plan.unresolved_ids)} pending app build(s) have "
                f"unresolvable origins and were not considered"
            )

    # A later green rerun of an old commit must not roll the inferred baseline
    # backward. Reuse the train's bounded origin correlation, without new reads.
    previous, baseline_notes = production_baseline(
        builds, args.deployed_trigger, args.app_origin_trigger,
    )
    notes.extend(baseline_notes)
    base_sha = ((previous or {}).get("substitutions") or {}).get("COMMIT_SHA") or ""
    if previous is not None:
        log(f"production baseline inferred from source order: "
            f"build {previous.get('id')} commit {base_sha}")

    if head is not None:
        log(f"predicted head: build {head.build_id} commit {head.commit}")
        rows = resolve_rows(args, gcloud_client, base_sha, head, notes, opener)

    still_forming = False
    if not halt_reason:
        tails = [
            queue_unrepresented_tail(
                builds, args.app_trigger, args.app_origin_trigger, cutoff,
                app_plan.candidate,
            ),
            queue_unrepresented_tail(
                builds, args.tf_trigger, args.tf_origin_trigger, cutoff,
                tf_plan.candidate,
            ),
        ]
        if any(tails):
            if head is not None:
                notes.append(LATE_MERGE_NOTE)
            else:
                still_forming = True

    payload = build_message(
        deploy_time_label=deploy_time_label,
        head=head,
        base_sha=base_sha,
        rows=rows,
        notes=notes,
        still_forming=still_forming,
        halt_reason=halt_reason,
        owner=args.github_owner,
        repo=args.github_repo,
    )

    if args.dry_run:
        log("dry run: rendered payload follows (not posted)")
        log(json.dumps(payload, indent=2))
        return 0

    post_webhook(webhook_url, payload, opener=opener)
    log("deploy-cutoff notification posted")
    return 0


def parse_bool(value: str) -> bool:
    return value.strip().lower() == "true"


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", required=True)
    parser.add_argument("--region", required=True)
    parser.add_argument("--cutoff-day", default="tuesday", choices=WEEKDAYS)
    parser.add_argument("--cutoff-tz", default="America/Los_Angeles")
    # Threaded from release-train.tf schedule locals by the Terraform trigger;
    # these defaults exist only for manual runs.
    parser.add_argument("--deploy-day", default="thursday", choices=WEEKDAYS)
    parser.add_argument("--deploy-time", default="23:00")
    parser.add_argument("--deploy-tz", default="America/New_York")
    parser.add_argument("--tf-trigger", default="g5-terraform-apply-staging")
    parser.add_argument("--tf-origin-trigger", default="g5-terraform-apply-qa")
    parser.add_argument("--app-trigger", default="g5-promote-staging")
    parser.add_argument("--app-origin-trigger", default="g5-deploy-qa")
    # PROD's deployed state — the PR-table diff base; separate from
    # the predicted staging app queue since the 2026-07-27 gate move.
    parser.add_argument("--deployed-trigger", default="g5-promote-prod")
    parser.add_argument("--github-owner", default="G5Labs")
    parser.add_argument("--github-repo", default="G5-Lean")
    parser.add_argument("--github-base-branch", default="main",
                        help="only PRs merged into this branch appear in the table")
    parser.add_argument("--github-connection", default="G5labs-G5-QA")
    parser.add_argument("--github-connection-repository", default="G5Labs-G5-Lean",
                        help="Cloud Build 2nd-gen repository resource name (usually owner-repo)")
    parser.add_argument("--webhook-secret", default="g5-cloudbuild-slack-webhook")
    parser.add_argument("--webhook-version", default="1",
                        help="pinned version shared with the Cloud Run notifier")
    parser.add_argument("--dry-run", type=parse_bool, default=False)
    parser.add_argument("--now", default="", help="test hook: ISO timestamp overriding wall clock")
    args = parser.parse_args(argv)

    now = parse_time(args.now) if args.now else dt.datetime.now(dt.timezone.utc)
    cb_client = CloudBuildClient(
        args.project,
        args.region,
        trigger_names=[
            args.tf_trigger,
            args.tf_origin_trigger,
            args.app_trigger,
            args.app_origin_trigger,
            args.deployed_trigger,
        ],
    )
    gcloud_client = GcloudClient(args.project, args.region)
    try:
        return run_notify(args, cb_client, gcloud_client, now)
    except Exception as exc:  # noqa: BLE001 — best-effort posture
        return fail_soft(str(exc))


if __name__ == "__main__":
    sys.exit(main())
