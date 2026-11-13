"""Exercise the ported Cloud Build shell steps with local Terraform/Docker stubs.

The actual YAML consumers run in dependency order. No cloud, real Terraform apply,
Docker daemon, or credentials are used; this is not live environment acceptance.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import shlex
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import terraform_release as release

ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "infra/ci/cloudbuild-terraform-release.yaml"
SHA = "a" * 40
DIGEST = "us-central1-docker.pkg.dev/g5lean-admin-1ccc/g5/terraform-bundle@sha256:" + "b" * 64


def read_config():
    """Read this config's fixed inline shell/scalar convention with stdlib only.

    The Cloud Build contract image has no PyYAML. Fail if the expected shape
    changes rather than silently skipping a step or its environment bindings.
    """
    def scalar(value):
        tokens = shlex.split(value, comments=True)
        if len(tokens) != 1:
            raise ValueError(f"Expected a single YAML scalar: {value!r}")
        return tokens[0]

    source = CONFIG.read_text()
    substitutions = re.search(r"^substitutions:\n(.*?)^steps:", source, re.M | re.S)
    if substitutions is None:
        raise ValueError("Missing substitutions/steps blocks")
    defaults = {key: scalar(value) for key, value in re.findall(
        r"^  (_[A-Z0-9_]+): (.+)$", substitutions[1], re.M)}
    options = source.split("options:\n", 1)[1].split("\nsubstitutions:", 1)[0]
    environment = re.search(r"^  env:\n((?:    - .*\n)+)", options, re.M)
    global_env = [scalar(line[6:]) for line in environment[1].splitlines()] if environment else []
    steps = []
    for block in source.split("\n  - id: ")[1:]:
        step = {"id": scalar(block.splitlines()[0])}
        args = re.search(r"^      - (-[a-z]+)\n      - \|\n((?:        .*\n|\n)*)", block, re.M)
        wait = re.search(r"^    waitFor: \[(.*)\]", block, re.M)
        entrypoint = re.search(r"^    entrypoint: (.+)$", block, re.M)
        if not args or not wait or not entrypoint:
            raise ValueError(f"Unsupported step shape: {step['id']}")
        step["entrypoint"] = scalar(entrypoint[1])
        step["args"] = [args[1], "\n".join(line[8:] for line in args[2].splitlines()) + "\n"]
        step["waitFor"] = [scalar(value.strip()) for value in wait[1].split(",") if value.strip()]
        directory = re.search(r"^    dir: (.+)$", block, re.M)
        if directory:
            step["dir"] = scalar(directory[1])
        env = re.search(r"^    env:\n((?:      - .*\n)+)", block, re.M)
        if env:
            step["env"] = [scalar(line[8:]) for line in env[1].splitlines()]
        steps.append(step)
    if not steps:
        raise ValueError("No shell steps found")
    return {"substitutions": defaults, "steps": steps, "env": global_env}


class TerraformReleaseShellTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.base = Path(self.directory.name)
        self.workspace = self.base / "workspace"
        self.workspace.mkdir()
        self.commands = self.base / "commands.jsonl"
        self.bundle = self.base / "image-bundle"
        for target in ("staging", "prod"):
            for root in ("platform", "addons"):
                path = self.bundle / "infra" / target / root
                path.mkdir(parents=True)
                (path / "main.tf").write_text("# supplied by immutable bundle\n")
        tests = self.bundle / "infra/terraform/modules/fixture/tests"
        tests.mkdir(parents=True)
        (tests / "contracts.tftest.hcl").write_text("# mocked module contract\n")
        (tests / "test_static.py").write_text("import unittest\nclass Contract(unittest.TestCase):\n def test_contract(self): pass\n")
        (self.bundle / "scripts").mkdir()
        (self.bundle / "scripts/ensure-base-images.sh").write_text("#!/bin/bash\nexit 0\n")
        config = self.bundle / "backend/g5-api-foundation/src/config.rs"
        config.parent.mkdir(parents=True)
        config.write_text("// fixture API settings\n")
        self.bin = self.workspace / "bin"
        self.bin.mkdir()
        program = '''#!/usr/bin/env python3
import json, os, pathlib, shutil, sys
kind = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ['COMMAND_LOG'], 'a') as out:
 out.write(json.dumps({'tool': kind, 'args': args, 'cwd': os.getcwd(),
                       'tf_workspace': os.environ.get('TF_WORKSPACE')}) + '\\n')
if kind == 'docker':
 if args[0] == 'create': print('fixture-container')
 if args[0] == 'inspect': print(os.environ['BUNDLE_DIGEST'])
 if args[0] == 'cp': shutil.copytree(os.environ['FIXTURE_BUNDLE_SOURCE'], args[2], dirs_exist_ok=True)
 if os.environ.get('FAIL_DOCKER') == args[0]: sys.exit(1)
if kind == 'terraform':
 root = pathlib.Path.cwd().name
 if args[0] == 'plan':
  pathlib.Path(next(a[5:] for a in args if a.startswith('-out='))).write_text(root)
  print('Plan: 1 to add, 0 to change, 0 to destroy.')
  sys.exit(int(os.environ.get('PLAN_EXIT_' + root.upper(), '2')))
 if args[0] == 'apply':
  if pathlib.Path(args[-1]).read_text() != root: sys.exit(9)
 if os.environ.get('FAIL_TERRAFORM') == args[0] + ':' + root: sys.exit(1)
'''
        for tool in ("docker", "terraform", "gcloud"):
            path = self.bin / tool
            path.write_text(program)
            path.chmod(0o755)
        self.config = read_config()
        self.subs = {**self.config["substitutions"], "COMMIT_SHA": SHA, "SHORT_SHA": SHA[:7],
            "_TF_ACTION": "apply", "_TF_BUNDLE_MODE": "use", "_TF_BUNDLE_REF": DIGEST,
            "_TF_WORKSPACE": "staging", "_STATE_BUCKET": "g5lean-staging-1ccc-lean-tfstate",
            "_AR_PROJECT": "g5lean-admin-1ccc", "_NEXT_TRIGGER": "g5-terraform-apply-prod",
            "_NEXT_TRIGGER_PROJECT": "g5-qa-1ccc", "_TF_CONFIG_JSON": json.dumps({
                "project_id": "g5lean-staging-1ccc", "region": "us-central1", "settings": {"app_hostname": "lean.g5labs.ai"}})}
        self.env = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}",
                    "COMMAND_LOG": str(self.commands), "BUNDLE_DIGEST": DIGEST,
                    # The shell fixture tests orchestration, not Terraform's
                    # evaluator. Real selector tests run in the Terraform jobs.
                    "G5_TERRAFORM_SHELL_STUB": "1",
                    "FIXTURE_BUNDLE_SOURCE": str(self.bundle)}

    def render(self, value):
        # Cloud Build replaces substitutions but preserves $$ for the shell.
        value = value.replace("$$", "\x00")
        value = re.sub(r"\$\{([A-Z_][A-Z0-9_]*)\}|\$([A-Z_][A-Z0-9_]*)",
            lambda match: str(self.subs[match[1] or match[2]]), value)
        return value.replace("\x00", "$").replace("/workspace", str(self.workspace))

    def run_pipeline(self):
        completed = {"install-terraform"}  # The executable is supplied by our stub.
        self.executed_steps = []
        for step in self.config["steps"]:
            if step["id"] == "install-terraform":
                continue
            self.assertTrue(set(step.get("waitFor", [])) <= completed, step["id"])
            self.executed_steps.append(step["id"])
            env = self.env.copy()
            for setting in [*self.config["env"], *step.get("env", [])]:
                key, value = setting.split("=", 1)
                env[key] = self.render(value)
            directory = self.workspace / self.render(step.get("dir", "."))
            result = subprocess.run([step["entrypoint"], *[self.render(arg) for arg in step["args"]]],
                cwd=directory, env=env, text=True, capture_output=True)
            if result.returncode:
                return step["id"], result.returncode, result.stdout + result.stderr
            completed.add(step["id"])
        return "complete", 0, ""

    def calls(self, tool=None, action=None):
        calls = [json.loads(line) for line in self.commands.read_text().splitlines()] if self.commands.exists() else []
        return [call for call in calls if (not tool or call["tool"] == tool) and (not action or call["args"][0] == action)]

    def assert_complete(self):
        step, code, output = self.run_pipeline()
        self.assertEqual((step, code), ("complete", 0), output)
        # Inspect the environment received by the actual Terraform subprocesses:
        # target-specific buckets use platform/default and addons/default state.
        for call in self.calls("terraform"):
            self.assertEqual(call["tf_workspace"], "default", call)

    def test_inherited_terraform_workspace_cannot_select_parallel_state(self):
        self.env["TF_WORKSPACE"] = "old-qa"
        self.assert_complete()

    def test_staging_applies_bundle_platform_then_addons_and_promotes_same_digest(self):
        self.assert_complete()
        self.assertEqual(self.calls("docker", "pull")[0]["args"], ["pull", DIGEST])
        self.assertEqual(self.calls("docker", "create")[0]["args"], ["create", DIGEST])
        init = self.calls("terraform", "init")
        for root in ("platform", "addons"):
            call = next(c for c in init if Path(c["cwd"]).name == root)
            self.assertIn("-backend-config=bucket=g5lean-staging-1ccc-lean-tfstate", call["args"])
            self.assertIn(f"-backend-config=prefix={root}", call["args"])
            config = json.loads((Path(call["cwd"]) / "release.auto.tfvars.json").read_text())
            self.assertEqual(config, json.loads(self.subs["_TF_CONFIG_JSON"]))
        tf = self.calls("terraform")
        platform_apply = next(i for i, c in enumerate(tf) if c["args"][0] == "apply" and Path(c["cwd"]).name == "platform")
        addons_init = next(i for i, c in enumerate(tf) if c["args"][0] == "init" and Path(c["cwd"]).name == "addons")
        self.assertLess(platform_apply, addons_init)
        self.assertTrue(all("-refresh=false" in c["args"] for c in self.calls("terraform", "plan")))
        self.assertFalse(self.calls("terraform", "workspace"))
        queue = self.calls("gcloud")[0]["args"]
        self.assertEqual(queue[:4], ["builds", "triggers", "run", "g5-terraform-apply-prod"])
        self.assertIn(f"--sha={SHA}", queue)
        self.assertIn(f"_TF_BUNDLE_REF={DIGEST}", queue[-1])
        self.assertIn("_TF_BUNDLE_MODE=use", queue[-1])
        self.assertFalse(any(c["args"][0] == "storage" for c in self.calls("gcloud")))

    def test_production_uses_same_digest_without_chaining(self):
        self.subs.update(_TF_WORKSPACE="prod", _STATE_BUCKET="g5lean-prod-1ccc-lean-tfstate", _NEXT_TRIGGER="", _NEXT_TRIGGER_PROJECT="")
        self.assert_complete()
        self.assertEqual(len(self.calls("terraform", "apply")), 2)
        self.assertFalse(self.calls("gcloud"))
        self.assertEqual(self.calls("docker", "pull")[0]["args"][1], DIGEST)

    def configure_source_plan(self, target):
        shutil.copytree(self.bundle, self.workspace, dirs_exist_ok=True)
        project = f"g5lean-{target}-1ccc"
        self.subs.update(
            _TF_ACTION="plan", _TF_BUNDLE_MODE="source", _TF_BUNDLE_REF="", COMMIT_SHA="",
            _TF_WORKSPACE=target, _STATE_BUCKET=f"{project}-lean-tfstate",
            _TF_CONFIG_JSON=json.dumps({"project_id": project, "region": "us-central1",
                "settings": {"external_access_enabled": False}}))
        # Keep the fixture's downstream trigger populated: plan mode must ignore
        # it even if a stale or manually supplied trigger binding is present.

    def assert_source_plan_has_no_mutations(self):
        self.assertFalse(self.calls("terraform", "apply"))
        self.assertFalse(self.calls("terraform", "workspace"))
        self.assertFalse(self.calls("docker"))
        self.assertFalse(self.calls("gcloud"))

    def assert_source_plan_targets_environment(self, target):
        self.configure_source_plan(target)
        self.assert_complete()
        self.assert_source_plan_has_no_mutations()
        plans = self.calls("terraform", "plan")
        self.assertEqual([Path(call["cwd"]).name for call in plans], ["platform", "addons"])
        for root in ("platform", "addons"):
            directory = (self.workspace / "tf-bundle/infra" / target / root).resolve()
            init = [call for call in self.calls("terraform", "init") if Path(call["cwd"]) == directory]
            self.assertEqual(len(init), 1)
            self.assertIn(f"-backend-config=bucket=g5lean-{target}-1ccc-lean-tfstate", init[0]["args"])
            self.assertIn(f"-backend-config=prefix={root}", init[0]["args"])
            config = json.loads((directory / "release.auto.tfvars.json").read_text())
            self.assertEqual(config, json.loads(self.subs["_TF_CONFIG_JSON"]))
            plan = next(call for call in plans if Path(call["cwd"]) == directory)
            self.assertIn(f"-out={self.workspace}/tfplan-{target}-{root}", plan["args"])
            self.assertIn("-detailed-exitcode", plan["args"])

    def test_staging_source_plan_uses_its_state_and_config_without_mutations(self):
        self.assert_source_plan_targets_environment("staging")

    def test_production_source_plan_uses_its_state_and_config_without_mutations(self):
        self.assert_source_plan_targets_environment("prod")

    def assert_source_plan_failure_stops_pipeline(self, target, root, exit_code):
        self.configure_source_plan(target)
        self.env[f"PLAN_EXIT_{root.upper()}"] = str(exit_code)
        step, code, output = self.run_pipeline()
        self.assertEqual((step, code), (f"tf-plan-{root}", exit_code), output)
        self.assert_source_plan_has_no_mutations()
        self.assertNotIn(f"tf-apply-{root}", self.executed_steps)
        self.assertNotIn("trigger-next", self.executed_steps)
        planned = [Path(call["cwd"]).name for call in self.calls("terraform", "plan")]
        self.assertEqual(planned, ["platform"] if root == "platform" else ["platform", "addons"])
        if root == "platform":
            self.assertNotIn("tf-init-addons", self.executed_steps)
            self.assertFalse(any(Path(call["cwd"]).name == "addons" for call in self.calls("terraform")))

    def test_staging_source_platform_plan_failure_stops_addons_and_promotion(self):
        self.assert_source_plan_failure_stops_pipeline("staging", "platform", 1)

    def test_production_source_platform_plan_failure_stops_addons_and_promotion(self):
        self.assert_source_plan_failure_stops_pipeline("prod", "platform", 1)

    def test_staging_source_addons_plan_failure_preserves_exit_code_without_mutations(self):
        self.assert_source_plan_failure_stops_pipeline("staging", "addons", 17)

    def test_production_source_addons_plan_failure_preserves_exit_code_without_mutations(self):
        self.assert_source_plan_failure_stops_pipeline("prod", "addons", 17)

    def test_materialized_bundle_runs_actual_lean_static_module_contracts(self):
        shutil.copytree(ROOT / "infra", self.bundle / "infra", dirs_exist_ok=True,
            ignore=shutil.ignore_patterns(".terraform", "__pycache__", "*.tfstate*"))
        shutil.copy2(ROOT / "backend/g5-api-foundation/src/config.rs",
            self.bundle / "backend/g5-api-foundation/src/config.rs")
        self.assert_complete()

    def test_zero_change_plans_still_apply_saved_plans(self):
        self.env.update(PLAN_EXIT_PLATFORM="0", PLAN_EXIT_ADDONS="0")
        self.assert_complete()
        self.assertEqual(len(self.calls("terraform", "apply")), 2)

    def test_platform_plan_failure_prevents_every_apply_and_chain(self):
        self.env["PLAN_EXIT_PLATFORM"] = "1"
        self.assertEqual(self.run_pipeline()[:2], ("tf-plan-platform", 1))
        self.assertFalse(self.calls("terraform", "apply"))
        self.assertFalse(self.calls("gcloud"))

    def test_unexpected_addons_plan_failure_prevents_its_apply_and_chain(self):
        self.env["PLAN_EXIT_ADDONS"] = "17"
        self.assertEqual(self.run_pipeline()[:2], ("tf-plan-addons", 17))
        self.assertEqual(len(self.calls("terraform", "apply")), 1)
        self.assertFalse(self.calls("gcloud"))

    def test_platform_apply_failure_prevents_addons_and_chain(self):
        self.env["FAIL_TERRAFORM"] = "apply:platform"
        self.assertEqual(self.run_pipeline()[:2], ("tf-apply-platform", 1))
        self.assertFalse(any(Path(c["cwd"]).name == "addons" for c in self.calls("terraform")))
        self.assertFalse(self.calls("gcloud"))

    def test_module_test_failure_prevents_mutation(self):
        self.env["FAIL_TERRAFORM"] = "test:fixture"
        self.assertEqual(self.run_pipeline()[:2], ("tf-test-modules", 1))
        self.assertFalse(self.calls("terraform", "apply"))
        self.assertFalse(self.calls("gcloud"))

    def test_static_test_failure_prevents_mutation(self):
        (self.bundle / "infra/terraform/modules/fixture/tests/test_static.py").write_text("raise RuntimeError('bad contract')\n")
        self.assertEqual(self.run_pipeline()[:2], ("tf-test-static", 1))
        self.assertFalse(self.calls("terraform", "apply"))

    def test_mutable_bundle_is_rejected_before_pull(self):
        self.subs["_TF_BUNDLE_REF"] = DIGEST.split("@")[0] + ":latest"
        self.assertEqual(self.run_pipeline()[:2], ("validate-inputs", 1))
        self.assertFalse(self.calls("docker"))

    def test_apply_requires_commit(self):
        self.subs["COMMIT_SHA"] = ""
        self.assertEqual(self.run_pipeline()[:2], ("validate-inputs", 1))
        self.assertFalse(self.calls("docker"))

    def test_explicit_promotion_disable_keeps_apply_without_chain(self):
        self.subs["_TF_DISABLE_PROMOTION"] = "true"
        self.assert_complete()
        self.assertEqual(len(self.calls("terraform", "apply")), 2)
        self.assertFalse(self.calls("gcloud"))

    def test_legacy_use_mode_resolves_commit_tag_to_digest_as_g5_does(self):
        self.subs["_TF_BUNDLE_REF"] = ""
        self.assert_complete()
        pulls = self.calls("docker", "pull")
        self.assertEqual(pulls[0]["args"][1], f"us-central1-docker.pkg.dev/g5lean-admin-1ccc/g5/terraform-bundle:{SHA}")
        self.assertEqual(pulls[1]["args"][1], DIGEST)
        self.assertIn(f"_TF_BUNDLE_REF={DIGEST}", self.calls("gcloud")[0]["args"][-1])


class QaProducerPolicyTests(unittest.TestCase):
    def test_legacy_trigger_defaults_only_allow_qa_without_release_bucket(self):
        project = "g5-qa-1ccc"
        trigger = {"substitutions": {}}
        build = {"substitutions": {"_ARTIFACT_PROJECT": project, "_RELEASE_BUCKET": ""}}
        release.validate_artifact_binding(trigger, build, project, project, "", require_bucket=False)
        for owner, bucket in (("g5lean-admin-1ccc", ""), (project, f"{project}-lean-releases")):
            with self.subTest(owner=owner, bucket=bucket), self.assertRaisesRegex(ValueError, "artifact policy"):
                release.validate_artifact_binding(trigger,
                    {"substitutions": {"_ARTIFACT_PROJECT": owner, "_RELEASE_BUCKET": bucket}},
                    project, owner, bucket, require_bucket=False)
        with self.assertRaises(ValueError):
            release.validate_artifact_binding(trigger, build, project, project, "", require_bucket=True)
        with self.assertRaisesRegex(ValueError, "artifact policy"):
            release.validate_artifact_binding(trigger, {"substitutions": {}}, project, project, "", require_bucket=False)

    def test_existing_qa_policy_precedes_self_apply_and_rejects_override(self):
        project = "g5-qa-1ccc"
        trigger = {"name": "g5-terraform-apply-qa", "id": "qa-trigger",
            "serviceAccount": f"projects/{project}/serviceAccounts/g5-lean-terraform@{project}.iam.gserviceaccount.com",
            "substitutions": {"_ARTIFACT_PROJECT": project, "_RELEASE_BUCKET": ""}}
        build = {**trigger, "buildTriggerId": trigger["id"], "sourceProvenance": {"resolvedGitSource": {"revision": SHA}}}
        args = SimpleNamespace(project=project, location="us-central1", build_id="current", sha=SHA,
            artifact_project=project, bucket="", next_trigger="")
        with patch.object(release, "build_info", return_value=build), patch.object(release, "gcloud", return_value=json.dumps(trigger)) as cloud:
            release.policy(args)
            args.artifact_project = "g5lean-admin-1ccc"
            with self.assertRaisesRegex(ValueError, "artifact policy"):
                release.policy(args)
            self.assertTrue(all(call.args[:3] == ("builds", "triggers", "describe") for call in cloud.call_args_list))

    def test_qa_policy_uses_resolved_source_instead_of_caller_substitution(self):
        self.assertEqual(release.source_sha({"substitutions": {"COMMIT_SHA": SHA}}), "")
        self.assertEqual(release.source_sha({"sourceProvenance": {"resolvedGitSource": {"revision": SHA}}}), SHA)


if __name__ == "__main__":
    unittest.main()
