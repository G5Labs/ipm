#!/usr/bin/env python3
"""Observe automatic Artifact Analysis results without changing release policy."""
from __future__ import annotations

from datetime import datetime, timezone
import json
import os
import re
import subprocess

from app_release import IMAGES

QUERY_TIMEOUT = 20
SEVERITIES = ("CRITICAL", "HIGH", "MEDIUM", "LOW", "MINIMAL", "SEVERITY_UNSPECIFIED")


def describe(ref):
    result = subprocess.run(
        ["gcloud", "artifacts", "docker", "images", "describe", ref,
         "--show-package-vulnerability", "--format=json", "--quiet"],
        text=True, capture_output=True, check=True, timeout=QUERY_TIMEOUT,
    )
    return json.loads(result.stdout)


def summarize(data, ref):
    """The SDK groups full occurrences by effective severity, plus discovery."""
    if not isinstance(data, dict) or data.get("image_summary", {}).get("fully_qualified_digest") != ref:
        raise ValueError("missing or mismatched image identity in scan response")
    discoveries = data.get("discovery_summary", {}).get("discovery", [])
    summary = data.get("package_vulnerability_summary")
    if not isinstance(summary, dict) or "vulnerabilities" not in summary:
        raise ValueError("missing vulnerability metadata")
    groups = summary["vulnerabilities"]
    if not isinstance(discoveries, list) or not isinstance(groups, dict):
        raise ValueError("malformed scan metadata")
    status = []
    complete = bool(discoveries)
    for occurrence in discoveries:
        discovery = occurrence["discovery"]
        analysis = discovery.get("analysisStatus", "UNKNOWN")
        continuous = discovery.get("continuousAnalysis", "UNKNOWN")
        scanned = discovery.get("lastScanTime", "UNKNOWN")
        errors = discovery.get("analysisError") or discovery.get("analysisStatusError")
        archived = discovery.get("archiveTime")
        status.append(f"analysis={analysis}, continuous={continuous}, lastScanTime={scanned}"
                      + (f", archiveTime={archived}" if archived else "")
                      + (f", errors={json.dumps(errors, sort_keys=True)}" if errors else ""))
        # Preserve RFC3339 nanoseconds verbatim: Python versions in builders
        # differ in timestamp parsing, and time formatting must not hide CVEs.
        # An old initial scan may still have continuously refreshed findings;
        # do not invent an expiry from lastScanTime alone.
        complete &= (analysis in {"FINISHED_SUCCESS", "COMPLETE"} and continuous == "ACTIVE"
                     and isinstance(scanned, str) and bool(scanned) and scanned != "UNKNOWN"
                     and not errors and not archived)
        if continuous == "INACTIVE" or archived:
            status.append("stale/inactive metadata; findings may no longer be updated")
    if not status:
        status.append("no discovery metadata; scan completion is unknown")

    counts = {severity: 0 for severity in SEVERITIES}
    details = []
    for severity, occurrences in groups.items():
        if not isinstance(occurrences, list):
            raise ValueError("malformed vulnerability group")
        counts[severity] = len(occurrences)
        for occurrence in occurrences:
            vulnerability = occurrence["vulnerability"]
            if not isinstance(vulnerability, dict) or vulnerability.get("effectiveSeverity") != severity:
                raise ValueError("malformed vulnerability severity")
            if severity not in {"HIGH", "CRITICAL"}:
                continue
            cve = occurrence.get("noteName", "UNKNOWN-CVE").rsplit("/", 1)[-1]
            packages = vulnerability.get("packageIssue", [])
            if not isinstance(packages, list):
                raise ValueError("malformed affected packages")
            if not packages:
                details.append(f"{severity} {cve}: package/fix metadata unavailable")
            for package in packages:
                details.append(
                    f"{severity} {cve}: package={package.get('affectedPackage', 'UNKNOWN')}"
                    f" affectedVersion={json.dumps(package.get('affectedVersion', {}), sort_keys=True)}"
                    f" fixAvailable={package.get('fixAvailable', 'UNKNOWN')}"
                    f" fixedPackage={package.get('fixedPackage', 'UNKNOWN')}"
                    f" fixedVersion={json.dumps(package.get('fixedVersion', {}), sort_keys=True)}"
                )
    return complete, status, counts, details


def report(env=None):
    env = os.environ if env is None else env
    print("CVE observations only; findings and unavailable reports do not gate deployment.", flush=True)
    print(f"Observed at {datetime.now(timezone.utc).isoformat()}", flush=True)
    for variable, image in IMAGES.items():
        ref = env.get(variable, "")
        if not ref:
            continue
        print(f"\n{image}: {ref}", flush=True)
        match = re.fullmatch(r"([a-z0-9-]+)-docker\.pkg\.dev/([a-z0-9-]+)/([a-z0-9._-]+)/"
                             + re.escape(image) + r"@(sha256:[0-9a-f]{64})", ref)
        if not match:
            print("INCOMPLETE: expected the validated immutable Artifact Registry image reference.", flush=True)
            continue
        region, project, repository, digest = match.groups()
        print(f"Report: https://console.cloud.google.com/artifacts/docker/{project}/{region}/"
              f"{repository}/{image}/{digest};tab=vulnerabilities?project={project}", flush=True)
        try:
            complete, statuses, counts, details = summarize(describe(ref), ref)
            print("Scan metadata: " + ("COMPLETE" if complete else "INCOMPLETE"), flush=True)
            for status in statuses:
                print(f"  {status}", flush=True)
            print("Returned vulnerability occurrences: " + ", ".join(f"{key}={value}" for key, value in counts.items()), flush=True)
            if not any(counts.values()):
                print("No vulnerability occurrences returned; this is not proof of absence.", flush=True)
            for detail in details:
                print(f"  {detail}", flush=True)
        except (ValueError, TypeError, KeyError, AttributeError, OSError, subprocess.SubprocessError) as error:
            # Error text is diagnostic only. Never turn a failed read into zero findings.
            print(f"INCOMPLETE: scan report unavailable ({type(error).__name__}: {error}).", flush=True)
            if isinstance(error, subprocess.CalledProcessError) and error.stderr:
                print(f"  gcloud: {error.stderr.strip()[:1000]}", flush=True)


if __name__ == "__main__":
    report()
