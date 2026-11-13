#!/usr/bin/env python3
"""Unit tests for approve_release_train.py ([D25] decision + sequencing logic).

Run: python3 -m unittest test_approve_release_train (from scripts).
"""
from __future__ import annotations

import datetime as dt
import io
import json
import unittest
import urllib.error
import urllib.parse
from types import SimpleNamespace
from zoneinfo import ZoneInfo

import approve_release_train as train

UTC = dt.timezone.utc
PT = ZoneInfo("America/Los_Angeles")
NO_SLEEP = lambda _s: None  # noqa: E731


def build(trigger, build_id, commit, create_time, status="SUCCESS", approval=None):
    entry = {
        "id": build_id,
        "createTime": create_time,
        "status": status,
        "substitutions": {"TRIGGER_NAME": trigger, "COMMIT_SHA": commit},
    }
    if approval:
        entry["approval"] = {"state": approval}
    return entry


def pending(trigger, build_id, commit, create_time):
    return build(trigger, build_id, commit, create_time, status="PENDING", approval="PENDING")


class ComputeCutoffTest(unittest.TestCase):
    def test_thursday_run_ships_through_end_of_tuesday(self):
        # Thursday 2026-07-16 23:00 EDT == 20:00 PT; boundary is Wed 07-15 00:00 PT.
        now = dt.datetime(2026, 7, 17, 3, 0, tzinfo=UTC)
        cutoff = train.compute_cutoff(now, "America/Los_Angeles", "tuesday")
        self.assertEqual(cutoff, dt.datetime(2026, 7, 15, 0, 0, tzinfo=PT))

    def test_exactly_at_boundary_uses_that_boundary(self):
        now = dt.datetime(2026, 7, 15, 0, 0, tzinfo=PT)
        cutoff = train.compute_cutoff(now, "America/Los_Angeles", "tuesday")
        self.assertEqual(cutoff, dt.datetime(2026, 7, 15, 0, 0, tzinfo=PT))

    def test_just_before_boundary_uses_previous_week(self):
        now = dt.datetime(2026, 7, 14, 23, 59, tzinfo=PT)
        cutoff = train.compute_cutoff(now, "America/Los_Angeles", "tuesday")
        self.assertEqual(cutoff, dt.datetime(2026, 7, 8, 0, 0, tzinfo=PT))

    def test_dst_spring_forward_week(self):
        # US DST starts Sunday 2026-03-08; the following Wednesday boundary
        # must be 00:00 PDT (UTC-7), not PST.
        now = dt.datetime(2026, 3, 12, 20, 0, tzinfo=PT)
        cutoff = train.compute_cutoff(now, "America/Los_Angeles", "tuesday")
        self.assertEqual(cutoff, dt.datetime(2026, 3, 11, 0, 0, tzinfo=PT))
        self.assertEqual(cutoff.utcoffset(), dt.timedelta(hours=-7))


class PlanQueueTest(unittest.TestCase):
    CUTOFF = dt.datetime(2026, 7, 15, 7, 0, tzinfo=UTC)  # Wed 00:00 PT

    def test_picks_newest_candidate_at_or_before_cutoff(self):
        builds = [
            build("g5-deploy-qa", "dev1", "aaa", "2026-07-13T10:00:00Z"),
            build("g5-deploy-qa", "dev2", "bbb", "2026-07-14T22:00:00Z"),
            build("g5-deploy-qa", "dev3", "ccc", "2026-07-16T09:00:00Z"),
            pending("g5-promote-prod", "p1", "aaa", "2026-07-13T11:00:00Z"),
            pending("g5-promote-prod", "p2", "bbb", "2026-07-14T23:00:00Z"),
            pending("g5-promote-prod", "p3", "ccc", "2026-07-16T10:00:00Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.reason, "advance")
        self.assertEqual(plan.candidate.build_id, "p2")
        # aaa is superseded; ccc is past the cutoff and stays pending.
        self.assertEqual([r.build_id for r in plan.rejects], ["p1"])

    def test_no_candidate_when_all_pending_past_cutoff(self):
        builds = [
            build("g5-deploy-qa", "dev3", "ccc", "2026-07-16T09:00:00Z"),
            pending("g5-promote-prod", "p3", "ccc", "2026-07-16T10:00:00Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.reason, "no_candidate")
        self.assertIsNone(plan.candidate)

    def test_does_not_advance_when_already_shipped(self):
        builds = [
            build("g5-deploy-qa", "dev1", "aaa", "2026-07-13T10:00:00Z"),
            # aaa was already approved (last week's train) and is still the
            # newest merge before the cutoff.
            build("g5-promote-prod", "old", "aaa", "2026-07-13T11:00:00Z",
                  status="SUCCESS", approval="APPROVED"),
            pending("g5-promote-prod", "dup", "aaa", "2026-07-13T11:30:00Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.reason, "does_not_advance")
        self.assertIsNone(plan.candidate)

    def test_approved_origin_aged_out_falls_back_to_create_time(self):
        builds = [
            # Approved build whose g5-deploy-qa origin is no longer listed;
            # its own createTime (07-10) is the conservative shipped point.
            build("g5-promote-prod", "old", "zzz", "2026-07-10T12:00:00Z",
                  status="SUCCESS", approval="APPROVED"),
            build("g5-deploy-qa", "dev2", "bbb", "2026-07-14T22:00:00Z"),
            pending("g5-promote-prod", "p2", "bbb", "2026-07-14T23:00:00Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.reason, "advance")
        self.assertEqual(plan.candidate.build_id, "p2")

    def test_rerun_of_old_commit_does_not_revive_its_pending_build(self):
        # Old commit aaa has a pending prod build from 07-13. Newer commit bbb
        # was approved and shipped 07-13/07-14. aaa's dev build is RE-RUN on
        # Tuesday 07-14 — that origin postdates aaa's prod build, must not
        # count for it, and must not get a rollback approved.
        builds = [
            build("g5-deploy-qa", "devA1", "aaa", "2026-07-13T08:00:00Z"),
            pending("g5-promote-prod", "pA", "aaa", "2026-07-13T09:00:00Z"),
            build("g5-deploy-qa", "devB", "bbb", "2026-07-13T12:00:00Z"),
            build("g5-promote-prod", "pB", "bbb", "2026-07-13T13:00:00Z",
                  status="SUCCESS", approval="APPROVED"),
            # Tuesday re-run of aaa's dev build.
            build("g5-deploy-qa", "devA2", "aaa", "2026-07-14T22:00:00Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.reason, "does_not_advance")
        self.assertIsNone(plan.candidate)

    def test_origin_only_after_creation_is_unresolved(self):
        builds = [
            # The only visible origin for aaa postdates the pending build.
            build("g5-deploy-qa", "devA2", "aaa", "2026-07-14T22:00:00Z"),
            pending("g5-promote-prod", "pA", "aaa", "2026-07-13T09:00:00Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.unresolved_ids, ["pA"])
        self.assertEqual(plan.reason, "no_candidate")

    def test_same_commit_duplicate_pending_is_rejected(self):
        builds = [
            build("g5-deploy-qa", "dev2", "bbb", "2026-07-14T22:00:00Z"),
            pending("g5-promote-prod", "p2a", "bbb", "2026-07-14T23:00:00Z"),
            pending("g5-promote-prod", "p2b", "bbb", "2026-07-14T23:30:00Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.candidate.build_id, "p2b")
        self.assertEqual([r.build_id for r in plan.rejects], ["p2a"])

    def test_unresolved_origin_left_untouched(self):
        builds = [
            build("g5-deploy-qa", "dev2", "bbb", "2026-07-14T22:00:00Z"),
            pending("g5-promote-prod", "ghost", "unknown", "2026-07-01T00:00:00Z"),
            pending("g5-promote-prod", "p2", "bbb", "2026-07-14T23:00:00Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.candidate.build_id, "p2")
        self.assertEqual(plan.unresolved_ids, ["ghost"])
        self.assertEqual(plan.rejects, [])

    def test_fossil_pending_with_unresolved_origin_is_skipped_not_unresolved(self):
        # [D25] decision horizon: a pending older than FOSSIL_PENDING_AGE whose
        # origin has aged out of the list window is a fossil the planner skips,
        # never an unclassifiable-queue halt (2026-07-29 incident: immortal
        # expired pendings are un-rejectable AND un-cancellable).
        builds = [
            build("g5-deploy-qa", "dev2", "bbb", "2026-07-14T22:00:00Z"),
            pending("g5-promote-prod", "relic", "old", "2026-05-27T14:33:00Z"),
            pending("g5-promote-prod", "p2", "bbb", "2026-07-14T23:00:00Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.candidate.build_id, "p2")
        self.assertEqual(plan.unresolved_ids, [])
        self.assertEqual(plan.fossil_ids, ["relic"])
        self.assertEqual(plan.rejects, [])

    def test_fossil_pending_with_resolvable_origin_is_not_rejected(self):
        # A fossil stays out of the superseded-reject hygiene list too:
        # rejecting an expired approval request 400s every week.
        builds = [
            build("g5-deploy-qa", "devold", "old", "2026-05-27T14:00:00Z"),
            pending("g5-promote-prod", "relic", "old", "2026-05-27T14:33:00Z"),
            build("g5-deploy-qa", "dev2", "bbb", "2026-07-14T22:00:00Z"),
            pending("g5-promote-prod", "p2", "bbb", "2026-07-14T23:00:00Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.candidate.build_id, "p2")
        self.assertEqual(plan.fossil_ids, ["relic"])
        self.assertEqual(plan.rejects, [])

    def test_pending_at_horizon_boundary_is_not_a_fossil(self):
        # Exactly FOSSIL_PENDING_AGE before the cutoff is still inside the
        # decision horizon: unresolved (fail closed), not fossil.
        boundary = (self.CUTOFF - train.FOSSIL_PENDING_AGE).strftime("%Y-%m-%dT%H:%M:%SZ")
        builds = [pending("g5-promote-prod", "edge", "eee", boundary)]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.unresolved_ids, ["edge"])
        self.assertEqual(plan.fossil_ids, [])

    def test_nanosecond_create_times_parse(self):
        builds = [
            build("g5-deploy-qa", "dev2", "bbb", "2026-07-14T22:00:00.166213441Z"),
            pending("g5-promote-prod", "p2", "bbb", "2026-07-14T23:00:00.999999999Z"),
        ]
        plan = train.plan_queue(builds, "g5-promote-prod", "g5-deploy-qa", self.CUTOFF)
        self.assertEqual(plan.candidate.build_id, "p2")


class FakeClient:
    """Records decisions; scripted list snapshots and describe statuses.

    `snapshots` is a list of build lists returned by successive list_builds
    calls; the last snapshot repeats once exhausted."""

    def __init__(self, builds=None, statuses=None, snapshots=None):
        self.project = "test-project"
        self.region = "us-central1"
        self.snapshots = list(snapshots) if snapshots is not None else [list(builds or [])]
        self.statuses = list(statuses or [])
        self.decisions = []
        self.list_calls = 0

    def list_builds(self):
        self.list_calls += 1
        if len(self.snapshots) > 1:
            return self.snapshots.pop(0)
        return self.snapshots[0]

    def decide(self, build_id, decision):
        self.decisions.append((build_id, decision))

    def describe_status(self, build_id):
        return self.statuses.pop(0) if self.statuses else "SUCCESS"


def make_args(**overrides):
    args = SimpleNamespace(
        cutoff_day="tuesday",
        cutoff_tz="America/Los_Angeles",
        tf_trigger="g5-terraform-apply-prod",
        tf_origin_trigger="g5-terraform-apply-qa",
        # Chained wait off by default so the queue-mechanics tests stay
        # focused; ChainedApplyTest exercises the chained path explicitly.
        tf_chained_trigger="",
        app_trigger="g5-promote-prod",
        app_origin_trigger="g5-deploy-qa",
        tf_wait_timeout_seconds=1,
        poll_seconds=0,
        dry_run=False,
    )
    for key, value in overrides.items():
        setattr(args, key, value)
    return args


NOW = dt.datetime(2026, 7, 17, 3, 0, tzinfo=UTC)  # Thu 23:00 EDT / 20:00 PT


def both_queue_builds():
    return [
        build("g5-terraform-apply-qa", "tdev", "tf1", "2026-07-14T20:00:00Z"),
        pending("g5-terraform-apply-prod", "tprod", "tf1", "2026-07-14T21:00:00Z"),
        build("g5-deploy-qa", "adev", "app1", "2026-07-14T22:00:00Z"),
        pending("g5-promote-prod", "aprod", "app1", "2026-07-14T23:00:00Z"),
    ]


class RunTrainTest(unittest.TestCase):
    def run_train(self, client, **arg_overrides):
        return train.run_train(make_args(**arg_overrides), client, NOW, sleep=NO_SLEEP)

    def test_terraform_success_then_app_approved(self):
        client = FakeClient(both_queue_builds(), statuses=["WORKING", "SUCCESS"])
        rc = self.run_train(client)
        self.assertEqual(rc, 0)
        self.assertEqual(
            client.decisions, [("tprod", "APPROVED"), ("aprod", "APPROVED")]
        )

    def test_terraform_failure_halts_without_touching_app_queue(self):
        client = FakeClient(both_queue_builds(), statuses=["FAILURE"])
        rc = self.run_train(client)
        self.assertEqual(rc, 1)
        self.assertEqual(client.decisions, [("tprod", "APPROVED")])

    def test_no_terraform_candidate_still_ships_app(self):
        builds = [
            build("g5-deploy-qa", "adev", "app1", "2026-07-14T22:00:00Z"),
            pending("g5-promote-prod", "aprod", "app1", "2026-07-14T23:00:00Z"),
        ]
        client = FakeClient(builds)
        rc = self.run_train(client)
        self.assertEqual(rc, 0)
        self.assertEqual(client.decisions, [("aprod", "APPROVED")])

    def test_dry_run_makes_no_decisions(self):
        client = FakeClient(both_queue_builds())
        rc = self.run_train(client, dry_run=True)
        self.assertEqual(rc, 0)
        self.assertEqual(client.decisions, [])
        self.assertEqual(client.list_calls, 1)

    def test_quiet_week_is_green_noop(self):
        client = FakeClient([])
        rc = self.run_train(client)
        self.assertEqual(rc, 0)
        self.assertEqual(client.decisions, [])

    def test_failed_rejection_is_best_effort_not_fatal(self):
        # An expired-approval fossil that 400s on rejection must not fail the
        # train; the candidate approval and the other rejections still land.
        builds = [
            build("g5-deploy-qa", "adev", "app1", "2026-07-14T22:00:00Z"),
            pending("g5-promote-prod", "aprod", "app1", "2026-07-14T23:00:00Z"),
            build("g5-deploy-qa", "afossil", "old1", "2026-07-13T10:00:00Z"),
            pending("g5-promote-prod", "fossil", "old1", "2026-07-13T11:00:00Z"),
            build("g5-deploy-qa", "adev0", "app0", "2026-07-13T12:00:00Z"),
            pending("g5-promote-prod", "aprod0", "app0", "2026-07-13T13:00:00Z"),
        ]

        class RejectingFossilClient(FakeClient):
            def decide(self, build_id, decision):
                if build_id == "fossil":
                    raise RuntimeError("400 Precondition check failed")
                super().decide(build_id, decision)

        client = RejectingFossilClient(builds)
        rc = train.run_train(make_args(), client, NOW, sleep=NO_SLEEP)
        self.assertEqual(rc, 0)
        self.assertIn(("aprod", "APPROVED"), client.decisions)
        self.assertIn(("aprod0", "REJECTED"), client.decisions)
        self.assertNotIn(("fossil", "REJECTED"), client.decisions)

    def test_candidate_approval_failure_still_raises(self):
        class ApprovalFailingClient(FakeClient):
            def decide(self, build_id, decision):
                if decision == "APPROVED":
                    raise RuntimeError("approval rejected")
                super().decide(build_id, decision)

        builds = [
            build("g5-deploy-qa", "adev", "app1", "2026-07-14T22:00:00Z"),
            pending("g5-promote-prod", "aprod", "app1", "2026-07-14T23:00:00Z"),
        ]
        client = ApprovalFailingClient(builds)
        with self.assertRaises(RuntimeError):
            train.run_train(make_args(), client, NOW, sleep=NO_SLEEP)

    def test_superseded_pending_rejected_after_approval(self):
        builds = both_queue_builds() + [
            build("g5-deploy-qa", "adev0", "app0", "2026-07-13T10:00:00Z"),
            pending("g5-promote-prod", "aprod0", "app0", "2026-07-13T11:00:00Z"),
        ]
        client = FakeClient(builds, statuses=["SUCCESS"])
        rc = self.run_train(client)
        self.assertEqual(rc, 0)
        self.assertIn(("aprod0", "REJECTED"), client.decisions)
        # Rejection happens only after the candidate approval.
        self.assertLess(
            client.decisions.index(("aprod", "APPROVED")),
            client.decisions.index(("aprod0", "REJECTED")),
        )

    def test_unresolved_terraform_queue_halts_before_any_approval(self):
        builds = both_queue_builds() + [
            pending("g5-terraform-apply-prod", "tghost", "unknown", "2026-07-01T00:00:00Z"),
        ]
        client = FakeClient(builds)
        rc = self.run_train(client)
        self.assertEqual(rc, 1)
        self.assertEqual(client.decisions, [])

    def test_unresolved_app_queue_does_not_block(self):
        builds = [
            build("g5-deploy-qa", "adev", "app1", "2026-07-14T22:00:00Z"),
            pending("g5-promote-prod", "aprod", "app1", "2026-07-14T23:00:00Z"),
            pending("g5-promote-prod", "aghost", "unknown", "2026-07-01T00:00:00Z"),
        ]
        client = FakeClient(builds)
        rc = self.run_train(client)
        self.assertEqual(rc, 0)
        self.assertEqual(client.decisions, [("aprod", "APPROVED")])

    def test_full_list_window_fails_closed(self):
        builds = both_queue_builds()
        original = train.LIST_LIMIT
        train.LIST_LIMIT = len(builds)
        try:
            client = FakeClient(builds)
            rc = self.run_train(client)
        finally:
            train.LIST_LIMIT = original
        self.assertEqual(rc, 1)
        self.assertEqual(client.decisions, [])

    def test_fossil_terraform_pending_does_not_halt_train(self):
        # 2026-07-29 incident: immortal expired pendings (un-rejectable 400
        # FAILED_PRECONDITION, cancel silently no-ops) must not hold the
        # weekly train red once their origins age out of the list window.
        builds = both_queue_builds() + [
            pending("g5-terraform-apply-prod", "trelic", "unknown", "2026-05-27T14:33:00Z"),
        ]
        client = FakeClient(builds, statuses=["SUCCESS"])
        rc = self.run_train(client)
        self.assertEqual(rc, 0)
        self.assertEqual(
            client.decisions, [("tprod", "APPROVED"), ("aprod", "APPROVED")]
        )

    def test_full_window_covering_horizon_proceeds(self):
        # A full list window whose oldest build predates the decision horizon
        # covers all decidable history: proceed loudly, not red — total queue
        # history now exceeds any fixed window ([D25]).
        builds = [
            build("g5-deploy-qa", "ancient", "zzz", "2026-05-01T00:00:00Z")
        ] + both_queue_builds()
        original = train.LIST_LIMIT
        train.LIST_LIMIT = len(builds)
        try:
            client = FakeClient(builds, statuses=["SUCCESS"])
            rc = self.run_train(client)
        finally:
            train.LIST_LIMIT = original
        self.assertEqual(rc, 0)
        self.assertIn(("tprod", "APPROVED"), client.decisions)

    def test_unstable_plan_fails_closed(self):
        # The terraform candidate flickers between consecutive fetches
        # (e.g. a just-approved newer build surfacing): refuse to act.
        first = both_queue_builds()
        second = both_queue_builds() + [
            build("g5-terraform-apply-qa", "tdev2", "tf2", "2026-07-14T22:30:00Z"),
            pending("g5-terraform-apply-prod", "tprod2", "tf2", "2026-07-14T23:30:00Z"),
        ]
        client = FakeClient(snapshots=[first, second])
        rc = self.run_train(client)
        self.assertEqual(rc, 1)
        self.assertEqual(client.decisions, [])

    def test_app_plan_recomputed_after_terraform_wait(self):
        # During the terraform wait a human rejects nothing but a NEW eligible
        # app promote (pre-cutoff origin) surfaces; the fresh post-wait plan
        # must pick it and supersede the older one.
        pre_wait = both_queue_builds()
        post_wait = both_queue_builds() + [
            build("g5-deploy-qa", "adev2", "app2", "2026-07-14T23:30:00Z"),
            pending("g5-promote-prod", "aprod2", "app2", "2026-07-14T23:45:00Z"),
        ]
        # tf confirmation = 3 fetches of pre_wait; app planning fetches see
        # post_wait.
        client = FakeClient(
            snapshots=[pre_wait, pre_wait, pre_wait, post_wait],
            statuses=["SUCCESS"],
        )
        rc = self.run_train(client)
        self.assertEqual(rc, 0)
        self.assertIn(("aprod2", "APPROVED"), client.decisions)
        self.assertIn(("aprod", "REJECTED"), client.decisions)
        self.assertNotIn(("aprod", "APPROVED"), client.decisions)


def staging_gate_builds(with_chained=True):
    """Post gate-move fixture: staging queues are approval-gated; the chained
    prod apply (no approval field) appears after the staging pending build."""
    builds = [
        build("g5-terraform-apply-qa", "tdev", "tf1", "2026-07-14T20:00:00Z"),
        pending("g5-terraform-apply-staging", "tstag", "tf1", "2026-07-14T21:00:00Z"),
        build("g5-deploy-qa", "adev", "app1", "2026-07-14T22:00:00Z"),
        pending("g5-promote-staging", "astag", "app1", "2026-07-14T23:00:00Z"),
    ]
    if with_chained:
        builds.append(
            build("g5-terraform-apply-prod", "tchain", "tf1",
                  "2026-07-16T04:00:00Z", status="WORKING")
        )
    return builds


class ChainedApplyTest(unittest.TestCase):
    """[D25] gate-move sequencing: after the staging apply succeeds, the train
    resolves and walks the auto-chained prod apply before the app queue."""

    ARGS = dict(
        tf_trigger="g5-terraform-apply-staging",
        tf_chained_trigger="g5-terraform-apply-prod",
        app_trigger="g5-promote-staging",
    )

    def run_train(self, client, **overrides):
        return train.run_train(
            make_args(**{**self.ARGS, **overrides}), client, NOW, sleep=NO_SLEEP
        )

    def test_chained_success_then_app_approved(self):
        # describe_status pops: staging apply SUCCESS, chained apply SUCCESS.
        client = FakeClient(staging_gate_builds(), statuses=["SUCCESS", "SUCCESS"])
        rc = self.run_train(client)
        self.assertEqual(rc, 0)
        self.assertEqual(
            client.decisions, [("tstag", "APPROVED"), ("astag", "APPROVED")]
        )

    def test_chained_failure_halts_without_touching_app_queue(self):
        client = FakeClient(staging_gate_builds(), statuses=["SUCCESS", "FAILURE"])
        rc = self.run_train(client)
        self.assertEqual(rc, 1)
        self.assertEqual(client.decisions, [("tstag", "APPROVED")])

    def test_chained_build_never_appearing_halts_red(self):
        client = FakeClient(staging_gate_builds(with_chained=False),
                            statuses=["SUCCESS"])
        rc = self.run_train(client)
        self.assertEqual(rc, 1)
        self.assertEqual(client.decisions, [("tstag", "APPROVED")])

    def test_no_terraform_candidate_skips_chained_wait(self):
        builds = [
            build("g5-deploy-qa", "adev", "app1", "2026-07-14T22:00:00Z"),
            pending("g5-promote-staging", "astag", "app1", "2026-07-14T23:00:00Z"),
        ]
        client = FakeClient(builds)
        rc = self.run_train(client)
        self.assertEqual(rc, 0)
        self.assertEqual(client.decisions, [("astag", "APPROVED")])


class FindChainedBuildTest(unittest.TestCase):
    AFTER = dt.datetime(2026, 7, 14, 21, 0, tzinfo=UTC)

    def test_picks_newest_matching_after_bound(self):
        builds = [
            build("g5-terraform-apply-prod", "old", "tf1", "2026-07-14T20:00:00Z"),
            build("g5-terraform-apply-prod", "newer", "tf1", "2026-07-16T04:00:00Z",
                  status="WORKING"),
            build("g5-terraform-apply-prod", "newest", "tf1", "2026-07-16T05:00:00Z",
                  status="QUEUED"),
        ]
        self.assertEqual(
            train.find_chained_build(builds, "g5-terraform-apply-prod", "tf1", self.AFTER),
            "newest",
        )

    def test_ignores_pending_approval_fossils(self):
        # A pre-gate-move fossil still PENDING in the old prod approval queue
        # must never be mistaken for the freshly auto-chained apply.
        builds = [
            pending("g5-terraform-apply-prod", "fossil", "tf1", "2026-07-16T04:00:00Z"),
        ]
        self.assertEqual(
            train.find_chained_build(builds, "g5-terraform-apply-prod", "tf1", self.AFTER),
            "",
        )

    def test_ignores_other_commits_and_triggers(self):
        builds = [
            build("g5-terraform-apply-prod", "other", "tf2", "2026-07-16T04:00:00Z"),
            build("g5-terraform-apply-staging", "wrongq", "tf1", "2026-07-16T04:00:00Z"),
        ]
        self.assertEqual(
            train.find_chained_build(builds, "g5-terraform-apply-prod", "tf1", self.AFTER),
            "",
        )


class WaitTerminalTest(unittest.TestCase):
    def test_returns_terminal_status(self):
        client = FakeClient([], statuses=["QUEUED", "WORKING", "SUCCESS"])
        status = train.wait_terminal(client, "b1", timeout_seconds=100,
                                     poll_seconds=0, sleep=NO_SLEEP)
        self.assertEqual(status, "SUCCESS")

    def test_timeout_returns_empty(self):
        client = FakeClient([], statuses=["WORKING"] * 50)
        clock = {"t": 0.0}

        def monotonic():
            return clock["t"]

        def sleep(seconds):
            clock["t"] += max(seconds, 1)

        status = train.wait_terminal(client, "b1", timeout_seconds=3,
                                     poll_seconds=1, sleep=sleep, monotonic=monotonic)
        self.assertEqual(status, "")

    def test_transient_describe_failure_is_retried(self):
        client = FakeClient([], statuses=["", "FAILURE"])
        status = train.wait_terminal(client, "b1", timeout_seconds=100,
                                     poll_seconds=0, sleep=NO_SLEEP)
        self.assertEqual(status, "FAILURE")


class FakeResponse:
    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def read(self):
        return b"{}"


class JsonResponse(FakeResponse):
    def __init__(self, payload):
        self._payload = payload

    def read(self):
        return json.dumps(self._payload).encode("utf-8")


def fake_run_ok(cmd, capture_output, text, timeout=None):
    if cmd[:3] == ["gcloud", "auth", "print-access-token"]:
        return SimpleNamespace(returncode=0, stdout="fake-token\n", stderr="")
    return SimpleNamespace(returncode=0, stdout="", stderr="")


class DecideRestTest(unittest.TestCase):
    def test_decide_posts_regional_approve_payload(self):
        captured = {}

        def opener(request, timeout=None):
            captured["url"] = request.full_url
            captured["body"] = request.data
            captured["method"] = request.get_method()
            captured["auth"] = request.get_header("Authorization")
            captured["timeout"] = timeout
            return FakeResponse()

        client = train.CloudBuildClient(
            "g5-dev-1ccc", "us-central1", run=fake_run_ok, opener=opener, sleep=NO_SLEEP
        )
        client.decide("abc-123", "APPROVED")
        self.assertEqual(
            captured["url"],
            "https://cloudbuild.googleapis.com/v1/projects/g5-dev-1ccc/"
            "locations/us-central1/builds/abc-123:approve",
        )
        self.assertEqual(captured["method"], "POST")
        self.assertEqual(captured["auth"], "Bearer fake-token")
        # Every decision carries the identifying release-train comment ([D25]
        # attributable decisions): approverAccount is unreliable downstream,
        # so the comment is what notification surfaces fall back to.
        body = json.loads(captured["body"])
        self.assertEqual(body["approvalResult"]["decision"], "APPROVED")
        self.assertEqual(
            body["approvalResult"]["comment"],
            train.DECISION_COMMENTS["APPROVED"],
        )
        self.assertIn("g5-release-train", body["approvalResult"]["comment"])
        self.assertIn("service account", body["approvalResult"]["comment"])
        self.assertEqual(captured["timeout"], train.DECIDE_REQUEST_TIMEOUT_SECONDS)

    def http_error(self, code):
        return urllib.error.HTTPError(
            "https://cloudbuild.googleapis.com", code, "err", None,
            io.BytesIO(b'{"error": {"status": "FAILED_PRECONDITION"}}'),
        )

    def state_dispatch_opener(self, state):
        """POSTs (:approve) raise 400; GETs (approval-state cross-check)
        return the given current state."""

        def opener(request, timeout=None):
            if request.get_method() == "POST":
                raise self.http_error(400)
            return JsonResponse({"approval": {"state": state}})

        return opener

    def test_ambiguous_4xx_with_matching_state_is_success(self):
        # First POST landed but its response was lost; the retry gets a 400
        # "not pending". The current approval state matches the requested
        # decision, so the train must treat it as applied.
        client = train.CloudBuildClient(
            "p", "r", run=fake_run_ok,
            opener=self.state_dispatch_opener("APPROVED"), sleep=NO_SLEEP,
        )
        client.decide("abc", "APPROVED")  # must not raise

    def test_4xx_with_unreadable_state_is_retried_not_conflict(self):
        # POST gets a 400, the reconciliation GET fails transiently (state
        # unknown), the retried POST then succeeds — the decision must land
        # without raising instead of failing mid-mutation on an unknown state.
        calls = {"post": 0}

        def opener(request, timeout=None):
            if request.get_method() == "POST":
                calls["post"] += 1
                if calls["post"] == 1:
                    raise self.http_error(400)
                return FakeResponse()
            raise urllib.error.URLError("reconciliation read blip")

        client = train.CloudBuildClient(
            "p", "r", run=fake_run_ok, opener=opener, sleep=NO_SLEEP
        )
        client.decide("abc", "APPROVED")  # must not raise
        self.assertEqual(calls["post"], 2)

    def test_4xx_with_conflicting_state_raises(self):
        client = train.CloudBuildClient(
            "p", "r", run=fake_run_ok,
            opener=self.state_dispatch_opener("REJECTED"), sleep=NO_SLEEP,
        )
        with self.assertRaises(RuntimeError):
            client.decide("abc", "APPROVED")

    def test_429_is_retried_then_succeeds(self):
        attempts = {"n": 0}

        def opener(request, timeout=None):
            attempts["n"] += 1
            if attempts["n"] == 1:
                raise self.http_error(429)
            return FakeResponse()

        client = train.CloudBuildClient(
            "p", "r", run=fake_run_ok, opener=opener, sleep=NO_SLEEP
        )
        client.decide("abc", "APPROVED")
        self.assertEqual(attempts["n"], 2)

    def test_list_builds_rest_query_is_scoped_and_windowed(self):
        captured = {}

        def opener(request, timeout=None):
            captured["url"] = request.full_url
            return JsonResponse({"builds": []})

        client = train.CloudBuildClient(
            "p", "r", trigger_names=["tf-prod", "tf-dev", "app-prod", "app-dev"],
            run=fake_run_ok, opener=opener, sleep=NO_SLEEP,
        )
        self.assertEqual(client.list_builds(), [])
        query = dict(
            urllib.parse.parse_qsl(urllib.parse.urlsplit(captured["url"]).query)
        )
        self.assertEqual(
            query["filter"],
            'substitutions.TRIGGER_NAME="tf-prod" OR '
            'substitutions.TRIGGER_NAME="tf-dev" OR '
            'substitutions.TRIGGER_NAME="app-prod" OR '
            'substitutions.TRIGGER_NAME="app-dev"',
        )
        # Pin the widened window: a silent regression to a smaller limit
        # re-opens the day-one saturated-window fail-closed trap.
        self.assertEqual(train.LIST_LIMIT, 2000)
        self.assertEqual(query["pageSize"], str(train.LIST_PAGE_SIZE))
        # The EXACT field mask matters: dropping nextPageToken from a partial
        # response silently ends pagination at page one (1000 < LIST_LIMIT,
        # so truncation would not even fail closed).
        self.assertEqual(
            query["fields"],
            "builds(id,createTime,status,substitutions,approval.state),nextPageToken",
        )

    def test_list_builds_paginates_until_window_or_end(self):
        pages = [
            {"builds": [{"id": "a"}], "nextPageToken": "t1"},
            {"builds": [{"id": "b"}]},
        ]
        calls = {"n": 0}

        def opener(request, timeout=None):
            page = pages[calls["n"]]
            calls["n"] += 1
            if calls["n"] == 2:
                assert "pageToken=t1" in request.full_url
            return JsonResponse(page)

        client = train.CloudBuildClient(
            "p", "r", run=fake_run_ok, opener=opener, sleep=NO_SLEEP
        )
        self.assertEqual([b["id"] for b in client.list_builds()], ["a", "b"])
        self.assertEqual(calls["n"], 2)

    def test_page_two_failure_restarts_the_whole_read(self):
        # A transient failure mid-pagination must restart from page one, not
        # splice attempt N's page one with attempt N+1's page two.
        requests = []

        def opener(request, timeout=None):
            requests.append(request.full_url)
            if len(requests) == 1:
                return JsonResponse({"builds": [{"id": "a"}], "nextPageToken": "t1"})
            if len(requests) == 2:
                raise urllib.error.URLError("page-two blip")
            if len(requests) == 3:
                assert "pageToken" not in request.full_url
                return JsonResponse({"builds": [{"id": "a"}], "nextPageToken": "t1"})
            return JsonResponse({"builds": [{"id": "b"}]})

        client = train.CloudBuildClient(
            "p", "r", run=fake_run_ok, opener=opener, sleep=NO_SLEEP
        )
        self.assertEqual([b["id"] for b in client.list_builds()], ["a", "b"])
        self.assertEqual(len(requests), 4)

    def test_401_invalidates_cached_token_and_retries(self):
        mints = {"n": 0}

        def run(cmd, capture_output, text, timeout=None):
            if cmd[:3] == ["gcloud", "auth", "print-access-token"]:
                mints["n"] += 1
                return SimpleNamespace(returncode=0, stdout=f"token-{mints['n']}\n", stderr="")
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        seen_tokens = []

        def opener(request, timeout=None):
            seen_tokens.append(request.get_header("Authorization"))
            if len(seen_tokens) == 1:
                raise urllib.error.HTTPError(request.full_url, 401, "unauth", None,
                                             io.BytesIO(b"{}"))
            return JsonResponse({"builds": []})

        client = train.CloudBuildClient("p", "r", run=run, opener=opener, sleep=NO_SLEEP)
        self.assertEqual(client.list_builds(), [])
        self.assertEqual(mints["n"], 2)
        self.assertEqual(seen_tokens, ["Bearer token-1", "Bearer token-2"])

    def test_permanent_get_rejection_does_not_wait_forever(self):
        # A 404 on the build read must surface, not read as "still running"
        # for the full multi-hour wait budget.
        def opener(request, timeout=None):
            raise urllib.error.HTTPError(request.full_url, 404, "gone", None,
                                         io.BytesIO(b"{}"))

        client = train.CloudBuildClient("p", "r", run=fake_run_ok, opener=opener, sleep=NO_SLEEP)
        with self.assertRaises(RuntimeError):
            client.describe_status("dead-build")

    def test_unreadable_polls_exhaust_wait_budget_early(self):
        # Raises (not "") so the caller reports a read outage, not a timeout.
        client = FakeClient([], statuses=[""] * (train.WAIT_UNKNOWN_BUDGET + 5))
        with self.assertRaises(RuntimeError):
            train.wait_terminal(client, "b1", timeout_seconds=10_000,
                                poll_seconds=0, sleep=NO_SLEEP)

    def test_token_mint_timeout_is_bounded_and_retryable(self):
        import subprocess as sp
        calls = {"n": 0}

        def run(cmd, capture_output, text, timeout=None):
            assert timeout == train.TOKEN_MINT_TIMEOUT_SECONDS
            calls["n"] += 1
            if calls["n"] == 1:
                raise sp.TimeoutExpired(cmd, timeout)
            return SimpleNamespace(returncode=0, stdout="tok\n", stderr="")

        client = train.CloudBuildClient(
            "p", "r", run=run, opener=lambda req, timeout=None: FakeResponse(), sleep=NO_SLEEP
        )
        client.decide("abc", "APPROVED")  # first mint times out, retry lands
        self.assertEqual(calls["n"], 2)

    def test_fetch_and_plan_full_window_boundary(self):
        # Exactly LIST_LIMIT entries must fail closed; one below must plan.
        cutoff = dt.datetime(2026, 7, 15, 7, 0, tzinfo=UTC)
        builds = [
            build("g5-deploy-qa", f"dev{i}", f"c{i}", "2026-07-14T22:00:00Z")
            for i in range(3)
        ]
        original = train.LIST_LIMIT
        train.LIST_LIMIT = len(builds)
        try:
            with self.assertRaises(RuntimeError):
                train.fetch_and_plan(FakeClient(builds), "g5-promote-prod",
                                     "g5-deploy-qa", cutoff)
            plan = train.fetch_and_plan(FakeClient(builds[:-1]), "g5-promote-prod",
                                        "g5-deploy-qa", cutoff)
            self.assertEqual(plan.reason, "no_candidate")
        finally:
            train.LIST_LIMIT = original

    def test_transient_token_failure_is_retried(self):
        calls = {"token": 0}

        def run(cmd, capture_output, text, timeout=None):
            if cmd[:3] == ["gcloud", "auth", "print-access-token"]:
                calls["token"] += 1
                if calls["token"] == 1:
                    return SimpleNamespace(returncode=1, stdout="", stderr="metadata blip")
                return SimpleNamespace(returncode=0, stdout="fake-token\n", stderr="")
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        client = train.CloudBuildClient(
            "p", "r", run=run, opener=lambda req, timeout=None: FakeResponse(), sleep=NO_SLEEP
        )
        client.decide("abc", "APPROVED")
        self.assertEqual(calls["token"], 2)


class IndependentQueueRevisionsTest(unittest.TestCase):
    """G5 selects app and Terraform revisions independently."""

    def test_app_only_revision_ships_after_previous_infrastructure_release(self):
        builds = [
            build("g5-terraform-apply-qa", "infra-qa", "infra-old", "2026-07-07T20:00:00Z"),
            build("g5-terraform-apply-staging", "infra-stage", "infra-old",
                  "2026-07-07T21:00:00Z", approval="APPROVED"),
            build("g5-terraform-apply-prod", "infra-prod", "infra-old", "2026-07-07T22:00:00Z"),
            build("g5-deploy-qa", "app-qa", "app-new", "2026-07-14T20:00:00Z"),
            pending("g5-promote-staging", "app-stage", "app-new", "2026-07-14T21:00:00Z"),
        ]
        args = make_args(tf_trigger="g5-terraform-apply-staging",
                         tf_chained_trigger="g5-terraform-apply-prod", app_trigger="g5-promote-staging")
        client = FakeClient(builds)
        result = train.run_train(args, client, NOW, sleep=NO_SLEEP)
        self.assertEqual(result, 0)
        self.assertEqual(client.decisions, [("app-stage", "APPROVED")])


if __name__ == "__main__":
    unittest.main()
