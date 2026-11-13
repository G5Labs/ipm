#!/usr/bin/env python3
"""Synthetic tests for shared release-notification helpers."""

from __future__ import annotations

import datetime as dt
import json
import unittest
import urllib.error
from types import SimpleNamespace

import release_notification_helpers as notify

UTC = dt.timezone.utc


def compare_commit(subject, *, login="dvd", name="Davide", date="2026-07-15T21:02:11Z",
                   body="", sha="a" * 40):
    message = subject if not body else f"{subject}\n\n{body}"
    return {
        "sha": sha,
        "author": {"login": login} if login else None,
        "commit": {
            "message": message,
            "author": {"name": name},
            "committer": {"date": date},
        },
    }


def pull(number, *, title="PR title", login="dvd", merged_at="2026-07-15T21:02:11Z",
         base_ref="main"):
    return {"number": number, "title": title, "user": {"login": login},
            "merged_at": merged_at, "base": {"ref": base_ref}}


class PrRowsSubjectFallbackTest(unittest.TestCase):
    """pr_rows without get_pulls: pure subject-parsing fallback path."""

    def test_squash_subjects_become_rows_ordered_by_merge_time(self):
        compare = {
            "total_commits": 2,
            "commits": [
                compare_commit("Fix preview cookie cliff (#1097)"),
                compare_commit("Add release train (#1252)", login="alice",
                               date="2026-07-14T10:00:00Z"),
            ],
        }
        rows, without_pr, truncated, failures = notify.pr_rows(compare)
        self.assertEqual((without_pr, truncated, failures), (0, 0, 0))
        self.assertEqual([row["number"] for row in rows], [1252, 1097])
        self.assertEqual(rows[1]["title"], "Fix preview cookie cliff")
        self.assertEqual(rows[1]["author"], "dvd")
        self.assertEqual(rows[0]["author"], "alice")
        self.assertEqual(
            rows[1]["merged_at"], dt.datetime(2026, 7, 15, 21, 2, 11, tzinfo=UTC)
        )

    def test_merge_commit_subject_fallback_takes_title_from_body(self):
        compare = {
            "total_commits": 1,
            "commits": [
                compare_commit("Merge pull request #42 from G5Labs/feat/x",
                               body="Add the x feature"),
            ],
        }
        rows, without_pr, _, _ = notify.pr_rows(compare)
        self.assertEqual(without_pr, 0)
        self.assertEqual(rows[0]["number"], 42)
        self.assertEqual(rows[0]["title"], "Add the x feature")

    def test_commits_without_pr_reference_are_counted_not_dropped_silently(self):
        compare = {
            "total_commits": 2,
            "commits": [
                compare_commit("hotfix directly on main"),
                compare_commit("Real PR (#7)"),
            ],
        }
        rows, without_pr, _, _ = notify.pr_rows(compare)
        self.assertEqual(len(rows), 1)
        self.assertEqual(without_pr, 1)

    def test_dedupes_by_pr_number_keeping_newest_time(self):
        compare = {
            "total_commits": 2,
            "commits": [
                compare_commit("Fix thing (#9)", date="2026-07-10T00:00:00Z"),
                compare_commit("Fix thing again (#9)", date="2026-07-12T00:00:00Z"),
            ],
        }
        rows, _, _, _ = notify.pr_rows(compare)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["merged_at"], dt.datetime(2026, 7, 12, tzinfo=UTC))

    def test_author_falls_back_to_git_name_when_login_missing(self):
        compare = {"total_commits": 1,
                   "commits": [compare_commit("Fix (#3)", login=None, name="Ext Contributor")]}
        rows, _, _, _ = notify.pr_rows(compare)
        self.assertEqual(rows[0]["author"], "Ext Contributor")

    def test_compare_truncation_is_surfaced(self):
        compare = {"total_commits": 300, "commits": [compare_commit("Fix (#3)")]}
        _, _, truncated, _ = notify.pr_rows(compare)
        self.assertEqual(truncated, 299)


class PrRowsAssociationTest(unittest.TestCase):
    """pr_rows with get_pulls: the commits/{sha}/pulls association is primary."""

    def test_association_resolves_commit_without_pr_suffix(self):
        # Real main history contains squash commits with NO (#N) suffix; the
        # association must still map them to their merged PR.
        compare = {
            "total_commits": 1,
            "commits": [compare_commit("feat(source-control): add commit history", sha="f" * 40)],
        }
        rows, without_pr, _, failures = notify.pr_rows(
            compare,
            get_pulls=lambda sha: [pull(1225, title="feat(source-control): add commit history",
                                        login="saehanjo-g5",
                                        merged_at="2026-07-16T14:35:18Z")],
        )
        self.assertEqual((without_pr, failures), (0, 0))
        self.assertEqual(rows[0]["number"], 1225)
        self.assertEqual(rows[0]["author"], "saehanjo-g5")
        self.assertEqual(rows[0]["merged_at"], dt.datetime(2026, 7, 16, 14, 35, 18, tzinfo=UTC))

    def test_unmerged_associated_prs_are_ignored(self):
        compare = {"total_commits": 1,
                   "commits": [compare_commit("Fix thing (#9)")]}
        rows, without_pr, _, failures = notify.pr_rows(
            compare, get_pulls=lambda sha: [pull(11, merged_at=None)],
        )
        # No merged association → subject fallback keeps the commit visible.
        self.assertEqual((without_pr, failures), (0, 0))
        self.assertEqual(rows[0]["number"], 9)

    def test_subject_fallback_cannot_resurrect_branch_filtered_pr(self):
        # When the association read succeeds and every merged PR targets
        # another branch, the "(#N)" subject fallback must not re-add it.
        compare = {"total_commits": 1,
                   "commits": [compare_commit("backported thing (#701)", sha="f" * 40)]}
        rows, without_pr, _, failures = notify.pr_rows(
            compare,
            get_pulls=lambda sha: [pull(701, title="backported thing", base_ref="release-1.2")],
            base_ref="main",
        )
        self.assertEqual((rows, without_pr, failures), ([], 1, 0))

    def test_base_ref_filter_excludes_backport_branch_prs(self):
        # The association endpoint returns EVERY PR touching the commit; a
        # backport PR sharing the squash commit must not appear as an upcoming
        # deploy row ([D26]).
        compare = {"total_commits": 1,
                   "commits": [compare_commit("feat: shipped thing", sha="f" * 40)]}
        rows, without_pr, _, failures = notify.pr_rows(
            compare,
            get_pulls=lambda sha: [
                pull(700, title="feat: shipped thing"),
                pull(701, title="backport", base_ref="release-1.2"),
            ],
            base_ref="main",
        )
        self.assertEqual((without_pr, failures), (0, 0))
        self.assertEqual([row["number"] for row in rows], [700])

    def test_association_failure_counts_and_falls_back_to_subject(self):
        def get_pulls(_sha):
            raise RuntimeError("rate limited")

        compare = {"total_commits": 1,
                   "commits": [compare_commit("Fix thing (#9)")]}
        rows, without_pr, _, failures = notify.pr_rows(compare, get_pulls)
        self.assertEqual(failures, 1)
        self.assertEqual(without_pr, 0)
        self.assertEqual(rows[0]["number"], 9)

    def test_association_failure_with_unparseable_subject_counts_without_pr(self):
        def get_pulls(_sha):
            raise RuntimeError("rate limited")

        compare = {"total_commits": 1,
                   "commits": [compare_commit("hotfix directly on main")]}
        rows, without_pr, _, failures = notify.pr_rows(compare, get_pulls)
        self.assertEqual(failures, 1)
        self.assertEqual(without_pr, 1)
        self.assertEqual(rows, [])


class FormattingTest(unittest.TestCase):
    def test_merged_time_renders_in_pacific_time(self):
        merged = dt.datetime(2026, 7, 15, 21, 2, tzinfo=UTC)
        self.assertEqual(notify.format_merged_time(merged), "Jul 15 14:02 PT")
        self.assertEqual(notify.format_merged_time(None), "unknown")

    def test_row_contains_link_author_title_time(self):
        row = {"number": 1097, "title": "Fix <cookie> & cliff", "author": "dvd",
               "merged_at": dt.datetime(2026, 7, 15, 21, 2, tzinfo=UTC)}
        text = notify.format_row(row, "G5Labs", "G5")
        self.assertIn("<https://github.com/G5Labs/G5/pull/1097|#1097>", text)
        self.assertIn("*dvd*", text)
        self.assertIn("Fix &lt;cookie&gt; &amp; cliff", text)
        self.assertIn("Jul 15 14:02 PT", text)

    def test_chunk_rows_respects_row_cap(self):
        lines = [f"row {i}" for i in range(45)]
        sections = notify.chunk_rows(lines)
        self.assertEqual(len(sections), 3)
        self.assertEqual(sections[0].count("\n") + 1, notify.MAX_ROWS_PER_SECTION)

    def test_chunk_rows_respects_char_cap(self):
        lines = ["x" * 1600, "y" * 1600]
        sections = notify.chunk_rows(lines)
        self.assertEqual(len(sections), 2)


class PostWebhookTest(unittest.TestCase):
    class OkResponse:
        def __enter__(self):
            return self

        def __exit__(self, *exc):
            return False

        def read(self):
            return b"ok"

    def test_posts_json_payload(self):
        captured = {}

        def opener(request, timeout=None):
            captured["url"] = request.full_url
            captured["body"] = request.data
            return self.OkResponse()

        notify.post_webhook("https://hooks.slack.com/services/x", {"text": "t"},
                            opener=opener, sleep=lambda _s: None)
        self.assertEqual(captured["url"], "https://hooks.slack.com/services/x")
        self.assertEqual(json.loads(captured["body"]), {"text": "t"})

    def test_client_error_raises_immediately(self):
        def opener(_request, timeout=None):
            raise urllib.error.HTTPError(
                "https://hooks.slack.com/services/x", 400, "bad", {}, None
            )

        with self.assertRaises(RuntimeError):
            notify.post_webhook("https://hooks.slack.com/services/x", {"text": "t"},
                                opener=opener, sleep=lambda _s: None)

    def test_server_errors_retry_then_raise(self):
        attempts = {"count": 0}

        def opener(_request, timeout=None):
            attempts["count"] += 1
            raise urllib.error.URLError("down")

        with self.assertRaises(RuntimeError):
            notify.post_webhook("https://hooks.slack.com/services/x", {"text": "t"},
                                opener=opener, sleep=lambda _s: None)
        self.assertEqual(attempts["count"], notify.POST_ATTEMPTS)


class SecretVersionTest(unittest.TestCase):
    def test_default_latest_and_explicit_pinned_version(self):
        calls = []

        def run(cmd, **kwargs):
            calls.append(cmd)
            return SimpleNamespace(returncode=0, stdout="synthetic-value\n", stderr="")

        client = notify.GcloudClient("g5-qa-1ccc", "us-central1", run=run)
        self.assertEqual(client.access_secret("test-webhook"), "synthetic-value")
        self.assertEqual(client.access_secret("test-webhook", "1"), "synthetic-value")
        self.assertEqual(calls, [
            ["gcloud", "secrets", "versions", "access", version,
             "--secret=test-webhook", "--project=g5-qa-1ccc"]
            for version in ("latest", "1")
        ])


class GithubReadTokenTest(unittest.TestCase):
    class Response:
        def __init__(self, body):
            self._body = body

        def __enter__(self):
            return self

        def __exit__(self, *exc):
            return False

        def read(self):
            return self._body

    def run_stub(self, cmd, capture_output, text, timeout=None):
        return SimpleNamespace(returncode=0, stdout="sa-token\n", stderr="")

    def test_posts_to_v2_access_read_token_endpoint(self):
        captured = {}

        def opener(request, timeout=None):
            captured["url"] = request.full_url
            captured["auth"] = request.get_header("Authorization")
            captured["method"] = request.get_method()
            return self.Response(b'{"token": "ghs_abc", "expirationTime": "x"}')

        client = notify.GcloudClient("g5-qa-1ccc", "us-central1",
                                     run=self.run_stub, opener=opener)
        token = client.github_read_token("G5labs-G5-QA", "G5Labs-G5-Lean")
        self.assertEqual(token, "ghs_abc")
        self.assertEqual(
            captured["url"],
            "https://cloudbuild.googleapis.com/v2/projects/g5-qa-1ccc/"
            "locations/us-central1/connections/G5labs-G5-QA/repositories/"
            "G5Labs-G5-Lean:accessReadToken",
        )
        self.assertEqual(captured["auth"], "Bearer sa-token")
        self.assertEqual(captured["method"], "POST")

    def test_empty_token_raises(self):
        client = notify.GcloudClient(
            "p", "r", run=self.run_stub,
            opener=lambda _r, timeout=None: self.Response(b"{}"),
        )
        with self.assertRaises(RuntimeError):
            client.github_read_token("G5Labs", "G5Labs-G5-Lean")

if __name__ == "__main__":
    unittest.main()
