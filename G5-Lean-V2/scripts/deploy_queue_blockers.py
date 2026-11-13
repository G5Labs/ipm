#!/usr/bin/env python3
"""List the older builds that still hold an environment's deploy turn.

`wait-deploy-turn` in infra/ci/cloudbuild-deploy.yaml (ontology
g5.quality.delivery.qa-deploy-order) serializes Helm by build creation order.
This script owns the blocker predicate for one poll; the step owns listing,
polling and the timeout.

An ongoing build blocks the current one when it targets the same environment
and project in build or deploy mode, was created earlier (ties break on build
id), and its deploy phase is unfinished: the named deploy-complete step
(`smoke-test`, the last step of the deploy sequence) has no terminal status.
A build with no step data (queued, not started), without that step, or with
the step pending/working still blocks, so the predicate fails closed toward
waiting. A candidate awaiting approval does not own a deploy turn. Work an older build does after its deploy phase (companion
publication, fleet cache warming) does not hold the queue.

Output (stdout): one tab-separated line per blocker, sorted by create time:
CREATE_TIME<TAB>BUILD_ID<TAB>COMMIT_SHA<TAB>TRIGGER_NAME<TAB>DEPLOY_STEP_STATUS
Empty output means the queue is clear. Exit 0 either way; 1 on unreadable input.

Stdlib only; must stay compatible with the Python <= 3.10 shipped in
gcr.io/cloud-builders images.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import sys

PROMOTION_MODES = {"build", "deploy"}
# Cloud Build step statuses after which the step runs no further.
TERMINAL_STEP_STATUSES = {
    "SUCCESS",
    "FAILURE",
    "INTERNAL_ERROR",
    "TIMEOUT",
    "CANCELLED",
    "EXPIRED",
}


def parse_time(value: str) -> dt.datetime:
    value = (value or "").strip()
    if value.endswith("Z"):
        value = value[:-1] + "+00:00"
    # Cloud Build createTime carries nanosecond precision; fromisoformat()
    # on Python <= 3.10 only accepts 3 or 6 fractional digits, so truncate
    # to microseconds.
    value = re.sub(r"\.(\d{6})\d+", r".\1", value)
    return dt.datetime.fromisoformat(value)


def deploy_step_status(build, step_id):
    """Status of the build's deploy-complete step, or "" when the step or
    its status is not reported yet."""
    for step in build.get("steps") or []:
        if (step.get("id") or "") == step_id:
            return step.get("status") or ""
    return ""


def deploy_phase_finished(build, step_id):
    return deploy_step_status(build, step_id) in TERMINAL_STEP_STATUSES


def blockers(builds, current_id, current_create, environment, target_project, deploy_complete_step):
    current_time = parse_time(current_create)
    found = []
    for build in builds:
        build_id = build.get("id") or ""
        if not build_id or build_id == current_id:
            continue

        # The release train may leave older candidates awaiting approval.
        if build.get("status") == "PENDING" or (build.get("approval") or {}).get("state") == "PENDING":
            continue

        substitutions = build.get("substitutions") or {}
        if substitutions.get("_PROMOTION_MODE") not in PROMOTION_MODES:
            continue
        if substitutions.get("_ENVIRONMENT") != environment:
            continue
        if substitutions.get("_TARGET_PROJECT") != target_project:
            continue

        create_time_raw = build.get("createTime") or ""
        if not create_time_raw:
            continue
        try:
            create_time = parse_time(create_time_raw)
        except ValueError:
            continue

        older = create_time < current_time or (create_time == current_time and build_id < current_id)
        if not older:
            continue
        if deploy_phase_finished(build, deploy_complete_step):
            continue

        found.append(
            (
                create_time_raw,
                build_id,
                substitutions.get("COMMIT_SHA", ""),
                substitutions.get("TRIGGER_NAME", ""),
                deploy_step_status(build, deploy_complete_step) or "not-started",
            )
        )
    return sorted(found)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--builds-json", required=True, help="gcloud builds list --ongoing JSON output")
    parser.add_argument("--current-build-id", required=True)
    parser.add_argument("--current-create-time", required=True)
    parser.add_argument("--environment", required=True)
    parser.add_argument("--target-project", required=True)
    parser.add_argument(
        "--deploy-complete-step",
        default="smoke-test",
        help="step id whose terminal status ends an older build's deploy phase",
    )
    args = parser.parse_args(argv)

    try:
        with open(args.builds_json, "r", encoding="utf-8") as fh:
            builds = json.load(fh)
        if not isinstance(builds, list):
            raise ValueError("expected a JSON list of builds")
        parse_time(args.current_create_time)  # reject a malformed create time before filtering
    except (OSError, ValueError) as exc:
        print(f"deploy_queue_blockers: unreadable input: {exc}", file=sys.stderr)
        return 1

    for row in blockers(
        builds,
        args.current_build_id,
        args.current_create_time,
        args.environment,
        args.target_project,
        args.deploy_complete_step,
    ):
        print("\t".join(row))
    return 0


if __name__ == "__main__":
    sys.exit(main())
