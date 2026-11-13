#!/usr/bin/env python3
"""Run the actual Filestore backup YAML against scripted HTTP and a virtual clock.

These offline regression tests interpret only the Workflows language features
used here. They exercise expressions and control flow, not GCP compilation,
HTTP retry timing, IAM, or real cloud execution. Python timestamp precision is
microseconds; nanosecond distinctions in the Workflows engine remain unverified.
No network requests are made.
"""
from collections import deque
from copy import deepcopy
from datetime import datetime, timezone
from pathlib import Path
from types import SimpleNamespace
import hashlib
import os
import re
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
MODULE = ROOT / "infra/terraform/modules/filestore-backup"
WORKFLOW = MODULE / "workflow.yaml"
API = "https://file.googleapis.com/v1/"


def _safe_load(text):
    """PyYAML when installed; otherwise the YAML parser bundled with gcloud."""
    try:
        import yaml
        return yaml.safe_load(text)
    except ModuleNotFoundError:
        pass
    sdk_root = os.environ.get("CLOUDSDK_ROOT_DIR")
    if not sdk_root:
        try:
            sdk_root = subprocess.run(
                ["gcloud", "info", "--format=value(installation.sdk_root)"],
                check=True, capture_output=True, text=True,
            ).stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            sdk_root = ""
    if sdk_root:
        sys.path.append(os.path.join(sdk_root, "lib", "third_party"))
    from ruamel import yaml as ruamel_yaml
    return ruamel_yaml.YAML(typ="safe").load(text)


DOC = _safe_load(WORKFLOW.read_text())


class AttrDict(dict):
    __getattr__ = dict.__getitem__


def wrap(value):
    if isinstance(value, dict):
        return AttrDict({key: wrap(item) for key, item in value.items()})
    if isinstance(value, list):
        return [wrap(item) for item in value]
    return value


def map_get(value, keys):
    for key in keys if isinstance(keys, list) else [keys]:
        value = value.get(key) if isinstance(value, dict) else None
    return value


def parse_time(value):
    # The offline clock is microsecond-precision, including on Python < 3.11.
    # This accepts API nanosecond formatting without claiming nanosecond parity.
    value = re.sub(r"(\.\d{6})\d+(?=Z|[+-]\d{2}:\d{2}$)", r"\1", value)
    return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()


NOW = parse_time("2026-11-04T08:00:00Z")
SOURCE = "projects/example-prod/locations/us-central1-a/instances/g5-prod"
PARENT = "projects/example-prod/locations/us-central1"
SHARE = "g5_repos"
PREFIX = "g5-prod"
TODAY = PARENT + "/backups/g5-prod-daily-2026-11-04"
ENV = {"SOURCE_INSTANCE": SOURCE, "SOURCE_SHARE": SHARE, "BACKUP_PARENT": PARENT,
       "BACKUP_PREFIX": PREFIX, "RETENTION_DAYS": "30"}


def backup(suffix="2026-10-04", **updates):
    result = {"name": PARENT + "/backups/g5-prod-daily-" + suffix,
              "sourceInstance": SOURCE, "sourceFileShare": SHARE, "state": "READY",
              "labels": {"managed-by": "g5-filestore-backup", "backup-kind": "daily"},
              "createTime": "2026-10-04T08:00:00Z"}
    result.update(updates)
    return result


class WorkflowError(Exception):
    def __init__(self, value):
        self.value = value
        super().__init__(str(value))


class Returned(Exception):
    def __init__(self, value):
        self.value = value


class Jump(Exception):
    def __init__(self, target):
        self.target = target


class ScriptedHTTP:
    """An ordered script: unexpected calls or changed ordering fail immediately."""
    def __init__(self):
        self.pending = deque()
        self.calls = []

    def expect(self, method, resource, body, count=1):
        self.pending.extend((method, API + resource, deepcopy(body)) for _ in range(count))

    def call(self, method, args):
        self.calls.append((method, args))
        if not self.pending:
            raise AssertionError(f"Unexpected HTTP call: {method} {args}")
        expected_method, expected_url, body = self.pending.popleft()
        if (method, args["url"]) != (expected_method, expected_url):
            raise AssertionError(f"Expected {expected_method} {expected_url}; got {method} {args}")
        if isinstance(body, WorkflowError):
            raise body
        return wrap({"body": body})


class OfflineWorkflow:
    """Small interpreter for this file's assign/call/switch/for/try/next forms.

    HTTP retries are deliberately not simulated. Assertions separately protect
    the declared retry and authentication settings. Unsupported steps fail rather
    than silently succeeding, and a step budget detects accidental infinite loops.
    """
    def __init__(self, http, document=DOC, env=None):
        self.http = http
        self.document = document
        self.env = dict(ENV if env is None else env)
        self.now = NOW
        self.budget = 100000

    def sleep(self, seconds):
        self.now += seconds

    def evaluate(self, value, scope):
        if isinstance(value, dict):
            return wrap({key: self.evaluate(item, scope) for key, item in value.items()})
        if isinstance(value, list):
            return [self.evaluate(item, scope) for item in value]
        if not isinstance(value, str) or not value.startswith("${"):
            return value
        library = {
            "map": SimpleNamespace(get=map_get),
            "list": SimpleNamespace(concat=lambda items, item: items + [item]),
            "text": SimpleNamespace(substring=lambda text, start, end: text[start:end]),
            "time": SimpleNamespace(parse=parse_time, format=lambda value, zone:
                                    datetime.fromtimestamp(value, timezone.utc).isoformat()),
            "sys": SimpleNamespace(now=lambda: self.now, get_env=lambda key: self.env.get(key, "")),
            "default": lambda value, fallback: fallback if value is None else value,
            "int": int, "len": len, "true": True, "false": False, "null": None,
        }
        return wrap(eval(value[2:-1], {"__builtins__": {}}, library | scope))  # fixed local YAML

    def call(self, workflow="main", **args):
        definition = self.document[workflow]
        if set(args) != set(definition.get("params", [])):
            raise AssertionError(f"Incorrect arguments to {workflow}: {args}")
        try:
            self.run_steps(definition["steps"], wrap(args))
        except Returned as result:
            return result.value

    def run_steps(self, steps, scope):
        entries = [next(iter(step.items())) for step in steps]
        positions = {name: index for index, (name, _) in enumerate(entries)}
        index = 0
        while index < len(entries):
            self.budget -= 1
            if self.budget < 0:
                raise AssertionError("Workflow exceeded the offline step budget")
            try:
                self.run_node(entries[index][1], scope)
            except Jump as jump:
                if jump.target not in positions:
                    raise
                index = positions[jump.target]
            else:
                index += 1

    def run_node(self, node, scope):
        unknown = set(node) - {"assign", "call", "args", "result", "switch", "condition",
                               "for", "try", "retry", "except", "next", "return", "raise"}
        if unknown:
            raise AssertionError(f"Unsupported Workflow features: {unknown}")
        if "try" in node:
            try:
                self.run_node(node["try"], scope)
            except WorkflowError as error:
                if "except" not in node:
                    raise
                handler = node["except"]
                scope[handler["as"]] = wrap(error.value)
                self.run_steps(handler["steps"], scope)
        for assignment in node.get("assign", []):
            for key, value in assignment.items():
                scope[key] = self.evaluate(value, scope)
        if "call" in node:
            name = node["call"]
            args = self.evaluate(node.get("args", {}), scope)
            if name.startswith("http."):
                result = self.http.call(name, args)
            elif name == "sys.sleep":
                result = self.sleep(**args)
            else:
                result = self.call(name, **args)
            if "result" in node:
                scope[node["result"]] = result
        if "for" in node:
            loop = node["for"]
            for value in self.evaluate(loop["in"], scope):
                scope[loop["value"]] = value
                self.run_steps(loop["steps"], scope)
        for branch in node.get("switch", []):
            if self.evaluate(branch["condition"], scope):
                self.run_node(branch, scope)
                break
        if "raise" in node:
            raise WorkflowError(self.evaluate(node["raise"], scope))
        if "return" in node:
            raise Returned(self.evaluate(node["return"], scope))
        if "next" in node:
            raise Jump(node["next"])


class WorkflowRegression(unittest.TestCase):
    def setUp(self):
        self.reset()

    def reset(self, **kwargs):
        self.http = ScriptedHTTP()
        self.runner = OfflineWorkflow(self.http, **kwargs)
        self.addCleanup(lambda http=self.http: self.assertEqual(len(http.pending), 0,
                                                               "Not all expected HTTP calls occurred"))

    def start(self, duplicate=False, current=None):
        creation = WorkflowError({"code": 409}) if duplicate else {"name": "operations/create", "done": True}
        self.http.expect("http.post", PARENT + "/backups", creation)
        self.http.expect("http.get", TODAY, current or backup("2026-11-04"))

    def listing(self, backups, token=None):
        page = {"backups": backups}
        if token:
            page["nextPageToken"] = token
        self.http.expect("http.get", PARENT + "/backups", page)

    def deletion(self, candidate):
        self.http.expect("http.get", candidate["name"], candidate)
        self.http.expect("http.delete", candidate["name"], {"name": "operations/delete", "done": True})

    def test_workflow_matches_pinned_deployed_handoff(self):
        # Handoff commit 1076d9a923e8202794b8bd4f37fdfeec81daf53e.
        # A later behavior change needs its own review, not an adoption-only edit.
        self.assertEqual(hashlib.sha256(WORKFLOW.read_bytes()).hexdigest(),
                         "34f8ee2098bba0388b907e1cf1cc05233065504d5ee95ccd7668958fa11e9f15")

    def test_fixed_configuration_and_http_contract(self):
        expected = set(ENV)
        used = set(re.findall(r'sys\.get_env\("([A-Z_]+)"\)', WORKFLOW.read_text()))
        block = re.search(r"user_env_vars\s*=\s*\{(.*?)^\s*\}",
                          (MODULE / "main.tf").read_text(), re.MULTILINE | re.DOTALL).group(1)
        declared = set(re.findall(r"^\s+([A-Z_]+)\s*=", block, re.MULTILINE))
        self.assertEqual(used, expected)
        self.assertEqual(declared, expected)
        self.assertNotIn("params", DOC["main"])

        def inspect(node):
            if isinstance(node, dict):
                if str(node.get("call", "")).startswith("http."):
                    self.assertEqual(node["args"]["auth"], {"type": "OAuth2"})
                if "try" in node:
                    self.assertEqual(node["retry"], "${http.default_retry}")
                self.assertNotIn("parallel", node)
                for value in node.values():
                    inspect(value)
            elif isinstance(node, list):
                for value in node:
                    inspect(value)
        inspect(DOC)

    def test_daily_name_and_labels_match_deployed_workflow(self):
        self.start()
        self.http.expect("http.get", PARENT + "/backups", {})  # empty inventory can omit backups
        result = self.runner.call()
        self.assertEqual(result, {"backup": TODAY, "state": "READY", "retentionDays": 30, "deleted": []})
        create = self.http.calls[0][1]
        self.assertEqual(create["query"], {"backupId": "g5-prod-daily-2026-11-04"})
        self.assertEqual(create["body"], {"sourceInstance": SOURCE, "sourceFileShare": SHARE,
                                         "labels": {"managed-by": "g5-filestore-backup", "backup-kind": "daily"}})

    def test_invalid_configuration_stops_before_any_api_call(self):
        for patch in ({"SOURCE_INSTANCE": ""}, {"SOURCE_SHARE": ""}, {"BACKUP_PARENT": ""},
                      {"BACKUP_PREFIX": ""}, {"RETENTION_DAYS": "29"}, {"BACKUP_PREFIX": "x" * 60}):
            with self.subTest(patch=patch):
                self.reset(env=ENV | patch)
                with self.assertRaisesRegex(WorkflowError, "invalid fixed backup configuration"):
                    self.runner.call()
                self.assertEqual(self.http.calls, [])

    def test_duplicate_day_409_reuses_verified_backup(self):
        self.start(duplicate=True)
        self.listing([])
        self.assertEqual(self.runner.call()["backup"], TODAY)
        self.assertEqual([method for method, _ in self.http.calls], ["http.post", "http.get", "http.get"])

    def test_duplicate_unexpected_source_share_labels_or_state_blocks_cleanup(self):
        for patch in ({"sourceInstance": "other"}, {"sourceFileShare": "other"}, {"labels": {}},
                      {"labels": {"managed-by": "someone-else", "backup-kind": "daily"}},
                      {"labels": {"managed-by": "g5-filestore-backup", "backup-kind": "initial"}},
                      {"state": "INVALID"}, {"state": "DELETING"}):
            with self.subTest(patch=patch):
                self.reset()
                self.start(duplicate=True, current=backup("2026-11-04", **patch))
                with self.assertRaisesRegex(WorkflowError, "refusing retention cleanup"):
                    self.runner.call()
                self.assertEqual(len(self.http.calls), 2)

    def test_creation_and_readiness_finish_before_listing(self):
        self.http.expect("http.post", PARENT + "/backups", {"name": "operations/create"})
        self.http.expect("http.get", "operations/create", {"name": "operations/create", "done": True})
        for state in ("CREATING", "FINALIZING", "READY"):
            self.http.expect("http.get", TODAY, backup("2026-11-04", state=state))
        self.listing([])
        self.assertEqual(self.runner.call()["deleted"], [])
        self.assertEqual(self.runner.now, NOW + 180)

    def test_full_elapsed_retention_and_all_pages_finish_before_sequential_deletes(self):
        expired = backup("edge", createTime="2026-10-05T07:59:59.999999Z")
        boundary = backup("boundary", createTime="2026-10-05T08:00:00.000000000Z")
        recent = backup("recent", createTime="2026-10-05T08:00:00.000001Z")
        initial = backup("initial", labels={"managed-by": "g5-filestore-backup", "backup-kind": "initial"})
        self.start()
        self.listing([expired, boundary], token="second-page")
        self.listing([recent, initial, backup("2026-11-04")])
        self.http.expect("http.get", expired["name"], expired)
        self.http.expect("http.delete", expired["name"], {"name": "operations/delete"})
        self.http.expect("http.get", "operations/delete", {"name": "operations/delete", "done": True})
        self.deletion(initial)
        self.assertEqual(self.runner.call()["deleted"], [expired["name"], initial["name"]])
        lists = [args for method, args in self.http.calls if method == "http.get" and args["url"] == API + PARENT + "/backups"]
        self.assertEqual([args["query"]["pageToken"] for args in lists], ["", "second-page"])

    def test_failed_later_page_prevents_all_deletions(self):
        self.start()
        self.listing([backup()], token="second-page")
        self.http.expect("http.get", PARENT + "/backups", WorkflowError({"code": 403}))
        with self.assertRaises(WorkflowError) as raised:
            self.runner.call()
        self.assertEqual(raised.exception.value, {"code": 403})
        self.assertFalse(any(method == "http.delete" for method, _ in self.http.calls))

    def test_missing_or_foreign_metadata_never_expires(self):
        cases = [{"name": TODAY}, {"sourceInstance": "other"}, {"sourceFileShare": "other"},
                 {"createTime": None}, {"labels": None}, {"labels": {}},
                 {"labels": {"managed-by": "someone-else", "backup-kind": "daily"}},
                 {"labels": {"managed-by": "g5-filestore-backup", "backup-kind": "manual"}}]
        cases += [{"state": state} for state in ("CREATING", "FINALIZING", "INVALID", "DELETING", None)]
        for patch in cases:
            with self.subTest(patch=patch):
                candidate = backup(**patch)
                if candidate["labels"] is None:
                    del candidate["labels"]
                self.assertFalse(self.runner.call("is_expired", backup=wrap(candidate), source=SOURCE,
                                                  share=SHARE, protectedName=TODAY, cutoff=NOW - 30 * 86400))

    def test_every_candidate_is_rechecked_for_mutable_metadata_before_delete(self):
        changes = [{"labels": {}}, {"labels": {"managed-by": "someone-else", "backup-kind": "daily"}},
                   {"labels": {"managed-by": "g5-filestore-backup", "backup-kind": "manual"}},
                   {"sourceInstance": "other"}, {"sourceFileShare": "other"}, {"state": "DELETING"},
                   {"createTime": "2026-11-04T07:00:00Z"}, {"name": TODAY}]
        candidates = [backup(str(index)) for index in range(len(changes))]
        self.start()
        self.listing(candidates)
        for candidate, change in zip(candidates, changes):
            self.http.expect("http.get", candidate["name"], candidate | change)
        self.assertEqual(self.runner.call()["deleted"], [])
        self.assertFalse(any(method == "http.delete" for method, _ in self.http.calls))

    def test_404_during_recheck_or_delete_skips_and_continues(self):
        absent, concurrent, remaining = [backup(str(index)) for index in range(3)]
        self.start()
        self.listing([absent, concurrent, remaining])
        self.http.expect("http.get", absent["name"], WorkflowError({"code": 404}))
        self.http.expect("http.get", concurrent["name"], concurrent)
        self.http.expect("http.delete", concurrent["name"], WorkflowError({"code": 404}))
        self.deletion(remaining)
        self.assertEqual(self.runner.call()["deleted"], [remaining["name"]])

    def test_partial_cleanup_failure_stops_before_the_next_candidate(self):
        for stage in ("read", "delete", "operation"):
            with self.subTest(stage=stage):
                self.reset()
                first, second, third = [backup(str(index)) for index in range(3)]
                self.start()
                self.listing([first, second, third])
                self.deletion(first)
                self.http.expect("http.get", second["name"], WorkflowError({"code": 403}) if stage == "read" else second)
                if stage != "read":
                    failure = WorkflowError({"code": 403}) if stage == "delete" else {"name": "operations/delete", "error": {"code": 13}}
                    self.http.expect("http.delete", second["name"], failure)
                with self.assertRaises(WorkflowError) as raised:
                    self.runner.call()
                self.assertEqual(raised.exception.value, {"code": 13 if stage == "operation" else 403})
                self.assertFalse(any(args["url"] == API + third["name"] for _, args in self.http.calls))

    def test_creation_failure_does_not_start_retention(self):
        for creation in (WorkflowError({"code": 403}), {"name": "operations/create", "done": True, "error": {"code": 13}}):
            with self.subTest(creation=creation):
                self.reset()
                self.http.expect("http.post", PARENT + "/backups", creation)
                with self.assertRaises(WorkflowError):
                    self.runner.call()
                self.assertEqual(len(self.http.calls), 1)

    def test_creation_operation_times_out_at_24_hours_without_cleanup(self):
        operation = {"name": "operations/create"}
        self.http.expect("http.post", PARENT + "/backups", operation)
        self.http.expect("http.get", operation["name"], operation, count=1440)
        with self.assertRaisesRegex(WorkflowError, "24-hour deadline"):
            self.runner.call()
        self.assertEqual(self.runner.now, NOW + 86400)

    def test_readiness_times_out_at_24_hours_without_cleanup(self):
        self.http.expect("http.post", PARENT + "/backups", WorkflowError({"code": 409}))
        self.http.expect("http.get", TODAY, backup("2026-11-04", state="FINALIZING"), count=1441)
        with self.assertRaisesRegex(WorkflowError, "did not become READY within 24 hours"):
            self.runner.call()
        self.assertEqual(self.runner.now, NOW + 86400)

    def test_each_deletion_receives_its_own_full_24_hour_deadline(self):
        candidate, untouched = backup("first"), backup("second")
        self.http.expect("http.post", PARENT + "/backups", {"name": "operations/create"})
        self.http.expect("http.get", "operations/create", {"name": "operations/create", "done": True})
        self.http.expect("http.get", TODAY, backup("2026-11-04"))
        self.listing([candidate, untouched])
        self.http.expect("http.get", candidate["name"], candidate)
        operation = {"name": "operations/delete"}
        self.http.expect("http.delete", candidate["name"], operation)
        self.http.expect("http.get", operation["name"], operation, count=1440)
        with self.assertRaisesRegex(WorkflowError, "24-hour deadline"):
            self.runner.call()
        self.assertEqual(self.runner.now, NOW + 60 + 86400)


if __name__ == "__main__":
    print("Offline YAML control-flow tests; no cloud execution, compilation, IAM or HTTP retry validation.", flush=True)
    unittest.main(verbosity=2)
