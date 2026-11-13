#!/usr/bin/env python3
"""Preserve the existing QA producer policy check before its self-apply.

Downstream releases use the G5 OCI pipeline in cloudbuild-terraform-release.yaml.
This helper neither packages nor applies Terraform and adds no downstream gate.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import release_artifacts as artifacts



def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, capture_output=True, **kwargs).stdout.strip()


def gcloud(*args):
    return run("gcloud", *args)


def build_info(project, location, build_id):
    if not re.fullmatch(r"[a-zA-Z0-9-]+", build_id):
        raise ValueError("invalid build id")
    return json.loads(gcloud("builds", "describe", build_id, "--project", project,
                             "--region", location, "--format=json"))


def source_sha(build):
    provenance = build.get("sourceProvenance", {})
    source = build.get("source", {})
    candidates = [provenance.get("resolvedRepoSource", {}).get("commitSha"),
                  provenance.get("resolvedGitSource", {}).get("revision"),
                  provenance.get("resolvedConnectedRepository", {}).get("revision"),
                  source.get("gitSource", {}).get("revision"),
                  source.get("connectedRepository", {}).get("revision")]
    return next((sha for sha in candidates if sha and re.fullmatch(r"[0-9a-f]{40}", sha)), "")


def validate_artifact_binding(trigger, build, project, artifact_project, bucket, *, require_bucket=True):
    artifacts.validate_layout(project, artifact_project, artifact_project, artifact_project,
                              bucket, require_bucket=require_bucket)
    # Older QA triggers predate these keys; absence retains QA ownership only.
    policy = {"_ARTIFACT_PROJECT": artifacts.QA_BUILD_PROJECT, "_RELEASE_BUCKET": "",
              **(trigger.get("substitutions") or {})}
    for substitutions in (policy, build.get("substitutions", {})):
        if (substitutions.get("_ARTIFACT_PROJECT") != artifact_project
                or substitutions.get("_RELEASE_BUCKET") != bucket):
            raise ValueError("Terraform trigger/build artifact policy differs from the selected destination")


def validate_producer(trigger, build, args):
    if (trigger.get("name") != "g5-terraform-apply-qa" or not trigger.get("id")
            or trigger["id"] != build.get("buildTriggerId") or trigger.get("disabled", False)):
        raise ValueError("Terraform bundles must originate from the enabled QA apply trigger")
    identity = f"g5-lean-terraform@{args.project}.iam.gserviceaccount.com"
    for metadata in (trigger, build):
        if metadata.get("serviceAccount", "").split("/")[-1] != identity:
            raise ValueError("Terraform bundle producer must use the trusted QA apply identity")
    validate_artifact_binding(trigger, build, args.project, args.artifact_project, args.bucket,
                              require_bucket=bool(args.next_trigger))


def policy(args):
    """Check the publisher before QA self-apply can change its trigger policy."""
    if args.project != artifacts.QA_BUILD_PROJECT or args.location != "us-central1":
        raise ValueError("unexpected release coordinator")
    current = build_info(args.project, args.location, args.build_id)
    trigger = json.loads(gcloud("builds", "triggers", "describe", "g5-terraform-apply-qa",
        "--project", args.project, "--region", args.location, "--format=json"))
    validate_producer(trigger, current, args)
    if source_sha(current) != args.sha:
        raise ValueError("QA bundle source provenance differs from the selected revision")



def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("policy",))
    for flag in ("project", "location", "build-id", "sha", "bucket", "next-trigger"):
        parser.add_argument(f"--{flag}", required=True)
    parser.add_argument("--artifact-project", default=artifacts.QA_BUILD_PROJECT)
    args = parser.parse_args(argv)
    policy(args)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Terraform producer policy failed: {error}", file=sys.stderr)
        sys.exit(1)
