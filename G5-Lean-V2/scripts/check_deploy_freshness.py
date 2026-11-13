#!/usr/bin/env python3
"""Compute the deploy-freshness verdict for cloudbuild-deploy.yaml.

`check-main-freshness` (cloudbuild_pipelines ontology [D5]/[D24]) decides
whether the current build may mutate its target environment. This script owns
the per-fetch verdict; the calling step owns credential resolution, the
eventual-consistency retry/stabilization loops, and deploy-control.env
plumbing.

Two predicates:

- Legacy newest-main ([D5]): the artifact commit must be the newest observed
  `g5-deploy-qa` build commit. Used in build/dev mode, and in promotion mode
  as the fallback when the target environment's deployed commit cannot be
  resolved. This predicate is what a continuous-merge + manual-approval stage
  can never satisfy — see [D24].
- Advance predicate ([D24], promotion mode with --deployed-commit): proceed
  iff the artifact commit's `g5-deploy-qa` origin build is strictly newer
  than the deployed commit's origin build. This keeps the anti-rollback /
  anti-migration-mismatch guarantee (an approved-but-older artifact still
  skips) while letting any artifact that advances the environment deploy,
  breaking the approval-latency freeze. A deployed commit whose origin build
  has aged out of the supplied build list counts as older (proceed): a
  deployment old enough to be unlisted is definitionally advanced by any
  listed artifact.

Output (stdout, single line, tab-separated): VERDICT<TAB>HEAD<TAB>REASON
  VERDICT: PROCEED | SKIP
  HEAD:    the freshness head commit recorded in deploy-control.env
  REASON:  newest | advances | deployed_origin_unlisted |
           stale_newer_main | already_deployed | rollback

Exit codes: 0 = verdict computed (PROCEED or SKIP are both successes);
1 = detection failure (unreadable input, or promotion-mode artifact origin
not visible yet) — the caller treats nonzero as a retryable miss.

Stdlib only; must stay compatible with the Python <= 3.10 shipped in
gcr.io/cloud-builders images.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import sys

DEPLOY_TRIGGER = "g5-deploy-qa"


def parse_time(value: str) -> dt.datetime:
    value = (value or "").strip()
    if value.endswith("Z"):
        value = value[:-1] + "+00:00"
    # Cloud Build createTime carries nanosecond precision; fromisoformat()
    # on Python <= 3.10 only accepts 3 or 6 fractional digits, so truncate
    # to microseconds.
    value = re.sub(r"\.(\d{6})\d+", r".\1", value)
    return dt.datetime.fromisoformat(value)


def collect_dev_deploys(builds):
    """Extract (create_time, build_id, commit_sha, raw_time, status) tuples
    for well-formed g5-deploy-qa builds, skipping malformed entries."""
    dev_deploys = []
    for build in builds:
        substitutions = build.get("substitutions") or {}
        if substitutions.get("TRIGGER_NAME") != DEPLOY_TRIGGER:
            continue

        build_id = build.get("id") or ""
        create_time_raw = build.get("createTime") or ""
        commit_sha = substitutions.get("COMMIT_SHA") or ""
        if not build_id or not create_time_raw or not commit_sha:
            continue

        try:
            create_time = parse_time(create_time_raw)
        except ValueError:
            continue

        dev_deploys.append(
            (create_time, build_id, commit_sha, create_time_raw, build.get("status") or "")
        )
    return dev_deploys


class OriginNotFound(Exception):
    """Promotion-mode artifact origin lookup failed; caller should retry."""


def artifact_origin_time(dev_deploys, artifact_commit, current_id, current_time, promotion_mode):
    """Origin time of the artifact commit's g5-deploy-qa build.

    Only builds at or before the current build's create time qualify (plus the
    current build itself), so a *re-deploy* of the same commit created later
    never masquerades as this promotion's origin.
    """
    origins = [
        entry
        for entry in dev_deploys
        if entry[2] == artifact_commit and (entry[1] == current_id or entry[0] <= current_time)
    ]
    if origins:
        return max(entry[0] for entry in origins)
    if promotion_mode != "build":
        raise OriginNotFound(
            f"Could not find a {DEPLOY_TRIGGER} origin build for promoted artifact {artifact_commit}"
        )
    return current_time


def deployed_origin_time(dev_deploys, deployed_commit):
    """Origin time of the deployed commit's g5-deploy-qa build, or None when
    it has aged out of the supplied build list."""
    origins = [entry for entry in dev_deploys if entry[2] == deployed_commit]
    if origins:
        return max(entry[0] for entry in origins)
    return None


def legacy_newest_verdict(dev_deploys, artifact_commit, current_id, origin_time):
    """[D5] newest-main predicate: skip when any newer different-commit
    g5-deploy-qa build exists (any status — a visible newer build is real)."""
    newer = [
        entry
        for entry in dev_deploys
        if entry[2] != artifact_commit
        and (entry[0] > origin_time or (entry[0] == origin_time and entry[1] > current_id))
    ]
    if newer:
        create_time, build_id, commit_sha, create_time_raw, status = max(
            newer, key=lambda entry: (entry[0], entry[1])
        )
        print(
            f"newer main deploy: {create_time_raw}\t{build_id}\t{commit_sha}\t{status}",
            file=sys.stderr,
        )
        return ("SKIP", commit_sha, "stale_newer_main")
    return ("PROCEED", artifact_commit, "newest")


def advance_verdict(dev_deploys, artifact_commit, deployed_commit, artifact_time):
    """[D24] advance predicate: proceed iff the artifact strictly advances the
    deployed commit. Ties (distinct commits, equal origin times) skip as a
    rollback: without a strict ordering we must not mutate the environment."""
    if deployed_commit == artifact_commit:
        return ("SKIP", deployed_commit, "already_deployed")

    deployed_time = deployed_origin_time(dev_deploys, deployed_commit)
    if deployed_time is None:
        print(
            f"deployed commit {deployed_commit} has no listed {DEPLOY_TRIGGER} origin; "
            "treating it as older than the artifact",
            file=sys.stderr,
        )
        return ("PROCEED", artifact_commit, "deployed_origin_unlisted")

    if artifact_time > deployed_time:
        return ("PROCEED", artifact_commit, "advances")
    return ("SKIP", deployed_commit, "rollback")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--builds-json", required=True)
    parser.add_argument("--current-build-id", required=True)
    parser.add_argument("--current-create-time", required=True)
    parser.add_argument("--artifact-commit", required=True)
    parser.add_argument("--promotion-mode", required=True)
    parser.add_argument("--deployed-commit", default="")
    args = parser.parse_args(argv)

    try:
        current_time = parse_time(args.current_create_time)
    except ValueError as exc:
        print(f"unparseable --current-create-time: {exc}", file=sys.stderr)
        return 1

    try:
        with open(args.builds_json, "r", encoding="utf-8") as fh:
            builds = json.load(fh)
    except (OSError, ValueError) as exc:
        print(f"unreadable builds json {args.builds_json}: {exc}", file=sys.stderr)
        return 1

    dev_deploys = collect_dev_deploys(builds)

    try:
        origin_time = artifact_origin_time(
            dev_deploys,
            args.artifact_commit,
            args.current_build_id,
            current_time,
            args.promotion_mode,
        )
    except OriginNotFound as exc:
        print(str(exc), file=sys.stderr)
        return 1

    if args.promotion_mode != "build" and args.deployed_commit:
        verdict = advance_verdict(
            dev_deploys, args.artifact_commit, args.deployed_commit, origin_time
        )
    else:
        verdict = legacy_newest_verdict(
            dev_deploys, args.artifact_commit, args.current_build_id, origin_time
        )

    print("\t".join(verdict))
    return 0


if __name__ == "__main__":
    sys.exit(main())
