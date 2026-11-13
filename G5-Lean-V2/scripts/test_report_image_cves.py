"""Automatic scan observations and the actual Cloud Build shell, without GCP."""
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import report_image_cves as cves

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = "us-central1-docker.pkg.dev/g5-qa-1ccc/g5"
REFS = {key: f"{REGISTRY}/{image}@sha256:" + "a" * 64 for key, image in cves.IMAGES.items()}


def response(ref=REFS["API_IMAGE_REF"], status="FINISHED_SUCCESS", continuous="ACTIVE", severity=None):
    data = {
        "image_summary": {"fully_qualified_digest": ref},
        "discovery_summary": {"discovery": [{"discovery": {
            "analysisStatus": status, "continuousAnalysis": continuous,
            "lastScanTime": "2026-10-20T12:00:00Z",
        }}]},
        "package_vulnerability_summary": {"vulnerabilities": {}},
    }
    if severity:
        data["package_vulnerability_summary"] = {"vulnerabilities": {severity: [{
            "noteName": "projects/goog-vulnz/notes/CVE-2026-1234",
            "vulnerability": {"effectiveSeverity": severity, "packageIssue": [{
                "affectedPackage": "openssl", "affectedVersion": {"name": "1.2.3"},
                "fixAvailable": True, "fixedPackage": "openssl", "fixedVersion": {"name": "1.2.4"},
            }]},
        }]}}
    return data


class ReportParsingTests(unittest.TestCase):
    def render(self, data, env=None):
        output = io.StringIO()
        with patch.object(cves, "describe", return_value=data), redirect_stdout(output):
            cves.report(env or {"API_IMAGE_REF": REFS["API_IMAGE_REF"]})
        return output.getvalue()

    def test_google_sdk_severity_groups_keep_severe_package_and_fix_details(self):
        for severity in ("HIGH", "CRITICAL"):
            with self.subTest(severity=severity):
                output = self.render(response(severity=severity))
                for expected in (f"{severity}=1", f"{severity} CVE-2026-1234", "package=openssl",
                                 "fixAvailable=True", "1.2.4", REFS["API_IMAGE_REF"],
                                 "https://console.cloud.google.com/artifacts/docker/g5-qa-1ccc/us-central1/g5/api/sha256:" + "a" * 64 + ";tab=vulnerabilities?project=g5-qa-1ccc"):
                    self.assertIn(expected, output)
                self.assertIn("Scan metadata: COMPLETE", output)

    def test_explicit_empty_vulnerability_map_reports_zero_returned_findings(self):
        output = self.render(response())
        self.assertIn("Scan metadata: COMPLETE", output)
        self.assertIn("CRITICAL=0", output)
        self.assertIn("No vulnerability occurrences returned; this is not proof of absence", output)

    def test_missing_vulnerability_metadata_is_incomplete_without_zero_counts(self):
        missing_summary = response()
        missing_summary.pop("package_vulnerability_summary")
        missing_groups = response()
        missing_groups["package_vulnerability_summary"] = {}
        for data in (missing_summary, missing_groups):
            with self.subTest(data=data):
                output = self.render(data)
                self.assertIn("INCOMPLETE: scan report unavailable", output)
                self.assertNotIn("Scan metadata: COMPLETE", output)
                self.assertNotIn("Returned vulnerability occurrences:", output)
                self.assertNotIn("CRITICAL=0", output)

    def test_unknown_pending_unsupported_failed_and_inactive_remain_incomplete(self):
        for state in ("PENDING", "SCANNING", "FINISHED_FAILED", "FINISHED_UNSUPPORTED", "UNKNOWN"):
            with self.subTest(state=state):
                output = self.render(response(status=state, severity="HIGH"))
                self.assertIn("Scan metadata: INCOMPLETE", output)
                self.assertIn(f"analysis={state}", output)
                self.assertIn("HIGH=1", output)
        for continuous in ("INACTIVE", "UNKNOWN"):
            with self.subTest(continuous=continuous):
                self.assertIn("Scan metadata: INCOMPLETE", self.render(response(continuous=continuous)))
        self.assertIn("stale/inactive", self.render(response(continuous="INACTIVE")))

    def test_missing_discovery_archive_and_partial_analysis_errors_are_incomplete(self):
        missing = response()
        missing.pop("discovery_summary")
        self.assertIn("no discovery metadata", self.render(missing))
        for field, value in (("archiveTime", "2026-10-20T12:00:00Z"),
                             ("analysisError", [{"message": "package scan failed"}]),
                             ("analysisStatusError", {"message": "scan failed"})):
            with self.subTest(field=field):
                data = response(status="COMPLETE")
                data["discovery_summary"]["discovery"][0]["discovery"][field] = value
                self.assertIn("Scan metadata: INCOMPLETE", self.render(data))
        data = response()
        data["discovery_summary"]["discovery"][0]["discovery"].pop("lastScanTime")
        self.assertIn("Scan metadata: INCOMPLETE", self.render(data))

    def test_nanosecond_timestamps_do_not_hide_findings(self):
        data = response(severity="CRITICAL")
        data["discovery_summary"]["discovery"][0]["discovery"]["lastScanTime"] = "2026-10-20T12:00:00.123456789Z"
        output = self.render(data)
        self.assertIn("Scan metadata: COMPLETE", output)
        self.assertIn("CRITICAL=1", output)
        self.assertIn(".123456789Z", output)
        for invalid in (None, "", {}):
            data["discovery_summary"]["discovery"][0]["discovery"]["lastScanTime"] = invalid
            output = self.render(data)
            self.assertIn("Scan metadata: INCOMPLETE", output)
            self.assertIn("CRITICAL=1", output)

    def test_old_initial_scan_does_not_override_active_continuous_analysis(self):
        data = response()
        data["discovery_summary"]["discovery"][0]["discovery"]["lastScanTime"] = "2020-01-01T00:00:00Z"
        self.assertIn("Scan metadata: COMPLETE", self.render(data))

    def test_malformed_and_wrong_digest_responses_never_become_zero_findings(self):
        cases = [[], {}, response(ref=REFS["EDGE_IMAGE_REF"])]
        for field, value in (("discovery_summary", {"discovery": "bad"}),
                             ("package_vulnerability_summary", None),
                             ("package_vulnerability_summary", {"vulnerabilities": None}),
                             ("package_vulnerability_summary", {"vulnerabilities": []}),
                             ("package_vulnerability_summary", {"vulnerabilities": {"HIGH": [{}]}}),
                             ("image_summary", None)):
            data = response()
            data[field] = value
            cases.append(data)
        for data in cases:
            with self.subTest(data=data):
                output = self.render(data)
                self.assertIn("INCOMPLETE: scan report unavailable", output)
                self.assertNotIn("CRITICAL=0", output)

    def test_one_bounded_read_per_present_release_image_and_errors_continue(self):
        output = io.StringIO()
        errors = [subprocess.TimeoutExpired("gcloud", 20), OSError("missing gcloud"),
                  subprocess.CalledProcessError(1, "gcloud"), json.JSONDecodeError("bad", "", 0)]
        with patch.object(cves, "describe", side_effect=errors + [response(ref) for ref in list(REFS.values())[4:]]) as query, redirect_stdout(output):
            cves.report(REFS)
        self.assertEqual([call.args[0] for call in query.call_args_list], list(REFS.values()))
        self.assertEqual(output.getvalue().count("INCOMPLETE: scan report unavailable"), len(errors))
        with patch.object(cves.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, json.dumps(response()), "")) as run:
            cves.describe(REFS["API_IMAGE_REF"])
        self.assertEqual(run.call_args.kwargs["timeout"], 20)
        self.assertEqual(run.call_args.args[0], ["gcloud", "artifacts", "docker", "images", "describe",
                                              REFS["API_IMAGE_REF"], "--show-package-vulnerability", "--format=json", "--quiet"])

    def test_empty_optional_images_are_skipped_and_mutable_refs_are_never_queried(self):
        with patch.object(cves, "describe") as query, redirect_stdout(io.StringIO()) as output:
            cves.report({"API_IMAGE_REF": f"{REGISTRY}/api:latest", "G5D_IMAGE_REF": ""})
        query.assert_not_called()
        self.assertIn("expected the validated immutable", output.getvalue())


class ReportCloudBuildShellTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.pipeline = (ROOT / "infra/ci/cloudbuild-deploy.yaml").read_text()
        cls.step = cls.pipeline.split("  - id: report-image-cves\n", 1)[1].split("\n  - id:", 1)[0]

    def invoke(self, environment="prod", skip=False, mode="high", images=None, crash=False):
        with tempfile.TemporaryDirectory() as directory:
            workspace = Path(directory)
            fixture = {ref: response(ref, status="PENDING" if mode == "pending" else "FINISHED_SUCCESS",
                                     severity="HIGH" if mode == "high" else None)
                       for ref in (REFS if images is None else images).values() if ref}
            if mode == "missing":
                fixture = {ref: {"image_summary": {"fully_qualified_digest": ref}} for ref in fixture}
            elif mode == "missing_summary":
                for data in fixture.values():
                    data.pop("package_vulnerability_summary")
            elif mode == "missing_groups":
                for data in fixture.values():
                    data["package_vulnerability_summary"] = {}
            (workspace / "responses").write_text(json.dumps(fixture))
            # Guards must exit without even reading image refs or invoking gcloud.
            if environment == "prod":
                (workspace / "deploy-control.env").write_text(f"export SKIP_DEPLOY={str(skip).lower()}\n")
                if not skip:
                    (workspace / "image-refs.env").write_text("\n".join(
                        f"export {key}={value}" for key, value in (REFS if images is None else images).items()))
            executable = workspace / "gcloud"
            executable.write_text(f'''#!{sys.executable}
import json, os, pathlib, sys
root = pathlib.Path(os.environ["FIXTURE_DIR"])
with (root / "calls").open("a") as log:
    log.write(json.dumps(sys.argv[1:]) + "\\n")
if os.environ["MODE"] == "error":
    print("permission denied", file=sys.stderr)
    sys.exit(1)
if os.environ["MODE"] == "malformed":
    print("not json")
else:
    print(json.dumps(json.loads((root / "responses").read_text())[sys.argv[5]]))
''')
            executable.chmod(0o755)
            block = self.step.split("      - |\n", 1)[1]
            lines = []
            for line in block.splitlines():
                if line and not line.startswith("        "):
                    break
                lines.append(line[8:] if line else "")
            script = "\n".join(lines).replace("${_ENVIRONMENT}", environment).replace("$$", "$")
            script = script.replace("/workspace/", f"{workspace}/")
            if crash:
                script = "python3() { return 73; }\n" + script
            result = subprocess.run(["/bin/bash", "-c", script], cwd=ROOT, capture_output=True, text=True,
                                    env={"PATH": f"{workspace}:{Path(sys.executable).parent}:{os.defpath}",
                                         "FIXTURE_DIR": directory, "MODE": mode}, timeout=15)
            calls = [json.loads(line) for line in (workspace / "calls").read_text().splitlines()] if (workspace / "calls").exists() else []
            return result, calls

    def test_production_executes_real_reporter_for_every_exact_digest(self):
        result, calls = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call[4] for call in calls], list(REFS.values()))
        self.assertEqual(result.stdout.count("HIGH=1"), len(REFS))
        self.assertIn("CVE-2026-1234", result.stdout)

    def test_optional_runtime_absence_preserves_exact_core_refs(self):
        refs = {key: REFS[key] if image in {"api", "g5d", "edge"} else "" for key, image in cves.IMAGES.items()}
        result, calls = self.invoke(images=refs)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call[4] for call in calls], [ref for ref in refs.values() if ref])

    def test_qa_staging_and_superseded_production_do_not_query(self):
        for environment, skip in (("qa", False), ("staging", False), ("prod", True)):
            with self.subTest(environment=environment, skip=skip):
                result, calls = self.invoke(environment, skip)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(calls, [])
                self.assertIn("Skipping CVE report", result.stdout)

    def test_missing_pending_failed_and_malformed_reports_do_not_gate(self):
        for mode in ("pending", "missing", "missing_summary", "missing_groups", "error", "malformed"):
            with self.subTest(mode=mode):
                result, calls = self.invoke(mode=mode)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(len(calls), len(REFS))
                self.assertEqual(result.stdout.count("INCOMPLETE"), len(REFS))
                if mode in {"missing", "missing_summary", "missing_groups"}:
                    self.assertNotIn("Returned vulnerability occurrences:", result.stdout)
                    self.assertNotIn("Scan metadata: COMPLETE", result.stdout)
                if mode == "error":
                    self.assertIn("permission denied", result.stdout)

    def test_unexpected_reporter_failure_is_allowed_at_cloud_build_boundary(self):
        result, calls = self.invoke(crash=True)
        self.assertEqual(result.returncode, 73)
        self.assertEqual(calls, [])
        self.assertRegex(self.step, r"(?m)^    allowFailure: true$")
        self.assertRegex(self.step, r"(?m)^    timeout: 180s$")


if __name__ == "__main__":
    unittest.main()
