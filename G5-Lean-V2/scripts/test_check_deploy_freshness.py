"""Unit tests for check_deploy_freshness.py.

Run with: python3 -m unittest test_check_deploy_freshness (from this
directory) or python3 -m unittest generation.scripts.test_check_deploy_freshness.

Each test pins a contract from cloudbuild_pipelines ontology [D5]/[D24]:
legacy newest-main semantics stay byte-compatible (build mode + promotion
fallback), the advance predicate proceeds strictly-newer / skips
rollback-or-tie / skips already-deployed / proceeds on an unlisted deployed
origin, promotion-mode origin misses stay retryable failures, and nanosecond
createTime values parse on Python <= 3.10.
"""
from __future__ import annotations

import io
import json
import os
import sys
import tempfile
import unittest
from unittest import mock

import check_deploy_freshness as cdf


def dev_build(build_id, create_time, commit, trigger="g5-deploy-qa", status="SUCCESS"):
    return {
        "id": build_id,
        "createTime": create_time,
        "status": status,
        "substitutions": {"TRIGGER_NAME": trigger, "COMMIT_SHA": commit},
    }


T1 = "2026-07-13T10:00:00.123456789Z"  # nanosecond precision on purpose
T2 = "2026-07-13T11:00:00Z"
T3 = "2026-07-13T12:00:00Z"


class CheckDeployFreshnessTest(unittest.TestCase):
    def run_main(self, builds, argv_extra, expect_exit=0):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
            json.dump(builds, fh)
            path = fh.name
        self.addCleanup(os.unlink, path)
        argv = ["--builds-json", path, *argv_extra]
        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch.object(sys, "stdout", stdout), mock.patch.object(sys, "stderr", stderr):
            code = cdf.main(argv)
        self.assertEqual(
            code, expect_exit, f"exit={code} stdout={stdout.getvalue()!r} stderr={stderr.getvalue()!r}"
        )
        return stdout.getvalue().strip(), stderr.getvalue()

    # ---- legacy newest-main predicate (build mode + promotion fallback) ----

    def test_build_mode_newest_proceeds(self):
        builds = [dev_build("b1", T1, "aaa"), dev_build("b2", T2, "bbb")]
        out, _ = self.run_main(
            builds,
            [
                "--current-build-id", "b2",
                "--current-create-time", T2,
                "--artifact-commit", "bbb",
                "--promotion-mode", "build",
            ],
        )
        self.assertEqual(out, "PROCEED\tbbb\tnewest")

    def test_build_mode_newer_commit_skips_stale(self):
        builds = [dev_build("b1", T1, "aaa"), dev_build("b2", T2, "bbb")]
        out, _ = self.run_main(
            builds,
            [
                "--current-build-id", "b1",
                "--current-create-time", T1,
                "--artifact-commit", "aaa",
                "--promotion-mode", "build",
            ],
        )
        self.assertEqual(out, "SKIP\tbbb\tstale_newer_main")

    def test_build_mode_missing_origin_uses_current_time(self):
        # Build mode never raises on a missing origin: the current build IS
        # the newest signal, so an empty history proceeds.
        out, _ = self.run_main(
            [],
            [
                "--current-build-id", "b9",
                "--current-create-time", T2,
                "--artifact-commit", "zzz",
                "--promotion-mode", "build",
            ],
        )
        self.assertEqual(out, "PROCEED\tzzz\tnewest")

    def test_promotion_fallback_without_deployed_commit_keeps_legacy_skip(self):
        # Promotion mode with an unresolvable deployed commit falls back to
        # the [D5] newest-main predicate — the pre-[D24] behavior.
        builds = [dev_build("b1", T1, "aaa"), dev_build("b2", T2, "bbb")]
        out, _ = self.run_main(
            builds,
            [
                "--current-build-id", "p1",
                "--current-create-time", T1,
                "--artifact-commit", "aaa",
                "--promotion-mode", "deploy",
            ],
        )
        self.assertEqual(out, "SKIP\tbbb\tstale_newer_main")

    def test_promotion_missing_origin_is_retryable_failure(self):
        builds = [dev_build("b2", T2, "bbb")]
        out, err = self.run_main(
            builds,
            [
                "--current-build-id", "p1",
                "--current-create-time", T2,
                "--artifact-commit", "aaa",
                "--promotion-mode", "deploy",
            ],
            expect_exit=1,
        )
        self.assertEqual(out, "")
        self.assertIn("Could not find a g5-deploy-qa origin build", err)

    def test_redeploy_of_same_commit_after_current_is_not_origin(self):
        # A later re-deploy of the artifact commit must not shift the origin
        # time forward past the current build's create time.
        builds = [
            dev_build("b1", T1, "aaa"),
            dev_build("b3", T3, "aaa"),
            dev_build("b2", T2, "bbb"),
        ]
        out, _ = self.run_main(
            builds,
            [
                "--current-build-id", "p1",
                "--current-create-time", T1,
                "--artifact-commit", "aaa",
                "--promotion-mode", "deploy",
            ],
        )
        self.assertEqual(out, "SKIP\tbbb\tstale_newer_main")

    # ---- [D24] advance predicate ----

    def test_advance_strictly_newer_proceeds_even_when_not_newest(self):
        # The deadlock case: artifact bbb is older than newest main ccc but
        # newer than deployed aaa — it must deploy.
        builds = [
            dev_build("b1", T1, "aaa"),
            dev_build("b2", T2, "bbb"),
            dev_build("b3", T3, "ccc"),
        ]
        out, _ = self.run_main(
            builds,
            [
                "--current-build-id", "p1",
                "--current-create-time", T2,
                "--artifact-commit", "bbb",
                "--promotion-mode", "deploy",
                "--deployed-commit", "aaa",
            ],
        )
        self.assertEqual(out, "PROCEED\tbbb\tadvances")

    def test_advance_rollback_skips(self):
        builds = [dev_build("b1", T1, "aaa"), dev_build("b2", T2, "bbb")]
        out, _ = self.run_main(
            builds,
            [
                "--current-build-id", "p1",
                "--current-create-time", T1,
                "--artifact-commit", "aaa",
                "--promotion-mode", "deploy",
                "--deployed-commit", "bbb",
            ],
        )
        self.assertEqual(out, "SKIP\tbbb\trollback")

    def test_advance_already_deployed_skips(self):
        builds = [dev_build("b1", T1, "aaa")]
        out, _ = self.run_main(
            builds,
            [
                "--current-build-id", "p1",
                "--current-create-time", T1,
                "--artifact-commit", "aaa",
                "--promotion-mode", "deploy",
                "--deployed-commit", "aaa",
            ],
        )
        self.assertEqual(out, "SKIP\taaa\talready_deployed")

    def test_advance_equal_origin_times_skip_as_rollback(self):
        # Distinct commits with equal origin times have no strict ordering —
        # never mutate the environment on a tie.
        builds = [dev_build("b1", T1, "aaa"), dev_build("b2", T1, "bbb")]
        out, _ = self.run_main(
            builds,
            [
                "--current-build-id", "p1",
                "--current-create-time", T1,
                "--artifact-commit", "aaa",
                "--promotion-mode", "deploy",
                "--deployed-commit", "bbb",
            ],
        )
        self.assertEqual(out, "SKIP\tbbb\trollback")

    def test_advance_unlisted_deployed_origin_proceeds(self):
        # Prod frozen for days: the deployed commit's origin build has aged
        # out of the 200-build window. Any listed artifact advances it.
        builds = [dev_build("b1", T1, "aaa")]
        out, err = self.run_main(
            builds,
            [
                "--current-build-id", "p1",
                "--current-create-time", T1,
                "--artifact-commit", "aaa",
                "--promotion-mode", "deploy",
                "--deployed-commit", "old" * 13 + "f",
            ],
        )
        self.assertEqual(out, "PROCEED\taaa\tdeployed_origin_unlisted")
        self.assertIn("no listed g5-deploy-qa origin", err)

    def test_build_mode_ignores_deployed_commit(self):
        # --deployed-commit is a promotion-mode input; build mode must keep
        # the legacy predicate even if a caller passes it.
        builds = [dev_build("b1", T1, "aaa"), dev_build("b2", T2, "bbb")]
        out, _ = self.run_main(
            builds,
            [
                "--current-build-id", "b1",
                "--current-create-time", T1,
                "--artifact-commit", "aaa",
                "--promotion-mode", "build",
                "--deployed-commit", "zzz",
            ],
        )
        self.assertEqual(out, "SKIP\tbbb\tstale_newer_main")

    # ---- input robustness ----

    def test_nanosecond_timestamps_parse(self):
        parsed = cdf.parse_time(T1)
        self.assertEqual(parsed.microsecond, 123456)

    def test_malformed_entries_are_skipped(self):
        builds = [
            {"id": "b0", "createTime": T1, "substitutions": {"TRIGGER_NAME": "g5-deploy-qa"}},
            {"id": "", "createTime": T1, "substitutions": {"TRIGGER_NAME": "g5-deploy-qa", "COMMIT_SHA": "x"}},
            dev_build("b1", "not-a-time", "aaa"),
            dev_build("b2", T2, "bbb", trigger="g5-promote-prod"),
            dev_build("b3", T3, "ccc"),
        ]
        self.assertEqual(
            [entry[1] for entry in cdf.collect_dev_deploys(builds)], ["b3"]
        )

    def test_unreadable_builds_json_fails_retryably(self):
        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch.object(sys, "stdout", stdout), mock.patch.object(sys, "stderr", stderr):
            code = cdf.main(
                [
                    "--builds-json", "/nonexistent/builds.json",
                    "--current-build-id", "b1",
                    "--current-create-time", T1,
                    "--artifact-commit", "aaa",
                    "--promotion-mode", "deploy",
                ]
            )
        self.assertEqual(code, 1)
        self.assertIn("unreadable builds json", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
