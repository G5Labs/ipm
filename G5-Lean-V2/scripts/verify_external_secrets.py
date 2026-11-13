#!/usr/bin/env python3
"""Verify ExternalSecret sync health for the current kubectl context.

A pre-existing Kubernetes Secret masks a failing ExternalSecret: workloads
keep mounting the stale copy, so Helm rollout and smoke checks pass while
the environment silently stops receiving secret updates. Ready=True can
itself be stale from the last successful refresh inside refreshInterval,
and status.refreshTime is second-resolution, so wall-clock comparisons are
ambiguous. This gate therefore:

1. Probes the ExternalSecrets CRD with --ignore-not-found (fail closed on
   probe errors; skip only when the CRD is genuinely absent).
2. Discovers every namespace containing ExternalSecrets cluster-wide, so
   legacy app namespaces synced by the external-secrets Terraform module
   (legacy_app_namespaces, e.g. "default" in dev/staging) are covered.
3. Captures each ExternalSecret's pre-annotation refreshTime baseline,
   forces a reconcile via an annotation update, and requires every
   refreshTime to CHANGE from its baseline — refreshTime only advances on
   a successful refresh, so a moved baseline proves sync works right now.
4. Asserts Ready=True on every ExternalSecret, and on failure dumps the
   Ready conditions plus the ExternalSecret events, which name the exact
   failing Secret Manager key.

Exit code 0 on success or clean skip; 1 on any verification failure.
Stdlib only; must stay compatible with the Python <= 3.10 shipped in
gcr.io/cloud-builders images.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time

ANNOTATION = "g5.dev/verify-deploy-sync"
CRD_NAME = "externalsecrets.external-secrets.io"


class VerificationError(Exception):
    """Raised when the gate must fail the deploy."""


def run_kubectl(args):
    """Run kubectl and return the CompletedProcess (no exception on rc!=0)."""
    return subprocess.run(
        ["kubectl", *args], capture_output=True, text=True, check=False
    )


def kubectl_or_fail(args, context):
    proc = run_kubectl(args)
    if proc.returncode != 0:
        raise VerificationError(
            f"failed to {context}: kubectl exited {proc.returncode}: "
            f"{proc.stderr.strip()}"
        )
    return proc.stdout


def crd_installed():
    """Fail closed on probe errors; only a genuinely absent CRD skips."""
    out = kubectl_or_fail(
        ["get", "crd", CRD_NAME, "--ignore-not-found", "-o", "name"],
        "probe for the ExternalSecrets CRD",
    )
    return bool(out.strip())


def fetch_external_secrets():
    out = kubectl_or_fail(
        ["get", "externalsecret", "-A", "-o", "json"],
        "list ExternalSecrets across namespaces",
    )
    try:
        return json.loads(out).get("items", [])
    except json.JSONDecodeError as exc:
        raise VerificationError(
            f"failed to parse ExternalSecret listing as JSON: {exc}"
        ) from exc


def es_key(item):
    meta = item.get("metadata") or {}
    return (meta.get("namespace") or "", meta.get("name") or "")


def refresh_time(item):
    return ((item.get("status") or {}).get("refreshTime")) or ""


def ready_condition(item):
    for cond in (item.get("status") or {}).get("conditions") or []:
        if cond.get("type") == "Ready":
            return cond.get("status") or "", cond.get("message") or ""
    return "", "no Ready condition reported"


def dump_failure(namespaces):
    """Best-effort diagnostics; the events name the missing Secret Manager key."""
    proc = run_kubectl(
        [
            "get",
            "externalsecret",
            "-A",
            "-o",
            "custom-columns="
            "NAMESPACE:.metadata.namespace,"
            "NAME:.metadata.name,"
            'READY:.status.conditions[?(@.type=="Ready")].status,'
            'MESSAGE:.status.conditions[?(@.type=="Ready")].message',
        ]
    )
    if proc.returncode == 0:
        print(proc.stdout, end="")
    for namespace in sorted(namespaces):
        print(
            f"\nRecent ExternalSecret events in {namespace} "
            "(these name the exact missing Secret Manager key):"
        )
        proc = run_kubectl(
            [
                "get",
                "events",
                "-n",
                namespace,
                "--field-selector",
                "involvedObject.kind=ExternalSecret",
                "--sort-by=.lastTimestamp",
            ]
        )
        if proc.returncode == 0:
            lines = proc.stdout.strip().splitlines()
            print("\n".join(lines[-10:]))
        else:
            print(f"(could not read events: {proc.stderr.strip()})")


def verify(timeout_seconds, poll_seconds):
    """Raise VerificationError on failure; return quietly on success/skip."""
    if not crd_installed():
        print("ExternalSecrets CRD not installed; skipping secret-sync check")
        return

    items = fetch_external_secrets()
    if not items:
        print("No ExternalSecrets in this cluster; skipping secret-sync check")
        return

    # Baselines come from the same fail-closed fetch: a read error can never
    # be recorded as an empty baseline that the old value would satisfy.
    baselines = {es_key(item): refresh_time(item) for item in items}
    namespaces = sorted({namespace for namespace, _ in baselines})
    print(
        f"Verifying {len(baselines)} ExternalSecret(s) across "
        f"namespace(s): {', '.join(namespaces)}"
    )

    sync_start = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    by_namespace = {}
    for namespace, name in baselines:
        by_namespace.setdefault(namespace, []).append(name)
    for namespace, names in sorted(by_namespace.items()):
        kubectl_or_fail(
            [
                "annotate",
                "externalsecret",
                *sorted(names),
                "-n",
                namespace,
                f"{ANNOTATION}={sync_start}",
                "--overwrite",
            ],
            f"force-sync annotate ExternalSecrets in namespace {namespace}",
        )

    deadline = time.monotonic() + timeout_seconds
    while True:
        # Transient poll errors just leave everything pending until the
        # deadline; a persistent error therefore still fails the gate.
        try:
            items = fetch_external_secrets()
        except VerificationError as exc:
            print(f"WARNING: poll fetch failed, retrying: {exc}")
            items = []
        current = {es_key(item): item for item in items}
        pending = [
            key
            for key, baseline in sorted(baselines.items())
            if refresh_time(current.get(key, {})) in ("", baseline)
        ]
        if not pending:
            break
        if time.monotonic() >= deadline:
            names = ", ".join(f"{ns}/{name}" for ns, name in pending)
            print(
                f"ERROR: ExternalSecret(s) did not complete a fresh "
                f"successful sync within {timeout_seconds}s: {names}"
            )
            print(
                "Secret sync from Secret Manager is broken "
                "(missing secret version, IAM, or store config)."
            )
            dump_failure({ns for ns, _ in pending})
            raise VerificationError("secret sync verification timed out")
        time.sleep(poll_seconds)

    not_ready = []
    for key in sorted(baselines):
        status, message = ready_condition(current[key])
        if status != "True":
            not_ready.append((key, message))
    if not_ready:
        for (namespace, name), message in not_ready:
            print(f"ERROR: {namespace}/{name} refreshed but Ready!=True: {message}")
        dump_failure({namespace for (namespace, _), _ in not_ready})
        raise VerificationError("ExternalSecret(s) refreshed but not Ready")

    print(f"All {len(baselines)} ExternalSecret(s) re-synced and Ready=True")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timeout-seconds", type=int, default=180)
    parser.add_argument("--poll-seconds", type=int, default=5)
    args = parser.parse_args(argv)
    try:
        verify(args.timeout_seconds, args.poll_seconds)
    except VerificationError as exc:
        print(f"ERROR: {exc}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
