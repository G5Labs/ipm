"""Run QA bundle producer shell steps with local Docker/cloud/Terraform fakes."""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
BASH = shutil.which("bash") or "/bin/bash"
SHA = "a" * 40
PRODUCER = ("build-bundle", "push-bundle", "resolve-bundle-ref", "publish-terraform-release")


def read_steps():
    """Read this file's inline bash blocks without adding a CI YAML dependency."""
    source = (ROOT / "infra/ci/cloudbuild-terraform-qa.yaml").read_text()
    steps = {}
    for block in source.split("\n  - id: ")[1:]:
        name = block.splitlines()[0]
        match = re.search(r"^      - (-[a-z]+)\n      - \|\n((?:        .*\n|\n)*)", block, re.M)
        wait = re.search(r"^    waitFor: \[(.*)\]", block, re.M)
        steps[name] = {
            "args": [match[1], "\n".join(line[8:] for line in match[2].splitlines()) + "\n"],
            "waitFor": re.findall(r"'([^']+)'", wait[1]) if wait else [],
        }
    return steps


class TerraformBundleProducerTests(unittest.TestCase):
    def setUp(self):
        self.make_workspace()

    def make_workspace(self):
        """Create an isolated pipeline fixture with command logs and freshness controls."""
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.workspace = Path(directory.name)
        self.commands = self.workspace / "commands.jsonl"
        self.bin = self.workspace / "bin"
        self.bin.mkdir()
        (self.workspace / "scripts").mkdir()
        for root in ("foundation", "delivery", "platform", "addons"):
            (self.workspace / "infra/qa" / root).mkdir(parents=True)
        program = f"#!{sys.executable}\n" + r'''
import json, os, pathlib, sys
tool, args = pathlib.Path(sys.argv[0]).name, sys.argv[1:]
workspace = pathlib.Path(os.environ['FIXTURE_WORKSPACE'])
with open(os.environ['COMMAND_LOG'], 'a') as out:
    out.write(json.dumps([tool, *args]) + '\n')
if tool == 'docker':
    if args[0] == 'build':
        assert not list(workspace.rglob('.terraform')), 'bundle includes apply-generated files'
    if args[0] == 'push' and os.environ.get('FAIL_PUSH'): sys.exit(7)
    if args[0] == 'inspect': print(os.environ['BUNDLE_DIGEST'])
elif tool == 'gcloud':
    if args[:2] == ['builds', 'describe']:
        if os.environ.get('FAIL_DESCRIBE'): sys.exit(6)
        if not os.environ.get('EMPTY_CREATED'): print('2026-10-21T10:00:00Z')
    elif args[:2] == ['builds', 'list']:
        query = next(arg.split('=', 1)[1] for arg in args if arg.startswith('--filter='))
        assert 'substitutions.TRIGGER_NAME="g5-terraform-apply-qa"' in query
        assert 'createTime>"2026-10-21T10:00:00Z"' in query and 'id!="qa-build"' in query
        assert 'status' not in query, 'failed/cancelled successors must not authorize stale applies'
        counter = workspace / 'freshness-checks'
        count = int(counter.read_text()) + 1 if counter.exists() else 1
        counter.write_text(str(count))
        if os.environ.get('FAIL_LIST') or count == int(os.environ.get('FAIL_LIST_AT', '0')): sys.exit(9)
        superseded_at = int(os.environ.get('SUPERSEDED_AT', '0'))
        if (superseded_at and count >= superseded_at) or (workspace / 'newer-build-exists').exists():
            print('newer-build', os.environ.get('NEWER_STATUS', 'SUCCESS'), 'c' * 40)
    elif args[:3] == ['builds', 'triggers', 'run']:
        assert (workspace / 'qa-applied').is_file(), 'promotion ran before QA apply'
    else: raise AssertionError(args)
elif tool == 'terraform':
    root = pathlib.Path(args[0].split('=', 1)[1])
    action = args[1]
    if action == 'init': (root / '.terraform').mkdir()
    elif action == 'plan':
        if os.environ.get('FAIL_PLAN'): sys.exit(1)
        pathlib.Path(next(arg[5:] for arg in args if arg.startswith('-out='))).write_text(root.name)
        if os.environ.get('SUPERSEDED_AFTER_PLAN') == root.name: (workspace / 'newer-build-exists').touch()
        if root.name in os.environ.get('NO_CHANGE_ROOTS', '').split(','):
            if root.name == 'addons': (workspace / 'qa-applied').touch()
            sys.exit(0)
        print('Plan: 1 to add, 0 to change, 0 to destroy.')
        sys.exit(2)
    elif action == 'show':
        changes = [{'address': 'fixture.resource', 'change': {'actions': ['delete']}}] if os.environ.get('DESTROY_PLAN') else []
        print(json.dumps({'resource_changes': changes}))
    elif action == 'apply':
        if os.environ.get('FAIL_APPLY'): sys.exit(8)
        assert pathlib.Path(args[-1]).read_text() == root.name
        if root.name == 'addons': (workspace / 'qa-applied').touch()
        if os.environ.get('SUPERSEDED_AFTER_APPLY') == root.name: (workspace / 'newer-build-exists').touch()
'''
        for tool in ("docker", "gcloud", "terraform", "base-helper"):
            executable = self.bin / tool
            executable.write_text(program)
            executable.chmod(0o755)
        (self.workspace / "scripts/ensure-base-images.sh").write_text('exec base-helper "$@"\n')
        shutil.copyfile(ROOT / "scripts/terraform-qa-freshness.sh",
                        self.workspace / "scripts/terraform-qa-freshness.sh")
        self.steps = read_steps()
        self.subs = {
            "_TF_ACTION": "apply", "_TF_NEXT_TRIGGER": "g5-terraform-apply-staging",
            "_REGION": "us-central1", "_ARTIFACT_PROJECT": "g5-qa-1ccc", "_AR_REPOSITORY": "g5",
            "_TF_ROOTS": "foundation,delivery,platform,addons", "_TF_ROOT_DIR": "infra/qa",
            "_TF_ALLOW_DESTROY": "false", "_QA_ENABLE_APPLICATION_TRIGGERS": "true",
            "PROJECT_ID": "g5-qa-1ccc", "COMMIT_SHA": SHA, "BUILD_ID": "qa-build",
            "LOCATION": "us-central1", "TRIGGER_NAME": "g5-terraform-apply-qa",
        }
        self.env = {**os.environ, "PATH": f"{self.bin}:/usr/bin:/bin",
                    "COMMAND_LOG": str(self.commands), "FIXTURE_WORKSPACE": str(self.workspace),
                    "BUNDLE_DIGEST": self.digest()}

    def digest(self):
        return f"us-central1-docker.pkg.dev/{self.subs['_ARTIFACT_PROJECT']}/g5/terraform-bundle@sha256:" + "b" * 64

    def render(self, value):
        # Expand once, as Cloud Build does: $$ protects shell variables and $().
        def replace(match):
            if match[0] == "$$":
                return "$"
            self.assertNotEqual(match[0], "$", "shell dollars must use Cloud Build $$ escaping")
            return str(self.subs[match[1] or match[2]])
        value = re.sub(r"\$\$|\$\{([^}]+)\}|\$([A-Za-z_][A-Za-z0-9_]*)|\$", replace, value)
        return value.replace("/workspace", str(self.workspace))

    def run_step(self, name):
        return subprocess.run([BASH, *map(self.render, self.steps[name]["args"])],
                              cwd=self.workspace, env=self.env, capture_output=True, text=True)

    def run_pipeline(self):
        completed = {"install-terraform", "validate-inputs", "tf-fmt"}
        for name in (*PRODUCER[:3], "tf-roots", PRODUCER[3]):
            self.assertTrue(set(self.steps[name]["waitFor"]) <= completed, name)
            result = self.run_step(name)
            if result.returncode:
                return name, result
            completed.add(name)
        return "complete", result

    def calls(self):
        return [json.loads(line) for line in self.commands.read_text().splitlines()] if self.commands.exists() else []

    def test_qa_and_admin_publish_same_immutable_digest_after_qa_apply(self):
        """Publish the selected owner's immutable bundle only after all QA roots apply."""
        for project in ("g5-qa-1ccc", "g5lean-admin-1ccc"):
            with self.subTest(project=project):
                # A separate workspace keeps the bundle's initial contents clean.
                self.make_workspace()
                self.subs["_ARTIFACT_PROJECT"] = project
                self.env["BUNDLE_DIGEST"] = self.digest() + "\r"
                step, result = self.run_pipeline()
                self.assertEqual(step, "complete", result.stdout + result.stderr)
                calls = self.calls()
                tag = self.digest().split("@")[0] + ":" + SHA
                self.assertEqual(calls[:3], [
                    ["base-helper", "base"],
                    ["docker", "build", "--tag=" + tag, "--file=infra/docker/Dockerfile.terraform-bundle", "."],
                    ["docker", "push", tag],
                ])
                self.assertEqual(calls[3][0:2], ["docker", "inspect"])
                self.assertEqual(calls[3][-1], tag)
                self.assertEqual((self.workspace / "tf-promotion.env").read_text(),
                                 f"export TF_BUNDLE_REF={self.digest()}\n")
                self.assertEqual(calls[-1], [
                    "gcloud", "builds", "triggers", "run", "g5-terraform-apply-staging",
                    "--project=g5-qa-1ccc", "--region=us-central1", "--sha=" + SHA,
                    f"--substitutions=_TF_ACTION=apply,_TF_BUNDLE_MODE=use,_TF_BUNDLE_REF={self.digest()},_AR_PROJECT={project},_AR_REPOSITORY=g5",
                ])
                self.assertEqual(sum(call[0] == "terraform" and call[2] == "apply" for call in calls), 4)
                self.assertEqual((self.workspace / "freshness-checks").read_text(), "9")
                self.assertEqual((self.workspace / "tf-control.env").read_text(), "export SKIP_TERRAFORM=false\n")

    def test_superseded_at_entry_succeeds_without_terraform_or_release(self):
        """Skip Terraform and release publication for any newer build status at entry."""
        for status in ("QUEUED", "WORKING", "SUCCESS", "FAILURE", "CANCELLED", "TIMEOUT", "INTERNAL_ERROR", "EXPIRED"):
            with self.subTest(newer_status=status):
                self.make_workspace()
                self.env.update(SUPERSEDED_AT="1", NEWER_STATUS=status)
                step, result = self.run_pipeline()
                self.assertEqual(step, "complete", result.stdout + result.stderr)
                self.assertEqual(result.returncode, 0)
                self.assertIn("Skipping Terraform release publication", result.stdout)
                self.assertFalse(any(call[0] == "terraform" for call in self.calls()))
                self.assertFalse(any(call[:4] == ["gcloud", "builds", "triggers", "run"] for call in self.calls()))
                self.assertEqual((self.workspace / "tf-control.env").read_text(), "export SKIP_TERRAFORM=true\n")

    def test_superseded_before_later_apply_keeps_prior_root_and_skips_rest(self):
        """Preserve completed roots while refusing an apply superseded after planning."""
        self.env["SUPERSEDED_AT"] = "4"  # Both roots enter; delivery becomes stale before apply.
        step, result = self.run_pipeline()
        self.assertEqual(step, "complete", result.stdout + result.stderr)
        calls = self.calls()
        applied = [call[1] for call in calls if call[0] == "terraform" and call[2] == "apply"]
        self.assertEqual(applied, ["-chdir=infra/qa/foundation"])
        self.assertFalse(any("infra/qa/platform" in call[1] or "infra/qa/addons" in call[1]
                             for call in calls if call[0] == "terraform"))
        self.assertFalse(any(call[:4] == ["gcloud", "builds", "triggers", "run"] for call in calls))
        self.assertEqual((self.workspace / "freshness-checks").read_text(), "4")

    def test_superseded_between_roots_stops_before_next_init(self):
        """Stop before initializing another root when the preceding apply is superseded."""
        self.env["SUPERSEDED_AFTER_APPLY"] = "foundation"
        step, result = self.run_pipeline()
        self.assertEqual(step, "complete", result.stdout + result.stderr)
        terraform_calls = [call for call in self.calls() if call[0] == "terraform"]
        self.assertTrue(terraform_calls)
        self.assertTrue(all(call[1] == "-chdir=infra/qa/foundation" for call in terraform_calls))
        self.assertFalse(any(call[:4] == ["gcloud", "builds", "triggers", "run"] for call in self.calls()))

    def test_superseded_after_final_root_never_publishes(self):
        """Refuse stale release publication after either a final apply or no-change plan."""
        for no_changes in (False, True):
            for status in ("WORKING", "SUCCESS", "FAILURE", "CANCELLED"):
                with self.subTest(no_changes=no_changes, newer_status=status):
                    self.make_workspace()
                    self.env["NEWER_STATUS"] = status
                    if no_changes:
                        self.env.update(NO_CHANGE_ROOTS="addons", SUPERSEDED_AFTER_PLAN="addons")
                    else:
                        self.env["SUPERSEDED_AFTER_APPLY"] = "addons"
                    step, result = self.run_pipeline()
                    self.assertEqual(step, "complete", result.stdout + result.stderr)
                    self.assertIn("does not confirm a successful takeover", result.stdout)
                    self.assertEqual(sum(call[0] == "terraform" and call[2] == "apply" for call in self.calls()),
                                     3 if no_changes else 4)
                    self.assertFalse(any(call[:4] == ["gcloud", "builds", "triggers", "run"] for call in self.calls()))
                    self.assertEqual((self.workspace / "tf-control.env").read_text(), "export SKIP_TERRAFORM=true\n")

    def test_publication_freshness_lookup_failure_fails_after_completed_applies(self):
        """Fail closed on publication freshness errors even when all QA applies succeed."""
        self.env["FAIL_LIST_AT"] = "9"  # Four root and four apply checks succeed.
        step, result = self.run_pipeline()
        self.assertEqual(step, "publish-terraform-release", result.stdout + result.stderr)
        self.assertEqual(result.returncode, 9)
        self.assertEqual(sum(call[0] == "terraform" and call[2] == "apply" for call in self.calls()), 4)
        self.assertFalse(any(call[:4] == ["gcloud", "builds", "triggers", "run"] for call in self.calls()))

    def test_lookup_plan_and_destructive_plan_errors_still_fail_without_apply_or_release(self):
        for failure in ("FAIL_DESCRIBE", "EMPTY_CREATED", "FAIL_LIST", "FAIL_PLAN", "DESTROY_PLAN"):
            with self.subTest(failure=failure):
                self.make_workspace()
                self.env[failure] = "1"
                step, result = self.run_pipeline()
                self.assertEqual(step, "tf-roots", result.stdout + result.stderr)
                self.assertNotEqual(result.returncode, 0)
                calls = self.calls()
                self.assertFalse(any(call[0] == "terraform" and call[2] == "apply" for call in calls))
                self.assertFalse(any(call[:4] == ["gcloud", "builds", "triggers", "run"] for call in calls))

    def test_plan_pipeline_keeps_planning_without_apply_or_freshness_lookups(self):
        self.subs["_TF_ACTION"] = "plan"
        step, result = self.run_pipeline()
        self.assertEqual(step, "complete", result.stdout + result.stderr)
        calls = self.calls()
        self.assertEqual(sum(call[0] == "terraform" and call[2] == "plan" for call in calls), 4)
        self.assertFalse(any(call[0] == "terraform" and call[2] == "apply" for call in calls))
        self.assertFalse(any(call[0] in {"gcloud", "docker"} for call in calls))

    def test_plan_and_disabled_promotion_do_not_publish_or_queue(self):
        for overrides in ({"_TF_ACTION": "plan"}, {"_TF_NEXT_TRIGGER": ""}):
            with self.subTest(overrides=overrides):
                self.subs.update(_TF_ACTION="apply", _TF_NEXT_TRIGGER="g5-terraform-apply-staging")
                self.subs.update(overrides)
                for step in PRODUCER:
                    result = self.run_step(step)
                    self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.calls(), [])
                self.assertFalse((self.workspace / "tf-promotion.env").exists())

    def test_missing_or_tag_only_digest_stops_before_apply_and_queue(self):
        for digest in ("", "us-central1-docker.pkg.dev/g5-qa-1ccc/g5/terraform-bundle:latest"):
            with self.subTest(digest=digest):
                self.make_workspace()
                self.env["BUNDLE_DIGEST"] = digest
                step, result = self.run_pipeline()
                self.assertEqual(step, "resolve-bundle-ref", result.stdout + result.stderr)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((self.workspace / "tf-promotion.env").exists())
                self.assertTrue(all(call[0] not in {"terraform", "gcloud"} for call in self.calls()))

    def test_failed_push_or_qa_apply_never_queues_staging(self):
        for failure, stopped_at, exit_code in (("FAIL_PUSH", "push-bundle", 7), ("FAIL_APPLY", "tf-roots", 8)):
            with self.subTest(failure=failure):
                self.make_workspace()
                self.env[failure] = "1"
                step, result = self.run_pipeline()
                self.assertEqual(step, stopped_at, result.stdout + result.stderr)
                self.assertEqual(result.returncode, exit_code)
                self.assertFalse(any(call[:4] == ["gcloud", "builds", "triggers", "run"] for call in self.calls()))

    def test_publisher_rejects_non_digest_handoff(self):
        (self.workspace / "tf-control.env").write_text("export SKIP_TERRAFORM=false\n")
        (self.workspace / "tf-promotion.env").write_text("export TF_BUNDLE_REF=terraform-bundle:latest\n")
        result = self.run_step("publish-terraform-release")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("non-digest", result.stdout)
        self.assertEqual(self.calls(), [])

    def test_publisher_requires_valid_control_before_release(self):
        for control in (None, "export SKIP_TERRAFORM=maybe\n", "export OTHER=true\n"):
            with self.subTest(control=control):
                self.make_workspace()
                if control is not None:
                    (self.workspace / "tf-control.env").write_text(control)
                (self.workspace / "tf-promotion.env").write_text(f"export TF_BUNDLE_REF={self.digest()}\n")
                (self.workspace / "qa-applied").touch()
                result = self.run_step("publish-terraform-release")
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.calls(), [])

    def test_skipped_publisher_needs_no_bundle_handoff(self):
        (self.workspace / "tf-control.env").write_text("export SKIP_TERRAFORM=true\n")
        result = self.run_step("publish-terraform-release")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls(), [])

    def test_dependencies_keep_bundle_pristine_and_queue_after_qa_success(self):
        chain = ("tf-fmt", *PRODUCER[:3], "tf-roots", PRODUCER[3])
        for before, after in zip(chain, chain[1:]):
            self.assertEqual(self.steps[after]["waitFor"], [before])


if __name__ == "__main__":
    unittest.main()
