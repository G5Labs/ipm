"""Cloud Build waitFor ordering contract (cicd ontology [D27]).

Cloud Build rejects a config whose `waitFor` references a step id that is not
defined EARLIER in the steps list — a latent-until-first-run failure (the PR
#1885 review caught exactly this: a forward reference that would have killed
the first real deploy at config parse). This check asserts, for every
`cloudbuild*.yaml` at the generation root, that each waitFor entry is either
'-' or the id of an earlier step, and that step ids are unique.

Intentionally line-based (stdlib only — the CI builder has no PyYAML): it
relies on the repo convention that steps declare `- id:` on one line and
`waitFor: [...]` inline on one line. A multi-line waitFor list would be
invisible to this check; keep the convention.

Run with: python3 test_cloudbuild_waitfor_order.py (from scripts)
or python3 -m unittest test_cloudbuild_waitfor_order.
"""
from __future__ import annotations

import glob
import json
import os
import re
import subprocess
import tempfile
from pathlib import Path
import sys
import unittest

import app_release

GENERATION_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

_ID_RE = re.compile(r"^\s*-\s+id:\s*['\"]?([A-Za-z0-9._-]+)['\"]?\s*$")
_WAITFOR_RE = re.compile(r"^\s*waitFor:\s*\[(.*)\]\s*$")


def inline_block_scalar_lengths(path: str) -> list:
    """Return (step id, length) for every `- |` literal block scalar under a
    step's `args:` in a Cloud Build YAML, measured the way YAML (and therefore
    Cloud Build) sees it: dedented to the block's indent, lines joined with
    newlines, one trailing newline (clip chomping). A plain-text parser so the
    guard needs no YAML library on the CI image."""
    lengths = []
    step_id = None
    with open(path, encoding="utf-8") as handle:
        lines = handle.read().split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]
        id_match = re.match(r"^  - id: (\S+)", line)
        if id_match:
            step_id = id_match.group(1)
        if re.match(r"^\s+- \|[-+]?$", line):
            block = []
            indent = None
            j = i + 1
            while j < len(lines):
                candidate = lines[j]
                if candidate.strip() == "":
                    block.append("")
                    j += 1
                    continue
                leading = len(candidate) - len(candidate.lstrip(" "))
                if indent is None:
                    indent = leading
                if leading < indent:
                    break
                block.append(candidate[indent:])
                j += 1
            while block and block[-1] == "":
                block.pop()
            lengths.append((step_id, len("\n".join(block) + "\n")))
            i = j
            continue
        i += 1
    return lengths


_SEED_HOST_INSTANCE_TYPE_RE = re.compile(r"G5_E2E_HOST_INSTANCE_TYPE=([a-z0-9-]+)")


def seed_host_vcpus(seed_step: str) -> int:
    """vCPUs of the instance type a seed-*-org step declares for its DB-only CI
    worker host. The API caps workspaceVmLimit at that count
    (routes/worker_hosts.rs: `1 ..= vCPUs(instanceType)`), so an advertised
    capacity above it fails the seed at run time (build 54399b47). Unset means
    the script default, c3-standard-4."""
    match = _SEED_HOST_INSTANCE_TYPE_RE.search(seed_step)
    name = match.group(1) if match else "c3-standard-4"
    return int(name.rsplit("-", 1)[1])


def waitfor_violations(path: str) -> list:
    """Report duplicate step ids and forward or unknown inline waitFor refs."""
    seen: list = []
    violations = []
    with open(path, encoding="utf-8") as handle:
        lines = handle.read().splitlines()
    for lineno, line in enumerate(lines, 1):
        id_match = _ID_RE.match(line)
        if id_match:
            step_id = id_match.group(1)
            if step_id in seen:
                violations.append(f"{path}:{lineno}: duplicate step id {step_id!r}")
            seen.append(step_id)
            continue
        wait_match = _WAITFOR_RE.match(line)
        if wait_match:
            refs = [
                r.strip().strip("'\"")
                for r in wait_match.group(1).split(",")
                if r.strip()
            ]
            for ref in refs:
                if ref == "-":
                    continue
                if ref not in seen:
                    violations.append(
                        f"{path}:{lineno}: waitFor {ref!r} is not an earlier step id "
                        "(forward or unknown reference — Cloud Build rejects this at parse)"
                    )
    return violations


class WaitForOrder(unittest.TestCase):
    def test_every_vm_worker_limits_writes_to_its_delegated_cgroup(self):
        """All worker lanes must match the delegated-cgroup entrypoint."""
        path = os.path.join(GENERATION_ROOT, "infra", "ci", "cloudbuild-ci-e2e.yaml")
        with open(path, encoding="utf-8") as handle:
            steps = handle.read().split("\n  - id: ")
        workers = [step for step in steps if "-v /workspace/scripts/ci-host-xfs-entrypoint.sh:" in step]
        self.assertEqual(len(workers), 4)
        for step in workers:
            with self.subTest(step=step.splitlines()[0]):
                self.assertIn("--cgroupns=host", step)
                self.assertIn("/sys/fs/cgroup:/sys/fs/cgroup:ro", step)
                self.assertIn("G5D_ROOTFS_STATE_DIR=/var/lib/g5d/rootfs", step)
                self.assertIn('-e G5D_SCRATCH_DISK_MIB="$$HOST_DISK_MIB"', step)
                self.assertIn('-e G5D_HOST_XFS_SIZE_GB="$$HOST_DISK_GIB"', step)
                self.assertRegex(step, r'HOST_DISK_MIB=\$\$\(cat /workspace/[a-z0-9-]+-host-disk-mib\)')
                self.assertRegex(step, r'HOST_DISK_GIB=\$\$\(cat /workspace/[a-z0-9-]+-host-disk-gib\)')

    def test_every_literal_playwright_flow_in_cloudbuild_exists(self):
        path = os.path.join(GENERATION_ROOT, "infra", "ci", "cloudbuild-ci-e2e.yaml")
        with open(path, encoding="utf-8") as handle:
            source = handle.read()
        selectors = set(
            re.findall(r"(?<!frontend/)e2e/flows/[a-z0-9._-]+\.spec\.ts", source)
        )
        self.assertTrue(selectors, "Cloud Build must retain explicit Playwright flow selectors")
        missing = sorted(
            selector
            for selector in selectors
            if not os.path.isfile(os.path.join(GENERATION_ROOT, "frontend", selector))
        )
        self.assertEqual(missing, [], "Cloud Build references deleted Playwright flow specs")

    def test_broad_playwright_concurrency_fits_workspace_host_capacity(self):
        """Admit source/fork fixtures and leave VM lifecycle headroom."""
        path = os.path.join(GENERATION_ROOT, "infra", "ci", "cloudbuild-ci-e2e.yaml")
        with open(path, encoding="utf-8") as handle:
            source = handle.read()

        worker_match = re.search(r"^  _PLAYWRIGHT_WORKERS: '([0-9]+)'$", source, re.MULTILINE)
        self.assertIsNotNone(worker_match)
        workers_per_shard = int(worker_match.group(1))

        start = source.index("  - id: start-workers\n")
        end = source.index("\n  - id:", start)
        host_step = source[start:end]
        capacity_match = re.search(r"G5D_HOST_MAX_MICROVMS=([0-9]+)", host_step)
        self.assertIsNotNone(capacity_match)
        host_capacity = int(capacity_match.group(1))

        seed_start = source.index("  - id: seed-ci-org\n")
        seed_end = source.index("\n  - id:", seed_start)
        seed_step = source[seed_start:seed_end]
        advertised_match = re.search(
            r"G5_E2E_HOST_WORKSPACE_VM_LIMIT=([0-9]+)", seed_step
        )
        self.assertIsNotNone(advertised_match)
        self.assertEqual(int(advertised_match.group(1)), host_capacity)
        self.assertLessEqual(
            host_capacity,
            seed_host_vcpus(seed_step),
            "the API rejects workspaceVmLimit above the declared instance type's vCPUs; set G5_E2E_HOST_INSTANCE_TYPE",
        )

        fixture_start = source.index("  - id: seed-playwright-workspace\n")
        fixture_end = source.index("\n  - id:", fixture_start)
        fixture_step = source[fixture_start:fixture_end]
        self.assertIn("./scripts/seed-test-workspace.sh", fixture_step)
        running_seed_workspaces = 1

        shard_ids = ["playwright-e2e-shard-1", "playwright-e2e-shard-2", "playwright-e2e-shard-3"]
        for shard_id in shard_ids:
            shard_start = source.index(f"  - id: {shard_id}\n")
            shard_end = source.index("\n  - id:", shard_start)
            self.assertIn(
                "PLAYWRIGHT_WORKERS=${_PLAYWRIGHT_WORKERS}",
                source[shard_start:shard_end],
            )

        # A browser worker can retain a source and its running fork. Counting
        # only one VM allowed four workers to exhaust the eight-slot host,
        # timing out a short editor test in queued_for_capacity.
        fixture_vms_per_worker = 2
        self.assertLess(
            running_seed_workspaces
            + workers_per_shard * len(shard_ids) * fixture_vms_per_worker,
            host_capacity,
            "the running seed and source/fork fixtures must leave a VM lifecycle slot",
        )

    def test_ci_image_step_is_shared_and_keyed_on_toolchain_inputs(self):
        """The CI image step is duplicated in both pipelines and must stay
        identical in every executable line (comments may differ); its content
        key must not include .sqlx (deployment g5.quality.delivery.ci.image-key)
        and its miss path must pull base/base-rust from the CI mirror without
        mirroring the workspace-warmed tier."""

        def executable_lines(path: str) -> list:
            with open(os.path.join(GENERATION_ROOT, path), encoding="utf-8") as handle:
                source = handle.read()
            start = source.index("  - id: build-ci-image\n")
            end = source.index("\n  - id:", start + 1)
            return [
                line for line in source[start:end].split("\n")
                if line.strip() and not line.strip().startswith("#")
            ]

        e2e = executable_lines("infra/ci/cloudbuild-ci-e2e.yaml")
        rust = executable_lines("infra/ci/cloudbuild-ci-rust.yaml")
        self.assertEqual(e2e, rust, "build-ci-image must stay byte-identical across pipelines in its executable lines")
        body = "\n".join(e2e)
        self.assertNotIn(".sqlx", body, ".sqlx must not be a CI image key input")
        for required in ("Dockerfile.ci-builder", "Dockerfile.base-rust-warm", "scripts/build-cloud-hypervisor-runtime.sh", "scripts/install-cloud-hypervisor-platform.sh", "infra/guest/versions.env", "infra/guest/kernel.config", "infra/guest/init.c", "Cargo.lock"):
            self.assertIn(required, body, f"{required} must remain a CI image key input")
        self.assertIn('G5_BASE_IMAGE_MIRROR_TIERS="base base-rust"', body)
        self.assertIn("ensure-base-images.sh base base-rust base-rust-warm", body)
    def test_provider_agent_fanout_fits_its_host_capacity(self):
        """deployment g5.quality.delivery.ci.provider-agent-parallel: the lane's
        parallel-safe selectors run on _PA_PLAYWRIGHT_WORKERS workers through
        the selector helper; the running seed plus those workers must stay
        below g5d-host-pa-1's slots, and the seed must advertise them."""
        path = os.path.join(GENERATION_ROOT, "infra", "ci", "cloudbuild-ci-e2e.yaml")
        with open(path, encoding="utf-8") as handle:
            source = handle.read()

        def step(step_id: str) -> str:
            start = source.index(f"  - id: {step_id}\n")
            end = source.find("\n  - id:", start + 1)
            return source[start:] if end == -1 else source[start:end]

        worker_match = re.search(r"^  _PA_PLAYWRIGHT_WORKERS: '([0-9]+)'$", source, re.MULTILINE)
        self.assertIsNotNone(worker_match)
        workers = int(worker_match.group(1))
        host_step = step("start-pa-worker")
        capacity = int(re.search(r"G5D_HOST_MAX_MICROVMS=([0-9]+)", host_step).group(1))
        advertised = int(re.search(r"G5_E2E_HOST_WORKSPACE_VM_LIMIT=([0-9]+)", step("seed-pa-org")).group(1))
        self.assertEqual(advertised, capacity)
        self.assertLessEqual(
            advertised,
            seed_host_vcpus(step("seed-pa-org")),
            "the API rejects workspaceVmLimit above the declared instance type's vCPUs; set G5_E2E_HOST_INSTANCE_TYPE",
        )
        running_seed = 1
        self.assertLess(running_seed + workers, capacity, "leave a VM lifecycle slot on g5d-host-pa-1")
        provider = step("provider-agent-playwright-e2e")
        self.assertIn("G5_PA_PLAYWRIGHT_WORKERS=${_PA_PLAYWRIGHT_WORKERS}", provider)
        self.assertIn('ci-run-playwright-selector.sh "$$VALUE_LOOP_SELECTOR"', provider)
        # The selector loop must not bypass the helper with a direct serial run.
        loop = provider[provider.index("while IFS='|' read -r VALUE_LOOP_SELECTOR"):provider.index("PLAYWRIGHT_VALUE_LOOPS")]
        self.assertNotIn("playwright test", loop)

    def test_browser_rebalance_and_receipt_require_complete_shard_reports(self):
        path = os.path.join(GENERATION_ROOT, "infra", "ci", "cloudbuild-ci-e2e.yaml")
        with open(path, encoding="utf-8") as handle:
            source = handle.read()

        def step(step_id: str) -> str:
            start = source.index(f"  - id: {step_id}\n")
            end = source.find("\n  - id:", start + 1)
            return source[start:] if end == -1 else source[start:end]

        planner = step("plan-playwright-shards")
        self.assertIn("plan_playwright_shards.py plan", planner)
        self.assertIn("plan_playwright_shards.py verify", planner)
        # Placement weights are the committed measurements, simulated on each
        # shard's actual worker count (ontology g5.quality.delivery.ci.browser-shards).
        self.assertIn("--durations /workspace/infra/ci/playwright-file-durations.json", planner)
        self.assertTrue(os.path.isfile(os.path.join(GENERATION_ROOT, "infra", "ci", "playwright-file-durations.json")))
        self.assertIn('--workers "$${PLAYWRIGHT_WORKERS}"', planner)
        self.assertIn("PLAYWRIGHT_WORKERS=${_PLAYWRIGHT_WORKERS}", planner)
        for index in (1, 2, 3):
            shard = step(f"playwright-e2e-shard-{index}")
            self.assertIn("'plan-playwright-shards'", shard)
            self.assertIn(f"shard-{index}.test-list", shard)
            self.assertIn(f"playwright-report-shard-{index}.json", shard)
            self.assertIn("--grep-invert 'Claude Code routing", shard)

        provider = step("provider-agent-playwright-e2e")
        # deployment g5.quality.delivery.ci.provider-agent-overlap: the lane
        # overlaps the broad shards, so the shards must NOT be waitFor edges;
        # the receipt tail instead polls every shard marker in-step before
        # it reads the shard reports.
        self.assertNotIn("'playwright-e2e-shard-1'", provider)
        self.assertNotIn("'playwright-e2e-shard-2'", provider)
        self.assertNotIn("'playwright-e2e-shard-3'", provider)
        for gate in ("seed-pa-workspace", "start-pa-client", "plan-playwright-shards"):
            self.assertIn(f"'{gate}'", provider)
        marker_poll = provider.index(
            "ci-wait-markers.sh 3600 /workspace/step-results-playwright-e2e-shard-1.txt "
            "/workspace/step-results-playwright-e2e-shard-2.txt "
            "/workspace/step-results-playwright-e2e-shard-3.txt",
            provider.index("VALUE_LOOP_RESULTS="),
        )
        # The poll must sit AFTER the lane's last own spec (the M9/M12
        # transfer case) and BEFORE the first read of a shard artifact, so
        # the lane's browser work never waits on the shards.
        self.assertGreater(marker_poll, provider.index("--grep 'M9/M12 transfer fences live chat'"))
        for shard_read in (
            "plan_playwright_shards.py verify",
            "cp /workspace/playwright-value-loop-inventory.json",
            "record_playwright_shard_results.py",
        ):
            self.assertLess(marker_poll, provider.index(shard_read), shard_read)
        # The lane's API restarts target its own replica, never the shards' API.
        self.assertIn("export E2E_API_CONTAINER=g5-api-pa", provider)
        self.assertIn("export E2E_BASE_URL=https://app-pa.g5-e2e.test", provider)
        self.assertIn("export E2E_PREVIEW_HOST_RESOLVER='MAP *.preview-pa.g5-e2e.test g5-test-client-pa'", provider)
        self.assertIn("plan_playwright_shards.py verify", provider)
        self.assertIn("record_playwright_shard_results.py", provider)
        self.assertIn("--provider-report test-results/report.json", provider)
        self.assertIn("--grep 'M9/M12 transfer fences live chat'", provider)

    """Protect the repository-wide Cloud Build step-ordering contract."""

    def test_inline_step_scripts_fit_cloud_build_arg_limit(self):
        """Cloud Build rejects a step arg over 10,000 chars at trigger time
        ("invalid .steps field: build step N arg 1 too long"), which fails the
        PR check before any build runs. Keep a margin so comments can grow."""
        limit = 10_000
        margin = 400
        paths = sorted(glob.glob(os.path.join(GENERATION_ROOT, "infra/ci/cloudbuild*.yaml")))
        offenders = []
        for path in paths:
            for step_id, length in inline_block_scalar_lengths(path):
                if length > limit - margin:
                    offenders.append(
                        f"{os.path.basename(path)} step {step_id}: {length} chars "
                        f"(limit {limit}, margin {margin}); move logic into a scripts/ helper"
                    )
        self.assertEqual(offenders, [], "\n".join(offenders))

    def test_all_cloudbuild_yamls_have_backward_waitfor_refs(self):
        """Require every inline dependency to reference an earlier unique step."""
        paths = sorted(glob.glob(os.path.join(GENERATION_ROOT, "infra/ci/cloudbuild*.yaml")))
        self.assertTrue(paths, "no cloudbuild*.yaml found — wrong cwd?")
        problems = []
        for path in paths:
            problems.extend(waitfor_violations(path))
        self.assertEqual(problems, [], "\n".join(problems))

    def test_workspace_host_seed_start_wait_chains_are_exact(self):
        """Keep every required lane bound to its seeded host identity."""
        path = os.path.join(GENERATION_ROOT, "infra/ci/cloudbuild-ci-e2e.yaml")
        with open(path, encoding="utf-8") as handle:
            text = handle.read()

        def step(step_id: str) -> str:
            start = text.index(f"  - id: {step_id}\n")
            end = text.find("\n  - id:", start + 1)
            return text[start:] if end == -1 else text[start:end]

        chains = [
            ("seed-ci-org", "start-workers", "wait-workers", None),
            ("seed-value-loop-org", "start-value-loop-worker", "wait-value-loop-workers", None),
            ("seed-rust-e2e-org", "start-rust-e2e-worker", "wait-rust-e2e-workers", "start-rust-e2e-api"),
            ("seed-pa-org", "start-pa-worker", "wait-pa-workers", "start-pa-api"),
        ]
        for seed_id, start_id, wait_id, api_id in chains:
            with self.subTest(chain=(seed_id, start_id, wait_id)):
                start_step = step(start_id)
                wait_step = step(wait_id)
                self.assertIn(seed_id, start_step)
                if api_id is not None:
                    self.assertIn(api_id, start_step)
                self.assertRegex(
                    start_step,
                    r"/usr/local/bin/ci-host-xfs-entrypoint\.sh \\\n\s+host",
                )
                self.assertNotIn("host-up", start_step)
                self.assertIn(start_id, wait_step)
                self.assertIn("FROM worker_hosts", wait_step)
                self.assertIn("host_id = '$$HOST_ID'", wait_step)
                self.assertIn("status = 'ready'", wait_step)
                self.assertIn("last_heartbeat IS NOT NULL", wait_step)

    def test_cold_worker_base_rebuild_waits_for_frontend_test(self):
        """A cold worker-base rebuild compiles the guest kernel on every CPU;
        running beside vitest it starved the test workers past their fixed
        60-second RPC deadline, failing frontend-test and cancelling the
        rebuild before it could publish (PR #1012 builds cd45c056, 5d54cd65;
        ontology g5.quality.delivery.ci.release-overlap). The fallback waits
        for frontend-test's marker; a warm pull keeps its edge and never
        waits."""
        path = os.path.join(GENERATION_ROOT, "infra", "ci", "cloudbuild-ci-e2e.yaml")
        with open(path, encoding="utf-8") as handle:
            text = handle.read()
        frontend = text[text.index("  - id: frontend-test\n"):text.index("  - id: frontend-audit\n")]
        self.assertLess(
            frontend.index("npx vitest run"),
            frontend.index("touch /workspace/frontend-test-done"),
            "frontend-test must write its marker only after vitest passed",
        )
        prepare = text[text.index("  - id: prepare-worker-base-image\n"):text.index("  - id: build-worker-runtime-shell\n")]
        self.assertIn("waitFor: ['pull-worker-base-image']", prepare, "the warm pull path keeps its single edge")
        fallback = prepare[prepare.index("building local fallback"):]
        wait = fallback.index("until [ -f /workspace/frontend-test-done ] || [ \"$${waited}\" -ge 1800 ]")
        self.assertLess(wait, fallback.index("--file=infra/docker/Dockerfile.g5d-base"), "the cold rebuild waits before it builds")
        self.assertNotIn("frontend-test-done", prepare[:prepare.index("building local fallback")], "a warm pull must not wait")

    def test_retired_execution_lifecycle_writer_guard_is_not_scheduled(self):
        """Do not invoke the deleted transitional writer inventory in CI."""
        path = os.path.join(GENERATION_ROOT, "infra/ci/cloudbuild-ci-rust.yaml")
        with open(path, encoding="utf-8") as handle:
            text = handle.read()
        self.assertNotIn("execution-lifecycle-writers", text)
        self.assertFalse(
            os.path.exists(
                os.path.join(
                    GENERATION_ROOT,
                    "scripts",
                    "check-execution-lifecycle-writers.py",
                )
            )
        )


class DeployStepPrerequisites(unittest.TestCase):
    """Protect deployment prerequisites, image refs, and runtime toolchain pins."""

    def test_pnc_cash_receipt_runtime_is_built_and_digest_plumbed(self):
        """Check the PNC build/push dependencies and promotion digest plumbing."""
        path = os.path.join(GENERATION_ROOT, "infra/ci/cloudbuild-deploy.yaml")
        with open(path, encoding="utf-8") as handle:
            text = handle.read()

        required = [
            '_G5D_PNC_CASH_RECEIPT_IMAGE_REF: ""',
            "- id: build-g5d-pnc-cash-receipt",
            "--file=infra/docker/Dockerfile.g5d-pnc-cash-receipt",
            "- id: push-g5d-pnc-cash-receipt",
            'docker push --all-tags "$${AR}/g5d-pnc-cash-receipt"',
            'G5D_PNC_CASH_RECEIPT_IMAGE_REF="$$(resolve_ref',
            'G5D_PNC_CASH_RECEIPT_IMAGE_REF="${_G5D_PNC_CASH_RECEIPT_IMAGE_REF}"',
            'export G5D_PNC_CASH_RECEIPT_IMAGE_REF=$${G5D_PNC_CASH_RECEIPT_IMAGE_REF}',
            "Expected immutable g5d-pnc-cash-receipt image ref",
            "PNC Cash Receipt image:",
        ]
        for fragment in required:
            self.assertIn(fragment, text)

        for step_id, prerequisite in [
            ("push-g5d-pnc-cash-receipt", "build-g5d-pnc-cash-receipt"),
            ("resolve-image-refs", "push-g5d-pnc-cash-receipt"),
            ("build-g5d-g5dev", "push-g5d-pnc-cash-receipt"),
        ]:
            with self.subTest(step=step_id):
                start = text.index(f"- id: {step_id}\n")
                end = text.index("\n  - id:", start)
                waits = [
                    match.group(1)
                    for line in text[start:end].splitlines()
                    if (match := _WAITFOR_RE.match(line))
                ]
                self.assertEqual(len(waits), 1, "expected one inline waitFor list")
                dependencies = [ref.strip().strip("'\"") for ref in waits[0].split(",")]
                self.assertIn(prerequisite, dependencies)

        self.assertLess(
            text.index("- id: build-g5d-pnc-cash-receipt"),
            text.index("- id: push-g5d-pnc-cash-receipt"),
        )
        self.assertLess(
            text.index("- id: push-g5d-pnc-cash-receipt"),
            text.index("- id: resolve-image-refs"),
        )

    def test_deployment_template_capacity_matches_all_managed_host_defaults(self):
        deploy = Path(GENERATION_ROOT, "infra/ci/cloudbuild-deploy.yaml").read_text()
        template = re.search(r'_WORKLOAD_TEMPLATE_SIZE_MIB: [\'"]([0-9]+)[\'"]', deploy)
        self.assertIsNotNone(template)
        for environment in ("qa", "staging", "prod"):
            with self.subTest(environment=environment):
                values = Path(GENERATION_ROOT, f"infra/helm/g5/values-{environment}.yaml").read_text()
                host = re.search(r'G5_WORKER_SCRATCH_DISK_MIB: [\'"]([0-9]+)[\'"]', values)
                self.assertIsNone(host, "managed hosts use their organization's recorded V")
                self.assertEqual(template.group(1), "81920",
                                 "default-capacity prebuilt artifact remains an optional acceleration")

    def test_pnc_cash_receipt_digest_guard_rejects_malformed_refs(self):
        """Run the production Bash guard against valid, empty, and invalid refs."""
        path = os.path.join(GENERATION_ROOT, "infra/ci/cloudbuild-deploy.yaml")
        with open(path, encoding="utf-8") as handle:
            text = handle.read()
        start = text.index("        # The PNC Cash Receipt image is")
        end = text.index("        # The g5dev image is", start)
        # Execute the actual inline bash guard after Cloud Build's $$ escaping,
        # without invoking Docker, writing deployment state, or mocking a service.
        guard = text[start:end].replace("$$", "$")
        prefix = "registry.example/team/pnc@sha256:"
        cases = [
            ("", True),
            (prefix + "a" * 64, True),
            (prefix + "ABCDEF0123456789" * 4, True),
            ("registry.example/team/pnc:latest", False),
            (prefix, False),
            (prefix + "not-a-digest", False),
            (prefix + "a" * 63, False),
            (prefix + "a" * 65, False),
            (prefix + "g" * 64, False),
            (prefix + "a" * 64 + ":latest", False),
            (prefix + "a" * 64 + "\n", False),
            ("@sha256:" + "a" * 64, False),
        ]
        for image_ref, accepted in cases:
            with self.subTest(image_ref=image_ref):
                result = subprocess.run(
                    ["bash", "-euo", "pipefail", "-c", guard],
                    env={"PATH": os.defpath, "G5D_PNC_CASH_RECEIPT_IMAGE_REF": image_ref},
                    capture_output=True,
                    text=True,
                    timeout=5,
                    check=False,
                )
                self.assertEqual(result.returncode, 0 if accepted else 1, result.stdout + result.stderr)
                if not accepted:
                    self.assertIn("Expected immutable g5d-pnc-cash-receipt image ref", result.stdout)

    def test_pnc_cash_receipt_dockerfile_pins_the_repository_toolchain(self):
        """Check the PNC runtime's pinned tools, guest PATH, ABI, and user."""
        path = os.path.join(
            GENERATION_ROOT, "infra/docker", "Dockerfile.g5d-pnc-cash-receipt"
        )
        with open(path, encoding="utf-8") as handle:
            text = handle.read()

        for fragment in [
            "ARG BASE_IMAGE=g5/base:26.04",
            "ARG MAVEN_VERSION=3.9.8",
            "ARG NODE_VERSION=24.22.1",
            "ARG NPM_VERSION=11.19.1",
            'com.g5labs.runtime.schema="v1"',
            'com.g5labs.runtime.base_image="ubuntu-26.04-glibc-2.43"',
            'com.g5labs.runtime.workload.class="java21-spring-boot-3.5-angular-20"',
            "MAVEN_OPTS=-Dmaven.repo.local=/work/.m2/repository",
            "USER 1000:1000",
            "procps",
            'export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/opt/g5/bin"',
            'node --version | grep -Fx "v${NODE_VERSION}"',
            'npm --version | grep -Fx "${NPM_VERSION}"',
            "getconf GNU_LIBC_VERSION | grep -F 'glibc 2.43'",
        ]:
            self.assertIn(fragment, text)
        self.assertNotIn("MAVEN_CONFIG=", text)

    def test_pnc_runtimes_fetch_maven_from_central_with_bounded_retries(self):
        """Both PNC images download Maven the same way after QA builds
        2c3bf5ba and 5ba2a296 timed out against archive.apache.org."""
        for name in ["Dockerfile.g5d-pnc-bankworkflow", "Dockerfile.g5d-pnc-cash-receipt"]:
            path = os.path.join(GENERATION_ROOT, "infra/docker", name)
            with open(path, encoding="utf-8") as handle:
                text = handle.read()
            with self.subTest(dockerfile=name):
                self.assertFalse(
                    "archive.apache.org/dist/maven" in text,
                    f"{name} still fetches Maven from archive.apache.org",
                )
                self.assertIn(
                    "RUN curl -fsSL --retry 4 --retry-delay 10 --retry-max-time 1200 \\\n"
                    "        --connect-timeout 20 --max-time 300 --speed-limit 51200 --speed-time 60 \\\n"
                    "        -o /tmp/apache-maven.tar.gz \\\n"
                    '        "https://repo.maven.apache.org/maven2/org/apache/maven/apache-maven/'
                    '${MAVEN_VERSION}/apache-maven-${MAVEN_VERSION}-bin.tar.gz" \\\n'
                    '    && echo "${MAVEN_SHA512}  /tmp/apache-maven.tar.gz" | sha512sum -c -',
                    text,
                )



class QaDeployCleanupTests(unittest.TestCase):
    """Exercise the retained deploy boundary without the retired handoff."""

    @classmethod
    def setUpClass(cls):
        cls.root = Path(GENERATION_ROOT)
        cls.pipeline = (cls.root / "infra/ci/cloudbuild-deploy.yaml").read_text()

    def step(self, name):
        return self.pipeline.split(f"  - id: {name}\n", 1)[1].split("\n  - id:", 1)[0]

    def script(self, name, workspace, **overrides):
        # These steps use the repository's normal eight-space literal block.
        block = self.step(name).split("      - |\n", 1)[1]
        lines = []
        for line in block.splitlines():
            if line and not line.startswith("        "):
                break
            lines.append(line[8:] if line else "")
        values = dict(re.findall(r"^  (_\w+): (.*)$", self.pipeline, re.MULTILINE))
        values = {key: value.strip('"') for key, value in values.items()}
        values.update(PROJECT_ID="g5-qa-1ccc", COMMIT_SHA="a" * 40,
                      _AR_PROJECT="g5-qa-1ccc", _TARGET_PROJECT="g5-qa-1ccc",
                      _WORKER_IMAGE_PROJECT="g5-qa-1ccc")
        values.update(overrides)
        # Cloud Build substitutes single-$ tokens, then unescapes $$ for Bash.
        script = re.sub(r"(?<!\$)\$\{([A-Z_][A-Z0-9_]*)\}",
                        lambda match: values.get(match[1], match[0]), "\n".join(lines))
        return script.replace("$$", "$").replace("/workspace/", f"{workspace}/")

    def invoke(self, script, **environment):
        # Only external effects are simulated; the pipeline's actual Bash runs.
        prelude = r'''
curl() { :; }
sha256sum() { cat >/dev/null; }
tar() { :; }
install() { :; }
gcloud() {
  if [[ "$1" == filestore ]]; then printf '10.0.0.1\n'; fi
}
helm() { printf 'HELM_ARG=%s\n' "$@"; }
'''
        with tempfile.TemporaryDirectory() as directory:
            # Exercise the real Python storage guard; only Kubernetes transport
            # is doubled. Other tests here deploy into an empty installation.
            kubectl = Path(directory) / "kubectl"
            kubectl.write_text(f'''#!{sys.executable}
import json, os, sys
if os.environ.get("FILESTORE_TEST_DENIED"):
    raise SystemExit(1)
documents = json.loads(os.environ.get("FILESTORE_TEST_INVENTORY", "{{}}"))
assert sys.argv[1] == "get"
print(json.dumps(documents.get(sys.argv[2], {{"items": []}})))
''')
            kubectl.chmod(0o755)
            return subprocess.run(["bash", "-c", prelude + script], cwd=self.root,
                                  env={"PATH": f"{directory}:{Path(sys.executable).parent}:{os.defpath}",
                                       "PROJECT_ID": "g5-qa-1ccc", "_EXTERNAL_ACCESS_ENABLED": "false", **environment},
                                  capture_output=True, text=True, timeout=10)

    def test_retired_steps_and_their_configuration_are_gone(self):
        for name in ("pr1884-authority-cutover", "notify-prod-deploy",
                     "tungsten-microvm-canary", "workspace-preview-canary",
                     "canary-preview-screenshots", "canary-screenshots-upload", "trigger-next"):
            with self.subTest(step=name):
                self.assertNotIn(f"- id: {name}\n", self.pipeline)
        active_config = self.pipeline + (self.root / "infra/qa/delivery/main.tf").read_text()
        active_config += (self.root / "scripts/reset-qa.py").read_text()
        for key in ("PR1884_CUTOVER", "_RUN_TUNGSTEN_CANARY",
                    "_RUN_PREVIEW_CANARY", "_SLACK_WEBHOOK_SECRET"):
            with self.subTest(key=key):
                self.assertNotIn(key, active_config)
        self.assertFalse((self.root / "scripts/run-pr1884-authority-cutover.sh").exists())
        self.assertFalse((self.root / "scripts/notify_prod_deploy.py").exists())
        canary = (self.root / "scripts/canary-tungsten-microvm.sh").read_text()
        for key in ("G5_CANARY_PREVIEW_URLS_FILE", "G5_CANARY_SCREENSHOT_ACK_FILE",
                    "G5_CANARY_SCREENSHOT_WAIT_SECS", "G5_CANARY_UI_RESULT_FILE",
                    "G5_CANARY_REQUIRE_UI_WORK", "G5_CANARY_DONE_FILE"):
            with self.subTest(orphaned_handoff=key):
                self.assertNotIn(key, canary)

    def dependencies(self, name):
        waits = re.search(r"^    waitFor: (.*)$", self.step(name), re.MULTILINE)
        return set(re.findall(r"'([^']+)'", waits[1])) if waits else set()

    def ancestors(self, name):
        seen, frontier = set(), {name}
        while frontier:
            step = frontier.pop()
            for dependency in self.dependencies(step):
                if dependency != "-" and dependency not in seen:
                    seen.add(dependency)
                    frontier.add(dependency)
        return seen

    def test_helm_joins_authentication_and_worker_image_validation(self):
        dependencies = self.dependencies
        self.assertEqual(dependencies("helm-deploy"), {"gke-auth", "resolve-worker-image"})
        for step in ("gke-auth", "resolve-worker-image"):
            self.assertIn("check-main-freshness", dependencies(step))
        self.assertIn("verify-release-provenance", dependencies("check-main-freshness"))
        self.assertIn("wait-deploy-turn", dependencies("verify-release-provenance"))
        self.assertIn("required-release-checks", dependencies("promote-release"))
        self.assertIn("smoke-test", dependencies("required-release-checks"))
        self.assertIn("helm-deploy", dependencies("verify-deploy"))
        self.assertIn("verify-deploy", dependencies("smoke-test"))

    def test_release_checks_use_the_python_capable_gcloud_builder(self):
        # The cloud-cli:stable image lacks python3 on PATH. Even QA's
        # disabled-check path executes Python to enforce promotion policy.
        self.assertIn("name: gcr.io/cloud-builders/gcloud\n",
                      self.step("required-release-checks"))

    def test_release_checks_export_preview_hostname(self):
        self.assertIn('export _PREVIEW_HOSTNAME="${_PREVIEW_HOSTNAME}"',
                      self.step("required-release-checks"))

    def test_cve_report_is_off_the_deploy_and_promotion_path(self):
        self.assertEqual(self.dependencies("report-image-cves"), {"check-main-freshness"})
        self.assertIn("allowFailure: true", self.step("report-image-cves"))
        self.assertIn("name: gcr.io/cloud-builders/gcloud", self.step("report-image-cves"))
        # Omitted waitFor means ALL preceding steps in Cloud Build, so check
        # actual implicit edges as well as the usual explicit dependency lists.
        preceding, graph = [], {}
        for name in re.findall(r"^  - id: (\S+)$", self.pipeline, re.MULTILINE):
            graph[name] = self.dependencies(name) if "    waitFor:" in self.step(name) else set(preceding)
            preceding.append(name)
        def ancestors(name):
            seen, pending = set(), list(graph[name])
            while pending:
                item = pending.pop()
                if item != "-" and item not in seen:
                    seen.add(item)
                    pending.extend(graph[item])
            return seen
        for name in ("helm-deploy", "verify-deploy", "smoke-test", "required-release-checks", "promote-release"):
            self.assertNotIn("report-image-cves", ancestors(name), name)

    def test_release_checks_bootstrap_skip_and_unchecked_staging_paths(self):
        cases = (
            ("qa", "false", "", 0, "External release acceptance DISABLED"),
            ("qa", "true", "", 0, "Skipped release checks"),
            ("staging", "false", "", 0, "External release acceptance DISABLED"),
            ("staging", "false", "g5-promote-prod", 1,
             "Staging cannot promote while external checks are disabled"),
        )
        for environment, skip, next_trigger, code, message in cases:
            with self.subTest(environment=environment, skip=skip, next_trigger=next_trigger), \
                    tempfile.TemporaryDirectory() as directory:
                workspace = Path(directory)
                (workspace / "deploy-control.env").write_text(f"SKIP_DEPLOY={skip}\n")
                script = self.script("required-release-checks", workspace, BUILD_ID="test-build",
                                     _ENVIRONMENT=environment, _NEXT_TRIGGER=next_trigger,
                                     _RELEASE_CHECKS_REQUIRED="false")
                # Exercise the real entrypoint; these paths must not install
                # packages, read credentials or invoke cloud APIs.
                for name in ("apt-get", "gcloud"):
                    command = workspace / name
                    command.write_text("#!/bin/sh\nexit 99\n")
                    command.chmod(0o755)
                result = subprocess.run(["/bin/bash", "-c", script], cwd=self.root,
                                        env={"PATH": f"{workspace}:{Path(sys.executable).parent}:{os.defpath}",
                                             "WORKSPACE_DIR": directory},
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                self.assertIn(message, result.stdout + result.stderr)

    def test_staging_canary_failure_stops_the_real_promotion_steps(self):
        # Run the actual Bash/Python entrypoints at the asserted Cloud Build
        # success boundary. Only package installation, canary execution and
        # cloud transport are doubled; no cloud resources are touched.
        self.assertIn("required-release-checks", self.dependencies("promote-release"))
        for name in ("required-release-checks", "smoke-test", "verify-deploy"):
            self.assertNotIn("allowFailure", self.step(name))
            self.assertNotIn("allowExitCodes", self.step(name))
        for failing_stage in ("chat", "preview", "secret", None):
            with self.subTest(failing_stage=failing_stage), tempfile.TemporaryDirectory() as directory:
                workspace = Path(directory)
                binary = workspace / "bin"
                binary.mkdir()
                sha = "a" * 40
                origin = "11111111-1111-1111-1111-111111111111"
                build_id = "22222222-2222-2222-2222-222222222222"
                bucket = "g5-qa-1ccc-lean-releases"
                manifest_uri = f"gs://{bucket}/app-releases/{origin}.json#123"
                registry = "us-central1-docker.pkg.dev/g5-qa-1ccc/g5"
                refs = {key: f"{registry}/{image}@sha256:" + "c" * 64
                        for key, image in app_release.IMAGES.items()}
                refs["WORKER_IMAGE_REF"] = "projects/g5-qa-1ccc/global/images/candidate-worker"
                (workspace / "image-refs.env").write_text("\n".join(f"{k}={v}" for k, v in refs.items()))
                (workspace / "worker-image.env").write_text(f"WORKER_IMAGE_REF={refs['WORKER_IMAGE_REF']}\n")
                (workspace / "deploy-control.env").write_text("SKIP_DEPLOY=false\n")
                (workspace / "manifest.json").write_text(json.dumps({
                    "version": 1, "commit": sha, "project": "g5-qa-1ccc", "qa_build_id": origin, "refs": refs}))
                fixture = {"org_id": origin, "fixture_workspace_id": build_id, "api_key": "g5k_synthetic"}
                (workspace / "secret.json").write_text(json.dumps({
                    "environment": "staging", "base_url": "https://lean.g5labs.ai", "chat": fixture,
                    "preview": {**fixture, "profile": "release-fixture"}}))
                programs = {
                    "apt-get": "#!/bin/sh\nexit 0\n",
                    "bash": '''#!/bin/sh
echo "$G5_CANARY_STAGES $G5_CANARY_API_URL $G5_CANARY_PREVIEW_HOST_SUFFIX" >> "$WORKSPACE_DIR/stages"
test "$FAIL_STAGE" != "$G5_CANARY_STAGES"
''',
                    "gcloud": f'''#!{sys.executable}
import json, os, pathlib, sys
root = pathlib.Path(os.environ["WORKSPACE_DIR"])
args = sys.argv[1:]
with (root / "cloud-calls").open("a") as log:
    log.write(json.dumps(args) + "\\n")
if args[:4] == ["secrets", "versions", "access", "latest"]:
    if os.environ["FAIL_STAGE"] == "secret":
        sys.exit(1)
    assert "--project=g5lean-staging-1ccc" in args
    print((root / "secret.json").read_text())
elif args[:2] == ["storage", "cat"]:
    print((root / "manifest.json").read_text())
elif args[:4] != ["builds", "triggers", "run", "g5-promote-prod"]:
    raise SystemExit("Unexpected cloud command")
''',
                }
                for name, body in programs.items():
                    path = binary / name
                    path.write_text(body)
                    path.chmod(0o755)
                overrides = dict(BUILD_ID=build_id, _ENVIRONMENT="staging", _TARGET_PROJECT="g5lean-staging-1ccc",
                                 _RELEASE_BUCKET=bucket, _RELEASE_MANIFEST=manifest_uri,
                                 _NEXT_TRIGGER="g5-promote-prod", _RELEASE_CHECKS_REQUIRED="true",
                                 _RELEASE_CHECK_BASE_URL="https://lean.g5labs.ai", _RELEASE_CHECK_SECRET="g5-release-checks",
                                 _PREVIEW_HOSTNAME="preview.lean.g5labs.ai")
                gate = self.script("required-release-checks", workspace, **overrides)
                promotion = self.script("promote-release", workspace, **overrides)
                # set -e models the hard dependency already checked above:
                # a nonzero check step must prevent downstream execution.
                command = f"set -e\n(\n{gate}\n)\n(\n{promotion}\n)\n"
                result = subprocess.run(["/bin/bash", "-c", command], cwd=self.root,
                                        env={"PATH": f"{binary}:{Path(sys.executable).parent}:{os.defpath}",
                                             "WORKSPACE_DIR": directory, "FAIL_STAGE": failing_stage or ""},
                                        capture_output=True, text=True, timeout=15)
                calls = [json.loads(line) for line in (workspace / "cloud-calls").read_text().splitlines()]
                promotions = [args for args in calls if args[:3] == ["builds", "triggers", "run"]]
                stages = (workspace / "stages").read_text().splitlines() if (workspace / "stages").exists() else []
                expected = [] if failing_stage == "secret" else ["chat"] if failing_stage == "chat" else ["chat", "preview"]
                self.assertEqual(stages, [f"{stage} https://lean.g5labs.ai preview.lean.g5labs.ai" for stage in expected])
                if failing_stage:
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual(promotions, [])
                else:
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual(len(promotions), 1)
                    self.assertEqual(promotions[0][3], "g5-promote-prod")
                    self.assertIn(f"--sha={sha}", promotions[0])
                    self.assertIn(f"_RELEASE_MANIFEST={manifest_uri}", promotions[0][-1])
                    self.assertIn(f"_SOURCE_BUILD_ID={build_id}", promotions[0][-1])

    def test_deploy_sequence_waits_on_image_pushes_not_companion_publication(self):
        # g5.quality.delivery.qa-deploy-order: the deploy decision starts once
        # every image is pushed; companion publication and the sccache warm
        # run beside it and never precede Helm.
        pushes = {"push-api", "push-g5d", "push-g5d-tungsten", "push-g5d-pairsoft-preview",
                  "push-g5d-pnc-bankworkflow", "push-g5d-pnc-cash-receipt", "push-g5d-g5dev",
                  "push-worker-base", "push-edge"}
        self.assertEqual(self.dependencies("resolve-image-refs"), pushes)
        deploy_ancestors = self.ancestors("smoke-test")
        self.assertLessEqual(pushes, deploy_ancestors)
        for off_path in ("publish-workload-templates", "warm-guest-sccache"):
            self.assertNotIn(off_path, deploy_ancestors)
        self.assertIn("push-g5d-g5dev", self.ancestors("publish-workload-templates"))
        self.assertIn("publish-workload-templates", self.dependencies("warm-guest-sccache"))

    def test_deploy_queue_gate_keeps_qa_and_release_boundaries(self):
        gate = self.step("wait-deploy-turn")
        self.assertIn("steps[].id,steps[].status", gate)
        self.assertIn("status,approval,substitutions", gate)
        self.assertIn("python3 ./scripts/deploy_queue_blockers.py", gate)
        terminal = "smoke-test"
        self.assertIn("verify-deploy", self.dependencies(terminal))
        downstream = [name for name in re.findall(r"^  - id: (\S+)$", self.pipeline, re.MULTILINE)
                      if terminal in self.dependencies(name)]
        self.assertEqual(downstream, ["confirm-workload-templates", "required-release-checks"])
        self.assertEqual(self.ancestors(terminal) & {"publish-workload-templates", "warm-guest-sccache"}, set())
        self.assertEqual(self.dependencies("promote-release"),
                         {"required-release-checks", "confirm-workload-templates", "warm-guest-sccache"})
        self.assertIn("allowFailure: true", self.step("warm-guest-sccache"))

    def test_companion_publication_cannot_interrupt_the_deploy_phase(self):
        # A failing step cancels the build's running steps, so the step beside
        # helm-deploy must never fail: a publication error stops the loop,
        # leaves the marker unwritten and exits 0; the step that waits for the
        # deploy phase to end turns the missing marker into a red build.
        self.assertEqual(self.dependencies("confirm-workload-templates"), {"publish-workload-templates", "smoke-test"})
        self.assertNotIn("allowFailure", self.step("confirm-workload-templates"))
        self.assertNotIn("allowFailure", self.step("publish-workload-templates"))
        for failing, marker in ((None, "published"), ("pull", None), ("inspect", None), ("publish", None)):
            with self.subTest(failing=failing), tempfile.TemporaryDirectory() as directory:
                workspace = Path(directory)
                (workspace / "delivery.env").write_text("AR=registry.example\nIMAGE_VERSION=candidate\n")
                prelude = f'''
docker() {{ if [[ "{failing}" == "$1" ]]; then echo "docker $1 failed" >&2; return 7; fi
            if [[ "$1" == inspect ]]; then echo "registry.example/img@sha256:{'a' * 64}"; fi; }}
bash() {{ echo "PUBLISH=$2"; [[ "{failing}" != publish ]]; }}
'''
                result = subprocess.run(["bash", "-c", prelude + self.script("publish-workload-templates", workspace)],
                                        env={"PATH": os.defpath, "PROJECT_ID": "g5-qa-1ccc"},
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                ok = workspace / "workload-templates.ok"
                self.assertEqual(ok.read_text().strip() if ok.exists() else None, marker)
                if failing:
                    self.assertIn("WARN: workload template publication failed for g5d-g5dev", result.stdout)
                    self.assertLessEqual(result.stdout.count("PUBLISH="), 1)
                else:
                    self.assertEqual(result.stdout.count("PUBLISH="), 5)
        for marker, accepted in ((None, False), ("published", True), ("reused", True)):
            with self.subTest(marker=marker), tempfile.TemporaryDirectory() as directory:
                if marker:
                    (Path(directory) / "workload-templates.ok").write_text(marker + "\n")
                result = self.invoke(self.script("confirm-workload-templates", directory))
                self.assertEqual(result.returncode == 0, accepted, result.stdout + result.stderr)
                if accepted:
                    self.assertIn(f"Workload templates: {marker}", result.stdout)
                else:
                    self.assertIn("publish-workload-templates did not complete", result.stdout)

    def test_deploy_queue_gate_runs_each_environment_to_its_completion_step(self):
        # Polls the real wait-deploy-turn Bash against a gcloud stub whose
        # `builds list` answers come from numbered fixture files, so the
        # second poll shows the older build past smoke-test while its
        # companion publication still runs.
        def listing(status, environment, project):
            terminal = "smoke-test" if environment == "qa" else "promote-release"
            older = {"id": "older", "createTime": "2026-10-20T22:24:00.123456789Z",
                     "substitutions": {"_PROMOTION_MODE": "build" if environment == "qa" else "deploy", "_ENVIRONMENT": environment,
                                       "_TARGET_PROJECT": project, "COMMIT_SHA": "b" * 40,
                                       "TRIGGER_NAME": "g5-deploy-qa"},
                     "steps": [{"id": "helm-deploy", "status": "SUCCESS"},
                               {"id": terminal, **({"status": status} if status else {})},
                               {"id": "publish-workload-templates", "status": "WORKING"}]}
            if environment != "qa":
                older["steps"].append({"id": "smoke-test", "status": "SUCCESS"})
            pending = {**older, "id": "unapproved", "status": "PENDING", "steps": []}
            return json.dumps([older, pending])

        for environment in ("qa", "staging", "prod"):
            project = "g5-qa-1ccc" if environment == "qa" else f"g5lean-{environment}-1ccc"
            for polls, statuses in ((1, ["SUCCESS"]), (2, ["WORKING", "FAILURE"]), (3, [None, "WORKING", "SUCCESS"])):
                with self.subTest(environment=environment, polls=polls), tempfile.TemporaryDirectory() as directory:
                    workspace = Path(directory)
                    (workspace / "image-refs.env").write_text("")
                    for index, status in enumerate(statuses, start=1):
                        (workspace / f"list-{index}.json").write_text(listing(status, environment, project))
                    prelude = f'''
    gcloud() {{
      case "$1 $2" in
        "builds describe") printf '2026-10-20T23:33:00Z\n' ;;
        "builds list")
          n=$(( $(cat "{directory}/polls" 2>/dev/null || echo 0) + 1 )); echo "$n" > "{directory}/polls"
          cat "{directory}/list-$n.json" ;;
        *) echo "unexpected gcloud $*" >&2; exit 99 ;;
      esac
    }}
    sleep() {{ echo "SLEEP=$1"; }}
    '''
                    script = self.script("wait-deploy-turn", workspace, _ENVIRONMENT=environment, _TARGET_PROJECT=project).replace(
                        "./scripts/deploy_queue_blockers.py", f"{GENERATION_ROOT}/scripts/deploy_queue_blockers.py")
                    result = subprocess.run(["bash", "-c", prelude + script],
                                            env={"PATH": os.defpath, "PROJECT_ID": "g5-qa-1ccc", "BUILD_ID": "me"},
                                            capture_output=True, text=True, timeout=20)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual((workspace / "polls").read_text().strip(), str(polls))
                    self.assertEqual(result.stdout.count("SLEEP=30"), polls - 1)
                    self.assertEqual(result.stdout.count(f"still own the {environment}/{project} queue"), polls - 1)
                    self.assertIn("Deploy queue clear after", result.stdout)
                    self.assertIn("DEPLOY_QUEUE_BUILD_CREATE_TIME=2026-10-20T23:33:00Z",
                                  (workspace / "deploy-queue.env").read_text())

    def test_actual_target_freshness_step_receives_each_artifact_layout(self):
        # Execute the rendered Cloud Build shell and its real Python delegate.
        # Only the gcloud/kubectl boundaries are replaced; previous step exports
        # are deliberately absent from this fresh process environment.
        for environment in ("staging", "prod"):
            for owner in ("g5-qa-1ccc", "g5lean-admin-1ccc"):
                with self.subTest(environment=environment, owner=owner), tempfile.TemporaryDirectory() as directory:
                    workspace = Path(directory)
                    binary = workspace / "bin"
                    binary.mkdir()
                    (binary / "python3").symlink_to(sys.executable)
                    gcloud = binary / "gcloud"
                    gcloud.write_text('#!/bin/sh\n[ "$1 $2 $3" = "container clusters get-credentials" ] || exit 99\nprintf "%s\\n" "$*" > "$CALL_LOG"\n')
                    gcloud.chmod(0o755)
                    kubectl = binary / "kubectl"
                    kubectl.write_text('#!/bin/sh\n[ "$1 $2 $3" = "get deployment g5-api" ] || exit 99\n')
                    kubectl.chmod(0o755)
                    (workspace / "delivery.env").write_text(
                        f"export AR=us-central1-docker.pkg.dev/{owner}/g5\n"
                        f"export IMAGE_VERSION={'a' * 40}\nexport TARGET_PROJECT=g5lean-{environment}-1ccc\n")
                    (workspace / "image-refs.env").write_text("")
                    (workspace / "deploy-queue.env").write_text("export DEPLOY_QUEUE_BUILD_CREATE_TIME=2026-10-21T00:00:00Z\n")
                    script = self.script("check-main-freshness", workspace, BUILD_ID="a" * 36,
                        _ENVIRONMENT=environment, _PROMOTION_MODE="deploy",
                        _TARGET_PROJECT=f"g5lean-{environment}-1ccc", _GKE_CLUSTER=f"g5-lean-{environment}",
                        _GKE_ZONE="us-central1", _ARTIFACT_PROJECT=owner, _AR_PROJECT=owner,
                        _WORKER_IMAGE_PROJECT=owner, _RELEASE_BUCKET=f"{owner}-lean-releases")
                    result = subprocess.run(["bash", "-c", script], cwd=self.root,
                        env={"PATH": f"{binary}:{os.defpath}", "PROJECT_ID": "g5-qa-1ccc",
                             "WORKSPACE_DIR": directory, "CALL_LOG": str(workspace / "cloud-call")},
                        capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn("export SKIP_DEPLOY=false", (workspace / "deploy-control.env").read_text())
                    self.assertIn(f"--project=g5lean-{environment}-1ccc", (workspace / "cloud-call").read_text())

    def test_build_and_explicit_deploy_input_guards_remain(self):
        pinned = {name: "registry.example/image@sha256:" + "a" * 64
                  for name in ("_API_IMAGE_REF", "_G5D_IMAGE_REF", "_EDGE_IMAGE_REF")}
        pinned["_WORKER_IMAGE_REF"] = "projects/g5-qa-1ccc/global/images/g5-worker-candidate"
        cases = [({}, True), ({"_PROMOTION_MODE": "deploy", **pinned}, True),
                 ({"_PROMOTION_MODE": "deploy"}, False),
                 ({"_PROMOTION_MODE": "other"}, False), ({"_ENVIRONMENT": "prod"}, False)]
        stage = {"_ENVIRONMENT": "staging", "_PROMOTION_MODE": "deploy", **pinned,
                 "_TARGET_PROJECT": "g5lean-staging-1ccc", "_GKE_CLUSTER": "g5-lean-staging",
                 "_RELEASE_BUCKET": "g5-qa-1ccc-lean-releases", "_RELEASE_MANIFEST": "gs://g5-qa-1ccc-lean-releases/manifest#1",
                 "_SOURCE_BUILD_ID": "origin"}
        cases.extend([(stage, True), ({**stage, "_PROMOTION_MODE": "build"}, False),
                      ({**stage, "_QA_DATABASE_RESET": "true", "_QA_RESET_RUN_ID": "reset"}, False),
                      ({**stage, "_TARGET_PROJECT": "g5lean-prod-1ccc"}, False),
                      ({**stage, "_GKE_CLUSTER": "g5-lean-prod"}, False),
                      ({**stage, "_SOURCE_BUILD_ID": ""}, False),
                      ({**stage, "_NEXT_TRIGGER": "g5-promote-prod"}, False),
                      ({**stage, "_NEXT_TRIGGER": "g5-promote-prod", "_RELEASE_CHECKS_REQUIRED": "true",
                        "_RELEASE_CHECK_BASE_URL": "https://staging.example", "_RELEASE_CHECK_SECRET": "check"}, True),
                      ({**stage, "_NEXT_TRIGGER": "foreign"}, False)])
        for mode in ("build", "deploy"):
            for name in ("_AR_PROJECT", "_TARGET_PROJECT", "_WORKER_IMAGE_PROJECT"):
                cases.append(({"_PROMOTION_MODE": mode, **pinned, name: "foreign"}, False))
        for overrides, accepted in cases:
            with self.subTest(overrides=overrides), tempfile.TemporaryDirectory() as directory:
                result = self.invoke(self.script("validate-inputs", directory, **overrides))
                self.assertEqual(result.returncode == 0, accepted, result.stdout + result.stderr)
                self.assertEqual((Path(directory) / "delivery.env").exists(), accepted)

    def test_helm_runs_without_cutover_handoff_and_preserves_modes(self):
        image = "registry.example/image@sha256:" + "a" * 64
        worker = "projects/g5-qa-1ccc/global/images/g5-worker-candidate"
        for reset, stale, handoff, promoted in (
            (False, False, True, True), (False, False, True, False),
            (True, False, True, True), (False, True, False, False),
            (True, True, False, False), (False, False, False, False),
        ):
            with self.subTest(reset=reset, stale=stale, handoff=handoff, promoted=promoted), tempfile.TemporaryDirectory() as directory:
                workspace = Path(directory)
                (workspace / "deploy-control.env").write_text(
                    f"SKIP_DEPLOY={str(stale).lower()}\nARTIFACT_COMMIT=old\nDEPLOY_FRESHNESS_HEAD=new\n")
                if not stale:
                    (workspace / "delivery.env").write_text("TARGET_PROJECT=g5-qa-1ccc\nAR=registry.example\nIMAGE_VERSION=candidate\n")
                    refs = "\n".join(f"{key}={image}" for key in ("API_IMAGE_REF", "G5D_IMAGE_REF", "EDGE_IMAGE_REF"))
                    if promoted:
                        refs += f"\nG5D_TUNGSTEN_IMAGE_REF={image}\nG5D_G5DEV_IMAGE_REF={image}"
                    (workspace / "image-refs.env").write_text(refs + "\n")
                    if handoff:
                        (workspace / "worker-image.env").write_text(f"WORKER_IMAGE_REF={worker}\n")
                result = self.invoke(self.script("helm-deploy", workspace, _QA_DATABASE_RESET=str(reset).lower()))
                self.assertEqual(result.returncode == 0, stale or handoff, result.stdout + result.stderr)
                args = [line.removeprefix("HELM_ARG=") for line in result.stdout.splitlines() if line.startswith("HELM_ARG=")]
                if stale or not handoff:
                    self.assertEqual(args, [])
                else:
                    self.assertIn("upgrade", args)
                    self.assertIn("--wait", args)
                    self.assertNotIn("--atomic", args)
                    self.assertIn(f"api.env.G5_WORKER_IMAGE={worker}", args)
                    self.assertIn(f"api.image.reference={image}", args)
                    self.assertIn(f"daemon.image.reference={image}", args)
                    self.assertIn(f"migration.image.reference={image}", args)
                    self.assertIn("./infra/helm/g5/values-control-plane.yaml", args)
                    self.assertEqual(any("G5_PROMOTED_BASE_IMAGES=" in arg for arg in args), promoted)
                    self.assertFalse(any("PR1884" in arg for arg in args))

    def test_filestore_guard_blocks_helm_before_hooks_or_writer_changes(self):
        from test_check_filestore_binding import binding

        image = "registry.example/image@sha256:" + "a" * 64
        worker = "projects/g5-qa-1ccc/global/images/g5-worker-candidate"
        for environment in ("qa", "staging", "prod"):
            for case in ("matched", "old-binding", "partial", "forbidden"):
                with self.subTest(environment=environment, case=case), tempfile.TemporaryDirectory() as directory:
                    workspace = Path(directory)
                    (workspace / "deploy-control.env").write_text("SKIP_DEPLOY=false\n")
                    (workspace / "delivery.env").write_text("TARGET_PROJECT=synthetic\nAR=registry.example\nIMAGE_VERSION=candidate\n")
                    (workspace / "worker-image.env").write_text(f"WORKER_IMAGE_REF={worker}\n")
                    (workspace / "image-refs.env").write_text("\n".join(
                        f"{key}={image}" for key in ("API_IMAGE_REF", "G5D_IMAGE_REF", "EDGE_IMAGE_REF")))
                    documents = binding()
                    if case == "old-binding":
                        documents["persistentvolumes"]["items"][0]["spec"]["nfs"]["server"] = "10.0.0.2"
                    if case == "partial":
                        documents["deployments"]["items"].pop()
                    result = self.invoke(self.script("helm-deploy", workspace, _ENVIRONMENT=environment),
                        FILESTORE_TEST_INVENTORY=json.dumps(documents),
                        FILESTORE_TEST_DENIED="1" if case == "forbidden" else "")
                    self.assertEqual(result.returncode == 0, case == "matched", result.stdout + result.stderr)
                    self.assertEqual("HELM_ARG=upgrade\n" in result.stdout, case == "matched")
                    if case == "matched":
                        self.assertLess(result.stdout.index("Filestore binding verified"),
                                        result.stdout.index("HELM_ARG=upgrade\n"))
                    else:
                        self.assertIn("Filestore deploy refused", result.stderr)


class WorkerImageStepPrerequisites(unittest.TestCase):
    """Protect worker-image steps that share and mutate /workspace."""

    def test_qa_deploy_owns_the_api_worker_pin(self):
        """A worker bake must not cause a second API rollout (#403 / D35)."""
        def read(relative):
            with open(os.path.join(GENERATION_ROOT, relative), encoding="utf-8") as handle:
                return handle.read()

        bake = read("infra/ci/cloudbuild-packer-worker.yaml")
        self.assertNotRegex(bake, r"kubectl\s+(?:set|patch|rollout|apply)\b")
        self.assertNotIn("container clusters get-credentials", bake)
        self.assertNotIn("_REPIN_DEV_API_WORKER_IMAGE", bake)
        self.assertIn("- id: gce-image-import", bake)
        self.assertIn("- id: publish-summary", bake)

        delivery = read("infra/qa/delivery/main.tf")
        self.assertIn('_WORKER_IMAGE_TRIGGER   = "g5-packer-worker-main-qa"', delivery)
        self.assertNotIn("_REPIN_DEV_API_WORKER_IMAGE", delivery)
        deploy = read("infra/ci/cloudbuild-deploy.yaml")
        self.assertIn("python3 ./scripts/await_worker_image.py", deploy)
        helm = deploy.split("  - id: helm-deploy\n", 1)[1].split("\n  - id:", 1)[0]
        self.assertIn("'resolve-worker-image'", helm)
        self.assertIn("api.env.G5_WORKER_IMAGE", helm)

    def test_iso_staging_starts_after_the_source_archive(self):
        """Do not mutate /workspace while compute-image-version runs tar."""
        path = os.path.join(
            GENERATION_ROOT, "infra", "ci", "cloudbuild-packer-worker.yaml"
        )
        with open(path, encoding="utf-8") as handle:
            text = handle.read()

        step_start = text.index("  - id: stage-iso\n")
        step_end = text.index("\n  - id:", step_start)
        step = text[step_start:step_end]
        self.assertIn("waitFor: ['compute-image-version']", step)

    def test_source_archive_is_written_outside_its_input_tree(self):
        """Do not create tar's output inside the /workspace input tree."""
        path = os.path.join(
            GENERATION_ROOT, "infra", "ci", "cloudbuild-packer-worker.yaml"
        )
        with open(path, encoding="utf-8") as handle:
            text = handle.read()

        step_start = text.index("  - id: compute-image-version\n")
        step_end = text.index("\n  - id:", step_start)
        step = text[step_start:step_end]
        self.assertIn("tar czf /tmp/g5d-src.tar.gz -C /workspace", step)
        self.assertIn(
            "mv /tmp/g5d-src.tar.gz /workspace/g5d-src.tar.gz", step
        )
        self.assertNotIn("tar czf /workspace/g5d-src.tar.gz", step)

    def test_qa_image_builder_can_publish_top_level_build_artifacts(self):
        """Grant the image builder both object writes and bucket preflight reads."""
        path = os.path.join(
            GENERATION_ROOT, "infra", "qa", "foundation", "main.tf"
        )
        with open(path, encoding="utf-8") as handle:
            text = handle.read()

        for resource_name, role in [
            ("images_build_artifacts_writer", "roles/storage.objectCreator"),
            ("images_build_artifacts_reader", "roles/storage.objectViewer"),
            ("images_build_artifacts_viewer", "roles/storage.bucketViewer"),
        ]:
            with self.subTest(resource=resource_name):
                start = text.index(
                    'resource "google_storage_bucket_iam_member" '
                    f'"{resource_name}" {{'
                )
                try:
                    end = text.index(
                        '\nresource "google_', start + 1
                    )
                except ValueError:
                    end = len(text)
                resource = text[start:end]
                self.assertIn(
                    'bucket = google_storage_bucket.artifacts["build-artifacts"].name',
                    resource,
                )
                self.assertIn(f'role   = "{role}"', resource)
                self.assertIn(
                    'member = "serviceAccount:${google_service_account.build["images"].email}"',
                    resource,
                )


if __name__ == "__main__":
    result = unittest.main(exit=False).result
    sys.exit(0 if result.wasSuccessful() else 1)
