"""Keep migration validation on the production entry point, including rehearsals."""

from pathlib import Path
import os
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
RUNNER = "/workspace/.ci-migration-artifacts/g5-db-migrate"


def steps(source):
    return {
        match.group(1): match.group(2)
        for match in re.finditer(
            r"^  - id: ([\w-]+)\n(.*?)(?=^  - id: |\Z)", source, re.M | re.S
        )
    }


def migration_ci_violations(source):
    config = steps(source)
    failures = []
    for step_id, step in config.items():
        if not step_id.startswith("db-migrate"):
            continue
        commands = "\n".join(
            line for line in step.splitlines() if not line.lstrip().startswith("#")
        )
        if re.search(r"\b(?:cargo\s+)?sqlx\s+migrate\s+run\b", commands):
            failures.append(f"{step_id} bypasses the production runner")
        if RUNNER not in commands or "G5_MIGRATION_SOURCE=" not in step:
            failures.append(f"{step_id} does not invoke the candidate production runner")
    primary = config["db-migrate"]
    if "DATABASE_URL=postgres://g5_migrator:" not in primary:
        failures.append("primary migration must use the restricted migrator")
    if "'build-db-migration-runner'" not in primary:
        failures.append("primary migration must wait for the candidate executable")
    if "'cargo-fetch'" not in config.get("build-db-migration-runner", ""):
        failures.append("runner build must follow Cargo cache validation")
    return failures


class MigrationCiContract(unittest.TestCase):
    def test_both_ci_lanes_use_the_candidate_production_runner(self):
        for lane in ("rust", "e2e"):
            with self.subTest(lane=lane):
                source = (ROOT / f"infra/ci/cloudbuild-ci-{lane}.yaml").read_text()
                self.assertEqual(migration_ci_violations(source), [])
                self.assertIn("test_migration_runner_ci.py", source)

    def test_guard_rejects_raw_sqlx_and_missing_candidate_dependency(self):
        source = (ROOT / "infra/ci/cloudbuild-ci-rust.yaml").read_text()
        for mutant in (
            source.replace(RUNNER, "sqlx migrate run --source migrations"),
            source.replace("'build-db-migration-runner', ", ""),
            source.replace("postgres://g5_migrator:", "postgres://g5:"),
        ):
            self.assertTrue(migration_ci_violations(mutant))

    def test_guard_allows_comments_and_quoted_executable(self):
        source = (ROOT / "infra/ci/cloudbuild-ci-rust.yaml").read_text()
        source = source.replace(RUNNER, f'"{RUNNER}" # use the candidate')
        source = source.replace("# Match Helm's runner", "# sqlx migrate run is deliberately bypassed\n        # Match Helm's runner")
        self.assertEqual(migration_ci_violations(source), [])

    def test_required_integration_lane_includes_every_api_migration_test(self):
        integration = steps((ROOT / "infra/ci/cloudbuild-ci-rust.yaml").read_text())["rust-integration"]
        self.assertIn("(package(=g5-api) | package(=fake-provider-cli)) and kind(=test)", integration)
        self.assertIn("--test '*'", integration)
        self.assertNotIn("test(migration_", integration)


class UpgradeRehearsalRouting(unittest.TestCase):
    """Routing probes only; the Rust/PostgreSQL regression proves DB behavior."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        scripts = self.root / "scripts"
        scripts.mkdir()
        self.script = scripts / "rehearse-migration-upgrade.sh"
        self.script.write_text((ROOT / "scripts/rehearse-migration-upgrade.sh").read_text())
        for helper in ("ci-provision-migrator.sh", "bootstrap-runtime-db-users.sh"):
            (scripts / helper).write_text("exit 0\n")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "calls"
        self.head = self.root / "head"
        self.head.mkdir()
        self.rows = self.root / "rows.sql"
        self.rows.write_text("SELECT 1;\n")
        self.write_tool("psql", 'printf "psql\\n" >> "$CALL_LOG"\n')
        self.write_tool("git", f'''#!{sys.executable}
import io, sys, tarfile
assert sys.argv[1:] == ['archive', 'base', 'backend/g5-api/src/db/migrations']
with tarfile.open(fileobj=sys.stdout.buffer, mode='w|') as archive:
    data = b'SELECT 1;'
    info = tarfile.TarInfo('backend/g5-api/src/db/migrations/001_base.sql')
    info.size = len(data)
    archive.addfile(info, io.BytesIO(data))
''', shebang=False)
        self.runner = self.write_tool("g5-db-migrate", '''printf 'runner %s %s\n' "$DATABASE_URL" "$G5_MIGRATION_SOURCE" >> "$CALL_LOG"
test -d "$G5_MIGRATION_SOURCE"
if [[ "$G5_MIGRATION_SOURCE" == "$HEAD_MIGRATIONS" ]]; then exit "${HEAD_EXIT:-0}"; fi
''')
        self.env = {
            **os.environ,
            "PATH": str(self.bin) + os.pathsep + os.environ["PATH"],
            "PGPASSWORD": "rehearsal_test",
            "PGHOST": "127.0.0.1",
            "PGPORT": "5432",
            "PGUSER": "g5",
            "G5_MIGRATOR_DB_USER": "g5_migrator",
            "G5_MIGRATOR_DB_PASSWORD": "g5_test",
            "REHEARSAL_DB": "g5_rehearsal",
            "BASE_REF": "base",
            "ROWS_SQL": str(self.rows),
            "HEAD_MIGRATIONS": str(self.head),
            "CALL_LOG": str(self.log),
            "G5_DB_MIGRATE_BIN": str(self.runner),
        }

    def write_tool(self, name, source, shebang=True):
        path = self.bin / name
        path.write_text(("#!/usr/bin/env bash\nset -eu\n" if shebang else "") + source)
        path.chmod(0o755)
        return path

    def run_script(self):
        return subprocess.run(["bash", str(self.script)], cwd=self.root, env=self.env, text=True, capture_output=True)

    def test_missing_or_nonexecutable_runner_fails_before_touching_postgres(self):
        for value in (None, str(self.rows)):
            with self.subTest(value=value):
                if value is None:
                    self.env.pop("G5_DB_MIGRATE_BIN", None)
                else:
                    self.env["G5_DB_MIGRATE_BIN"] = value
                result = self.run_script()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("G5_DB_MIGRATE_BIN", result.stderr)
                self.assertFalse(self.log.exists())

    def test_base_upgrade_and_replay_all_use_the_production_runner(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        runs = [line for line in self.log.read_text().splitlines() if line.startswith("runner ")]
        self.assertEqual(len(runs), 3)
        self.assertTrue(all("postgres://g5_migrator:" in line for line in runs))
        self.assertTrue(runs[0].endswith("/base/backend/g5-api/src/db/migrations"))
        self.assertEqual(runs[1:], [f"runner postgres://g5_migrator:g5_test@127.0.0.1:5432/g5_rehearsal {self.head}"] * 2)

    def test_upgrade_failure_stops_without_reporting_a_successful_replay(self):
        self.env["HEAD_EXIT"] = "23"
        result = self.run_script()
        self.assertEqual(result.returncode, 23)
        self.assertNotIn("OK:", result.stderr)
        self.assertEqual(self.log.read_text().count("runner "), 2)


if __name__ == "__main__":
    unittest.main()
