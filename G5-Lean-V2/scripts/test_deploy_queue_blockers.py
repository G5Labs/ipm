"""Unit tests for deploy_queue_blockers.py (ontology
g5.quality.delivery.qa-deploy-order).

Run with: python3 -m unittest test_deploy_queue_blockers (from this directory).

Pins: same environment/project/mode filtering and creation-order ties as the
former inline wait-deploy-turn filter; an older build releases the queue once
its smoke-test step is terminal (any terminal status, including a skipped
deploy's SUCCESS) even while later steps run; queued builds, builds without
step data and builds whose smoke-test is pending/working still block.
"""
from __future__ import annotations

import io
import json
import os
import sys
import tempfile
import unittest
from unittest import mock

import deploy_queue_blockers as dqb

T1 = "2026-10-20T22:24:00.123456789Z"  # nanosecond precision on purpose
T2 = "2026-10-20T22:32:00Z"
T3 = "2026-10-20T23:33:00Z"


def build(build_id, create_time, steps=None, env="qa", project="g5-qa-1ccc", mode="build", commit="c" * 40):
    entry = {
        "id": build_id,
        "createTime": create_time,
        "substitutions": {
            "_PROMOTION_MODE": mode,
            "_ENVIRONMENT": env,
            "_TARGET_PROJECT": project,
            "COMMIT_SHA": commit,
            "TRIGGER_NAME": "g5-deploy-qa",
        },
    }
    if steps is not None:
        entry["steps"] = steps
    return entry


def steps(**status_by_id):
    """Cloud Build reports a step without status until it starts."""
    rows = []
    for step_id in ("resolve-image-refs", "wait-deploy-turn", "check-main-freshness", "helm-deploy",
                    "verify-deploy", "smoke-test", "publish-workload-templates", "warm-guest-sccache"):
        row = {"id": step_id}
        if step_id in status_by_id:
            row["status"] = status_by_id[step_id]
        rows.append(row)
    return rows


class DeployQueueBlockersTest(unittest.TestCase):
    def blockers(self, builds, current_id="me", current_create=T3, **overrides):
        kwargs = dict(environment="qa", target_project="g5-qa-1ccc", deploy_complete_step="smoke-test")
        kwargs.update(overrides)
        return [row[1] for row in dqb.blockers(builds, current_id, current_create, **kwargs)]

    def test_older_build_with_unfinished_deploy_phase_blocks(self):
        older = build("old", T1, steps(**{"helm-deploy": "WORKING"}))
        self.assertEqual(self.blockers([older]), ["old"])

    def test_older_build_past_smoke_test_releases_the_queue_while_its_tail_runs(self):
        for status in sorted(dqb.TERMINAL_STEP_STATUSES):
            with self.subTest(status=status):
                older = build("old", T1, steps(**{"smoke-test": status, "publish-workload-templates": "WORKING"}))
                self.assertEqual(self.blockers([older]), [])

    def test_pending_or_working_smoke_test_still_blocks(self):
        for status in ("PENDING", "QUEUED", "WORKING", "STATUS_UNKNOWN"):
            with self.subTest(status=status):
                older = build("old", T1, steps(**{"smoke-test": status}))
                self.assertEqual(self.blockers([older]), ["old"])

    def test_queued_build_without_step_data_blocks(self):
        self.assertEqual(self.blockers([build("old", T1)]), ["old"])
        self.assertEqual(self.blockers([build("old", T1, steps=[])]), ["old"])

    def test_unapproved_candidates_do_not_block_but_approved_queued_builds_do(self):
        for state in ({"status": "PENDING"}, {"approval": {"state": "PENDING"}},
                      {"status": "QUEUED", "approval": {"state": "APPROVED"}}):
            with self.subTest(state=state):
                older = build("old", T1, env="staging", project="g5lean-staging-1ccc", mode="deploy")
                older.update(state)
                expected = ["old"] if state.get("status") == "QUEUED" else []
                self.assertEqual(self.blockers([older], environment="staging",
                    target_project="g5lean-staging-1ccc", deploy_complete_step="promote-release"), expected)

    def test_target_release_keeps_turn_until_promotion_finishes(self):
        for status in (None, "QUEUED", "WORKING", "SUCCESS", "FAILURE"):
            with self.subTest(status=status):
                older = build("old", T1, [
                    {"id": "smoke-test", "status": "SUCCESS"},
                    {"id": "required-release-checks", "status": "SUCCESS"},
                    {"id": "promote-release", **({"status": status} if status else {})},
                ], env="staging", project="g5lean-staging-1ccc", mode="deploy")
                self.assertEqual(self.blockers([older], environment="staging",
                    target_project="g5lean-staging-1ccc", deploy_complete_step="promote-release"),
                    [] if status in ("SUCCESS", "FAILURE") else ["old"])

    def test_build_lacking_the_deploy_complete_step_blocks_until_it_ends(self):
        older = build("old", T1, [{"id": "helm-deploy", "status": "SUCCESS"}])
        self.assertEqual(self.blockers([older]), ["old"])

    def test_newer_builds_never_block(self):
        newer = build("new", T3, steps(**{"helm-deploy": "WORKING"}))
        self.assertEqual(self.blockers([newer], current_create=T2), [])

    def test_equal_create_time_breaks_ties_on_build_id(self):
        peer_lower = build("a-peer", T3, steps())
        peer_higher = build("z-peer", T3, steps())
        self.assertEqual(self.blockers([peer_lower, peer_higher], current_id="m-me"), ["a-peer"])

    def test_other_environment_project_or_mode_is_ignored(self):
        others = [
            build("other-env", T1, steps(), env="staging"),
            build("other-project", T1, steps(), project="g5-prod"),
            build("other-mode", T1, steps(), mode="other"),
            build("self", T1, steps()),
        ]
        self.assertEqual(self.blockers(others, current_id="self"), [])

    def test_deploy_mode_older_build_blocks_like_build_mode(self):
        older = build("old", T1, steps(**{"smoke-test": "WORKING"}), mode="deploy")
        self.assertEqual(self.blockers([older]), ["old"])

    def test_malformed_entries_are_skipped(self):
        bad = [{"id": "", "createTime": T1}, build("no-time", ""), build("bad-time", "yesterday", steps())]
        self.assertEqual(self.blockers(bad), [])

    def test_output_is_sorted_and_names_the_deploy_step_status(self):
        builds = [
            build("second", T2, steps(**{"smoke-test": "WORKING"}), commit="b" * 40),
            build("first", T1, commit="a" * 40),
        ]
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
            json.dump(builds, fh)
            path = fh.name
        self.addCleanup(os.unlink, path)
        stdout = io.StringIO()
        with mock.patch.object(sys, "stdout", stdout):
            code = dqb.main([
                "--builds-json", path, "--current-build-id", "me", "--current-create-time", T3,
                "--environment", "qa", "--target-project", "g5-qa-1ccc",
            ])
        self.assertEqual(code, 0)
        lines = stdout.getvalue().splitlines()
        self.assertEqual(
            lines,
            [f"{T1}\tfirst\t{'a' * 40}\tg5-deploy-qa\tnot-started", f"{T2}\tsecond\t{'b' * 40}\tg5-deploy-qa\tWORKING"],
        )

    def test_clear_queue_prints_nothing(self):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
            json.dump([build("old", T1, steps(**{"smoke-test": "SUCCESS"}))], fh)
            path = fh.name
        self.addCleanup(os.unlink, path)
        stdout = io.StringIO()
        with mock.patch.object(sys, "stdout", stdout):
            code = dqb.main([
                "--builds-json", path, "--current-build-id", "me", "--current-create-time", T3,
                "--environment", "qa", "--target-project", "g5-qa-1ccc",
            ])
        self.assertEqual(code, 0)
        self.assertEqual(stdout.getvalue(), "")

    def test_unreadable_input_fails(self):
        stderr = io.StringIO()
        with mock.patch.object(sys, "stderr", stderr):
            code = dqb.main([
                "--builds-json", "/nonexistent/builds.json", "--current-build-id", "me",
                "--current-create-time", T3, "--environment", "qa", "--target-project", "g5-qa-1ccc",
            ])
        self.assertEqual(code, 1)
        self.assertIn("unreadable input", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
