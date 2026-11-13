"""Unit tests for verify_external_secrets.py.

Run with: python3 -m unittest generation.scripts.test_verify_external_secrets
or from this directory: python3 -m unittest test_verify_external_secrets

Each test pins a failure mode found during PR #362 review: fail-closed CRD
probing and listing, fail-closed baselines, baseline-change (not wall-clock)
refresh semantics, legacy-namespace coverage, and Ready=True assertion.
"""
from __future__ import annotations

import subprocess
import unittest
from unittest import mock

import verify_external_secrets as ves


def completed(args, returncode=0, stdout="", stderr=""):
    return subprocess.CompletedProcess(
        ["kubectl", *args], returncode, stdout=stdout, stderr=stderr
    )


def es_item(namespace, name, refresh="", ready="True", message=""):
    status = {}
    if refresh:
        status["refreshTime"] = refresh
    status["conditions"] = [
        {"type": "Ready", "status": ready, "message": message}
    ]
    return {
        "metadata": {"namespace": namespace, "name": name},
        "status": status,
    }


def listing(*items):
    import json

    return json.dumps({"items": list(items)})


class FakeKubectl:
    """Dispatches run_kubectl calls to scripted responses per command kind."""

    def __init__(self):
        self.crd_response = completed([], stdout="customresourcedefinition/x\n")
        self.list_responses = []
        self.annotate_response = completed([])
        self.annotate_calls = []
        self.other_response = completed([])

    def __call__(self, args):
        if args[:2] == ["get", "crd"]:
            return self.crd_response
        if args[:2] == ["get", "externalsecret"] and "-o" in args and "json" in args:
            if not self.list_responses:
                raise AssertionError("unexpected extra ExternalSecret listing")
            response = self.list_responses[0]
            if len(self.list_responses) > 1:
                self.list_responses.pop(0)
            return response
        if args[0] == "annotate":
            self.annotate_calls.append(args)
            return self.annotate_response
        return self.other_response


class VerifyExternalSecretsTest(unittest.TestCase):
    def setUp(self):
        self.fake = FakeKubectl()
        patches = [
            mock.patch.object(ves, "run_kubectl", self.fake),
            mock.patch.object(ves.time, "sleep", lambda _x: None),
        ]
        for patcher in patches:
            patcher.start()
            self.addCleanup(patcher.stop)

    def verify(self, timeout=1):
        ves.verify(timeout_seconds=timeout, poll_seconds=0)

    def test_crd_probe_error_fails_closed(self):
        self.fake.crd_response = completed([], returncode=1, stderr="forbidden")
        with self.assertRaisesRegex(ves.VerificationError, "probe"):
            self.verify()

    def test_crd_absent_skips(self):
        self.fake.crd_response = completed([], stdout="")
        self.verify()  # no listing scripted: skip must not list

    def test_listing_error_fails_closed(self):
        self.fake.list_responses = [completed([], returncode=1, stderr="rbac")]
        with self.assertRaisesRegex(ves.VerificationError, "list ExternalSecrets"):
            self.verify()

    def test_no_external_secrets_skips(self):
        self.fake.list_responses = [completed([], stdout=listing())]
        self.verify()

    def test_happy_path_requires_baseline_change(self):
        before = es_item("g5", "g5-api-secrets", refresh="2026-06-09T21:00:00Z")
        after = es_item("g5", "g5-api-secrets", refresh="2026-06-09T21:00:01Z")
        self.fake.list_responses = [
            completed([], stdout=listing(before)),
            completed([], stdout=listing(after)),
        ]
        self.verify()
        self.assertEqual(len(self.fake.annotate_calls), 1)

    def test_same_second_stale_refresh_times_out(self):
        # refreshTime never changes from baseline -> must fail even though
        # Ready=True and the timestamp could equal a wall-clock sync_start.
        stale = es_item("g5", "g5-api-secrets", refresh="2026-06-09T21:00:00Z")
        self.fake.list_responses = [completed([], stdout=listing(stale))]
        with self.assertRaisesRegex(ves.VerificationError, "timed out"):
            self.verify(timeout=0)

    def test_never_synced_empty_refresh_times_out(self):
        empty = es_item("g5", "g5-api-secrets", refresh="")
        self.fake.list_responses = [completed([], stdout=listing(empty))]
        with self.assertRaisesRegex(ves.VerificationError, "timed out"):
            self.verify(timeout=0)

    def test_refreshed_but_not_ready_fails(self):
        before = es_item("g5", "g5-api-secrets", refresh="2026-06-09T21:00:00Z")
        after = es_item(
            "g5",
            "g5-api-secrets",
            refresh="2026-06-09T21:00:01Z",
            ready="False",
            message="could not get secret data from provider",
        )
        self.fake.list_responses = [
            completed([], stdout=listing(before)),
            completed([], stdout=listing(after)),
        ]
        with self.assertRaisesRegex(ves.VerificationError, "not Ready"):
            self.verify()

    def test_legacy_namespaces_are_covered(self):
        # One ExternalSecret in the Helm namespace, one in legacy "default";
        # only the Helm-namespace secret refreshes -> the gate must fail on
        # the legacy one rather than passing on the Helm namespace alone.
        before = [
            es_item("g5", "g5-api-secrets", refresh="2026-06-09T21:00:00Z"),
            es_item("default", "g5-api-secrets", refresh="2026-06-09T21:00:00Z"),
        ]
        after = [
            es_item("g5", "g5-api-secrets", refresh="2026-06-09T21:00:05Z"),
            es_item("default", "g5-api-secrets", refresh="2026-06-09T21:00:00Z"),
        ]
        self.fake.list_responses = [
            completed([], stdout=listing(*before)),
            completed([], stdout=listing(*after)),
        ]
        with self.assertRaisesRegex(ves.VerificationError, "timed out"):
            self.verify(timeout=0)
        annotated_namespaces = {call[call.index("-n") + 1] for call in self.fake.annotate_calls}
        self.assertEqual(annotated_namespaces, {"default", "g5"})

    def test_annotate_error_fails_closed(self):
        item = es_item("g5", "g5-api-secrets", refresh="2026-06-09T21:00:00Z")
        self.fake.list_responses = [completed([], stdout=listing(item))]
        self.fake.annotate_response = completed([], returncode=1, stderr="forbidden")
        with self.assertRaisesRegex(ves.VerificationError, "annotate"):
            self.verify()

    def test_transient_poll_error_retries_then_times_out(self):
        item = es_item("g5", "g5-api-secrets", refresh="2026-06-09T21:00:00Z")
        self.fake.list_responses = [
            completed([], stdout=listing(item)),
            completed([], returncode=1, stderr="transient"),
        ]
        with self.assertRaisesRegex(ves.VerificationError, "timed out"):
            self.verify(timeout=0)

    def test_main_exit_codes(self):
        self.fake.crd_response = completed([], stdout="")
        self.assertEqual(ves.main([]), 0)
        self.fake.crd_response = completed([], returncode=1, stderr="boom")
        self.assertEqual(ves.main([]), 1)


if __name__ == "__main__":
    unittest.main()
