#!/usr/bin/env python3
"""Synthetic coverage for the Lean cutoff notification and train integration."""
from __future__ import annotations

import datetime as dt
import io
import json
import pathlib
import unittest
from types import SimpleNamespace
from unittest import mock
from zoneinfo import ZoneInfo

import notify_deploy_cutoff as notify
from approve_release_train import PendingBuild, QueuePlan
from check_deploy_freshness import parse_time

UTC = dt.timezone.utc

C1, C2, C3, C4, C5 = "1" * 40, "2" * 40, "3" * 40, "4" * 40, "5" * 40
# Wednesday 2026-07-15 00:00 America/Los_Angeles == 07:00Z: the cutoff instant.
NOW = "2026-07-15T07:00:30Z"
CUTOFF = "2026-07-15T07:00:00Z"


def build(trigger, commit, create_time, *, status="SUCCESS", approval=None, build_id=None):
    entry = {
        "id": build_id or f"{trigger}-{commit[:4]}-{create_time}",
        "status": status,
        "createTime": create_time,
        "substitutions": {"TRIGGER_NAME": trigger, "COMMIT_SHA": commit},
    }
    if approval:
        entry["approval"] = {"state": approval}
    return entry


def deployed_c1():
    """c1 merged, promoted, and deployed the previous week."""
    return [
        build("g5-deploy-qa", C1, "2026-07-08T12:00:00Z"),
        build("g5-promote-prod", C1, "2026-07-09T20:00:00Z",
              status="SUCCESS", approval="APPROVED", build_id="deployed"),
    ]


def world():
    """c1 deployed; c2/c3 merged before the cutoff with c3's promote pending."""
    return deployed_c1() + [
        build("g5-deploy-qa", C2, "2026-07-13T18:00:00Z"),
        build("g5-promote-prod", C2, "2026-07-13T19:00:00Z",
              status="PENDING", approval="PENDING"),
        build("g5-deploy-qa", C3, "2026-07-14T22:00:00Z"),
        build("g5-promote-prod", C3, "2026-07-14T23:00:00Z",
              status="PENDING", approval="PENDING", build_id="candidate"),
    ]


def staging_world():
    """The same commits with approvals queued at Lean's staging gate."""
    builds = world()
    for entry in builds:
        if entry["status"] == "PENDING":
            entry["substitutions"]["TRIGGER_NAME"] = "g5-promote-staging"
    return builds


def pending(origin_iso, build_id, commit):
    return PendingBuild(parse_time(origin_iso), build_id, commit)


class FakeResponse:
    def __init__(self, payload):
        self._payload = payload

    def read(self):
        return json.dumps(self._payload).encode("utf-8")

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class FakeOpener:
    """Serves the GitHub compare/pulls reads and records Slack posts."""

    def __init__(self, compare=None, pulls_by_sha=None, forbid_post=False):
        self.compare = compare if compare is not None else {"commits": [], "total_commits": 0}
        self.pulls_by_sha = pulls_by_sha or {}
        self.forbid_post = forbid_post
        self.posted = []
        self.requested_urls = []

    def __call__(self, request, timeout=None):
        url = request.full_url
        self.requested_urls.append(url)
        if "hooks.slack" in url:
            if self.forbid_post:
                raise AssertionError("webhook must not be posted in this scenario")
            self.posted.append(json.loads(request.data.decode("utf-8")))
            return FakeResponse({})
        if "/compare/" in url:
            return FakeResponse(self.compare)
        if url.endswith("/pulls"):
            sha = url.rsplit("/", 2)[-2]
            return FakeResponse(self.pulls_by_sha.get(sha, []))
        raise AssertionError(f"unexpected URL fetched: {url}")


def compare_payload(status="ahead"):
    commits = [
        {"sha": C2, "commit": {"message": "feat: two (#101)",
                               "author": {"name": "ann"},
                               "committer": {"date": "2026-07-13T17:58:00Z"}},
         "author": {"login": "ann"}},
        {"sha": C3, "commit": {"message": "fix: three (#102)",
                               "author": {"name": "bob"},
                               "committer": {"date": "2026-07-14T21:58:00Z"}},
         "author": {"login": "bob"}},
    ]
    pulls = {
        C2: [{"number": 101, "title": "feat: two", "user": {"login": "ann"},
              "merged_at": "2026-07-13T17:58:00Z", "base": {"ref": "main"}}],
        C3: [{"number": 102, "title": "fix: three", "user": {"login": "bob"},
              "merged_at": "2026-07-14T21:58:00Z", "base": {"ref": "main"}}],
    }
    return {"commits": commits, "total_commits": 2, "status": status}, pulls


def make_args(**overrides):
    defaults = dict(
        cutoff_day="tuesday", cutoff_tz="America/Los_Angeles",
        deploy_day="thursday", deploy_time="23:00", deploy_tz="America/New_York",
        tf_trigger="g5-terraform-apply-prod", tf_origin_trigger="g5-terraform-apply-qa",
        app_trigger="g5-promote-prod", app_origin_trigger="g5-deploy-qa",
        # Same trigger for predicted queue and deployed base in the legacy
        # fixtures; the gate-move split is exercised explicitly below.
        deployed_trigger="g5-promote-prod",
        github_owner="G5Labs", github_repo="G5-Lean", github_base_branch="main",
        github_connection="G5labs-G5-QA", github_connection_repository="G5Labs-G5-Lean",
        webhook_secret="g5-cloudbuild-slack-webhook", webhook_version="1", dry_run=False,
    )
    defaults.update(overrides)
    return SimpleNamespace(**defaults)


def fake_clients(builds, *, secret="https://hooks.slack.example/T000/B000/x", token="gh-token"):
    cb = SimpleNamespace(list_builds=lambda: builds)

    def access_secret(_name, _version):
        if isinstance(secret, Exception):
            raise secret
        return secret

    def github_read_token(_connection, _repository):
        if isinstance(token, Exception):
            raise token
        return token

    gcloud = SimpleNamespace(access_secret=access_secret, github_read_token=github_read_token)
    return cb, gcloud


class NextDeployTimeTest(unittest.TestCase):
    def test_cutoff_firing_predicts_next_thursday_2300_eastern(self):
        deploy_at = notify.next_deploy_time(
            parse_time(NOW), "thursday", "23:00", "America/New_York"
        )
        self.assertEqual(
            deploy_at,
            dt.datetime(2026, 7, 16, 23, 0, tzinfo=ZoneInfo("America/New_York")),
        )

    def test_exactly_at_the_deploy_instant_rolls_to_next_week(self):
        at_slot = dt.datetime(2026, 7, 16, 23, 0, tzinfo=ZoneInfo("America/New_York"))
        deploy_at = notify.next_deploy_time(at_slot, "thursday", "23:00", "America/New_York")
        self.assertEqual(
            deploy_at,
            dt.datetime(2026, 7, 23, 23, 0, tzinfo=ZoneInfo("America/New_York")),
        )

    def test_invalid_deploy_time_raises(self):
        with self.assertRaises(RuntimeError):
            notify.next_deploy_time(parse_time(NOW), "thursday", "eleven pm", "America/New_York")

    def test_spring_forward_gap_slot_raises(self):
        # US DST 2026 begins Sunday March 8; 02:30 does not exist that night.
        with self.assertRaises(RuntimeError):
            notify.next_deploy_time(
                parse_time("2026-03-06T12:00:00Z"), "sunday", "02:30", "America/New_York"
            )

    def test_fall_back_ambiguous_slot_uses_earlier_fold(self):
        # US DST 2026 ends Sunday November 1; 01:30 occurs twice — the earlier
        # (EDT, UTC-4) instant is chosen.
        deploy_at = notify.next_deploy_time(
            parse_time("2026-10-30T12:00:00Z"), "sunday", "01:30", "America/New_York"
        )
        self.assertEqual(deploy_at.utcoffset(), dt.timedelta(hours=-4))


class PredictedHeadTest(unittest.TestCase):
    def test_app_candidate_preferred_even_when_terraform_origin_is_newer(self):
        # Origin createTime order is not git-ancestry order: a re-run of an
        # older commit's terraform dev build must not displace the app head.
        app = QueuePlan("g5-promote-prod", "advance",
                        pending("2026-07-13T18:00:00Z", "b-app", C2))
        tf = QueuePlan("g5-terraform-apply-prod", "advance",
                       pending("2026-07-14T22:00:00Z", "b-tf", C3))
        self.assertEqual(notify.predicted_head(app, tf).commit, C2)

    def test_terraform_only_week_falls_back_to_tf_candidate(self):
        app = QueuePlan("g5-promote-prod", "does_not_advance")
        tf = QueuePlan("g5-terraform-apply-prod", "advance",
                       pending("2026-07-14T22:00:00Z", "b-tf", C3))
        self.assertEqual(notify.predicted_head(app, tf).build_id, "b-tf")

    def test_no_advancing_queue_is_none(self):
        self.assertIsNone(notify.predicted_head(
            QueuePlan("g5-promote-prod", "does_not_advance"),
            QueuePlan("g5-terraform-apply-prod", "no_candidate"),
        ))


class ProductionBaselineTest(unittest.TestCase):
    def choose(self, builds):
        return notify.production_baseline(builds, "g5-promote-prod", "g5-deploy-qa")

    def test_older_unknown_origin_cannot_compete_with_known_baseline(self):
        for created in ["2026-07-08T11:00:00Z", "2026-07-08T12:00:00Z"]:
            with self.subTest(created=created):
                baseline, notes = self.choose(deployed_c1() + [
                    build("g5-promote-prod", C4, created),
                ])
                self.assertEqual(baseline["substitutions"]["COMMIT_SHA"], C1)
                self.assertEqual(notes, [])

    def test_missing_origin_without_a_known_baseline_is_uncertain(self):
        baseline, notes = self.choose([
            build("g5-promote-prod", C4, "2026-07-14T20:00:00Z"),
        ])
        self.assertIsNone(baseline)
        self.assertIn("production baseline uncertain", " ".join(notes))

    def test_equal_newest_origins_for_different_commits_are_uncertain(self):
        baseline, notes = self.choose(deployed_c1() + [
            build("g5-deploy-qa", C2, "2026-07-13T18:00:00Z"),
            build("g5-deploy-qa", C4, "2026-07-13T18:00:00Z"),
            build("g5-promote-prod", C2, "2026-07-14T20:00:00Z"),
            build("g5-promote-prod", C4, "2026-07-14T21:00:00Z"),
        ])
        self.assertIsNone(baseline)
        self.assertIn("different commits share", " ".join(notes))

    def test_same_commit_duplicates_and_older_ties_do_not_obscure_newest_origin(self):
        baseline, notes = self.choose(deployed_c1() + [
            build("g5-deploy-qa", C4, "2026-07-08T12:00:00Z"),
            build("g5-promote-prod", C4, "2026-07-09T21:00:00Z"),
            build("g5-deploy-qa", C2, "2026-07-13T18:00:00Z"),
            build("g5-promote-prod", C2, "2026-07-14T20:00:00Z"),
            build("g5-promote-prod", C2, "2026-07-14T21:00:00Z"),
        ])
        self.assertEqual(baseline["substitutions"]["COMMIT_SHA"], C2)
        self.assertEqual(notes, [])

    def test_malformed_records_cannot_supply_a_baseline(self):
        malformed = [
            build("g5-promote-prod", "", "2026-07-14T20:00:00Z"),
            build("g5-promote-prod", C4, "invalid timestamp"),
        ]
        baseline, notes = self.choose(malformed)
        self.assertIsNone(baseline)
        self.assertIn("2 malformed", " ".join(notes))
        baseline, _ = self.choose(deployed_c1() + malformed)
        self.assertEqual(baseline["substitutions"]["COMMIT_SHA"], C1)

    def test_non_successful_prod_and_other_triggers_are_not_baselines(self):
        for status in ["PENDING", "WORKING", "FAILURE", "CANCELLED"]:
            with self.subTest(status=status):
                baseline, notes = self.choose([
                    build("g5-deploy-qa", C2, "2026-07-13T18:00:00Z"),
                    build("g5-promote-prod", C2, "2026-07-14T20:00:00Z", status=status),
                    build("g5-promote-staging", C2, "2026-07-14T20:00:00Z"),
                ])
                self.assertIsNone(baseline)
                self.assertEqual(notes, [])


class QueueUnrepresentedTailTest(unittest.TestCase):
    def tail(self, builds, candidate=None):
        return notify.queue_unrepresented_tail(
            builds, "g5-promote-prod", "g5-deploy-qa", parse_time(CUTOFF), candidate
        )

    def test_failed_and_post_cutoff_builds_are_ignored(self):
        builds = world() + [
            build("g5-deploy-qa", C4, "2026-07-15T06:50:00Z", status="FAILURE"),
            build("g5-deploy-qa", C4, "2026-07-15T07:10:00Z", status="WORKING"),
        ]
        candidate = pending("2026-07-14T22:00:00Z", "candidate", C3)
        self.assertEqual(self.tail(builds, candidate), "")

    def test_in_flight_pre_cutoff_merge_newer_than_candidate_is_a_tail(self):
        builds = world() + [
            build("g5-deploy-qa", C4, "2026-07-15T06:50:00Z", status="WORKING"),
        ]
        candidate = pending("2026-07-14T22:00:00Z", "candidate", C3)
        self.assertEqual(self.tail(builds, candidate), C4)

    def test_origin_older_than_shipped_point_is_not_a_tail(self):
        # The deployed c1's own origin must never read as "still in the dev
        # pipeline" — it is the queue's shipped point.
        self.assertEqual(self.tail(deployed_c1(), None), "")


class RunNotifyTest(unittest.TestCase):
    def run_notify(self, builds, opener, *, token="gh-token", **arg_overrides):
        args = make_args(**arg_overrides)
        cb, gcloud = fake_clients(builds, token=token)
        code = notify.run_notify(args, cb, gcloud, parse_time(NOW), opener=opener)
        self.assertEqual(code, 0)
        return opener

    def test_posts_pr_table_for_the_predicted_candidate(self):
        compare, pulls = compare_payload()
        opener = self.run_notify(world(), FakeOpener(compare, pulls))
        self.assertEqual(len(opener.posted), 1)
        payload = opener.posted[0]
        self.assertIn("2 PR(s)", payload["text"])
        rendered = json.dumps(payload["blocks"])
        self.assertIn("Deployment cutoff now", rendered)
        self.assertIn("PRs expected in the deployment on", rendered)
        self.assertIn("Thursday Jul 16, 23:00 EDT", rendered)
        self.assertIn(f"{C1[:9]}", rendered)
        self.assertIn(f"{C3[:9]}", rendered)
        self.assertIn("pull/101", rendered)
        self.assertIn("pull/102", rendered)
        # All pre-cutoff merges are queued: no late-merge caveat, no
        # non-linear-history caveat on a clean "ahead" compare.
        self.assertNotIn("still in the QA pipeline", rendered)
        self.assertNotIn("non-linear history", rendered)

    def test_staging_prediction_diffs_against_prod_deployed_base(self):
        # 2026-07-27 gate move: the predicted queues are staging's, but the
        # PR-table diff base stays PROD's deployed commit (--deployed-trigger).
        compare, pulls = compare_payload()
        builds = deployed_c1() + [
            build("g5-deploy-qa", C2, "2026-07-13T18:00:00Z"),
            build("g5-promote-staging", C2, "2026-07-13T19:00:00Z",
                  status="PENDING", approval="PENDING"),
            build("g5-deploy-qa", C3, "2026-07-14T22:00:00Z"),
            build("g5-promote-staging", C3, "2026-07-14T23:00:00Z",
                  status="PENDING", approval="PENDING", build_id="candidate"),
            # Staging already deployed c2 (it runs ahead of prod mid-cycle);
            # its own history must NOT become the diff base.
            build("g5-promote-staging", C2, "2026-07-13T19:30:00Z",
                  status="SUCCESS", approval="APPROVED"),
        ]
        opener = self.run_notify(
            builds, FakeOpener(compare, pulls),
            app_trigger="g5-promote-staging",
            tf_trigger="g5-terraform-apply-staging",
            deployed_trigger="g5-promote-prod",
        )
        payload = opener.posted[0]
        rendered = json.dumps(payload["blocks"])
        self.assertIn("2 PR(s)", payload["text"])
        # Base = prod's deployed c1, head = the staging queue's candidate c3.
        self.assertIn(f"{C1[:9]}", rendered)
        self.assertIn(f"{C3[:9]}", rendered)
        # The rendered range must diff from PROD's deployed commit, not from
        # staging's own newer deployed history. (ensure_ascii=False so the
        # unicode arrow is matchable rather than →-escaped.)
        rendered_utf8 = json.dumps(payload["blocks"], ensure_ascii=False)
        self.assertIn(f"`{C1[:9]}` → `{C3[:9]}`", rendered_utf8)
        self.assertNotIn(f"`{C2[:9]}` →", rendered_utf8)

    def test_late_inflight_merge_adds_honesty_note(self):
        compare, pulls = compare_payload()
        builds = world() + [
            build("g5-deploy-qa", C4, "2026-07-15T06:50:00Z", status="WORKING"),
        ]
        opener = self.run_notify(builds, FakeOpener(compare, pulls))
        rendered = json.dumps(opener.posted[0]["blocks"])
        self.assertIn("still in the QA pipeline", rendered)
        self.assertEqual(rendered.count("final selection at approval time"), 1)

    def test_manual_prod_success_advances_base_and_omits_already_deployed_prs(self):
        compare, pulls = compare_payload()
        compare["commits"] = [compare["commits"][1]]
        compare["total_commits"] = 1
        builds = staging_world() + [
            build("g5-promote-prod", C2, "2026-07-14T20:00:00Z",
                  status="SUCCESS", build_id="manual-prod-success"),
        ]
        opener = self.run_notify(
            builds, FakeOpener(compare, pulls), app_trigger="g5-promote-staging",
        )
        self.assertIn(
            f"https://api.github.com/repos/G5Labs/G5-Lean/compare/{C2}...{C3}",
            opener.requested_urls,
        )
        rendered = json.dumps(opener.posted[0], ensure_ascii=False)
        self.assertIn(f"`{C2[:9]}` → `{C3[:9]}`", rendered)
        self.assertIn("pull/102", rendered)
        self.assertNotIn("pull/101", rendered)
        self.assertIn("1 PR(s) expected", rendered)

    def test_later_old_commit_green_skip_does_not_repeat_shipped_prs(self):
        compare, pulls = compare_payload()
        compare["commits"] = [compare["commits"][1]]
        compare["total_commits"] = 1
        builds = staging_world() + [
            build("g5-promote-prod", C2, "2026-07-14T20:00:00Z"),
            build("g5-promote-prod", C1, "2026-07-15T06:00:00Z",
                  build_id="old-commit-green-skip"),
            # This QA rerun happened AFTER that prod build; it cannot be its origin.
            build("g5-deploy-qa", C1, "2026-07-15T07:10:00Z"),
        ]
        opener = self.run_notify(
            builds, FakeOpener(compare, pulls), app_trigger="g5-promote-staging",
        )
        self.assertIn(
            f"https://api.github.com/repos/G5Labs/G5-Lean/compare/{C2}...{C3}",
            opener.requested_urls,
        )
        rendered = json.dumps(opener.posted[0], ensure_ascii=False)
        self.assertIn(f"`{C2[:9]}` → `{C3[:9]}`", rendered)
        self.assertIn("pull/102", rendered)
        self.assertNotIn("pull/101", rendered)

    def test_potentially_newer_missing_origin_suppresses_pr_range(self):
        builds = staging_world() + [
            build("g5-promote-prod", C4, "2026-07-14T20:00:00Z"),
        ]
        opener = self.run_notify(
            builds, FakeOpener(), app_trigger="g5-promote-staging",
        )
        self.assertFalse(any("/compare/" in url for url in opener.requested_urls))
        rendered = json.dumps(opener.posted[0], ensure_ascii=False)
        self.assertIn("production baseline uncertain", rendered)
        self.assertIn("Merged-PR list unavailable", rendered)
        self.assertNotIn(f"`{C1[:9]}` →", rendered)

    def test_approval_without_successful_prod_does_not_advance_base(self):
        compare, pulls = compare_payload()
        builds = staging_world() + [
            build("g5-promote-staging", C2, "2026-07-14T19:00:00Z",
                  status="WORKING", approval="APPROVED"),
            build("g5-promote-prod", C2, "2026-07-14T20:00:00Z",
                  status="WORKING", build_id="manual-prod-incomplete"),
        ]
        opener = self.run_notify(
            builds, FakeOpener(compare, pulls), app_trigger="g5-promote-staging",
        )
        self.assertIn(
            f"https://api.github.com/repos/G5Labs/G5-Lean/compare/{C1}...{C3}",
            opener.requested_urls,
        )
        rendered = json.dumps(opener.posted[0], ensure_ascii=False)
        self.assertIn(f"`{C1[:9]}` → `{C3[:9]}`", rendered)
        self.assertIn("pull/101", rendered)
        self.assertIn("pull/102", rendered)

    def test_configured_webhook_version_reaches_secret_read(self):
        args = make_args(webhook_version="7")
        cb, gcloud = fake_clients([])
        gcloud.access_secret = mock.Mock(return_value="PLACEHOLDER")
        notify.run_notify(args, cb, gcloud, parse_time(NOW),
                          opener=FakeOpener(forbid_post=True))
        gcloud.access_secret.assert_called_once_with(args.webhook_secret, "7")

    def test_already_applied_terraform_commit_is_not_a_late_merge(self):
        # Regression: the note correlation is per queue — an already-applied
        # terraform commit must not read as "still in the QA pipeline".
        builds = deployed_c1() + [
            build("g5-terraform-apply-qa", C5, "2026-07-14T10:00:00Z"),
            build("g5-terraform-apply-prod", C5, "2026-07-14T11:00:00Z",
                  status="SUCCESS", approval="APPROVED"),
        ]
        opener = self.run_notify(builds, FakeOpener())
        payload = opener.posted[0]
        rendered = json.dumps(payload["blocks"])
        self.assertIn("Nothing new to ship", rendered)
        self.assertIn("nothing new to ship", payload["text"])
        self.assertNotIn("still in the QA pipeline", rendered)

    def test_headless_late_merge_renders_still_forming_not_nothing_new(self):
        # Regression: at the cutoff instant a merge can be mid-pipeline with
        # no promote queued yet — that is NOT a quiet week, and the two claims
        # must never co-occur.
        builds = deployed_c1() + [
            build("g5-deploy-qa", C4, "2026-07-15T06:50:00Z", status="WORKING"),
        ]
        opener = self.run_notify(builds, FakeOpener())
        payload = opener.posted[0]
        rendered = json.dumps(payload["blocks"])
        self.assertIn("No candidate is queued yet", rendered)
        self.assertIn("selection still forming", payload["text"])
        self.assertNotIn("Nothing new to ship", rendered)

    def test_quiet_week_posts_nothing_new_message(self):
        opener = self.run_notify(deployed_c1(), FakeOpener())
        payload = opener.posted[0]
        rendered = json.dumps(payload["blocks"])
        self.assertIn("Nothing new to ship", rendered)
        self.assertIn("nothing new to ship", payload["text"])
        self.assertNotIn("still in the QA pipeline", rendered)

    def test_unresolved_terraform_queue_reports_prediction_unavailable(self):
        # Regression: the real train halts red on an unclassifiable terraform
        # queue — the message must not promise a table the train will not ship.
        builds = world() + [
            build("g5-terraform-apply-prod", C4, "2026-07-14T12:00:00Z",
                  status="PENDING", approval="PENDING"),
        ]
        opener = self.run_notify(builds, FakeOpener(forbid_post=False))
        payload = opener.posted[0]
        rendered = json.dumps(payload["blocks"])
        self.assertIn("prediction unavailable", payload["text"])
        self.assertIn("halt red", rendered)
        self.assertNotIn("PRs expected in the deployment", rendered)

    def test_unresolved_app_builds_add_caveat_note(self):
        compare, pulls = compare_payload()
        builds = world() + [
            build("g5-promote-prod", C4, "2026-07-14T12:00:00Z",
                  status="PENDING", approval="PENDING"),
        ]
        opener = self.run_notify(builds, FakeOpener(compare, pulls))
        rendered = json.dumps(opener.posted[0]["blocks"])
        self.assertIn("unresolvable origins", rendered)

    def test_non_ahead_compare_adds_non_linear_history_caveat(self):
        compare, pulls = compare_payload(status="diverged")
        opener = self.run_notify(world(), FakeOpener(compare, pulls))
        rendered = json.dumps(opener.posted[0]["blocks"])
        self.assertIn("non-linear history", rendered)

    def test_pr_lookup_failure_fallback_text_says_unavailable(self):
        # Regression: the screen-reader fallback must state the same verdict
        # as the blocks — never "0 PR(s)" over an unavailable table.
        opener = FakeOpener()
        self.run_notify(world(), opener, token=RuntimeError("no token"))
        payload = opener.posted[0]
        self.assertIn("merged-PR list unavailable", payload["text"])
        rendered = json.dumps(payload["blocks"])
        self.assertIn("Merged-PR list unavailable", rendered)

    def test_placeholder_secret_skips_without_posting(self):
        args = make_args()
        cb, gcloud = fake_clients(world(), secret="PLACEHOLDER-set-me")
        opener = FakeOpener(forbid_post=True)
        code = notify.run_notify(args, cb, gcloud, parse_time(NOW), opener=opener)
        self.assertEqual(code, 0)
        self.assertEqual(opener.posted, [])

    def test_absent_secret_skips_cleanly(self):
        args = make_args()
        cb, gcloud = fake_clients(
            world(),
            secret=RuntimeError("gcloud secrets failed: NOT_FOUND: not found"),
        )
        code = notify.run_notify(args, cb, gcloud, parse_time(NOW),
                                 opener=FakeOpener(forbid_post=True))
        self.assertEqual(code, 0)

    def test_secret_outage_is_not_a_clean_skip(self):
        # Permission/network failures must reach main's catch-all marker, not
        # masquerade as "not configured" ([D27]).
        args = make_args()
        cb, gcloud = fake_clients(
            world(),
            secret=RuntimeError("gcloud secrets failed: PERMISSION_DENIED"),
        )
        with self.assertRaises(RuntimeError):
            notify.run_notify(args, cb, gcloud, parse_time(NOW),
                              opener=FakeOpener(forbid_post=True))

    def test_dry_run_renders_without_posting(self):
        compare, pulls = compare_payload()
        opener = FakeOpener(compare, pulls, forbid_post=True)
        self.run_notify(world(), opener, dry_run=True)
        self.assertEqual(opener.posted, [])

    def test_full_list_window_adds_inaccuracy_note(self):
        compare, pulls = compare_payload()
        filler = [
            build("g5-deploy-qa", C1, "2026-07-01T00:00:00Z", build_id=f"filler-{i}")
            for i in range(notify.LIST_LIMIT - len(world()))
        ]
        opener = self.run_notify(world() + filler, FakeOpener(compare, pulls))
        rendered = json.dumps(opener.posted[0]["blocks"])
        self.assertIn("window full", rendered)

    def test_full_window_covering_horizon_adds_no_inaccuracy_note(self):
        # [D27]: a full window that still reaches back past the [D25] decision
        # horizon is complete for prediction purposes.
        compare, pulls = compare_payload()
        filler = [
            build("g5-deploy-qa", C1, "2026-05-01T00:00:00Z", build_id=f"filler-{i}")
            for i in range(notify.LIST_LIMIT - len(world()))
        ]
        opener = self.run_notify(world() + filler, FakeOpener(compare, pulls))
        rendered = json.dumps(opener.posted[0]["blocks"])
        self.assertNotIn("window full", rendered)

    def test_fossil_terraform_pending_adds_note_not_unavailable(self):
        # 2026-07-29 incident: an immortal expired pending must not turn the
        # prediction unavailable once the train skips it as a fossil.
        compare, pulls = compare_payload()
        builds = world() + [
            build("g5-terraform-apply-prod", C4, "2026-05-27T14:33:00Z",
                  status="PENDING", approval="PENDING"),
        ]
        opener = self.run_notify(builds, FakeOpener(compare, pulls))
        payload = opener.posted[0]
        rendered = json.dumps(payload["blocks"])
        self.assertNotIn("prediction unavailable", payload["text"])
        self.assertIn("PRs expected in the deployment", rendered)
        self.assertIn("fossil pending build(s)", rendered)


class MessagePredictionTest(unittest.TestCase):
    def test_every_message_state_has_one_unconditional_snapshot_caveat(self):
        head = pending("2026-07-14T22:00:00Z", "candidate", C3)
        for candidate, rows, forming, halt in [
            (head, [{"number": 101, "title": "change", "author": "ann",
                     "merged_at": parse_time("2026-07-13T17:58:00Z")}], False, ""),
            (head, [], False, ""),
            (head, None, False, ""),
            (None, None, True, ""),
            (None, None, False, ""),
            (None, None, False, "queue history unavailable"),
        ]:
            with self.subTest(rows=rows, forming=forming, halt=halt):
                payload = notify.build_message(
                    deploy_time_label="Thursday 23:00 ET", head=candidate,
                    base_sha=C1, rows=rows, notes=[], still_forming=forming,
                    halt_reason=halt, owner="G5Labs", repo="G5-Lean",
                )
                rendered = json.dumps(payload)
                self.assertEqual(rendered.count("final selection at approval time"), 1)
                self.assertIn("later deployments can change what remains", rendered)
                self.assertNotIn("will be deployed", rendered)


class MainFailSoftTest(unittest.TestCase):
    def test_malformed_webhook_never_reaches_network_or_leaks_secret_to_logs(self):
        for secret in [
            "synthetic-secret-without-a-scheme",
            "http://hooks.slack.example/synthetic-secret",
            "file:///synthetic-secret",
            "https://",
            "https://[synthetic-secret",
            "https://hooks.slack.example:synthetic-secret/path",
            "https://hooks.slack.example:99999/synthetic-secret",
            "https://user:synthetic-secret@hooks.slack.example/path",
            "https://hooks.slack.example/synthetic-secret\ntrailing",
            "https://synthetic-secret\x00.example/path",
        ]:
            with self.subTest(secret=secret):
                cb = mock.Mock()
                cb.list_builds.return_value = []
                gcloud = mock.Mock()
                gcloud.access_secret.return_value = secret
                stdout, stderr = io.StringIO(), io.StringIO()
                with mock.patch.object(notify, "CloudBuildClient", return_value=cb), \
                        mock.patch.object(notify, "GcloudClient", return_value=gcloud), \
                        mock.patch.object(notify, "post_webhook") as post, \
                        mock.patch.object(notify.sys, "stdout", stdout), \
                        mock.patch.object(notify.sys, "stderr", stderr):
                    code = notify.main([
                        "--project", "p", "--region", "us-central1", "--now", NOW,
                        "--webhook-secret", "test-webhook", "--webhook-version", "7",
                    ])
                self.assertEqual(code, 0)
                cb.list_builds.assert_not_called()
                gcloud.github_read_token.assert_not_called()
                post.assert_not_called()
                output = stdout.getvalue() + stderr.getvalue()
                self.assertIn("NOTIFY-DEPLOY-CUTOFF-FAILED", output)
                self.assertIn("webhook secret test-webhook version 7", output)
                self.assertIn("not a valid HTTPS URL", output)
                self.assertNotIn(secret, output)
                self.assertNotIn("synthetic-secret", output)

    def test_valid_https_webhook_is_returned_unchanged(self):
        secret = "https://hooks.slack.example/services/T000/B000/synthetic-secret"
        gcloud = mock.Mock()
        gcloud.access_secret.return_value = secret
        self.assertEqual(notify.resolve_webhook(gcloud, "test-webhook", "7"), secret)
        gcloud.access_secret.assert_called_once_with("test-webhook", "7")

    def test_lean_defaults_and_pinned_webhook_version(self):
        with mock.patch.object(notify, "CloudBuildClient") as cb, \
                mock.patch.object(notify, "GcloudClient"), \
                mock.patch.object(notify, "run_notify", return_value=0) as run:
            self.assertEqual(notify.main([
                "--project", "g5-qa-1ccc", "--region", "us-central1", "--now", NOW,
            ]), 0)
        args = run.call_args.args[0]
        self.assertEqual(args.github_owner, "G5Labs")
        self.assertEqual(args.github_repo, "G5-Lean")
        self.assertEqual(args.github_connection, "G5labs-G5-QA")
        self.assertEqual(args.github_connection_repository, "G5Labs-G5-Lean")
        self.assertEqual(args.webhook_version, "1")
        self.assertEqual(cb.call_args.kwargs["trigger_names"], [
            "g5-terraform-apply-staging", "g5-terraform-apply-qa",
            "g5-promote-staging", "g5-deploy-qa", "g5-promote-prod",
        ])

    def test_any_unhandled_error_exits_zero_with_loud_marker(self):
        def boom():
            raise RuntimeError("listing exploded")

        cb = SimpleNamespace(list_builds=boom)
        gcloud = SimpleNamespace(
            access_secret=lambda name, version: "https://hooks.slack.example/x",
            github_read_token=lambda connection, repository: "tok",
        )
        with mock.patch.object(notify, "CloudBuildClient", return_value=cb), \
                mock.patch.object(notify, "GcloudClient", return_value=gcloud), \
                mock.patch.object(notify.sys, "stderr") as stderr:
            code = notify.main([
                "--project", "p", "--region", "us-central1", "--now", NOW,
            ])
        self.assertEqual(code, 0)
        written = "".join(
            call.args[0] for call in stderr.write.call_args_list if call.args
        )
        self.assertIn("NOTIFY-DEPLOY-CUTOFF-FAILED", written)


class CutoffYamlTzdataGuardTest(unittest.TestCase):
    """[D27] yaml contract: deploy-cutoff-notify installs OS tzdata before the script.

    The gcloud builder's zoneinfo has no IANA database; without the install the
    merged-PR table's America/Los_Angeles render dies with ZoneInfoNotFoundError,
    which the fail-soft posture reduces to a silent NOTIFY-DEPLOY-CUTOFF-FAILED
    marker (observed on the feature's first two real prod firings, 2026-07-31).
    """

    def test_notify_step_installs_tzdata_before_running_the_script(self):
        yaml_path = pathlib.Path(__file__).resolve().parent.parent / "infra" / "ci" / "cloudbuild-deploy-cutoff.yaml"
        text = yaml_path.read_text(encoding="utf-8")
        step_start = text.index("- id: deploy-cutoff-notify")
        step_end = text.index("\nsubstitutions:", step_start)
        step = text[step_start:step_end]
        install = step.find("DEBIAN_FRONTEND=noninteractive timeout 120 apt-get install -y -qq tzdata")
        invoke = step.find("notify_deploy_cutoff.py")
        self.assertNotEqual(
            invoke, -1,
            "deploy-cutoff-notify step no longer invokes notify_deploy_cutoff.py; update this guard",
        )
        self.assertNotEqual(
            install, -1,
            "deploy-cutoff-notify must install tzdata: zoneinfo has no IANA database in "
            "gcr.io/cloud-builders/gcloud, and DEBIAN_FRONTEND=noninteractive is "
            "load-bearing (tzdata's configure step otherwise prompts and hangs the build)",
        )
        self.assertLess(install, invoke, "tzdata install must precede the script invocation")
        # [D27] fail-soft + bounded: the install must not be able to fail or
        # stall the deploy under the step's `set -euo pipefail`.
        self.assertIn(
            "timeout 120 apt-get update", step[:install],
            "apt commands must be timeout-wrapped so a hang cannot stall the step",
        )
        fallthrough = step.find("NOTIFY-DEPLOY-CUTOFF-FAILED: timezone dependency setup failed")
        self.assertNotEqual(
            fallthrough, -1,
            "the install must have an explicit continue-on-failure handler so an apt "
            "outage degrades to a loud marker instead of failing the notification",
        )
        self.assertTrue(
            install < fallthrough < invoke,
            "the continue-on-failure handler must sit between the install and the script",
        )


if __name__ == "__main__":
    unittest.main()
