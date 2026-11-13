"""Contract test — platform-issue reporting is bootstrapped safely and stays syncable.

Regression guard for the PR #1587 first-deploy failure and its follow-up
hardening: the optional `g5-platform-issue-reporting-secrets` ExternalSecret is
applied while its six `g5-platform-issue-*` Secret Manager containers must never
be versionless (ESO renders all-or-nothing and cannot sync a versionless
container -> "Secret does not exist" -> the fail-closed verify-deploy gate is
deterministically red).

The first fix seeded an inert *disabled default* version via a
Terraform-managed `google_secret_manager_secret_version` resource. That resource
was itself a hazard: Terraform believed it owned a version it could later
destroy or push back over a live operator value. The current fix (ontology
terraform_secrets [D1]/[D11]/[I3]) replaces it with:

  * a NON-DESTRUCTIVE state migration — a `removed { lifecycle { destroy =
    false } }` block that forgets the old instances WITHOUT deleting their
    remote Secret Manager versions; and
  * an idempotent `terraform_data` bootstrap that runs a metadata-only helper
    (modules/secrets/scripts/bootstrap_platform_issue_default.py). The helper
    lists version STATE only, adds the inert disabled default ONLY when the
    container has no ENABLED version (operator values always win; re-applies
    no-op), never calls `gcloud secrets versions access`, never logs/stores the
    payload, and FAILS CLOSED (writes nothing, non-zero exit) if it cannot list.

The gate itself is NOT weakened: the ExternalSecret still applies gated only on
`var.sync_api_secret`, remoteRefs exactly the six containers, and remains under
`verify_external_secrets.py`.

This is a STATIC + fake-gcloud test rather than a `.tftest.hcl` plan test: the
external-secrets module declares a `helm` provider and a `helm_release`, whose
plan reads live release state, so `terraform test` cannot plan these modules
offline, and no terraform/tofu binary is available in the profile. The static
assertions lock the invariants a plan test would otherwise assert; the
fake-gcloud subprocess tests lock the helper's runtime behavior deterministically.

Run with: python3 -m unittest test_platform_issue_reporting_default
"""
from __future__ import annotations

import os
import re
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_MAIN = Path(__file__).resolve().parents[1] / "main.tf"
SECRETS_DIR = Path(__file__).resolve().parents[3] / "modules" / "secrets"
SECRETS_MAIN = SECRETS_DIR / "main.tf"
BOOTSTRAP_HELPER = SECRETS_DIR / "scripts" / "bootstrap_platform_issue_default.py"

PLATFORM_ISSUE_CONTAINERS = (
    "g5-platform-issue-reporting-enabled",
    "g5-platform-issue-github-repo",
    "g5-platform-issue-github-app-id",
    "g5-platform-issue-github-app-installation-id",
    "g5-platform-issue-github-app-private-key",
    "g5-platform-issue-github-api-base-url",
)

# Approved inert defaults: the gate is literally disabled; every delivery field
# is the single-space non-empty sentinel (GCP rejects empty payloads; g5-api
# normalizes the blank to None). Keep in lockstep with modules/secrets/main.tf.
EXPECTED_DEFAULTS = {
    "g5-platform-issue-reporting-enabled": "false",
    "g5-platform-issue-github-repo": " ",
    "g5-platform-issue-github-app-id": " ",
    "g5-platform-issue-github-app-installation-id": " ",
    "g5-platform-issue-github-app-private-key": " ",
    "g5-platform-issue-github-api-base-url": " ",
}

# The API-sync switch is the ONLY gate on the ExternalSecret — no separate
# platform-issue enable flag. The bootstrapped disabled default (not a flag) is
# what makes the target syncable from the first deploy.
EXPECTED_FOR_EACH = "var.sync_api_secret ? toset([var.namespace]) : toset([])"


def _balanced_body(source: str, open_brace_index: int) -> str:
    """Return the brace-balanced body starting just after `open_brace_index`."""
    depth = 1
    index = open_brace_index
    while index < len(source) and depth:
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
        index += 1
    return source[open_brace_index : index - 1]


def _resource_body(source: str, resource_type: str, name: str) -> str:
    """Return the brace-balanced body of a named resource block."""
    header = re.search(
        r'resource\s+"%s"\s+"%s"\s*\{' % (re.escape(resource_type), re.escape(name)),
        source,
    )
    if not header:
        raise AssertionError(f'resource "{resource_type}" "{name}" not found')
    return _balanced_body(source, header.end())


def _assignment_block(source: str, name: str) -> str:
    """Return the brace-balanced body of an HCL `name = { ... }` assignment."""
    header = re.search(r"\b%s\s*=\s*\{" % re.escape(name), source)
    if not header:
        raise AssertionError(f'assignment "{name} = {{" not found')
    return _balanced_body(source, header.end())


def _removed_block(source: str) -> str:
    """Return the body of the `removed {` block that targets the old resource."""
    for header in re.finditer(r"removed\s*\{", source):
        body = _balanced_body(source, header.end())
        if (
            "google_secret_manager_secret_version" in body
            and "platform_issue_reporting_default" in body
        ):
            return body
    raise AssertionError("removed block for the old secret-version resource not found")


class StaticTerraformInvariantsTest(unittest.TestCase):
    def setUp(self):
        self.module = MODULE_MAIN.read_text(encoding="utf-8")
        self.secrets = SECRETS_MAIN.read_text(encoding="utf-8")

    def test_old_managed_secret_version_resource_is_gone(self):
        # The Terraform-owned version resource must no longer exist: it is the
        # exact hazard this change removes (Terraform could destroy/overwrite a
        # live operator value).
        self.assertNotRegex(
            self.secrets,
            r'resource\s+"google_secret_manager_secret_version"\s+"platform_issue_reporting_default"',
            "the managed google_secret_manager_secret_version.platform_issue_reporting_default "
            "resource must be removed in favor of the non-destructive bootstrap",
        )

    def test_removed_block_migrates_state_without_remote_deletion(self):
        # The migration must ABANDON old state without destroying remote
        # versions: a `removed` block with lifecycle { destroy = false }.
        body = _removed_block(self.secrets)
        self.assertRegex(
            body,
            r"from\s*=\s*google_secret_manager_secret_version\.platform_issue_reporting_default",
            "the removed block must target the old secret-version resource",
        )
        self.assertRegex(
            body,
            r"lifecycle\s*\{[^}]*destroy\s*=\s*false",
            "the removed block must set lifecycle { destroy = false } so remote "
            "Secret Manager versions are NOT destroyed by the migration",
        )

    def test_bootstrap_resource_is_managed_only_and_calls_metadata_helper(self):
        # The replacement bootstrap must iterate the managed-only seed set (so
        # adopted containers are never touched) and invoke the metadata-only
        # helper — never a raw payload read.
        body = _resource_body(
            self.secrets,
            "terraform_data",
            "platform_issue_reporting_default_bootstrap",
        )
        self.assertRegex(
            body,
            r"for_each\s*=\s*local\.platform_issue_reporting_default_seed",
            "the bootstrap must iterate local.platform_issue_reporting_default_seed "
            "(managed-only), not an ad-hoc or wider set",
        )
        self.assertIn(
            "bootstrap_platform_issue_default.py",
            body,
            "the bootstrap must invoke the metadata-only helper script",
        )
        # The inert value must be handed to the helper via the provisioner
        # ENVIRONMENT, never on the command line (argv is visible in the process
        # table / /proc/PID/cmdline).
        self.assertIn(
            "BOOTSTRAP_INERT_VALUE = each.value",
            body,
            "the bootstrap must pass the inert default via the provisioner "
            "environment (BOOTSTRAP_INERT_VALUE), not on the command line",
        )
        command_line = re.search(r'command\s*=\s*"([^"]*)"', body)
        self.assertIsNotNone(command_line, "bootstrap must run a command")
        self.assertNotIn(
            "--value",
            command_line.group(1),
            "the inert value must not be passed as a command-line argument",
        )
        self.assertNotIn(
            "each.value",
            command_line.group(1),
            "the inert value must not be interpolated onto the command line",
        )
        # A missing python3 interpreter must fail closed with a clear diagnostic
        # instead of an opaque `local-exec` exec error, without coupling the step
        # to a specific apply image.
        self.assertRegex(
            command_line.group(1),
            r"command -v python3.*\|\|.*exit 1",
            "the bootstrap command must guard python3 presence and fail closed "
            "(exit non-zero) when the apply image lacks an interpreter",
        )
        self.assertNotIn(
            "versions access",
            body,
            "the bootstrap must never read secret payloads (no `gcloud secrets versions access`)",
        )

    def test_helper_never_reads_payloads(self):
        # Defense in depth: the helper source itself must only reference the
        # metadata `list` verb and the `add` verb — never `access`.
        helper = BOOTSTRAP_HELPER.read_text(encoding="utf-8")
        verbs = set(_gcloud_verbs(helper).split())
        # Assert the EXACT verb set so the check fails closed: an empty set —
        # e.g. `_gcloud_verbs` no longer matching after a reformat — would slip
        # past `assertNotIn`/`issubset` and silently prove nothing.
        self.assertSetEqual(
            verbs,
            {"list", "add"},
            "the helper must use exactly the metadata `list` and `add` verbs, "
            f"never `access`; an empty set means the extractor stopped matching: {sorted(verbs)}",
        )

    def test_default_map_seeds_every_field_with_the_approved_value(self):
        # Verify every seeded VALUE (not just the container key): the gate is
        # literally "false" and each delivery field is the single-space non-empty
        # sentinel. A regression to "" would be rejected by Secret Manager.
        body = _assignment_block(
            self.secrets, "platform_issue_reporting_default_versions"
        )
        for container, value in EXPECTED_DEFAULTS.items():
            self.assertRegex(
                body,
                r'"%s"\s*=\s*"%s"' % (re.escape(container), re.escape(value)),
                f'{container} must be seeded exactly "{value}" — the gate disabled and '
                "the delivery fields a non-empty single-space sentinel (GCP rejects empty)",
            )
        self.assertNotRegex(
            body,
            r'"g5-platform-issue-[^"]*"\s*=\s*""',
            "no platform-issue default may be an empty string (Secret Manager rejects "
            "empty payloads)",
        )

    def test_default_seed_filters_to_managed_secrets(self):
        # A whole-document match would pass on unrelated uses of
        # local.managed_secrets; scope to the seed block itself.
        block = _assignment_block(
            self.secrets, "platform_issue_reporting_default_seed"
        )
        self.assertIn(
            "local.managed_secrets",
            block,
            "the seed set must filter on local.managed_secrets so adopted "
            "(pre-existing) containers are not re-seeded with the inert default",
        )

    def test_external_secret_unconditional_and_all_or_nothing(self):
        """Require API sync to project exactly the six bootstrapped issue secrets."""
        # (a) The ExternalSecret applies gated ONLY on the API-sync switch — no
        # separate platform-issue enable flag (assert the whole expression).
        body = _resource_body(
            self.module, "null_resource", "platform_issue_reporting_external_secret"
        )
        for_each = re.search(r"for_each\s*=\s*([^\n]+)", body)
        self.assertIsNotNone(for_each, "resource must keep a for_each")
        self.assertEqual(
            for_each.group(1).strip(),
            EXPECTED_FOR_EACH,
            "the ExternalSecret must apply from the first deploy gated only on "
            f"var.sync_api_secret ({EXPECTED_FOR_EACH!r}), with no extra enable flag "
            "and no verify-gate exemption",
        )
        # (b) ESO renders all-or-nothing, so the rendered ExternalSecret must
        # remoteRef EXACTLY the six containers (set equality, not subset).
        manifest = _assignment_block(
            self.module, "platform_issue_reporting_external_secret_yamls"
        )
        self.assertIn("data = local.platform_issue_reporting_secret_remote_refs", manifest)
        refs = re.search(r"platform_issue_reporting_secret_remote_refs\s*=\s*\[(.*?)\n  \]", self.module, re.S)[1]
        remote_ref_keys = set(
            re.findall(r'remoteRef\s*=\s*\{\s*key\s*=\s*"([^"]+)"', refs)
        )
        self.assertSetEqual(
            remote_ref_keys,
            set(PLATFORM_ISSUE_CONTAINERS),
            "the platform-issue ExternalSecret must remoteRef exactly the six "
            "bootstrapped containers (all-or-nothing ESO; an extra unmatched key "
            "would be versionless)",
        )


def _gcloud_verbs(helper_source: str) -> str:
    """Concatenate the gcloud arg-lists that appear in the helper source.

    Used only to prove the helper never references the `access` verb.
    """
    return "\n".join(re.findall(r'"versions",\s*"(\w+)"', helper_source))


class _FakeGcloud:
    """A deterministic fake `gcloud` on PATH that logs calls and canned-answers.

    State is driven entirely by environment variables passed to the helper
    subprocess, so each scenario is hermetic:
      FAKE_GCLOUD_LOG   - file that receives one line per invocation (argv)
      FAKE_ENABLED_IDS  - comma-separated secret ids that report an ENABLED version
      FAKE_LIST_FAIL    - "1" makes `versions list` exit non-zero (list failure)
    """

    SCRIPT = """#!/usr/bin/env python3
import os, sys
with open(os.environ["FAKE_GCLOUD_LOG"], "a", encoding="utf-8") as fh:
    fh.write(" ".join(sys.argv[1:]) + "\\n")
argv = sys.argv[1:]
if len(argv) >= 4 and argv[0] == "secrets" and argv[1] == "versions":
    verb, secret_id = argv[2], argv[3]
    if verb == "list":
        if os.environ.get("FAKE_LIST_FAIL") == "1":
            sys.stderr.write("FAKE: could not list versions\\n")
            sys.exit(1)
        enabled = [s for s in os.environ.get("FAKE_ENABLED_IDS", "").split(",") if s]
        if secret_id in enabled:
            sys.stdout.write("projects/p/secrets/%s/versions/1\\n" % secret_id)
        sys.exit(0)
    if verb == "add":
        sys.stdin.read()  # consume payload; never store or echo it
        sys.exit(0)
    if verb == "access":
        # The helper must NEVER reach here; still behave so a bug is observable.
        sys.exit(0)
sys.stderr.write("FAKE: unexpected gcloud call: %s\\n" % " ".join(argv))
sys.exit(1)
"""

    def __init__(self, root: Path):
        self.bindir = root / "bin"
        self.bindir.mkdir(parents=True, exist_ok=True)
        self.log = root / "gcloud-calls.log"
        self.log.write_text("", encoding="utf-8")
        gcloud = self.bindir / "gcloud"
        gcloud.write_text(self.SCRIPT, encoding="utf-8")
        gcloud.chmod(gcloud.stat().st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)

    def calls(self) -> list[str]:
        return [ln for ln in self.log.read_text(encoding="utf-8").splitlines() if ln]


class BootstrapHelperBehaviorTest(unittest.TestCase):
    def _run(self, secret_id, value, enabled_ids=(), list_fail=False):
        tmp = Path(self._tmp.name)
        fake = _FakeGcloud(tmp)
        env = dict(os.environ)
        env["PATH"] = f"{fake.bindir}{os.pathsep}{env.get('PATH', '')}"
        env["FAKE_GCLOUD_LOG"] = str(fake.log)
        env["FAKE_ENABLED_IDS"] = ",".join(enabled_ids)
        env["FAKE_LIST_FAIL"] = "1" if list_fail else "0"
        # The inert value is supplied via the environment, never on argv —
        # exactly as the terraform provisioner passes it.
        env["BOOTSTRAP_INERT_VALUE"] = value
        result = subprocess.run(
            [
                sys.executable,
                str(BOOTSTRAP_HELPER),
                "--project",
                "test-project",
                "--secret-id",
                secret_id,
            ],
            env=env,
            capture_output=True,
            text=True,
        )
        return result, fake.calls()

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)

    def test_versionless_container_adds_exactly_one_inert_version(self):
        result, calls = self._run("g5-platform-issue-github-repo", " ", enabled_ids=())
        self.assertEqual(result.returncode, 0, result.stderr)
        adds = [c for c in calls if c.startswith("secrets versions add")]
        self.assertEqual(len(adds), 1, f"expected exactly one add, got: {calls}")
        self.assertFalse(
            any(c.startswith("secrets versions access") for c in calls),
            "helper must never call `versions access`",
        )

    def test_existing_enabled_version_adds_nothing(self):
        sid = "g5-platform-issue-github-repo"
        result, calls = self._run(sid, " ", enabled_ids=(sid,))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(
            any(c.startswith("secrets versions add") for c in calls),
            f"operator/enabled version present — helper must not add: {calls}",
        )

    def test_partially_initialized_set_seeds_only_versionless(self):
        # Three of the six already carry an ENABLED version; only the other
        # three (versionless) must receive an inert default.
        enabled = PLATFORM_ISSUE_CONTAINERS[:3]
        versionless = PLATFORM_ISSUE_CONTAINERS[3:]
        added = []
        for sid in PLATFORM_ISSUE_CONTAINERS:
            result, calls = self._run(
                sid, EXPECTED_DEFAULTS[sid], enabled_ids=enabled
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            if any(c.startswith("secrets versions add") for c in calls):
                added.append(sid)
        self.assertEqual(
            sorted(added),
            sorted(versionless),
            "only the versionless containers may be seeded; enabled ones untouched",
        )

    def test_metadata_list_failure_fails_closed_without_writing(self):
        result, calls = self._run(
            "g5-platform-issue-github-repo", " ", list_fail=True
        )
        self.assertNotEqual(
            result.returncode, 0, "list failure must fail closed (non-zero exit)"
        )
        self.assertFalse(
            any(c.startswith("secrets versions add") for c in calls),
            f"fail-closed: no write may happen when versions cannot be listed: {calls}",
        )

    def test_helper_never_calls_access_or_emits_payload(self):
        sentinel = "SENTINEL_PAYLOAD_DO_NOT_LOG"
        result, calls = self._run(
            "g5-platform-issue-github-repo", sentinel, enabled_ids=()
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(
            any(c.startswith("secrets versions access") for c in calls),
            "helper must never call `versions access`",
        )
        self.assertNotIn(
            sentinel,
            result.stdout + result.stderr,
            "helper must never echo the secret payload to stdout/stderr",
        )
        # The payload also must not appear in the (metadata-only) call log.
        self.assertNotIn(
            sentinel,
            "\n".join(calls),
            "the payload must never appear in a logged gcloud argument",
        )


if __name__ == "__main__":
    unittest.main()
