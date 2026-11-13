"""Unit tests for resolve_worker_image.py.

Run with: python3 -m unittest test_resolve_worker_image (from
scripts/ — the module imports its sibling by bare name).

Each test pins a contract from cloudbuild_pipelines ontology [D30]: exact
short-sha match wins over newer non-matching images, ancestor selection walks
newest-first using only local history, provably-non-ancestor candidates are
skipped, the shallow-checkout timestamp fallback only accepts images created
at or before the promoted commit's committer time (never ahead), unlabeled or
malformed images are ignored, and the no-candidate case exits nonzero.
"""
from __future__ import annotations

import contextlib
import io
import json
import unittest

import resolve_worker_image as rwi

COMMIT = "5f7fd6922dffffffffffffffffffffffffffffff"
COMMIT_EPOCH = 1_786_800_000
WORKER_NAME = "g5-worker-v20260816-0033-e2e5954"
WORKER_REF = f"projects/g5-dev-1ccc/global/images/{WORKER_NAME}"


def image(name, sha, created):
    labels = {"g5_git_short_sha": sha} if sha is not None else {}
    return {"name": name, "labels": labels, "creationTimestamp": created}


def ts(epoch):
    import datetime

    return (
        datetime.datetime.fromtimestamp(epoch, tz=datetime.timezone.utc)
        .isoformat()
        .replace("+00:00", "+00:00")
    )


class FakeRunner:
    """Scripted subprocess runner: images list, git times, ancestry."""

    def __init__(
        self,
        images,
        local_shas=None,
        ancestors=None,
        commit_epoch=COMMIT_EPOCH,
        shallow=False,
        described_image=None,
    ):
        self.images = images
        self.local_shas = local_shas or {}
        self.ancestors = set(ancestors or [])
        self.commit_epoch = commit_epoch
        self.shallow = shallow
        self.described_image = described_image
        self.calls = []

    def __call__(self, cmd):
        self.calls.append(list(cmd))
        if cmd[0] == "gcloud":
            if "describe" in cmd:
                return 0, json.dumps(self.described_image or {}), ""
            return 0, json.dumps(self.images), ""
        if cmd[0] == "git":
            sub = cmd[3]
            if sub == "show":
                return 0, f"{self.commit_epoch}\n", ""
            if sub == "rev-parse" and cmd[-1] == "--is-shallow-repository":
                return 0, ("true" if self.shallow else "false") + "\n", ""
            if sub == "rev-parse":
                prefix = cmd[-1].split("=", 1)[1]
                vals = self.local_shas.get(prefix)
                if vals is None:
                    return 0, "", ""
                if isinstance(vals, str):
                    vals = [vals]
                return 0, "\n".join(vals) + "\n", ""
            if sub == "cat-file":
                return 0, "commit\n", ""
            if sub == "merge-base":
                # Legacy string entries mean "ancestor of anything"; tuple
                # entries pin an exact (ancestor, descendant) pair.
                anc, desc = cmd[-2], cmd[-1]
                ok = anc in self.ancestors or (anc, desc) in self.ancestors
                return (0, "", "") if ok else (1, "", "")
        raise AssertionError(f"unexpected command: {cmd}")


def run_main(runner, commit=COMMIT, provided_ref=None):
    source_args = (
        ["--provided-ref", provided_ref]
        if provided_ref is not None
        else ["--commit", commit, "--repo-dir", "/repo"]
    )
    stdout, stderr = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        rc = rwi.main(
            [
                "--project",
                "g5-dev-1ccc",
                "--family",
                "g5-worker",
                *source_args,
            ],
            runner=runner,
        )
    return rc, stdout.getvalue().strip(), stderr.getvalue()


class ExactMatch(unittest.TestCase):
    def test_exact_wins_over_newer_non_matching(self):
        runner = FakeRunner(
            [
                image("g5-worker-newer-abcdef0", "abcdef0", ts(COMMIT_EPOCH + 900)),
                image("g5-worker-exact-5f7fd69", "5f7fd69", ts(COMMIT_EPOCH - 900)),
            ],
            local_shas={"5f7fd69": COMMIT},
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertEqual(out, "projects/g5-dev-1ccc/global/images/g5-worker-exact-5f7fd69")
        self.assertIn("mode=exact", err)

    def test_rebuild_duplicates_pick_newest(self):
        runner = FakeRunner(
            [
                image("g5-worker-old-5f7fd69", "5f7fd69", ts(COMMIT_EPOCH - 5000)),
                image("g5-worker-new-5f7fd69", "5f7fd69", ts(COMMIT_EPOCH - 100)),
            ],
            local_shas={"5f7fd69": COMMIT},
        )
        rc, out, _ = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(out.endswith("/g5-worker-new-5f7fd69"))

    def test_prefix_collision_is_not_exact(self):
        # The label prefixes the promoted commit but resolves to a DIFFERENT
        # commit locally (7-hex collision): must be demoted, never "exact".
        other_commit = "5f7fd69" + "0" * 33
        full_anc = "a" * 40
        runner = FakeRunner(
            [
                image("g5-worker-collide-5f7fd69", "5f7fd69", ts(COMMIT_EPOCH + 900)),
                image("g5-worker-anc-aaaaaaa", "aaaaaaa", ts(COMMIT_EPOCH - 900)),
            ],
            local_shas={"5f7fd69": other_commit, "aaaaaaa": full_anc},
            ancestors=[full_anc],
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(
            out.endswith("/g5-worker-anc-aaaaaaa"),
            "a colliding prefix label must not be treated as exact proof",
        )
        self.assertIn("mode=ancestor", err)

    def test_ambiguous_prefix_demotes_and_ancestor_still_wins(self):
        # Two local commits share the 7-hex prefix: the candidate is demoted
        # (never fatal, never exact) and a provable ancestor still wins.
        full_anc = "a" * 40
        ambiguous = ["5f7fd69" + "1" * 33, "5f7fd69" + "2" * 33]
        runner = FakeRunner(
            [
                image("g5-worker-ambig-5f7fd69", "5f7fd69", ts(COMMIT_EPOCH + 200)),
                image("g5-worker-anc-aaaaaaa", "aaaaaaa", ts(COMMIT_EPOCH - 900)),
            ],
            local_shas={"5f7fd69": ambiguous, "aaaaaaa": full_anc},
            ancestors=[full_anc],
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(out.endswith("/g5-worker-anc-aaaaaaa"))
        self.assertIn("ambiguous", err)
        self.assertIn("mode=ancestor", err)

    def test_unconfirmable_exact_label_demotes_to_heuristic(self):
        # Label prefixes the commit but cannot be resolved locally at all
        # (e.g. ambiguous prefix): no exact claim; falls through the phases.
        runner = FakeRunner(
            [image("g5-worker-maybe-5f7fd69", "5f7fd69", ts(COMMIT_EPOCH - 900))]
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(out.endswith("/g5-worker-maybe-5f7fd69"))
        self.assertIn("mode=timestamp-fallback", err)


class DeployHandoffValidation(unittest.TestCase):
    def test_ready_same_project_family_ref_is_accepted_without_git(self):
        runner = FakeRunner(
            [],
            described_image={
                "name": WORKER_NAME,
                "family": "g5-worker",
                "status": "READY",
            },
        )
        rc, out, err = run_main(runner, provided_ref=WORKER_REF)
        self.assertEqual(rc, 0)
        self.assertEqual(out, WORKER_REF)
        self.assertIn("validated promoted worker image", err)
        self.assertFalse(
            [call for call in runner.calls if call[0] == "git"],
            "deploy-only validation must never consult Git",
        )

    def test_sentinel_family_malformed_and_wrong_project_refs_fail_before_gcloud(self):
        invalid_refs = [
            "DEPLOY-INJECTED",
            "projects/g5-dev-1ccc/global/images/family/g5-worker",
            "projects/other-project/global/images/g5-worker-valid",
            "projects/g5-dev-1ccc/global/images/name?x=1",
        ]
        for provided_ref in invalid_refs:
            with self.subTest(provided_ref=provided_ref):
                runner = FakeRunner([])
                rc, out, err = run_main(runner, provided_ref=provided_ref)
                self.assertEqual(rc, 1)
                self.assertEqual(out, "")
                self.assertIn("resolve-worker-image failed", err)
                self.assertFalse(runner.calls)

    def test_non_ready_image_is_rejected(self):
        runner = FakeRunner(
            [],
            described_image={
                "name": WORKER_NAME,
                "family": "g5-worker",
                "status": "CREATING",
            },
        )
        rc, out, err = run_main(runner, provided_ref=WORKER_REF)
        self.assertEqual(rc, 1)
        self.assertEqual(out, "")
        self.assertIn("status", err)

    def test_wrong_family_image_is_rejected(self):
        runner = FakeRunner(
            [],
            described_image={
                "name": WORKER_NAME,
                "family": "other-family",
                "status": "READY",
            },
        )
        rc, out, err = run_main(runner, provided_ref=WORKER_REF)
        self.assertEqual(rc, 1)
        self.assertEqual(out, "")
        self.assertIn("family", err)

    def test_build_mode_still_uses_git_backed_resolution(self):
        runner = FakeRunner(
            [image("g5-worker-exact-5f7fd69", "5f7fd69", ts(COMMIT_EPOCH - 900))],
            local_shas={"5f7fd69": COMMIT},
        )
        rc, _, _ = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue([call for call in runner.calls if call[0] == "git"])


class AncestorSelection(unittest.TestCase):
    def test_newest_ancestor_wins_and_non_ancestor_skipped(self):
        full_anc = "a" * 40
        full_non = "b" * 40
        runner = FakeRunner(
            [
                image("g5-worker-nonanc-bbbbbbb", "bbbbbbb", ts(COMMIT_EPOCH + 500)),
                image("g5-worker-anc-aaaaaaa", "aaaaaaa", ts(COMMIT_EPOCH - 500)),
            ],
            local_shas={"bbbbbbb": full_non, "aaaaaaa": full_anc},
            ancestors=[full_anc],
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(out.endswith("/g5-worker-anc-aaaaaaa"))
        self.assertIn("mode=ancestor", err)

    def test_no_git_fetch_is_ever_attempted(self):
        full_anc = "a" * 40
        runner = FakeRunner(
            [image("g5-worker-anc-aaaaaaa", "aaaaaaa", ts(COMMIT_EPOCH - 500))],
            local_shas={"aaaaaaa": full_anc},
            ancestors=[full_anc],
        )
        rc, _, _ = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertFalse(
            [c for c in runner.calls if c[0] == "git" and "fetch" in c],
            "resolver must never fetch (no in-build GitHub token)",
        )


class TimestampFallback(unittest.TestCase):
    def test_unresolvable_older_image_accepted(self):
        runner = FakeRunner(
            [image("g5-worker-shallow-ccccccc", "ccccccc", ts(COMMIT_EPOCH - 3600))]
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(out.endswith("/g5-worker-shallow-ccccccc"))
        self.assertIn("mode=timestamp-fallback", err)

    def test_unresolvable_newer_image_never_selected(self):
        runner = FakeRunner(
            [image("g5-worker-future-ddddddd", "ddddddd", ts(COMMIT_EPOCH + 3600))]
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 1)
        self.assertEqual(out, "")
        self.assertIn("no eligible worker image", err)

    def test_mixed_resolvable_non_ancestor_falls_through_to_older_shallow(self):
        full_non = "b" * 40
        runner = FakeRunner(
            [
                image("g5-worker-nonanc-bbbbbbb", "bbbbbbb", ts(COMMIT_EPOCH + 500)),
                image("g5-worker-shallow-ccccccc", "ccccccc", ts(COMMIT_EPOCH - 3600)),
            ],
            local_shas={"bbbbbbb": full_non},
            ancestors=[],
        )
        rc, out, _ = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(out.endswith("/g5-worker-shallow-ccccccc"))

    def test_rebuilt_old_ancestor_never_outranks_newer_commit_ancestor(self):
        # Old ancestor A's image was REBUILT after newer ancestor B's image:
        # graph-nearest (B) must win, not newest-built (A).
        full_a = "a" * 40
        full_b = "b" * 40
        runner = FakeRunner(
            [
                image("g5-worker-rebuilt-aaaaaaa", "aaaaaaa", ts(COMMIT_EPOCH - 100)),
                image("g5-worker-newer-bbbbbbb", "bbbbbbb", ts(COMMIT_EPOCH - 5000)),
            ],
            local_shas={"aaaaaaa": full_a, "bbbbbbb": full_b},
            ancestors=[(full_a, COMMIT), (full_b, COMMIT), (full_a, full_b)],
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(
            out.endswith("/g5-worker-newer-bbbbbbb"),
            "a rebuilt image of an older commit must not outrank a newer-commit ancestor",
        )
        self.assertIn("mode=ancestor", err)

    def test_proven_ancestor_outranks_newer_unresolved_candidate(self):
        full_anc = "a" * 40
        runner = FakeRunner(
            [
                image("g5-worker-shallow-eeeeeee", "eeeeeee", ts(COMMIT_EPOCH - 100)),
                image("g5-worker-anc-aaaaaaa", "aaaaaaa", ts(COMMIT_EPOCH - 5000)),
            ],
            local_shas={"aaaaaaa": full_anc},
            ancestors=[full_anc],
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(
            out.endswith("/g5-worker-anc-aaaaaaa"),
            "heuristic candidate must never outrank a proven ancestor",
        )
        self.assertIn("mode=ancestor", err)


class ShallowNegativeAncestry(unittest.TestCase):
    def test_shallow_negative_stays_eligible_for_fallback(self):
        # Shallow checkout: the labeled commit object exists locally but the
        # connecting history does not, so merge-base says "not ancestor".
        # That negative is unproven — the candidate must reach the fallback.
        full = "c" * 40
        runner = FakeRunner(
            [image("g5-worker-shallowneg-ccccccc", "ccccccc", ts(COMMIT_EPOCH - 3600))],
            local_shas={"ccccccc": full},
            ancestors=[],
            shallow=True,
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(out.endswith("/g5-worker-shallowneg-ccccccc"))
        self.assertIn("mode=timestamp-fallback", err)
        self.assertIn("unproven", err)

    def test_full_clone_negative_is_excluded(self):
        # Same shape in a FULL clone: the negative is proof; no candidate.
        full = "c" * 40
        runner = FakeRunner(
            [image("g5-worker-nonanc-ccccccc", "ccccccc", ts(COMMIT_EPOCH - 3600))],
            local_shas={"ccccccc": full},
            ancestors=[],
            shallow=False,
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 1)
        self.assertEqual(out, "")
        self.assertIn("no eligible worker image", err)

    def test_proven_ancestor_still_outranks_shallow_negative(self):
        full_neg = "c" * 40
        full_anc = "a" * 40
        runner = FakeRunner(
            [
                image("g5-worker-shallowneg-ccccccc", "ccccccc", ts(COMMIT_EPOCH - 100)),
                image("g5-worker-anc-aaaaaaa", "aaaaaaa", ts(COMMIT_EPOCH - 900)),
            ],
            local_shas={"ccccccc": full_neg, "aaaaaaa": full_anc},
            ancestors=[full_anc],
            shallow=True,
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(out.endswith("/g5-worker-anc-aaaaaaa"))
        self.assertIn("mode=ancestor", err)


class TimestampParsing(unittest.TestCase):
    def test_trailing_z_accepted(self):
        import datetime

        zulu = (
            datetime.datetime.fromtimestamp(
                COMMIT_EPOCH - 100, tz=datetime.timezone.utc
            ).strftime("%Y-%m-%dT%H:%M:%S.361Z")
        )
        runner = FakeRunner([image("g5-worker-shallow-fffffff", "fffffff", zulu)])
        rc, out, _ = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(out.endswith("/g5-worker-shallow-fffffff"))

    def test_naive_timestamp_rejected(self):
        runner = FakeRunner(
            [image("g5-worker-shallow-77aa77a", "77aa77a", "2026-08-14T10:00:00")]
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 1)
        self.assertEqual(out, "")
        self.assertIn("timezone-naive", err)


class GitFailureClassification(unittest.TestCase):
    def test_operational_rev_parse_failure_is_fatal_not_heuristic(self):
        class BrokenGit(FakeRunner):
            def __call__(self, cmd):
                if cmd[0] == "git" and cmd[3] == "rev-parse" and cmd[-1] != "--is-shallow-repository":
                    return 128, "", "fatal: bad object store"
                return super().__call__(cmd)

        runner = BrokenGit(
            [image("g5-worker-old-88bb88b", "88bb88b", ts(COMMIT_EPOCH - 3600))]
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 1)
        self.assertEqual(out, "")
        self.assertIn("rev-parse", err)

    def test_operational_merge_base_failure_is_fatal_not_negative(self):
        full = "a" * 40

        class BrokenMergeBase(FakeRunner):
            def __call__(self, cmd):
                if cmd[0] == "git" and cmd[3] == "merge-base":
                    return 128, "", "fatal: bad revision"
                return super().__call__(cmd)

        runner = BrokenMergeBase(
            [image("g5-worker-anc-aaaaaaa", "aaaaaaa", ts(COMMIT_EPOCH - 3600))],
            local_shas={"aaaaaaa": full},
        )
        rc, out, err = run_main(runner)
        self.assertEqual(rc, 1)
        self.assertEqual(out, "")
        self.assertIn("merge-base", err)


class InputHygiene(unittest.TestCase):
    def test_unlabeled_and_malformed_images_skipped(self):
        runner = FakeRunner(
            [
                image("g5-worker-unlabeled", None, ts(COMMIT_EPOCH - 100)),
                image("BAD_NAME!", "5f7fd69", ts(COMMIT_EPOCH - 100)),
                image("g5-worker-bad-sha", "not-a-sha", ts(COMMIT_EPOCH - 100)),
                image("g5-worker-exact-5f7fd69", "5f7fd69", ts(COMMIT_EPOCH - 200)),
            ]
        )
        rc, out, _ = run_main(runner)
        self.assertEqual(rc, 0)
        self.assertTrue(out.endswith("/g5-worker-exact-5f7fd69"))

    def test_short_commit_rejected(self):
        runner = FakeRunner([])
        rc, out, err = run_main(runner, commit="5f7fd69")
        self.assertEqual(rc, 1)
        self.assertEqual(out, "")
        self.assertIn("full 40-hex sha", err)

    def test_empty_family_fails(self):
        runner = FakeRunner([])
        rc, _, err = run_main(runner)
        self.assertEqual(rc, 1)
        self.assertIn("no READY sha-labeled images", err)

    def test_gcloud_failure_fails(self):
        class FailingRunner(FakeRunner):
            def __call__(self, cmd):
                if cmd[0] == "gcloud":
                    return 1, "", "PERMISSION_DENIED: example"
                return super().__call__(cmd)

        rc, _, err = run_main(FailingRunner([]))
        self.assertEqual(rc, 1)
        self.assertIn("resolve-worker-image failed", err)


if __name__ == "__main__":
    unittest.main()
