#!/usr/bin/env python3
"""Render the real chart for the hosted identity boundary and local compatibility."""
import os
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
CHART = ROOT / "infra/helm/g5"
HELM = os.environ.get("HELM_BIN") or shutil.which("helm")


class WorkerHostIdentity(unittest.TestCase):
    def render(self, environment=None, project="local-project", account=None):
        """Render the real chart with optional hosted overlay and identity overrides."""
        self.assertTrue(HELM, "Set HELM_BIN or install Helm to exercise actual chart rendering")
        command = [HELM, "template", "g5", str(CHART), "-f", str(CHART / "values.yaml"),
                   "-f", str(CHART / "values-control-plane.yaml")]
        if environment:
            command += ["-f", str(CHART / f"values-{environment}.yaml")]
        values = {"global.projectId": project, "global.clusterName": f"g5-lean-{environment or 'local'}",
                  "global.registry": "example.invalid/test", "global.imageTag": "test",
                  "api.env.G5_WORKER_IMAGE": "projects/image-project/global/images/g5-worker-test"}
        if account is not None:
            values["api.env.G5_WORKER_SERVICE_ACCOUNT"] = account
        for key, value in values.items():
            command += ["--set-string", f"{key}={value}"]
        return subprocess.run(command, cwd=ROOT, capture_output=True, text=True)

    def test_hosted_overlays_attach_environment_identity(self):
        """Require each hosted overlay to select its own dedicated worker-host account."""
        for environment, project in [("qa", "g5-qa-1ccc"), ("staging", "g5lean-staging-1ccc"), ("prod", "g5lean-prod-1ccc")]:
            with self.subTest(environment=environment):
                rendered = self.render(environment, project)
                self.assertEqual(rendered.returncode, 0, rendered.stderr)
                self.assertIn(f'g5-lean-{environment}-host@{project}.iam.gserviceaccount.com', rendered.stdout)
                self.assertIn('name: G5_WORKER_SERVICE_ACCOUNT', rendered.stdout)

    def test_hosted_missing_or_wrong_identity_is_rejected(self):
        """Reject absent, default, or foreign host identities in hosted chart renders."""
        for account in ["", "default", "g5-lean-prod-host@g5lean-prod-1ccc.iam.gserviceaccount.com", "89784110315-compute@developer.gserviceaccount.com"]:
            with self.subTest(account=account):
                rendered = self.render("qa", "g5-qa-1ccc", account)
                self.assertNotEqual(rendered.returncode, 0)
                self.assertIn("G5_WORKER_SERVICE_ACCOUNT must be", rendered.stderr)
        self.assertNotEqual(self.render(project="g5-qa-1ccc").returncode, 0)

    def test_unrelated_install_can_omit_host_configuration(self):
        """Preserve chart rendering without a host account for unrelated installations."""
        rendered = self.render()
        self.assertEqual(rendered.returncode, 0, rendered.stderr)
        self.assertNotIn('name: G5_WORKER_SERVICE_ACCOUNT', rendered.stdout)


if __name__ == "__main__":
    unittest.main()
