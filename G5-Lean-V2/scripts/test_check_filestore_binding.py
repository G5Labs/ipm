"""Hermetic deployment storage checks; synthetic Kubernetes responses only."""
from __future__ import annotations

import copy
import io
import json
import subprocess
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import check_filestore_binding as guard


ARGS = ["--namespace", "g5", "--pvc", "g5-repos-pvc", "--pv", "g5-repos-pv",
        "--server", "10.0.0.1", "--share", "g5_repos"]


def binding():
    return {
        "deployments": {"items": [
            {"metadata": {"name": name, "namespace": "g5"}, "spec": {
                "replicas": 1, "template": {"spec": {"volumes": [
                    {"name": "repos", "persistentVolumeClaim": {"claimName": "g5-repos-pvc"}}
                ]}}}}
            for name in ("g5-api", "g5-daemon")
        ]},
        "persistentvolumeclaims": {"items": [{
            "metadata": {"name": "g5-repos-pvc", "namespace": "g5", "uid": "claim-uid"},
            "spec": {"volumeName": "g5-repos-pv"}, "status": {"phase": "Bound"},
        }]},
        "persistentvolumes": {"items": [{
            "metadata": {"name": "g5-repos-pv"},
            "spec": {"persistentVolumeReclaimPolicy": "Retain",
                     "claimRef": {"name": "g5-repos-pvc", "namespace": "g5", "uid": "claim-uid"},
                     "nfs": {"server": "10.0.0.1", "path": "/g5_repos"}},
            "status": {"phase": "Bound"},
        }]},
    }


class FilestoreBindingTests(unittest.TestCase):
    def run_guard(self, documents=None, *, args=ARGS, failure=None, code=0):
        documents = binding() if documents is None else documents
        commands = []

        def kubectl(command, **options):
            commands.append(command)
            self.assertEqual(command[:2], ["kubectl", "get"])
            self.assertIn("--request-timeout=30s", command)
            self.assertEqual(options["timeout"], 40)
            resource = command[2]
            if failure and failure[0] == resource:
                if isinstance(failure[1], Exception):
                    raise failure[1]
                return failure[1]
            return SimpleNamespace(returncode=0, stdout=json.dumps(documents[resource]))

        stdout, stderr = io.StringIO(), io.StringIO()
        with patch.object(guard.subprocess, "run", side_effect=kubectl), \
                patch("sys.stdout", stdout), patch("sys.stderr", stderr):
            result = guard.main(args)
        self.assertEqual(result, code, stdout.getvalue() + stderr.getvalue())
        if code:
            self.assertIn("Filestore deploy refused", stderr.getvalue())
            self.assertEqual(stdout.getvalue(), "")
        return stdout.getvalue(), commands

    def test_matched_existing_installation_can_deploy(self):
        output, commands = self.run_guard()
        self.assertIn("binding verified", output)
        self.assertTrue(all(command[-2:] == ["--namespace", "g5"] for command in commands[:2]))
        self.assertNotIn("--namespace", commands[2])

    def test_completed_manual_cutover_accepts_the_new_selected_binding(self):
        # A different identity is valid once both workload templates and the
        # actual claim binding have already moved together under maintenance.
        documents = json.loads(json.dumps(binding()).replace("g5-repos-", "g5-repos-zonal-")
                               .replace("10.0.0.1", "10.0.0.2"))
        args = [value.replace("g5-repos-", "g5-repos-zonal-").replace("10.0.0.1", "10.0.0.2")
                for value in ARGS]
        self.run_guard(documents, args=args)

    def test_stopped_or_unready_workloads_and_unrelated_deployments_do_not_block_recovery(self):
        documents = binding()
        for deployment in documents["deployments"]["items"]:
            deployment["spec"]["replicas"] = 0
            deployment["status"] = {"availableReplicas": 0}
        documents["deployments"]["items"].append({"metadata": {"name": "g5-edge"}})
        self.run_guard(documents)

    def test_empty_installation_bootstraps(self):
        output, _ = self.run_guard({resource: {"items": []} for resource in binding()})
        self.assertIn("bootstrap permitted", output)

    def test_existing_binding_cannot_be_switched_by_new_or_stale_build_inputs(self):
        for option, value in (("--pvc", "g5-repos-zonal-pvc"),
                              ("--pv", "g5-repos-zonal-pv"),
                              ("--server", "10.0.0.2"), ("--share", "other_share")):
            args = list(ARGS)
            args[args.index(option) + 1] = value
            with self.subTest(option=option):
                self.run_guard(args=args, code=1)

    def test_partial_or_precreated_installation_is_not_bootstrap(self):
        for resource in binding():
            documents = binding()
            documents[resource]["items"] = []
            with self.subTest(missing=resource):
                self.run_guard(documents, code=1)
        documents = binding()
        documents["deployments"]["items"].pop()
        self.run_guard(documents, code=1)
        # Both Deployments lost, and a stale build selects absent new names:
        # retained original storage still disproves a genuine first install.
        documents["deployments"]["items"] = []
        args = [value.replace("g5-repos-", "g5-repos-zonal-") for value in ARGS]
        self.run_guard(documents, args=args, code=1)

    def test_each_workload_must_use_the_selected_writable_claim(self):
        for index in (0, 1):
            for volume in (None, {"claimName": "old-pvc"},
                           {"claimName": "g5-repos-pvc", "readOnly": True}):
                documents = binding()
                volumes = documents["deployments"]["items"][index]["spec"]["template"]["spec"]["volumes"]
                volumes[0] = {"name": "repos", "persistentVolumeClaim": volume} if volume else {"name": "repos", "emptyDir": {}}
                with self.subTest(workload=index, volume=volume):
                    self.run_guard(documents, code=1)

    def test_bound_status_and_current_claim_incarnation_are_required(self):
        changes = [
            ("persistentvolumeclaims", "metadata", "uid", ""),
            ("persistentvolumeclaims", "metadata", "namespace", "other"),
            ("persistentvolumeclaims", "spec", "volumeName", "old-pv"),
            ("persistentvolumeclaims", "status", "phase", "Pending"),
            ("persistentvolumes", "status", "phase", "Released"),
            ("persistentvolumes", "spec", "persistentVolumeReclaimPolicy", "Delete"),
        ]
        for resource, section, key, value in changes:
            documents = binding()
            documents[resource]["items"][0][section][key] = value
            with self.subTest(resource=resource, key=key):
                self.run_guard(documents, code=1)
        for key in ("uid", "name", "namespace"):
            documents = binding()
            documents["persistentvolumes"]["items"][0]["spec"]["claimRef"][key] = "stale"
            with self.subTest(claimRef=key):
                self.run_guard(documents, code=1)

    def test_nfs_identity_and_read_write_mode_are_required(self):
        for nfs in ({}, {"server": "10.0.0.2", "path": "/g5_repos"},
                    {"server": "10.0.0.1", "path": "/other"},
                    {"server": "10.0.0.1", "path": "/g5_repos", "readOnly": True}):
            documents = binding()
            documents["persistentvolumes"]["items"][0]["spec"]["nfs"] = nfs
            with self.subTest(nfs=nfs):
                self.run_guard(documents, code=1)

    def test_deletion_in_progress_is_not_a_stable_binding(self):
        for resource in binding():
            documents = binding()
            documents[resource]["items"][0]["metadata"]["deletionTimestamp"] = "2026-11-07T00:00:00Z"
            with self.subTest(resource=resource):
                self.run_guard(documents, code=1)

    def test_permission_timeout_and_command_failures_never_become_absence(self):
        for resource in binding():
            for failure in (SimpleNamespace(returncode=1, stdout=""),
                            subprocess.TimeoutExpired("kubectl", 40), FileNotFoundError("kubectl")):
                with self.subTest(resource=resource, failure=failure):
                    self.run_guard(failure=(resource, failure), code=1)

    def test_unreadable_or_malformed_inventory_fails_closed(self):
        for text in ("", "not json", "null", "{}", '{"items":null}', '{"items":[null]}',
                     '{"items":[{"metadata":{}}]}'):
            self.run_guard(failure=("deployments", SimpleNamespace(returncode=0, stdout=text)), code=1)
        documents = binding()
        documents["deployments"]["items"].append(copy.deepcopy(documents["deployments"]["items"][0]))
        self.run_guard(documents, code=1)


if __name__ == "__main__":
    unittest.main()
