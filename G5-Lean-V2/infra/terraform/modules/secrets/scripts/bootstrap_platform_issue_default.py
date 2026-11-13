#!/usr/bin/env python3
"""Idempotent, non-destructive bootstrap of one inert platform-issue default.

Terraform invokes this once per managed `g5-platform-issue-*` Secret Manager
container (see modules/secrets/main.tf and ontology
software_architecture/infra/terraform_secrets [D1]/[D11]/[I3]). It exists to
keep the isolated `g5-platform-issue-reporting-secrets` ExternalSecret syncable
from the very first deploy — ESO renders all-or-nothing and cannot sync a
versionless container — WITHOUT letting Terraform own a `secret_version`
resource that a later apply could destroy or push back over a live operator
value.

Contract (ontology [D1]):

* Probe version state with METADATA ONLY: `gcloud secrets versions list ...`.
  This never reads a secret payload — `gcloud secrets versions access` is never
  invoked, and no payload is ever printed or persisted by this helper.
* Add the inert disabled default **only when the container currently has no
  ENABLED version**. The inert default is itself added ENABLED, so a second run
  finds it and is a no-op (idempotent). An operator-supplied ENABLED version
  (real config) likewise makes this a no-op — operator values always win.
* **Fail closed**: if version state cannot be listed, abort with a non-zero exit
  and write NOTHING, so `terraform apply` fails visibly rather than silently
  skipping (which could later clobber a live value).

The inert value is passed via the BOOTSTRAP_INERT_VALUE environment variable —
never on the command line. Even though it is only ever the inert default
("false", "disabled" or a single space) and never a real credential, keeping values off
argv avoids exposure in the process table (`ps` / /proc/PID/cmdline) and keeps
this helper safe if it is ever reused for a genuine secret. The value is written
to the stdin of `gcloud secrets versions add` (never argv), and is never logged.

Exit codes: 0 = enabled version already present (skipped) OR inert default
added; non-zero = could not determine version state / write failed (fail closed).
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys

INERT_VALUE_ENV = "BOOTSTRAP_INERT_VALUE"

# Bound every gcloud call. The helper runs inside a Terraform `local-exec`
# provisioner, so an unbounded `gcloud` hang (stalled metadata server, wedged
# network path) would block `terraform apply` indefinitely with no diagnostic.
# A finite timeout converts that hang into the intended fail-closed error.
GCLOUD_TIMEOUT_SECONDS = 120


def _log(message: str) -> None:
    # Diagnostics only — never the secret value.
    print(message, file=sys.stderr)


def has_enabled_version(project: str, secret_id: str) -> bool:
    """Return True iff the container has at least one ENABLED version.

    Raises RuntimeError (fail closed) if the list command cannot be run to a
    successful completion — the caller must NOT write on this signal.
    """
    try:
        result = subprocess.run(
            [
                "gcloud",
                "secrets",
                "versions",
                "list",
                secret_id,
                "--project",
                project,
                "--filter",
                "state:ENABLED",
                "--format",
                "value(name)",
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=GCLOUD_TIMEOUT_SECONDS,
        )
    except OSError as exc:  # gcloud not found / not executable
        raise RuntimeError(f"could not invoke gcloud to list versions: {exc}") from exc
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(
            f"timed out listing versions for {secret_id} after "
            f"{GCLOUD_TIMEOUT_SECONDS}s; refusing to write (fail closed)"
        ) from exc

    if result.returncode != 0:
        raise RuntimeError(
            f"gcloud could not list versions for {secret_id} "
            f"(exit {result.returncode}); refusing to write (fail closed)"
        )

    return bool(result.stdout.strip())


def add_inert_default(project: str, secret_id: str, value: str) -> None:
    """Add the inert default as a new ENABLED version via stdin (no payload log)."""
    try:
        result = subprocess.run(
            [
                "gcloud",
                "secrets",
                "versions",
                "add",
                secret_id,
                "--project",
                project,
                "--data-file",
                "-",
            ],
            input=value,
            capture_output=True,
            text=True,
            check=False,
            timeout=GCLOUD_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(
            f"timed out adding inert default version for {secret_id} after "
            f"{GCLOUD_TIMEOUT_SECONDS}s; refusing to retry (fail closed)"
        ) from exc
    if result.returncode != 0:
        # Surface gcloud's own stderr, which does not echo the payload.
        raise RuntimeError(
            f"failed to add inert default version for {secret_id} "
            f"(exit {result.returncode}): {result.stderr.strip()}"
        )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Bootstrap one inert platform-issue default version if none is enabled.",
    )
    parser.add_argument("--project", required=True, help="GCP project id")
    parser.add_argument(
        "--secret-id", required=True, help="Secret Manager container id to bootstrap"
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    # The inert default arrives via the environment, never argv (see module
    # docstring). It is read as-is: a single space is a valid, intentional value.
    value = os.environ.get(INERT_VALUE_ENV)
    if value is None:
        _log(f"ERROR: {INERT_VALUE_ENV} is not set")
        return 2

    try:
        if has_enabled_version(args.project, args.secret_id):
            _log(
                f"{args.secret_id}: an enabled version already exists; "
                "leaving operator value untouched (no-op)"
            )
            return 0
        add_inert_default(args.project, args.secret_id, value)
        _log(f"{args.secret_id}: bootstrapped inert disabled default version")
        return 0
    except RuntimeError as exc:
        _log(f"ERROR: {exc}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
