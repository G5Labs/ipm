"""Hermetic release tests; no cloud operations or secrets required."""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import app_release as ar
import run_release_checks as checks

SHA = "a" * 40
OLD = "b" * 40
ORIGIN = "11111111-1111-1111-1111-111111111111"
PARENT = "22222222-2222-2222-2222-222222222222"
CURRENT = "33333333-3333-3333-3333-333333333333"
PROJECT = "g5-qa-1ccc"
REGISTRY = f"us-central1-docker.pkg.dev/{PROJECT}/g5"
REFS = {key: f"{REGISTRY}/{name}@sha256:" + "c" * 64 for key, name in ar.IMAGES.items()}
REFS["WORKER_IMAGE_REF"] = f"projects/{PROJECT}/global/images/g5-worker-immutable"
BUCKET = f"{PROJECT}-lean-releases"
URI = f"gs://{BUCKET}/app-releases/{ORIGIN}.json#123"
CENTRAL = f"projects/{PROJECT}/serviceAccounts/g5-lean-release@{PROJECT}.iam.gserviceaccount.com"


def build(build_id=ORIGIN, trigger="qa-trigger", sha=SHA, status="SUCCESS", **substitutions):
    return {"id": build_id, "buildTriggerId": trigger, "status": status,
            "serviceAccount": f"projects/{PROJECT}/serviceAccounts/g5-lean-deploy@{PROJECT}.iam.gserviceaccount.com",
            "sourceProvenance": {"resolvedGitSource": {"revision": sha}},
            "substitutions": {"COMMIT_SHA": sha, "_ARTIFACT_PROJECT": PROJECT,
                              "_AR_PROJECT": PROJECT, "_WORKER_IMAGE_PROJECT": PROJECT,
                              "_RELEASE_BUCKET": BUCKET, **substitutions}}


def manifest():
    return {"version": 1, "commit": SHA, "project": PROJECT, "qa_build_id": ORIGIN, "refs": dict(REFS)}


def qa_trigger(artifact_project=PROJECT):
    return {"id": "qa-trigger", "name": "g5-deploy-qa",
            "serviceAccount": f"projects/{PROJECT}/serviceAccounts/g5-lean-deploy@{PROJECT}.iam.gserviceaccount.com",
            "substitutions": {"_ARTIFACT_PROJECT": artifact_project, "_AR_PROJECT": artifact_project,
                              "_WORKER_IMAGE_PROJECT": artifact_project,
                              "_RELEASE_BUCKET": f"{artifact_project}-lean-releases",
                              "_NEXT_TRIGGER": "g5-promote-staging"}}


def promotion_trigger(environment="staging", checks=True, artifact_project=PROJECT):
    project, cluster = ar.TARGETS[environment]
    policy = {
        **ar.public_settings(environment, {}),
        "_ENVIRONMENT": environment, "_TARGET_PROJECT": project, "_GKE_CLUSTER": cluster,
        "_GKE_ZONE": "us-central1", "_HELM_RELEASE": "g5", "_HELM_NAMESPACE": "g5",
        "_RELEASE_BUCKET": f"{artifact_project}-lean-releases",
        "_ARTIFACT_PROJECT": artifact_project,
        "_AR_PROJECT": artifact_project, "_AR_REGION": "us-central1", "_AR_REPOSITORY": "g5",
        "_WORKER_IMAGE_PROJECT": artifact_project, "_PROMOTION_MODE": "deploy",
        "_RELEASE_CHECKS_REQUIRED": str(checks).lower(),
        "_RELEASE_CHECK_BASE_URL": f"https://{environment}.example" if checks else "",
        "_RELEASE_CHECK_SECRET": "release-fixtures" if checks else "",
        "_NEXT_TRIGGER": "g5-promote-prod" if environment == "staging" and checks else "",
        "_FILESTORE_INSTANCE": f"g5-lean-{environment}-filestore", "_FILESTORE_ZONE": "us-central1-a",
        "_FILESTORE_PV_NAME": "g5-repos-pv", "_FILESTORE_PVC_NAME": "g5-repos-pvc",
        "_FILESTORE_STORAGE": "1Ti",
    }
    return {"id": f"{environment}-trigger", "name": f"g5-promote-{environment}",
            "serviceAccount": CENTRAL, "approvalConfig": {"approvalRequired": environment == "staging"},
            "substitutions": policy}


def promotion_build(environment="staging", checks=True, build_id=CURRENT, artifact_project=PROJECT, **overrides):
    trigger = promotion_trigger(environment, checks, artifact_project)
    record = build(build_id, trigger=trigger["id"], **{**trigger["substitutions"], **overrides})
    record["serviceAccount"] = CENTRAL
    if environment == "staging":
        record["approval"] = {"state": "APPROVED"}
    return record


class ReleaseTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name)
        self.env = {"PROJECT_ID": PROJECT, "BUILD_ID": CURRENT, "COMMIT_SHA": SHA,
                    "_ARTIFACT_PROJECT": PROJECT, "_AR_PROJECT": PROJECT, "_WORKER_IMAGE_PROJECT": PROJECT,
                    "_AR_REGION": "us-central1", "_AR_REPOSITORY": "g5", "_ENVIRONMENT": "staging",
                    "WORKSPACE_DIR": str(self.path), "_RELEASE_BUCKET": BUCKET,
                    "_RELEASE_MANIFEST": URI, "_SOURCE_BUILD_ID": ORIGIN,
                    "_WORKER_IMAGE_REF": REFS["WORKER_IMAGE_REF"],
                    "_TARGET_PROJECT": "g5lean-staging-1ccc", "_GKE_CLUSTER": "g5-lean-staging",
                    "_GKE_ZONE": "us-central1", "_HELM_NAMESPACE": "g5", "_NEXT_TRIGGER": "g5-promote-prod"}
        (self.path / "image-refs.env").write_text("\n".join(f"export {key}={value}" for key, value in REFS.items()))
        (self.path / "worker-image.env").write_text("WORKER_IMAGE_REF=" + REFS["WORKER_IMAGE_REF"])
        (self.path / "deploy-control.env").write_text("SKIP_DEPLOY=false\n")
        self.release = ar.Release(self.env)

    def test_source_revision_is_not_trusted_from_substitutions(self):
        with self.assertRaises(ValueError):
            ar.build_source({"substitutions": {"COMMIT_SHA": SHA}, "source": {"gitSource": {"revision": "main"}}})
        self.assertEqual(ar.build_source(build()), SHA)
        self.assertEqual(ar.build_source({"source": {"connectedRepository": {"revision": SHA}}}), SHA)

    def test_refs_reject_mutable_foreign_and_missing_images(self):
        ar.validate_refs(REFS, REGISTRY, PROJECT)
        for key, value in (("API_IMAGE_REF", REGISTRY + "/api:latest"),
                           ("API_IMAGE_REF", "foreign/api@sha256:" + "c" * 64),
                           ("G5D_G5DEV_IMAGE_REF", ""),
                           ("WORKER_IMAGE_REF", f"projects/{PROJECT}/global/images/family/g5-worker")):
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                ar.validate_refs({**REFS, key: value}, REGISTRY, PROJECT)

    def test_manifest_requires_bucket_generation_and_origin(self):
        with patch.object(ar, "gcloud", return_value=json.dumps(manifest())):
            self.assertEqual(self.release.manifest(), manifest())
            for uri in (URI.split("#")[0], URI.replace(BUCKET, "ci-writable"), URI.replace(ORIGIN, CURRENT)):
                with self.subTest(uri=uri), self.assertRaises(ValueError):
                    ar.Release({**self.env, "_RELEASE_MANIFEST": uri}).manifest()

    def test_parent_must_finish_successfully_on_expected_trigger_and_source(self):
        pending, succeeded = build(status="WORKING"), build()
        with patch.object(self.release, "trigger_id", return_value="qa-trigger"), \
             patch.object(self.release, "describe", side_effect=[pending, succeeded]), \
             patch.object(ar.time, "sleep") as sleep:
            self.assertEqual(self.release.successful_build(ORIGIN, "g5-deploy-qa"), succeeded)
            sleep.assert_called_once()
        for bad in (build(status="FAILURE"), build(trigger="other"), build(sha=OLD)):
            with self.subTest(bad=bad), patch.object(self.release, "trigger_id", return_value="qa-trigger"), \
                 patch.object(self.release, "describe", return_value=bad), self.assertRaises(ValueError):
                self.release.successful_build(ORIGIN, "g5-deploy-qa")

    def run_preflight(self, parent=None, source=SHA, current=None, triggers=None):
        def cloud(*args):
            if args[:3] == ("builds", "triggers", "describe"):
                if args[3] == "g5-deploy-qa":
                    return json.dumps(qa_trigger(self.release.artifact_project))
                environment = args[3].removeprefix("g5-promote-")
                return json.dumps((triggers or {}).get(environment, promotion_trigger(environment, artifact_project=self.release.artifact_project)))
            raise AssertionError(args)
        def successful(build_id, trigger_name):
            if trigger_name == "g5-deploy-qa":
                return build(**qa_trigger(self.release.artifact_project)["substitutions"])
            return parent or build()
        with patch.object(ar, "run", return_value=source), \
             patch.object(self.release, "describe", return_value=current or promotion_build(self.release.environment, artifact_project=self.release.artifact_project)), \
             patch.object(self.release, "manifest", return_value=manifest()), \
             patch.object(self.release, "successful_build", side_effect=successful), \
             patch.object(self.release, "trigger_id", return_value="infra-trigger"), \
             patch.object(ar, "gcloud", side_effect=cloud):
            self.release.preflight()

    def test_application_release_does_not_query_infrastructure_history(self):
        # G5's train selects infrastructure and app candidates independently.
        # The cloud stub refuses every Terraform trigger/history query.
        self.run_preflight()
        with self.assertRaises(ValueError):
            self.run_preflight(source=OLD)

    def test_production_requires_the_tested_staging_manifest(self):
        self.release = ar.Release({**self.env, "_ENVIRONMENT": "prod", "_SOURCE_BUILD_ID": PARENT,
                                  **promotion_trigger("prod")["substitutions"]})
        self.run_preflight(parent=promotion_build(build_id=PARENT, _RELEASE_MANIFEST=URI))
        with self.assertRaises(ValueError):
            self.run_preflight(parent=promotion_build(build_id=PARENT, _RELEASE_MANIFEST=URI + "0"))

    def test_current_build_must_be_the_approved_target_trigger(self):
        for key, value in (("buildTriggerId", "prod-trigger"), ("approval", {"state": "PENDING"}),
                           ("serviceAccount", "projects/qa/serviceAccounts/pr-ci@example.com")):
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.run_preflight(current={**promotion_build(), key: value})

    def test_public_configuration_is_bound_to_trusted_trigger_without_history_queries(self):
        settings = {"external_access_enabled": True, "app_hostname": "next.lean.g5labs.ai",
                    "preview_hostname": "preview.next.lean.g5labs.ai"}
        trigger = promotion_trigger()
        trigger["substitutions"].update(ar.public_settings("staging", settings))
        trigger["substitutions"]["_RELEASE_CHECK_BASE_URL"] = "https://next.lean.g5labs.ai"
        current = promotion_build(**trigger["substitutions"])
        self.release = ar.Release({**self.env, **trigger["substitutions"]})
        triggers = {"staging": trigger}
        # The existing cloud stub refuses infrastructure history/config lookups.
        self.run_preflight(current=current, triggers=triggers)
        for key, value in (("_APP_HOSTNAME", "different.lean.g5labs.ai"),
                           ("_PREVIEW_HOSTNAME", "preview.different.lean.g5labs.ai"),
                           ("_EXTERNAL_ACCESS_ENABLED", "false"),
                           ("_INGRESS_STATIC_IP_NAME", "g5-lean-prod-ip"),
                           ("_CERTIFICATE_MAP_NAME", "g5-lean-prod-ip-cert-map")):
            changed = {**current, "substitutions": {**current["substitutions"], key: value}}
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "overrides trusted trigger"):
                self.run_preflight(current=changed, triggers=triggers)
        self.release = ar.Release({**self.env, **trigger["substitutions"], "_APP_HOSTNAME": "different.lean.g5labs.ai"})
        with self.assertRaisesRegex(ValueError, "Process differs"):
            self.run_preflight(current=current, triggers=triggers)

    def test_public_names_remain_configurable_inside_the_delegated_child(self):
        for environment in ar.TARGETS:
            zone = "lean.g5labs.ai" if environment == "staging" else "app.g5labs.ai"
            self.assertEqual(ar.public_settings(environment, {})["_EXTERNAL_ACCESS_ENABLED"], "false")
            for enabled in (False, True):
                for app in (zone, "next." + zone):
                    settings = {"external_access_enabled": enabled, "app_hostname": app,
                                "preview_hostname": "preview." + app}
                    with self.subTest(environment=environment, enabled=enabled, app=app):
                        values = ar.public_settings(environment, settings)
                        self.assertEqual(values["_APP_HOSTNAME"], app)
                        self.assertEqual(values["_PREVIEW_HOSTNAME"], "preview." + app)
                        self.assertEqual(values["_EXTERNAL_ACCESS_ENABLED"], str(enabled).lower())
            for host in ("g5labs.ai", "www.g5labs.ai", "qa.g5labs.ai", "preview.qa.g5labs.ai",
                         "app.g5labs.ai" if environment == "staging" else "lean.g5labs.ai",
                         "lean-staging.g5labs.ai", "preview.lean-staging.g5labs.ai",
                         "next-stage.g5labs.ai", "example.com", "evil" + zone, zone + ".example.com",
                         "invalid.." + zone, "Upper." + zone, "*." + zone):
                for field in ("app_hostname", "preview_hostname"):
                    with self.subTest(environment=environment, host=host, field=field), self.assertRaises(ValueError):
                        ar.public_settings(environment, {field: host})
            with self.subTest(environment=environment), self.assertRaisesRegex(ValueError, "distinct"):
                ar.public_settings(environment, {"app_hostname": zone, "preview_hostname": zone})
        with self.assertRaises(ValueError):
            ar.public_settings("staging", {"external_access_enabled": True})
        with self.assertRaises(ValueError):
            ar.public_settings("qa", {})

    def test_build_cannot_override_trigger_target_or_acceptance_policy(self):
        for key, value in (("_TARGET_PROJECT", "g5lean-prod-1ccc"), ("_ENVIRONMENT", "prod"),
                           ("_GKE_CLUSTER", "g5-lean-prod"), ("_RELEASE_CHECKS_REQUIRED", "false"),
                           ("_RELEASE_CHECK_BASE_URL", "https://oldprod.example"),
                           ("_RELEASE_CHECK_SECRET", "unrelated-secret"), ("_HELM_NAMESPACE", "other")):
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.run_preflight(current=promotion_build(**{key: value}))

    def test_fixed_target_binding_survives_matching_caller_and_trigger_mistake(self):
        trigger = promotion_trigger()
        trigger["substitutions"]["_TARGET_PROJECT"] = "g5lean-prod-1ccc"
        self.release = ar.Release({**self.env, "_TARGET_PROJECT": "g5lean-prod-1ccc"})
        with self.assertRaisesRegex(ValueError, "Trusted trigger has invalid _TARGET_PROJECT"):
            self.run_preflight(current=promotion_build(_TARGET_PROJECT="g5lean-prod-1ccc"),
                               triggers={"staging": trigger})

    def test_trigger_policy_changes_fail_closed_for_pending_builds(self):
        for changes in ({"disabled": True}, {"approvalConfig": {"approvalRequired": False}},
                        {"serviceAccount": "projects/qa/serviceAccounts/pr-ci@example.com"}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.run_preflight(triggers={"staging": {**promotion_trigger(), **changes}})
        with self.assertRaises(ValueError):
            self.run_preflight(current=promotion_build(checks=False))

    def test_production_refuses_unchecked_or_wrong_target_staging_parent(self):
        self.release = ar.Release({**self.env, **promotion_trigger("prod")["substitutions"],
                                  "_SOURCE_BUILD_ID": PARENT})
        for parent in (promotion_build(checks=False, build_id=PARENT, _RELEASE_MANIFEST=URI),
                       promotion_build(build_id=PARENT, _TARGET_PROJECT="g5lean-prod-1ccc", _RELEASE_MANIFEST=URI),
                       promotion_build("prod", build_id=PARENT, _RELEASE_MANIFEST=URI)):
            with self.subTest(parent=parent), self.assertRaises(ValueError):
                self.run_preflight(parent=parent)
        # Even an explicitly unchecked staging trigger cannot satisfy active
        # prod acceptance. Both actual trigger metadata and parent build agree.
        with self.assertRaisesRegex(ValueError, "bootstrap is insufficient"):
            self.run_preflight(parent=promotion_build(checks=False, build_id=PARENT, _RELEASE_MANIFEST=URI),
                               triggers={"staging": promotion_trigger(checks=False)})

    def test_explicit_bootstrap_staging_and_manual_prod_remain_supported(self):
        for environment in ("staging", "prod"):
            trigger = promotion_trigger(environment, checks=False)
            self.release = ar.Release({**self.env, **trigger["substitutions"],
                                      "_SOURCE_BUILD_ID": ORIGIN if environment == "staging" else PARENT})
            parent = build() if environment == "staging" else promotion_build(
                checks=False, build_id=PARENT, _RELEASE_MANIFEST=URI)
            with self.subTest(environment=environment):
                self.run_preflight(parent=parent, current=promotion_build(environment, checks=False),
                                   triggers={env: promotion_trigger(env, checks=False) for env in ar.TARGETS})

    def test_promote_pins_source_and_never_overrides_commit_substitution(self):
        calls = []
        with patch.object(self.release, "manifest", return_value=manifest()), \
             patch.object(ar, "gcloud", side_effect=lambda *args: calls.append(args) or ""):
            self.release.promote()
        call = calls[-1]
        self.assertIn("--sha=" + SHA, call)
        self.assertFalse(any("--branch" in arg or "COMMIT_SHA=" in arg for arg in call))
        substitutions = next(arg for arg in call if arg.startswith("--substitutions="))
        self.assertIn("_SOURCE_BUILD_ID=" + CURRENT, substitutions)
        self.assertIn("_RELEASE_MANIFEST=" + URI, substitutions)
        for key, value in REFS.items():
            self.assertIn("_" + key + "=" + value, substitutions)

    def test_qa_publication_is_create_only_and_child_uses_pinned_generation(self):
        release = ar.Release({**self.env, "_ENVIRONMENT": "qa", "_NEXT_TRIGGER": "g5-promote-staging"})
        calls = []
        def cloud(*args):
            calls.append(args)
            if args[:3] == ("builds", "triggers", "describe"):
                return json.dumps(qa_trigger())
            return "77" if args[:3] == ("storage", "objects", "describe") else ""
        with patch.object(ar, "run", return_value=SHA), patch.object(release, "describe", return_value=build(CURRENT)), \
             patch.object(ar, "gcloud", side_effect=cloud):
            release.promote()
        upload = next(call for call in calls if call[:2] == ("storage", "cp"))
        self.assertIn("--if-generation-match=0", upload)
        record = json.loads((self.path / "app-release.json").read_text())
        self.assertEqual(record["qa_build_id"], CURRENT)
        self.assertEqual(record["refs"], REFS)
        self.assertIn(f"{CURRENT}.json#77", calls[-1][-1])

    def test_disabled_and_skipped_promotions_have_no_cloud_effects(self):
        with patch.object(ar, "gcloud") as cloud:
            ar.Release({**self.env, "_NEXT_TRIGGER": ""}).promote()
            (self.path / "deploy-control.env").write_text("SKIP_DEPLOY=true\n")
            self.release.promote()
            cloud.assert_not_called()

    def test_admin_publication_and_staging_handoff_keep_qa_provenance(self):
        admin = "g5lean-admin-1ccc"
        refs = {key: value.replace(PROJECT, admin) for key, value in REFS.items()}
        (self.path / "image-refs.env").write_text("\n".join(f"{key}={value}" for key, value in refs.items()))
        (self.path / "worker-image.env").write_text("WORKER_IMAGE_REF=" + refs["WORKER_IMAGE_REF"])
        env = {**self.env, "_ENVIRONMENT": "qa", "_ARTIFACT_PROJECT": admin, "_AR_PROJECT": admin,
               "_WORKER_IMAGE_PROJECT": admin, "_RELEASE_BUCKET": f"{admin}-lean-releases",
               "_NEXT_TRIGGER": "g5-promote-staging"}
        producer = ar.Release(env)
        calls = []
        def cloud(*args):
            calls.append(args)
            if args[:3] == ("builds", "triggers", "describe"):
                return json.dumps(qa_trigger(admin))
            if args[:3] == ("storage", "objects", "describe"):
                return "77"
            if args[:2] == ("storage", "cat"):
                return (self.path / "app-release.json").read_text()
            return ""
        with patch.object(ar, "run", return_value=SHA), \
             patch.object(producer, "describe", return_value=build(CURRENT, **qa_trigger(admin)["substitutions"])), \
             patch.object(ar, "gcloud", side_effect=cloud):
            producer.promote()
            record = json.loads((self.path / "app-release.json").read_text())
            uri = f"gs://{admin}-lean-releases/app-releases/{CURRENT}.json#77"
            staging = ar.Release({**env, "_ENVIRONMENT": "staging", "_NEXT_TRIGGER": "g5-promote-prod",
                                  "_RELEASE_MANIFEST": uri})
            self.assertEqual(staging.manifest(), record)
            staging.promote()
        self.assertEqual(record["project"], PROJECT)
        self.assertEqual(record["artifact_project"], admin)
        self.assertEqual(record["refs"], refs)
        uploads = [call for call in calls if call[:2] == ("storage", "cp")]
        self.assertEqual(len(uploads), 1, "Only the QA producer may create the manifest")
        self.assertIn("--if-generation-match=0", uploads[0])
        queues = [call for call in calls if call[:3] == ("builds", "triggers", "run")]
        self.assertEqual([call[3] for call in queues], ["g5-promote-staging", "g5-promote-prod"])
        for call in queues:
            self.assertIn(f"--project={PROJECT}", call)
            self.assertIn(uri, call[-1])

    def test_legacy_qa_trigger_accepts_only_default_artifact_destination(self):
        trigger = qa_trigger()
        for key in ("_ARTIFACT_PROJECT", "_RELEASE_BUCKET", "_NEXT_TRIGGER"):
            trigger["substitutions"].pop(key)
        env = {**self.env, "_ENVIRONMENT": "qa", "_RELEASE_BUCKET": "", "_NEXT_TRIGGER": ""}
        producer = ar.Release(env)
        current = build(CURRENT, _RELEASE_BUCKET="")
        with patch.object(producer, "describe", return_value=current), \
             patch.object(ar, "gcloud", return_value=json.dumps(trigger)):
            producer.policy()
        for owner, bucket in (("g5lean-admin-1ccc", ""), (PROJECT, BUCKET)):
            selected = {"_ARTIFACT_PROJECT": owner, "_AR_PROJECT": owner,
                        "_WORKER_IMAGE_PROJECT": owner, "_RELEASE_BUCKET": bucket}
            with self.subTest(owner=owner, bucket=bucket), self.assertRaisesRegex(ValueError, "artifact policy"):
                ar.Release({**env, **selected}).artifact_binding(trigger, build(**selected), current=True)
        current["substitutions"].pop("_ARTIFACT_PROJECT")
        with self.assertRaisesRegex(ValueError, "artifact policy"):
            producer.artifact_binding(trigger, current, current=True)

    def test_artifact_switch_requires_matching_live_trigger_policy_before_upload(self):
        admin = "g5lean-admin-1ccc"
        selected = {"_ARTIFACT_PROJECT": admin, "_AR_PROJECT": admin,
                    "_WORKER_IMAGE_PROJECT": admin, "_RELEASE_BUCKET": f"{admin}-lean-releases"}
        producer = ar.Release({**self.env, **selected, "_ENVIRONMENT": "qa"})
        with patch.object(producer, "describe", return_value=build(CURRENT, **selected)), \
             patch.object(ar, "gcloud", return_value=json.dumps(qa_trigger())) as cloud, \
             self.assertRaisesRegex(ValueError, "artifact policy"):
            producer.policy()
        self.assertTrue(all(call.args[:3] == ("builds", "triggers", "describe") for call in cloud.call_args_list))
        with patch.object(ar, "gcloud") as cloud, self.assertRaises(ValueError):
            ar.Release({**self.env, "_ARTIFACT_PROJECT": admin})
        cloud.assert_not_called()

    def test_first_deploy_and_same_sha_retry_do_not_require_newest_main(self):
        for raw in ("", json.dumps({"metadata": {"annotations": {"g5.g5labs.ai/deployed-commit": SHA}}})):
            with patch.object(ar, "gcloud"), patch.object(ar, "run", return_value=raw):
                self.release.freshness()
            self.assertIn("SKIP_DEPLOY=false", (self.path / "deploy-control.env").read_text())

    def test_freshness_blocks_missing_marker_and_non_ancestor(self):
        with patch.object(ar, "gcloud"), patch.object(ar, "run", return_value='{"metadata": {}}'), self.assertRaises(ValueError):
            self.release.freshness()
        raw = json.dumps({"metadata": {"annotations": {"g5.g5labs.ai/deployed-commit": OLD}}})
        for ancestry, success in ((0, True), (1, False), (128, False)):
            with patch.object(ar, "gcloud"), patch.object(ar, "run", return_value=raw), \
                 patch.object(self.release, "is_forward", return_value=(ancestry == 0)):
                if success:
                    self.release.freshness()
                else:
                    with self.assertRaises(ValueError):
                        self.release.freshness()


class CheckConfigurationTest(unittest.TestCase):
    def fixture(self):
        item = {"org_id": ORIGIN, "fixture_workspace_id": PARENT, "api_key": "g5k_test"}
        return {"environment": "staging", "base_url": "https://stage.example", "chat": dict(item),
                "preview": {**item, "profile": "release-fixture"}}

    def test_fixture_must_be_bound_to_target_and_have_both_real_checks(self):
        config = self.fixture()
        checks.validate_config(config, "staging", "https://stage.example")
        for invalid in ({**config, "environment": "prod"}, {**config, "base_url": "https://oldprod.example"},
                        {**config, "preview": {}}, {**config, "chat": {}}):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                checks.validate_config(invalid, "staging", "https://stage.example")
        for url in ("", "http://stage.example", "https://secret@stage.example", "https://stage.example/path"):
            with self.subTest(url=url), self.assertRaises(ValueError):
                checks.validate_config(config, "staging", url)

    def test_checks_run_both_real_stages_and_propagate_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, "deploy-control.env").write_text("SKIP_DEPLOY=false\n")
            Path(directory, "image-refs.env").write_text("\n".join(f"{key}={value}" for key, value in REFS.items()))
            env = {"WORKSPACE_DIR": directory, "_RELEASE_CHECKS_REQUIRED": "true", "_ENVIRONMENT": "staging",
                   "_TARGET_PROJECT": "g5lean-staging-1ccc", "_AR_REGION": "us-central1",
                   "_RELEASE_CHECK_BASE_URL": "https://stage.example", "_RELEASE_CHECK_SECRET": "fixture-secret",
                   "_PREVIEW_HOSTNAME": "preview.stage.example",
                   "G5_CANARY_PREVIEW_HOST_SUFFIX": "must-not-pass.example",
                   "ANTHROPIC_AUTH_TOKEN": "must-not-pass", "G5_CANARY_SLACK_WEBHOOK": "must-not-pass"}
            for failing_stage in (None, "chat", "preview"):
                with patch.dict(os.environ, env, clear=True), \
                     patch.object(checks.subprocess, "check_output", return_value=json.dumps(self.fixture())), \
                     patch.object(checks.subprocess, "run") as run:
                    def canary(*args, **kwargs):
                        if kwargs["env"]["G5_CANARY_STAGES"] == failing_stage:
                            raise subprocess.CalledProcessError(1, ["bash", "canary"])
                    run.side_effect = canary
                    if failing_stage:
                        with self.assertRaises(subprocess.CalledProcessError):
                            checks.main()
                    else:
                        self.assertEqual(checks.main(), 0)
                    self.assertEqual([call.kwargs["env"]["G5_CANARY_STAGES"] for call in run.call_args_list],
                                     ["chat"] if failing_stage == "chat" else ["chat", "preview"])
                    for call in run.call_args_list:
                        self.assertTrue(call.kwargs["check"])
                        child = call.kwargs["env"]
                        self.assertEqual(child["G5_CANARY_API_URL"], "https://stage.example")
                        self.assertEqual(child["G5_CANARY_PREVIEW_HOST_SUFFIX"], "preview.stage.example")
                        self.assertEqual(child["G5_CANARY_REFRESH_TOKEN_FILE"], "/dev/null")
                        self.assertNotIn("ANTHROPIC_AUTH_TOKEN", child)
                        self.assertNotIn("G5_CANARY_SLACK_WEBHOOK", child)

    def test_required_checks_refuse_missing_or_malformed_preview_hostname(self):
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, "deploy-control.env").write_text("SKIP_DEPLOY=false\n")
            Path(directory, "image-refs.env").write_text("\n".join(f"{key}={value}" for key, value in REFS.items()))
            for hostname in (None, "", "https://preview.example", "preview.example/path",
                             "preview.example:443", "user@preview.example", "preview.example?query",
                             "preview.example#fragment", "*.preview.example", "preview..example",
                             "-preview.example", "preview-.example", "preview_example", "preview.example.",
                             " preview.example", "preview.example\n", "a" * 64 + ".example",
                             ".".join(["a" * 63] * 4)):
                env = {"WORKSPACE_DIR": directory, "_RELEASE_CHECKS_REQUIRED": "true", "_ENVIRONMENT": "staging",
                       "_TARGET_PROJECT": "g5lean-staging-1ccc", "_AR_REGION": "us-central1",
                       "_RELEASE_CHECK_BASE_URL": "https://stage.example", "_RELEASE_CHECK_SECRET": "fixture-secret"}
                if hostname is not None:
                    env["_PREVIEW_HOSTNAME"] = hostname
                with self.subTest(hostname=hostname), patch.dict(os.environ, env, clear=True), \
                     patch.object(checks.subprocess, "check_output", return_value=json.dumps(self.fixture())) as cloud, \
                     patch.object(checks.subprocess, "run") as run:
                    with self.assertRaisesRegex(ValueError, "_PREVIEW_HOSTNAME"):
                        checks.main()
                    cloud.assert_not_called()
                    run.assert_not_called()

    def test_missing_credentials_cannot_green_skip_required_checks(self):
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, "deploy-control.env").write_text("SKIP_DEPLOY=false\n")
            with patch.dict(os.environ, {"WORKSPACE_DIR": directory, "_RELEASE_CHECKS_REQUIRED": "true"}, clear=True), \
                 patch.object(checks.subprocess, "check_output") as cloud, self.assertRaises(ValueError):
                checks.main()
            cloud.assert_not_called()
            with patch.dict(os.environ, {"WORKSPACE_DIR": directory, "_RELEASE_CHECKS_REQUIRED": "false",
                                        "_NEXT_TRIGGER": "g5-promote-prod", "_ENVIRONMENT": "staging"}, clear=True), \
                 self.assertRaises(ValueError):
                checks.main()


if __name__ == "__main__":
    unittest.main()
