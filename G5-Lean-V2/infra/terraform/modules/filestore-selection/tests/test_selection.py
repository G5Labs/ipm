"""Exercise real committed-map transitions in an isolated providerless root."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


MODULE = Path(__file__).resolve().parents[1]
CONFIG = MODULE.parents[2] / "config" / "filestore-migration.json"
# The release static-test step installs Terraform here before materialization.
TERRAFORM = shutil.which("terraform") or "/workspace/bin/terraform"


@unittest.skipIf(os.environ.get("G5_TERRAFORM_SHELL_STUB") == "1",
                 "Shell harness supplies a Terraform stub; selector transitions require real Terraform")
class SelectionTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="g5-filestore-selection-")
        self.addCleanup(self.temp.cleanup)
        infra = Path(self.temp.name) / "infra"
        self.root = infra / "terraform" / "modules" / "filestore-selection"
        self.root.mkdir(parents=True)
        shutil.copyfile(MODULE / "main.tf", self.root / "main.tf")
        self.config = infra / "config" / "filestore-migration.json"
        self.config.parent.mkdir()
        self.migrations = json.loads(CONFIG.read_text())
        self.config.write_text(json.dumps(self.migrations))
        self.run_tf("init", "-backend=false", "-input=false")

    def run_tf(self, *args, success=True):
        result = subprocess.run(
            [TERRAFORM, f"-chdir={self.root}", args[0], "-no-color", *args[1:]],
            text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        )
        self.assertEqual(result.returncode == 0, success, result.stdout)
        return result.stdout

    def plan(self, environment, success=True):
        self.config.write_text(json.dumps(self.migrations))
        output = self.run_tf(
            "plan", "-input=false", "-lock=false", "-refresh=false",
            f"-var=environment={environment}", "-out=selection.tfplan", success=success,
        )
        if not success:
            self.assertIn("The committed Filestore map must define", output)
            return None
        return json.loads(self.run_tf("show", "-json", "selection.tfplan"))["planned_values"]["outputs"]["configuration"]["value"]

    def test_preparation_does_not_select_any_candidate(self):
        self.migrations = {name: {"provision_zonal": True, "active": "legacy"}
                           for name in ("qa", "staging", "prod")}
        for environment in self.migrations:
            with self.subTest(environment=environment):
                selected = self.plan(environment)
                self.assertTrue(selected["provision_zonal"])
                self.assertEqual(selected["selected"], selected["legacy"])
                self.assertNotEqual(selected["selected"]["instance_name"], selected["zonal"]["instance_name"])

    def test_each_environment_can_switch_independently(self):
        for environment in ("qa", "staging", "prod"):
            with self.subTest(environment=environment):
                self.migrations = {name: {"provision_zonal": True, "active": "legacy"}
                                   for name in ("qa", "staging", "prod")}
                self.migrations[environment]["active"] = "zonal"
                selected = self.plan(environment)
                self.assertEqual(selected["selected"], selected["zonal"])
                self.assertEqual(selected["selected"]["instance_name"], f"g5-lean-{environment}-repos-zonal")
                self.assertEqual(selected["selected"]["pvc_name"], "g5-repos-zonal-pvc")
                other = "prod" if environment != "prod" else "qa"
                unchanged = self.plan(other)
                self.assertEqual(unchanged["selected"], unchanged["legacy"])

    def test_committed_map_provisions_and_selects_zonal_in_all_environments(self):
        self.assertEqual(self.migrations, {
            name: {"provision_zonal": True, "active": "zonal"}
            for name in ("qa", "staging", "prod")
        })
        for environment in self.migrations:
            with self.subTest(environment=environment):
                selected = self.plan(environment)
                self.assertTrue(selected["provision_zonal"])
                self.assertEqual(selected["selected"], selected["zonal"])

    def test_cannot_select_unprovisioned_candidate(self):
        self.migrations["qa"] = {"provision_zonal": False, "active": "zonal"}
        self.plan("qa", success=False)

    def test_invalid_selection_anywhere_fails_closed(self):
        self.migrations["prod"]["active"] = "unknown"
        self.plan("qa", success=False)

    def test_boolean_must_not_be_a_string(self):
        self.migrations["qa"]["provision_zonal"] = "true"
        self.plan("qa", success=False)

    def test_unexpected_environment_or_field_is_rejected(self):
        self.migrations["dev"] = {"provision_zonal": True, "active": "zonal"}
        self.plan("qa", success=False)
        del self.migrations["dev"]
        self.migrations["qa"]["activate"] = True
        self.plan("qa", success=False)


if __name__ == "__main__":
    unittest.main()
