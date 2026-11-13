"""Worker publication/provenance contracts with stateful local CLI fakes."""
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

import worker_artifacts as wa
import resolve_worker_image as resolver
import await_worker_image as gate
from test_await_worker_image import GateRunner, FakeClock, EXACT, COMMIT, FAMILY, TRIGGER, TRIGGER_ID

QA = "g5-qa-1ccc"
ADMIN = "g5lean-admin-1ccc"
BUILD_ID = "12345678-1234-1234-1234-123456789abc"
VERSION = "g5-worker-v20261021-1200-" + COMMIT[:5] + "-" + BUILD_ID.replace("-", "")


def result(stdout="", code=0):
    return subprocess.CompletedProcess([], code, stdout, "simulated conflict" if code else "")


class PublicationTest(unittest.TestCase):
    def test_pipeline_version_and_layout_guard_runs_before_any_publication(self):
        root = Path(__file__).resolve().parents[1]
        yaml = (root / "infra/ci/cloudbuild-packer-worker.yaml").read_text()
        section = yaml.split("  - id: compute-image-version\n", 1)[1]
        body = section.split("      - |\n", 1)[1].split("        # Codex review N3:", 1)[0]
        body = "\n".join(line[8:] for line in body.splitlines())
        for owner, image, registry, accepted in ((QA, QA, QA, True), (ADMIN, ADMIN, ADMIN, True),
                                                (ADMIN, QA, ADMIN, False), (ADMIN, ADMIN, QA, False),
                                                ("foreign-project", "foreign-project", "foreign-project", False)):
            with self.subTest(owner=owner, image=image, registry=registry), tempfile.TemporaryDirectory() as directory:
                values = {"PROJECT_ID": QA, "_ARTIFACT_PROJECT": owner, "_GCE_IMAGE_PROJECT": image,
                          "_AR_PROJECT": registry, "_AR_REGION": "us-central1", "_AR_REPOSITORY": "g5-worker-images",
                          "_IMAGES_BUCKET_SUFFIX": "g5-worker-images", "_IMAGE_FAMILY": "g5-worker",
                          "_COSIGN_KMS_KEY": f"gcpkms://projects/{QA}/locations/us-central1/keyRings/g5-worker-signer/cryptoKeys/cosign/cryptoKeyVersions/1",
                          "COMMIT_SHA": COMMIT, "BUILD_ID": BUILD_ID, "_RUN_FULL_BUILD": "true"}
                script = re.sub(r"(?<!\$)\$\{([A-Z_]+)\}", lambda match: values[match[1]], body)
                script = script.replace("$$", "$").replace("/workspace/scripts/worker_artifacts.py", str(root / "scripts/worker_artifacts.py"))
                script = script.replace("/workspace/", directory + "/")
                completed = subprocess.run(["bash", "-euo", "pipefail", "-c", script], text=True, capture_output=True)
                self.assertEqual(completed.returncode == 0, accepted, completed.stdout + completed.stderr)
                version_file = Path(directory, "image_version.txt")
                self.assertEqual(version_file.exists(), accepted)
                if accepted:
                    value = version_file.read_text().strip()
                    self.assertEqual(len(value), 63)
                    self.assertRegex(value, r"^g5-worker-v[0-9]{8}-[0-9]{4}-" + COMMIT[:5] + "-" + BUILD_ID.replace("-", "") + "$")
        self.assertIn('ISO_CACHE="gs://${PROJECT_ID}-${_IMAGES_BUCKET_SUFFIX}/iso/', yaml)
        self.assertIn('"gs://${PROJECT_ID}-${_IMAGES_BUCKET_SUFFIX}/latest.json"', yaml)
        self.assertIn("location: 'gs://${PROJECT_ID}-build-artifacts/", yaml)

    def test_layout_has_one_approved_owner_and_qa_operational_pointer(self):
        for owner in (QA, ADMIN):
            selected = wa.layout(QA, owner, owner, "us-central1", "g5-worker-images", "g5-worker-images", "g5-worker")
            self.assertEqual(selected["bucket"], f"{owner}-g5-worker-images")
            self.assertEqual(selected["repository"], f"us-central1-docker.pkg.dev/{owner}/g5-worker-images/g5-worker")
            self.assertEqual(selected["latest"], f"gs://{QA}-g5-worker-images/latest.json")
        for build, owner, image in ((QA, "foreign-project", "foreign-project"),
                                    (ADMIN, ADMIN, ADMIN), (QA, ADMIN, QA)):
            with self.subTest(owner=owner, image=image), self.assertRaises(ValueError):
                wa.layout(build, owner, image, "us-central1", "g5-worker-images", "g5-worker-images", "g5-worker")

    def test_object_first_write_and_equal_retry_never_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory, "disk.raw")
            path.write_bytes(b"real artifact bytes\0")
            sha, md5, size = wa.fingerprint(path)
            obj = {"bucket": f"{ADMIN}-g5-worker-images", "name": f"{VERSION}/disk.raw",
                   "md5Hash": md5, "size": str(size), "metadata": {"sha256": sha}, "generation": "45"}
            uri = f"gs://{obj['bucket']}/{obj['name']}"
            for upload_status in (0, 1):
                calls = []
                def cloud(*args, **kwargs):
                    calls.append(args)
                    if args[2] == "cp":
                        self.assertEqual(kwargs["env"]["CLOUDSDK_STORAGE_PARALLEL_COMPOSITE_UPLOAD_ENABLED"], "False")
                    return result(code=upload_status) if args[2] == "cp" else result(json.dumps(obj))
                self.assertEqual(wa.upload_create_only(path, uri, cloud), obj)
                self.assertIn("--if-generation-match=0", calls[0])
                self.assertIn("--raw", calls[1])
            for field, value in (("md5Hash", "bad"), ("size", "900"), ("metadata", {}),
                                 ("name", "latest.json"), ("generation", "")):
                def collision(*args, **kwargs):
                    return result(code=1) if args[2] == "cp" else result(json.dumps({**obj, field: value}))
                with self.subTest(field=field), self.assertRaises(ValueError):
                    wa.upload_create_only(path, uri, collision)

    def test_oci_publication_is_deterministic_and_content_addressed(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = Path(directory)
            for filename in ("manifest.json", "sbom.spdx.json", "disk-pointer.json"):
                (stage / filename).write_text(json.dumps({"file": filename}))
            calls, blobs = [], {}
            def registry(*args, **kwargs):
                calls.append(args)
                if args[2] == "push":
                    payload = Path(args[-1]).read_bytes()
                    self.assertEqual(args[-2].split("@sha256:")[1], hashlib.sha256(payload).hexdigest())
                    blobs[args[-2]] = payload
                    return result()
                return result(blobs[args[-1]].decode())
            repository = f"us-central1-docker.pkg.dev/{ADMIN}/g5-worker-images/g5-worker"
            first = wa.publish_oci(stage, repository, registry)
            self.assertEqual(first, wa.publish_oci(stage, repository, registry))
            self.assertTrue(all("@sha256:" in call[3] for call in calls))
            (stage / "manifest.json").write_text('{"different":true}')
            self.assertNotEqual(first, wa.publish_oci(stage, repository, registry))

    @unittest.skipUnless(os.environ.get("ORAS_BINARY"), "set ORAS_BINARY for real local OCI roundtrip")
    def test_real_oras_local_layout_roundtrip_and_retry(self):
        binary = os.environ["ORAS_BINARY"]
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            stage = base / "stage"
            stage.mkdir()
            names = ("manifest.json", "sbom.spdx.json", "disk-pointer.json")
            for name in names:
                (stage / name).write_text(json.dumps({"fixture": name}))
            repository = f"us-central1-docker.pkg.dev/{ADMIN}/g5-worker-images/g5-worker"
            def local(*args, **kwargs):
                args = list(args)
                args[0] = binary
                args[3] = args[3].replace(repository, str(base / "layout"))
                args.append("--oci-layout")
                return subprocess.run(args, check=True, text=True, capture_output=True)
            first = wa.publish_oci(stage, repository, local)
            self.assertEqual(first, wa.publish_oci(stage, repository, local))
            output = base / "pulled"
            subprocess.run([binary, "pull", "--oci-layout", str(base / "layout") + "@" + first.split("@")[1],
                            "-o", str(output)], check=True, capture_output=True)
            for name in names:
                self.assertEqual((stage / name).read_bytes(), (output / name).read_bytes())

    def test_image_retry_requires_same_source_metadata_and_features(self):
        images, calls = [], []
        def cloud(*args, **kwargs):
            calls.append(args)
            if args[3] == "list":
                return result(json.dumps(images))
            options = dict(arg[2:].split("=", 1) for arg in args if arg.startswith("--") and "=" in arg)
            images.append({"name": args[4], "family": options["family"], "status": "READY",
                           "description": options["description"],
                           "labels": dict(pair.split("=", 1) for pair in options["labels"].split(",")),
                           "guestOsFeatures": [{"type": feature} for feature in options["guest-os-features"].split(",")]})
            return result()
        arguments = (ADMIN, VERSION, f"gs://{ADMIN}-g5-worker-images/{VERSION}/{VERSION}.tar.gz",
                     "f" * 64, "123", QA, BUILD_ID, COMMIT, "20261021-1200", "registry/metadata@sha256:" + "a" * 64)
        self.assertLessEqual(len(VERSION), 63)
        self.assertEqual(wa.import_image(*arguments, runner=cloud), f"projects/{ADMIN}/global/images/{VERSION}")
        self.assertEqual(wa.import_image(*arguments, runner=cloud), f"projects/{ADMIN}/global/images/{VERSION}")
        self.assertEqual(sum(call[3] == "create" for call in calls), 1)
        original = copy.deepcopy(images[0])
        for changed in ({"description": "different"}, {"family": "different"}, {"status": "PENDING"},
                        {"labels": {}}, {"guestOsFeatures": []}):
            images[0] = {**original, **changed}
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                wa.import_image(*arguments, runner=cloud)
        self.assertEqual(sum(call[3] == "create" for call in calls), 1)

    def test_verification_client_accepts_old_tags_and_new_digests(self):
        root = Path(__file__).resolve().parents[1]
        repository = f"us-central1-docker.pkg.dev/{ADMIN}/g5-worker-images/g5-worker"
        for reference, signed_version, accepted in ((repository + ":" + VERSION, VERSION, True),
                (repository + "@sha256:" + "a" * 64, VERSION, True),
                (repository.replace(ADMIN, QA) + "@sha256:" + "a" * 64, VERSION, False),
                (repository + "@sha256:" + "a" * 64, "another-version", False)):
            with self.subTest(reference=reference, signed_version=signed_version), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                tools, work, fixtures = base / "bin", base / "work", base / "fixtures"
                for path in (tools, work, fixtures):
                    path.mkdir()
                archive = b"the worker archive bytes"
                sha = hashlib.sha256(archive).hexdigest()
                (fixtures / f"{VERSION}.tar.gz").write_bytes(archive)
                (fixtures / "manifest.json").write_text('{}')
                (fixtures / "summary.json").write_text(json.dumps({"gar_ref": reference, "tarball_sha256": sha}))
                (fixtures / "disk-pointer.json").write_text(json.dumps({"tarball_sha256": sha, "image_version": signed_version}))
                for tool, source in {
                    "cosign": "pass",
                    "gsutil": "shutil.copyfile(Path(os.environ['FIXTURES']) / Path(sys.argv[2]).name, sys.argv[3])",
                    "oras": "shutil.copyfile(Path(os.environ['FIXTURES']) / 'disk-pointer.json', Path.cwd() / 'disk-pointer.json')",
                }.items():
                    path = tools / tool
                    path.write_text(f"#!{sys.executable}\nimport os,sys,shutil\nfrom pathlib import Path\n{source}\n")
                    path.chmod(0o755)
                env = {**os.environ, "PATH": str(tools) + os.pathsep + os.environ["PATH"],
                       "PROJECT_ID": QA, "ARTIFACT_PROJECT": ADMIN, "COSIGN_KEY": "fixture-public-key",
                       "WORK_DIR": str(work), "FIXTURES": str(fixtures)}
                completed = subprocess.run(["bash", str(root / "scripts/verify-worker-image.sh"), VERSION],
                                           env=env, text=True, capture_output=True)
                self.assertEqual(completed.returncode == 0, accepted, completed.stdout + completed.stderr)


def admin_image():
    image = copy.deepcopy(EXACT)
    image["labels"].update(g5_build_project=QA, g5_build_id=BUILD_ID, g5_git_commit=COMMIT)
    return image


def source_build(status="SUCCESS"):
    return {"id": BUILD_ID, "buildTriggerId": TRIGGER_ID, "status": status,
            "serviceAccount": f"projects/{QA}/serviceAccounts/g5-lean-images@{QA}.iam.gserviceaccount.com",
            "sourceProvenance": {"resolvedGitSource": {"revision": COMMIT}},
            "substitutions": {"_ARTIFACT_PROJECT": ADMIN, "_AR_PROJECT": ADMIN,
                              "_GCE_IMAGE_PROJECT": ADMIN, "_RUN_FULL_BUILD": "true"}}


class ProvenanceTest(unittest.TestCase):
    def test_admin_image_requires_successful_qa_worker_build_and_matching_owner(self):
        calls = []
        build = source_build()
        def cloud(command):
            calls.append(command)
            return (0, json.dumps(build), "") if command[2] == "describe" else (0, TRIGGER_ID, "")
        resolver.validate_image_provenance(admin_image(), ADMIN, QA, "us-central1", cloud)
        self.assertTrue(all(f"--project={QA}" in call for call in calls))
        good = copy.deepcopy(build)
        for changed in ({"status": "FAILURE"}, {"buildTriggerId": "wrong"}, {"serviceAccount": "pr-ci"},
                        {"sourceProvenance": {"resolvedGitSource": {"revision": "b" * 40}}},
                        {"substitutions": {**good["substitutions"], "_GCE_IMAGE_PROJECT": QA}}):
            build = {**good, **changed}
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                resolver.validate_image_provenance(admin_image(), ADMIN, QA, "us-central1", cloud)
        with self.assertRaises(ValueError):
            resolver.validate_image_provenance(EXACT, ADMIN, QA, "us-central1", cloud)

    def test_admin_gate_queries_images_in_admin_and_waits_for_qa_success(self):
        class AdminRunner(GateRunner):
            def __init__(self):
                super().__init__([[admin_image()]])
                self.publication_status = ["WORKING", "SUCCESS"]
                self.commands = []
            def __call__(self, command):
                self.commands.append(command)
                if command[:3] == ["gcloud", "builds", "describe"]:
                    return 0, json.dumps(source_build(self.publication_status.pop(0))), ""
                if command[:4] == ["gcloud", "builds", "triggers", "describe"] and "--format=value(id)" in command:
                    return 0, TRIGGER_ID, ""
                return super().__call__(command)
        runner, clock = AdminRunner(), FakeClock()
        resolved = gate.await_worker_image(image_project=ADMIN, family=FAMILY, commit=COMMIT, repo_dir=".",
            build_project=QA, region="us-central1", trigger=TRIGGER, wait_secs=60, ready_grace_secs=60,
            poll_secs=5, runner=runner, sleep=clock.sleep, clock=clock)
        self.assertEqual(resolved, f"projects/{ADMIN}/global/images/{EXACT['name']}")
        self.assertEqual(clock.sleeps, [5])
        for command in runner.commands:
            if command[:3] == ["gcloud", "compute", "images"]:
                self.assertIn(f"--project={ADMIN}", command)
            elif command[:2] == ["gcloud", "builds"]:
                self.assertIn(f"--project={QA}", command)


if __name__ == "__main__":
    unittest.main()
