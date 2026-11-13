"""Unit tests for await_worker_image.py.

Run with: python3 -m unittest test_await_worker_image (from scripts/ — the
module imports its siblings by bare name).

The fake runner scripts `gcloud compute images list` (a queue of image lists
so an image can "appear" later), `gcloud builds list` (a queue of
worker-image build lists), trigger describe/run and build describe, and the git
calls the gate makes: label resolution, ancestry, the range diff and the
range commits. A fake clock advances only through the injected sleep, so wait
bounds are exact.

Commit layout, oldest first: ANCESTOR_COMMIT (an older baked image), MID_COMMIT
(an earlier merge whose bake may lag), COMMIT (the promoted commit) and
DESCENDANT_COMMIT (a newer main commit with its own image).
"""
from __future__ import annotations

import contextlib
import io
import json
import re
import subprocess
import unittest
import urllib.error
from unittest import mock

import await_worker_image as awi
from test_resolve_worker_image import COMMIT, COMMIT_EPOCH, FakeRunner, image, ts

PROJECT = "g5-qa-1ccc"
FAMILY = "g5-worker"
TRIGGER = "g5-packer-worker-main-qa"
TRIGGER_ID = "77303f6c-49ce-4156-b6a7-c2e2da65fd38"
EXACT_SHA = COMMIT[:7]
ANCESTOR_SHA = "a1b2c3d"
ANCESTOR_COMMIT = ANCESTOR_SHA + "0" * 33
MID_SHA = "b2c3d4e"
MID_COMMIT = MID_SHA + "0" * 33
DESCENDANT_SHA = "d4e5f6a"
DESCENDANT_COMMIT = DESCENDANT_SHA + "0" * 33

ANCESTOR = image("g5-worker-v1-a1b2c3d", ANCESTOR_SHA, ts(COMMIT_EPOCH - 3600))
# The earlier merge's bake finished after the promoted commit was committed.
MID = image("g5-worker-v2-b2c3d4e", MID_SHA, ts(COMMIT_EPOCH + 1800))
EXACT = image(f"g5-worker-v3-{EXACT_SHA}", EXACT_SHA, ts(COMMIT_EPOCH + 3600))
DESCENDANT = image("g5-worker-v4-d4e5f6a", DESCENDANT_SHA, ts(COMMIT_EPOCH + 7200))
# A rebuild of the older ancestor commit, created after every other image.
ANCESTOR_REBUILD = image("g5-worker-v5-a1b2c3d", ANCESTOR_SHA, ts(COMMIT_EPOCH + 9000))

INCLUDED = ["infra/packer/g5-worker/**", "backend/g5d/**", "backend/g5d-*/**", "backend/g5-contracts/**", "Cargo.lock"]
WORKER_FILE = "backend/g5-contracts/src/terminal.rs"

LOCAL_SHAS = {
    ANCESTOR_SHA: ANCESTOR_COMMIT,
    MID_SHA: MID_COMMIT,
    EXACT_SHA: COMMIT,
    DESCENDANT_SHA: DESCENDANT_COMMIT,
}
ANCESTRY = {(ANCESTOR_COMMIT, COMMIT), (MID_COMMIT, COMMIT), (ANCESTOR_COMMIT, MID_COMMIT)}
# Range from each image commit to the promoted commit.
REVLISTS = {
    ANCESTOR_COMMIT: [COMMIT, MID_COMMIT],
    MID_COMMIT: [COMMIT],
}


def build(build_id, status, commit=COMMIT, create_time="2026-09-21T10:00:00+00:00"):
    return {"id": build_id, "status": status, "createTime": create_time, "substitutions": {"COMMIT_SHA": commit}}


def ref(img):
    return f"projects/{PROJECT}/global/images/{img['name']}"


class GateRunner(FakeRunner):
    """Resolver fake plus queued images and worker-image builds, the trigger
    description, and per-base range diffs and commits."""

    def __init__(self, image_lists, build_lists=([],), trigger=None, diffs=None, revlists=None,
                 run_result=(0, "recovery-bake\n", ""), recovery_builds=None, **kwargs):
        kwargs.setdefault("local_shas", LOCAL_SHAS)
        kwargs.setdefault("ancestors", ANCESTRY)
        super().__init__(images=[], **kwargs)
        self.image_lists = list(image_lists)
        self.build_lists = list(build_lists)
        self.trigger = trigger if trigger is not None else {"id": TRIGGER_ID, "includedFiles": INCLUDED}
        self.diffs = diffs or {}
        self.revlists = REVLISTS if revlists is None else revlists
        self.builds_cmds = []
        self.describe_calls = 0
        self.diff_cmds = []
        self.run_result = run_result
        self.recovery_builds = list(recovery_builds or [build("recovery-bake", "WORKING")])
        self.run_cmds = []
        self.build_describe_cmds = []

    @property
    def builds_calls(self):
        return len(self.builds_cmds)

    def __call__(self, cmd):
        if cmd[:4] == ["gcloud", "builds", "triggers", "run"]:
            self.run_cmds.append(list(cmd))
            return self.run_result
        if cmd[:3] == ["gcloud", "builds", "describe"]:
            self.build_describe_cmds.append(list(cmd))
            current = self.recovery_builds.pop(0) if len(self.recovery_builds) > 1 else self.recovery_builds[0]
            return 0, json.dumps(current), ""
        if cmd[:3] == ["gcloud", "builds", "list"]:
            self.builds_cmds.append(list(cmd))
            current = self.build_lists.pop(0) if len(self.build_lists) > 1 else self.build_lists[0]
            # Apply a client-side commit filter the way gcloud would.
            wanted = re.search(r'substitutions\.COMMIT_SHA="([0-9a-f]+)"', " ".join(cmd))
            if wanted:
                current = [b for b in current if b["substitutions"]["COMMIT_SHA"] == wanted.group(1)]
            return 0, json.dumps(current), ""
        if cmd[:4] == ["gcloud", "builds", "triggers", "describe"]:
            self.describe_calls += 1
            return 0, json.dumps(self.trigger), ""
        if cmd[:4] == ["gcloud", "compute", "images", "list"]:
            self.images = self.image_lists.pop(0) if len(self.image_lists) > 1 else self.image_lists[0]
            return 0, json.dumps(self.images), ""
        if cmd[0] == "git":
            sub = cmd[3]
            if sub == "rev-parse" and "--verify" in cmd:
                return 0, COMMIT + "\n", ""
            if sub == "merge-base" and cmd[-2] == cmd[-1]:
                return 0, "", ""
            if sub == "diff":
                self.diff_cmds.append(list(cmd))
                return 0, "\n".join(self.diffs.get(cmd[-2], [])) + "\n", ""
            if sub == "rev-list":
                base = cmd[-1].split("..", 1)[0]
                return 0, "\n".join(self.revlists.get(base, [])) + "\n", ""
        return super().__call__(cmd)


class FakeClock:
    def __init__(self):
        self.now = 1000.0
        self.sleeps = []

    def sleep(self, secs):
        self.sleeps.append(secs)
        self.now += secs

    def __call__(self):
        return self.now


def run(runner, clock, wait_secs=5400, ready_grace_secs=600, poll_secs=60, **extra):
    err = io.StringIO()
    with contextlib.redirect_stderr(err):
        result = awi.await_worker_image(
            image_project=PROJECT,
            family=FAMILY,
            commit=COMMIT,
            repo_dir=".",
            build_project=PROJECT,
            region="us-central1",
            trigger=TRIGGER,
            wait_secs=wait_secs,
            ready_grace_secs=ready_grace_secs,
            poll_secs=poll_secs,
            runner=runner,
            sleep=clock.sleep,
            clock=clock,
            **extra,
        )
    return result, err.getvalue()


def dirty_since_ancestor(**kwargs):
    """MID_COMMIT changed worker inputs after ANCESTOR was baked; the promoted
    commit itself changed only docs."""
    diffs = {ANCESTOR_COMMIT: [WORKER_FILE, "docs/x.md"], MID_COMMIT: ["docs/x.md"], f"{COMMIT}^": ["docs/x.md"]}
    return GateRunner(diffs=diffs, **kwargs)


class GateTest(unittest.TestCase):
    def test_exact_image_pins_without_reading_cloud_build(self):
        runner = GateRunner([[DESCENDANT, ANCESTOR, EXACT]], [[build("b1", "WORKING")]])
        clock = FakeClock()
        result, err = run(runner, clock)
        self.assertEqual(result, ref(EXACT))
        self.assertEqual((runner.builds_calls, runner.describe_calls), (0, 0))
        self.assertEqual(clock.sleeps, [])
        self.assertIn("no wait", err)

    def test_exact_image_pins_while_trigger_is_disabled(self):
        runner = GateRunner([[ANCESTOR, EXACT]], trigger={"id": TRIGGER_ID, "disabled": True})
        result, _ = run(runner, FakeClock())
        self.assertEqual(result, ref(EXACT))

    def test_clean_range_keeps_ancestor_without_listing_builds(self):
        runner = GateRunner([[ANCESTOR]], diffs={ANCESTOR_COMMIT: ["docs/x.md", "frontend/src/a.ts"]})
        clock = FakeClock()
        result, err = run(runner, clock)
        self.assertEqual(result, ref(ANCESTOR))
        self.assertEqual(runner.builds_calls, 0)
        self.assertEqual(clock.sleeps, [])
        self.assertIn("none match", err)
        # A rename out of a worker-input path must show its old path.
        self.assertIn("--no-renames", runner.diff_cmds[0])

    def test_newer_non_ancestor_image_is_skipped(self):
        runner = GateRunner([[DESCENDANT, ANCESTOR]], diffs={ANCESTOR_COMMIT: ["docs/x.md"]})
        result, _ = run(runner, FakeClock())
        self.assertEqual(result, ref(ANCESTOR))

    def test_older_rebuild_does_not_shadow_a_nearer_covering_image(self):
        runner = dirty_since_ancestor(image_lists=[[ANCESTOR_REBUILD, MID, ANCESTOR]])
        clock = FakeClock()
        result, _ = run(runner, clock)
        self.assertEqual(result, ref(MID))
        self.assertEqual(runner.builds_calls, 0)
        self.assertEqual(clock.sleeps, [])

    def test_shallow_checkout_compares_only_images_inside_the_ancestor_span(self):
        # Once ANCESTOR_REBUILD proves an ancestor, only MID (inside its span)
        # is compared; DESCENDANT is skipped without a GitHub call.
        runner = GateRunner([[ANCESTOR_REBUILD, DESCENDANT, MID]], local_shas={EXACT_SHA: COMMIT}, shallow=True)
        spans = {
            ANCESTOR_SHA: awi.Span(True, [WORKER_FILE], frozenset({MID_COMMIT, COMMIT})),
            MID_SHA: awi.Span(True, ["docs/x.md"], frozenset({COMMIT})),
        }
        asked = []

        def github_compare(base, head):
            asked.append(base)
            return spans[base]

        result, _ = run(runner, FakeClock(), github_compare=github_compare)
        self.assertEqual(result, ref(MID))
        self.assertEqual(asked, [ANCESTOR_SHA, MID_SHA])

    def test_no_ancestor_image_fails_closed(self):
        runner = GateRunner([[DESCENDANT]])
        with self.assertRaises(LookupError):
            run(runner, FakeClock())

    def test_missing_bake_starts_once_then_waits_and_pins_its_image(self):
        # An earlier merge changed worker inputs while its trigger was paused.
        # The promoted commit only changed docs. Lists remain empty throughout:
        # use the run's build ID rather than repeatedly launching missing bakes.
        runner = dirty_since_ancestor(
            image_lists=[[ANCESTOR]] * 6 + [[EXACT, ANCESTOR]],
            recovery_builds=[build("recovery-bake", status) for status in ("QUEUED", "WORKING", "SUCCESS")],
        )
        clock = FakeClock()
        result, err = run(runner, clock)
        self.assertEqual(result, ref(EXACT))
        self.assertEqual(runner.builds_calls, awi.LIST_RECHECKS)
        self.assertEqual(runner.run_cmds, [[
            "gcloud", "builds", "triggers", "run", TRIGGER,
            f"--project={PROJECT}", "--region=us-central1", f"--sha={COMMIT}",
            "--format=value(metadata.build.id)",
        ]])
        self.assertEqual(len(runner.build_describe_cmds), 3)
        for cmd in runner.build_describe_cmds:
            self.assertEqual(cmd[3], "recovery-bake")
            self.assertIn(f"--project={PROJECT}", cmd)
            self.assertIn("--region=us-central1", cmd)
        self.assertEqual(clock.sleeps, [30, 30, 60, 60, 60])
        self.assertIn(f"worker-image inputs changed since {ANCESTOR['name']} ({WORKER_FILE})", err)
        self.assertIn(f"started worker-image build recovery-bake via {TRIGGER} for {COMMIT}", err)
        self.assertIn("waiting for its image to reach READY", err)

    def test_trigger_run_failure_fails_closed_and_names_trigger_and_commit(self):
        runner = dirty_since_ancestor(
            image_lists=[[ANCESTOR]], run_result=(1, "", "PERMISSION_DENIED: iam.serviceAccounts.actAs"),
        )
        with self.assertRaises(RuntimeError) as ctx:
            run(runner, FakeClock())
        message = str(ctx.exception)
        self.assertIn(f"could not start worker-image trigger {TRIGGER} for {COMMIT}", message)
        self.assertIn("PERMISSION_DENIED", message)
        self.assertEqual(len(runner.run_cmds), 1)
        self.assertEqual(runner.build_describe_cmds, [])

    def test_trigger_run_without_build_id_fails_closed_without_retry(self):
        runner = dirty_since_ancestor(image_lists=[[ANCESTOR]], run_result=(0, "\n", ""))
        with self.assertRaisesRegex(RuntimeError, f"{TRIGGER} for {COMMIT} returned no build id"):
            run(runner, FakeClock())
        self.assertEqual(len(runner.run_cmds), 1)

    def test_trigger_run_timeout_names_trigger_and_commit_without_retry(self):
        class TimedOut(GateRunner):
            def __call__(self, cmd):
                if cmd[:4] == ["gcloud", "builds", "triggers", "run"]:
                    self.run_cmds.append(list(cmd))
                    raise subprocess.TimeoutExpired(cmd, 120)
                return super().__call__(cmd)

        runner = TimedOut([[ANCESTOR]], diffs={ANCESTOR_COMMIT: [WORKER_FILE]})
        with self.assertRaisesRegex(RuntimeError, f"could not start worker-image trigger {TRIGGER} for {COMMIT}"):
            run(runner, FakeClock())
        self.assertEqual(len(runner.run_cmds), 1)

    def test_bake_appearing_during_rechecks_prevents_trigger_run(self):
        for appears_on in (2, awi.LIST_RECHECKS):
            with self.subTest(appears_on=appears_on):
                runner = dirty_since_ancestor(
                    image_lists=[[ANCESTOR]] * appears_on + [[MID, ANCESTOR]],
                    build_lists=[[]] * (appears_on - 1) + [[build("m1", "WORKING", MID_COMMIT)]],
                )
                clock = FakeClock()
                result, _ = run(runner, clock)
                self.assertEqual(result, ref(MID))
                self.assertEqual(runner.run_cmds, [])
                self.assertEqual(clock.sleeps, [30] * (appears_on - 1) + [60])

    def test_recovery_bake_failure_or_cancellation_fails_without_another_run(self):
        for status in sorted(awi.FAILED_STATUSES):
            with self.subTest(status=status):
                runner = dirty_since_ancestor(
                    image_lists=[[ANCESTOR]], recovery_builds=[build("recovery-bake", status)],
                )
                with self.assertRaisesRegex(RuntimeError, f"build recovery-bake for {COMMIT} ended {status}"):
                    run(runner, FakeClock())
                self.assertEqual(len(runner.run_cmds), 1)

    def test_recovery_bake_uses_the_original_wait_bound(self):
        runner = dirty_since_ancestor(image_lists=[[ANCESTOR]])
        clock = FakeClock()
        with self.assertRaisesRegex(RuntimeError, "still WORKING after the 150s wait bound"):
            run(runner, clock, wait_secs=150)
        self.assertEqual(clock.sleeps, [30, 30, 60, 30])
        self.assertEqual(len(runner.run_cmds), 1)

    def test_exhausted_wait_bound_does_not_start_a_bake(self):
        runner = dirty_since_ancestor(image_lists=[[ANCESTOR]])
        with self.assertRaisesRegex(RuntimeError, "no .* build appeared within the 60s wait bound"):
            run(runner, FakeClock(), wait_secs=60)
        self.assertEqual(runner.run_cmds, [])

    def test_recovery_bake_success_without_ready_image_exhausts_grace(self):
        runner = dirty_since_ancestor(
            image_lists=[[ANCESTOR]], recovery_builds=[build("recovery-bake", "SUCCESS")],
        )
        clock = FakeClock()
        with self.assertRaisesRegex(RuntimeError, "succeeded but no READY.*within 120s"):
            run(runner, clock, ready_grace_secs=120)
        self.assertEqual(clock.sleeps, [30, 30, 60, 60])
        self.assertEqual(len(runner.run_cmds), 1)

    def test_recovery_bake_status_read_failure_fails_closed_without_retry(self):
        class Unreadable(GateRunner):
            def __call__(self, cmd):
                if cmd[:3] == ["gcloud", "builds", "describe"]:
                    return 1, "", "PERMISSION_DENIED"
                return super().__call__(cmd)

        runner = Unreadable([[ANCESTOR]], diffs={ANCESTOR_COMMIT: [WORKER_FILE]})
        with self.assertRaisesRegex(RuntimeError, "builds describe recovery-bake failed.*PERMISSION_DENIED"):
            run(runner, FakeClock())
        self.assertEqual(len(runner.run_cmds), 1)

    def test_recovery_bake_missing_or_mismatched_evidence_fails_closed(self):
        for evidence in ({}, build("other-bake", "SUCCESS"), build("recovery-bake", "SUCCESS", DESCENDANT_COMMIT)):
            with self.subTest(evidence=evidence):
                runner = dirty_since_ancestor(image_lists=[[ANCESTOR]], recovery_builds=[evidence])
                with self.assertRaisesRegex(RuntimeError, "returned no matching build status"):
                    run(runner, FakeClock())
                self.assertEqual(len(runner.run_cmds), 1)

    def test_trigger_paused_during_rechecks_is_not_run(self):
        class Paused(GateRunner):
            def __call__(self, cmd):
                if cmd[:4] == ["gcloud", "builds", "triggers", "describe"] and self.describe_calls:
                    self.trigger["disabled"] = True
                return super().__call__(cmd)

        runner = Paused([[ANCESTOR]], diffs={ANCESTOR_COMMIT: [WORKER_FILE]})
        with self.assertRaisesRegex(RuntimeError, "is disabled"):
            run(runner, FakeClock())
        self.assertEqual(runner.builds_calls, awi.LIST_RECHECKS)
        self.assertEqual(runner.run_cmds, [])

    def test_running_bake_of_an_earlier_merge_is_awaited_then_its_image_pinned(self):
        runner = dirty_since_ancestor(
            image_lists=[[ANCESTOR], [ANCESTOR], [MID, ANCESTOR]],
            build_lists=[[build("m1", "WORKING", MID_COMMIT)], [build("m1", "SUCCESS", MID_COMMIT)]],
        )
        clock = FakeClock()
        result, err = run(runner, clock, poll_secs=60)
        self.assertEqual(result, ref(MID))
        # one poll while the bake runs, one grace poll while its image publishes
        self.assertEqual(clock.sleeps, [60, 60])
        self.assertIn(f"worker-image build m1 for {MID_COMMIT} is WORKING", err)

    def test_builds_are_listed_by_trigger_id_server_side(self):
        runner = GateRunner(
            [[ANCESTOR], [ANCESTOR, EXACT]],
            [[build("b1", "WORKING")], [build("b1", "SUCCESS")]],
            diffs={ANCESTOR_COMMIT: ["backend/g5d/src/main.rs"]},
        )
        result, _ = run(runner, FakeClock())
        self.assertEqual(result, ref(EXACT))
        self.assertTrue(runner.builds_cmds)
        for cmd in runner.builds_cmds:
            filters = [arg for arg in cmd if arg.startswith("--filter=")]
            self.assertEqual(filters, [f'--filter=trigger_id="{TRIGGER_ID}"'])
            self.assertIn("--project=g5-qa-1ccc", cmd)

    def test_running_bake_of_the_promoted_commit_is_awaited_then_exact_pinned(self):
        runner = GateRunner(
            [[ANCESTOR], [ANCESTOR], [ANCESTOR, EXACT]],
            [[build("b1", "QUEUED")], [build("b1", "WORKING")]],
            diffs={ANCESTOR_COMMIT: ["backend/g5d/src/main.rs"]},
        )
        clock = FakeClock()
        result, _ = run(runner, clock, poll_secs=60)
        self.assertEqual(result, ref(EXACT))
        self.assertEqual(clock.sleeps, [60, 60])

    def test_builds_outside_the_range_are_ignored(self):
        runner = dirty_since_ancestor(
            image_lists=[[ANCESTOR]] * 4 + [[EXACT, ANCESTOR]],
            build_lists=[[build("d1", "WORKING", DESCENDANT_COMMIT)]],
        )
        clock = FakeClock()
        result, _ = run(runner, clock)
        self.assertEqual(result, ref(EXACT))
        self.assertEqual(runner.builds_calls, awi.LIST_RECHECKS)
        self.assertEqual(len(runner.run_cmds), 1)

    def test_newest_build_in_range_wins_when_several_exist(self):
        old_failed = build("b0", "FAILURE", create_time="2026-09-21T09:00:00+00:00")
        rerun_ok = build("b1", "SUCCESS", create_time="2026-09-21T11:00:00+00:00")
        runner = GateRunner(
            [[ANCESTOR], [ANCESTOR, EXACT]], [[old_failed, rerun_ok]], diffs={ANCESTOR_COMMIT: ["Cargo.lock"]}
        )
        result, _ = run(runner, FakeClock())
        self.assertEqual(result, ref(EXACT))

    def test_failed_bake_in_range_fails_closed(self):
        runner = dirty_since_ancestor(image_lists=[[ANCESTOR]], build_lists=[[build("m1", "FAILURE", MID_COMMIT)]])
        with self.assertRaises(RuntimeError) as ctx:
            run(runner, FakeClock())
        self.assertIn(f"worker-image build m1 for {MID_COMMIT} ended FAILURE", str(ctx.exception))
        self.assertIn(f"re-run {TRIGGER} for {MID_COMMIT}", str(ctx.exception))
        self.assertEqual(runner.run_cmds, [])

    def test_wait_bound_fails_closed(self):
        runner = dirty_since_ancestor(image_lists=[[ANCESTOR]], build_lists=[[build("b1", "WORKING")]])
        clock = FakeClock()
        with self.assertRaises(RuntimeError) as ctx:
            run(runner, clock, wait_secs=150, poll_secs=60)
        self.assertIn("still WORKING after the 150s wait bound", str(ctx.exception))
        self.assertEqual(clock.sleeps, [60, 60, 30])

    def test_success_without_ready_image_fails_after_grace(self):
        runner = dirty_since_ancestor(image_lists=[[ANCESTOR]], build_lists=[[build("b1", "SUCCESS")]])
        clock = FakeClock()
        with self.assertRaises(RuntimeError) as ctx:
            run(runner, clock, ready_grace_secs=120, poll_secs=60)
        self.assertIn("no READY g5-worker image covering", str(ctx.exception))
        self.assertEqual(clock.sleeps, [60, 60])

    def test_each_successful_bake_gets_its_own_ready_grace(self):
        # MID's bake succeeds first and starts a grace period; its image then
        # appears but lacks the promoted commit's own worker change, whose bake
        # succeeds after MID's grace expired. That later bake still gets a grace.
        m1 = build("m1", "SUCCESS", MID_COMMIT, "2026-09-21T10:00:00+00:00")
        c1_running = build("c1", "WORKING", COMMIT, "2026-09-21T10:05:00+00:00")
        c1_done = build("c1", "SUCCESS", COMMIT, "2026-09-21T10:05:00+00:00")
        runner = GateRunner(
            [[ANCESTOR], [MID, ANCESTOR], [MID, ANCESTOR], [MID, ANCESTOR], [EXACT, MID, ANCESTOR]],
            [[m1], [c1_running, m1], [c1_running, m1], [c1_done, m1]],
            diffs={ANCESTOR_COMMIT: [WORKER_FILE, "backend/g5d/src/main.rs"], MID_COMMIT: ["backend/g5d/src/main.rs"]},
        )
        clock = FakeClock()
        result, _ = run(runner, clock, ready_grace_secs=120, poll_secs=60)
        self.assertEqual(result, ref(EXACT))
        self.assertEqual(clock.sleeps, [60, 60, 60, 60])

    def test_disabled_trigger_fails_closed_on_an_ancestor_pin(self):
        runner = GateRunner(
            [[ANCESTOR]], trigger={"id": TRIGGER_ID, "disabled": True, "includedFiles": INCLUDED},
            diffs={ANCESTOR_COMMIT: ["docs/x.md"]},
        )
        with self.assertRaises(RuntimeError) as ctx:
            run(runner, FakeClock())
        self.assertIn("is disabled", str(ctx.exception))

    def test_disabled_trigger_without_a_bake_fails_closed_without_rechecks(self):
        runner = dirty_since_ancestor(
            image_lists=[[ANCESTOR]], trigger={"id": TRIGGER_ID, "disabled": True, "includedFiles": INCLUDED}
        )
        clock = FakeClock()
        with self.assertRaises(RuntimeError) as ctx:
            run(runner, clock)
        self.assertIn("is disabled", str(ctx.exception))
        self.assertEqual(clock.sleeps, [])
        self.assertEqual(runner.run_cmds, [])

    def test_builds_list_failure_is_reported(self):
        class Broken(GateRunner):
            def __call__(self, cmd):
                if cmd[:3] == ["gcloud", "builds", "list"]:
                    return 1, "", "PERMISSION_DENIED: cloudbuild.builds.list"
                return super().__call__(cmd)

        runner = Broken([[ANCESTOR]], diffs={ANCESTOR_COMMIT: [WORKER_FILE]})
        with self.assertRaises(RuntimeError) as ctx:
            run(runner, FakeClock())
        self.assertIn("gcloud builds list failed", str(ctx.exception))
        self.assertIn("PERMISSION_DENIED", str(ctx.exception))

    def test_shallow_checkout_without_github_access_fails_closed(self):
        runner = GateRunner([[ANCESTOR]], local_shas={EXACT_SHA: COMMIT}, shallow=True)
        with self.assertRaises(RuntimeError) as ctx:
            run(runner, FakeClock())
        self.assertIn("shallow checkout, no GitHub access", str(ctx.exception))

    def test_shallow_checkout_pins_a_covering_image_baked_after_the_commit(self):
        # Only the promoted commit is local. The earlier merge's image was
        # created after the promoted commit's committer time, which the
        # timestamp heuristic would reject; GitHub proves it covers.
        runner = GateRunner([[MID, ANCESTOR]], local_shas={EXACT_SHA: COMMIT}, shallow=True)
        asked = []

        def github_compare(base, head):
            asked.append((base, head))
            return awi.Span(True, ["docs/x.md"], frozenset({COMMIT}))

        result, _ = run(runner, FakeClock(), github_compare=github_compare)
        self.assertEqual(result, ref(MID))
        self.assertEqual(asked, [(MID_SHA, COMMIT)])

    def test_shallow_checkout_compares_each_image_once_across_polls(self):
        runner = GateRunner(
            [[DESCENDANT, ANCESTOR], [DESCENDANT, MID, ANCESTOR]],
            [[build("m1", "WORKING", MID_COMMIT)]],
            local_shas={EXACT_SHA: COMMIT},
            shallow=True,
        )
        spans = {
            DESCENDANT_SHA: awi.NOT_ANCESTOR,
            ANCESTOR_SHA: awi.Span(True, [WORKER_FILE], frozenset({MID_COMMIT, COMMIT})),
            MID_SHA: awi.Span(True, ["docs/x.md"], frozenset({COMMIT})),
        }
        asked = []

        def github_compare(base, head):
            asked.append(base)
            return spans[base]

        result, _ = run(runner, FakeClock(), github_compare=github_compare)
        self.assertEqual(result, ref(MID))
        self.assertEqual(asked, [DESCENDANT_SHA, ANCESTOR_SHA, MID_SHA])


class TriggerFilterTest(unittest.TestCase):
    def test_globs_follow_cloud_build_semantics(self):
        t = awi.touches_worker_inputs
        self.assertTrue(t(["backend/g5d/src/cli/oci.rs"], INCLUDED, []))
        self.assertTrue(t(["backend/g5d/tests/x.rs"], INCLUDED, []))
        self.assertTrue(t(["infra/packer/g5-worker/a/b/c.pkr.hcl"], INCLUDED, []))
        self.assertTrue(t(["Cargo.lock"], INCLUDED, []))
        self.assertFalse(t(["backend/g5-api/src/main.rs", "docs/x.md"], INCLUDED, []))
        # `*` does not cross a directory boundary; `**` does
        self.assertFalse(t(["backend/g5d/src/cli/oci.rs"], ["backend/*"], []))
        self.assertTrue(t(["backend/g5d/src/cli/oci.rs"], ["backend/**"], []))
        self.assertTrue(t(["a/b/c.rs"], ["**/c.rs"], []))
        self.assertTrue(t(["c.rs"], ["**/c.rs"], []))

    def test_ignored_files_drop_matches_and_empty_included_means_any_change(self):
        t = awi.touches_worker_inputs
        self.assertFalse(t(["backend/g5d/README.md"], INCLUDED, ["**/*.md"]))
        self.assertTrue(t(["backend/g5d/README.md", "Cargo.lock"], INCLUDED, ["**/*.md"]))
        self.assertTrue(t(["anything"], [], []))
        self.assertFalse(t([], [], []))

    def test_truncated_file_list_counts_as_a_worker_change(self):
        trigger = awi.WorkerTrigger(TRIGGER_ID, False, INCLUDED, [])
        self.assertEqual(awi.worker_input_changes(awi.Span(True, None, frozenset()), trigger), ["300+ changed files, list truncated"])


class FakeResponse:
    def __init__(self, payload):
        self.payload = payload

    def read(self):
        return json.dumps(self.payload).encode("utf-8")

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class GitHubCompareTest(unittest.TestCase):
    def compare(self, payload=None, error=None):
        seen = []

        def opener(request, timeout):
            seen.append((request.full_url, request.get_method(), request.get_header("Authorization")))
            if request.full_url.endswith(":accessReadToken"):
                return FakeResponse({"token": "gh-read-token"})
            if error is not None:
                raise urllib.error.HTTPError(request.full_url, error, "err", {}, io.BytesIO(b"nope"))
            return FakeResponse(payload)

        client = awi.GitHubCompare(
            project=PROJECT,
            region="us-central1",
            connection="G5labs-G5-QA",
            repository="G5Labs-G5-Lean",
            owner="G5Labs",
            repo="G5-Lean",
            runner=lambda cmd: (0, "gcloud-token\n", ""),
            opener=opener,
        )
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            return client(ANCESTOR_SHA, COMMIT), seen

    def test_reads_token_then_compares_with_renames_and_commits(self):
        payload = {
            "status": "ahead",
            "files": [
                {"filename": "docs/x.md"},
                {"filename": "tools/moved.rs", "previous_filename": "backend/g5d/src/moved.rs"},
            ],
            "commits": [{"sha": MID_COMMIT}, {"sha": COMMIT}],
        }
        span, seen = self.compare(payload)
        self.assertEqual(span, awi.Span(True, ["docs/x.md", "tools/moved.rs", "backend/g5d/src/moved.rs"], frozenset({MID_COMMIT, COMMIT})))
        self.assertEqual(seen[0][1:], ("POST", "Bearer gcloud-token"))
        self.assertEqual(seen[1][0], f"https://api.github.com/repos/G5Labs/G5-Lean/compare/{ANCESTOR_SHA}...{COMMIT}")
        self.assertEqual(seen[1][2], "Bearer gh-read-token")

    def test_identical_has_no_commits(self):
        span, _ = self.compare({"status": "identical", "files": [], "commits": []})
        self.assertEqual(span, awi.Span(True, [], frozenset()))

    def test_behind_diverged_and_unknown_commits_are_not_ancestors(self):
        for payload in ({"status": "behind"}, {"status": "diverged"}):
            self.assertEqual(self.compare(payload)[0], awi.NOT_ANCESTOR)
        self.assertEqual(self.compare(error=404)[0], awi.NOT_ANCESTOR)

    def test_capped_file_list_is_unknown(self):
        files = [{"filename": f"docs/{i}.md"} for i in range(awi.GITHUB_COMPARE_FILES_CAP)]
        span, _ = self.compare({"status": "ahead", "files": files, "commits": [{"sha": COMMIT}]})
        self.assertIsNone(span.files)

    def test_unexpected_status_and_http_errors_raise(self):
        with self.assertRaises(RuntimeError) as ctx:
            self.compare({"status": "weird"})
        self.assertIn("unexpected status", str(ctx.exception))
        with self.assertRaises(RuntimeError) as ctx:
            self.compare(error=403)
        self.assertIn("(403)", str(ctx.exception))

    def flaky(self, token=(), compare=()):
        """A client whose token and compare requests each raise their own
        queued failures in turn, then answer; returns (client, attempted
        urls, slept delays)."""
        pending = {"token": list(token), "compare": list(compare)}
        urls, slept = [], []

        def opener(request, timeout):
            urls.append(request.full_url)
            endpoint = "token" if request.full_url.endswith(":accessReadToken") else "compare"
            if pending[endpoint]:
                failure = pending[endpoint].pop(0)
                if isinstance(failure, int):
                    raise urllib.error.HTTPError(request.full_url, failure, "err", {}, io.BytesIO(b"busy"))
                raise failure
            if endpoint == "token":
                return FakeResponse({"token": "gh-read-token"})
            return FakeResponse({"status": "ahead", "files": [], "commits": [{"sha": COMMIT}]})

        client = awi.GitHubCompare(
            project=PROJECT, region="us-central1", connection="c", repository="r", owner="o", repo="p",
            runner=lambda cmd: (0, "gcloud-token\n", ""), opener=opener, sleep=slept.append,
        )
        return client, urls, slept

    def call(self, client):
        with contextlib.redirect_stderr(io.StringIO()):
            return client(ANCESTOR_SHA, COMMIT)

    def attempts(self, urls):
        token = sum(u.endswith(":accessReadToken") for u in urls)
        return token, len(urls) - token

    def test_transient_token_and_compare_failures_are_retried(self):
        # deploy f683f16c: accessReadToken 503; deploy 1953f272: compare 504.
        first, second = awi.HTTP_RETRY_DELAYS_SECONDS[:2]
        # (queued failures, (token attempts, compare attempts), waits); each
        # request starts its own retry schedule.
        cases = [
            ({"token": [503]}, (2, 1), [first]),
            ({"compare": [504, 502]}, (1, 3), [first, second]),
            ({"compare": [urllib.error.URLError("connection reset")]}, (1, 2), [first]),
            ({"token": [urllib.error.URLError("timed out")], "compare": [503]}, (2, 2), [first, first]),
        ]
        for failures, attempts, waits in cases:
            with self.subTest(failures=failures):
                client, urls, slept = self.flaky(**failures)
                self.assertEqual(self.call(client), awi.Span(True, [], frozenset({COMMIT})))
                self.assertEqual(self.attempts(urls), attempts)
                self.assertEqual(slept, waits)

    def test_persistent_5xx_fails_after_bounded_retries(self):
        attempts = len(awi.HTTP_RETRY_DELAYS_SECONDS) + 1
        client, urls, slept = self.flaky(token=[503] * attempts)
        with self.assertRaises(RuntimeError) as ctx:
            self.call(client)
        self.assertIn("(503)", str(ctx.exception))
        self.assertEqual(len(urls), attempts)
        self.assertEqual(slept, list(awi.HTTP_RETRY_DELAYS_SECONDS))

    def test_4xx_is_not_retried(self):
        for code in (403, 404):
            client, urls, slept = self.flaky(token=[code])
            with self.assertRaises(RuntimeError):
                self.call(client)
            self.assertEqual((len(urls), slept), (1, []))
        client, urls, slept = self.flaky(compare=[403])
        with self.assertRaises(RuntimeError):
            self.call(client)
        self.assertEqual((self.attempts(urls), slept), ((1, 1), []))
        client, urls, slept = self.flaky(compare=[404])
        self.assertEqual(self.call(client), awi.NOT_ANCESTOR)
        self.assertEqual((self.attempts(urls), slept), ((1, 1), []))

    def test_missing_token_is_an_error(self):
        client = awi.GitHubCompare(
            project=PROJECT, region="us-central1", connection="c", repository="r", owner="o", repo="p",
            runner=lambda cmd: (0, "gcloud-token\n", ""), opener=lambda request, timeout: FakeResponse({}),
        )
        with self.assertRaises(RuntimeError) as ctx:
            client(ANCESTOR_SHA, COMMIT)
        self.assertIn("returned no token", str(ctx.exception))


class MainTest(unittest.TestCase):
    def main(self, runner, *extra):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            rc = awi.main(
                ["--project", PROJECT, "--family", FAMILY, "--commit", COMMIT, "--build-project", PROJECT,
                 "--region", "us-central1", "--trigger", TRIGGER, *extra],
                runner=runner,
            )
        return rc, out.getvalue(), err.getvalue()

    def test_rejects_short_commit(self):
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            rc = awi.main(
                ["--project", PROJECT, "--family", FAMILY, "--commit", "abc1234",
                 "--build-project", PROJECT, "--region", "us-central1", "--trigger", TRIGGER],
                runner=lambda cmd: (0, "[]", ""),
            )
        self.assertEqual(rc, 1)
        self.assertIn("full 40-hex sha", err.getvalue())

    def test_failure_exits_nonzero_and_prints_nothing(self):
        runner = dirty_since_ancestor(image_lists=[[ANCESTOR]], build_lists=[[build("b1", "CANCELLED")]])
        rc, out, err = self.main(runner, "--wait-mins", "0", "--poll-secs", "1")
        self.assertEqual(rc, 1)
        self.assertEqual(out, "")
        self.assertIn("ended CANCELLED", err)

    def test_missing_bake_start_failure_exits_nonzero_instead_of_pinning_old_image(self):
        # Replays deploy 21b20232 with full local history: the promoted
        # commit's first-parent diff is docs-only, so a gate that proves only
        # that diff pins ANCESTOR, which lacks MID_COMMIT's worker change.
        runner = dirty_since_ancestor(image_lists=[[ANCESTOR]], run_result=(1, "", "PERMISSION_DENIED"))
        with mock.patch.object(awi, "LIST_RECHECK_SECS", 0):
            rc, out, err = self.main(runner)
        self.assertEqual(rc, 1, err)
        self.assertEqual(out, "")
        self.assertIn(f"worker-image inputs changed since {ANCESTOR['name']}", err)
        self.assertIn(f"could not start worker-image trigger {TRIGGER} for {COMMIT}", err)
        self.assertEqual(len(runner.run_cmds), 1)


if __name__ == "__main__":
    unittest.main()
