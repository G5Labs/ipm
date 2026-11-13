#!/usr/bin/env python3
"""Run repository script regression tests; propagate every failure to Docker CI."""
from pathlib import Path
import subprocess
import sys
import platform
import os

ROOT = Path(__file__).resolve().parents[1]
OTHER_LANES = {
    "scripts/test-parallel.sh": "Rust test orchestrator; suites run in Docker CI phases 1 and 5",
    "scripts/test-cloud-hypervisor-ci.sh": "strict Linux/KVM boot/network and image runtime gate in Rust/Packer CI",
    "scripts/test-cloud-hypervisor-guest.sh": "full workload Linux/KVM acceptance; dedicated QA qualification",
    "scripts/test-cloud-hypervisor-network.sh": "real two-VM network gate invoked by test-cloud-hypervisor-ci.sh",
    "scripts/test_cloud_hypervisor_network.py": "real two-VM network gate invoked by test-cloud-hypervisor-ci.sh",
    "scripts/test-cloud-hypervisor-sandbox.sh": "real passt AppArmor/seccomp gate invoked by test-cloud-hypervisor-ci.sh",
    "scripts/test_cloud_hypervisor_seed.py": "trusted seed/bootstrap Linux/KVM acceptance; dedicated QA qualification",
    "scripts/test-guest-init-mount-roots.py": "mount-topology replay and negative control invoked inside the test-cloud-hypervisor-ci.sh guest",
    "scripts/test-guest-nested-kvm.py": "nested-virtualization /dev/kvm probes, inner-host prerequisites and the third-level vm-exec boot invoked inside the test-cloud-hypervisor-ci.sh and acceptance guests",
    "scripts/test-guest-docker-exec.sh": "real Docker exec/Compose health checks invoked inside the CI and full-environment acceptance guests",
}

ENVIRONMENT_LANES = {
    "scripts/test-self-hosted-smoke.sh": ("G5_SELFHOST_SMOKE_API_IMAGE", "G5_SELFHOST_SMOKE_EDGE_IMAGE"),
}


def main():
    files = sorted({
        path for directory in (ROOT / "scripts", ROOT / "infra/docker")
        for path in directory.rglob("test*")
        if path.is_file() and path.suffix in (".py", ".sh")
        and path.name.startswith(("test_", "test-"))
    })
    failed = []
    executed = 0
    for path in files:
        relative = path.relative_to(ROOT).as_posix()
        if relative in OTHER_LANES:
            print(f"OTHER LANE: {relative}: {OTHER_LANES[relative]}", flush=True)
            continue
        if relative in ENVIRONMENT_LANES:
            required = ENVIRONMENT_LANES[relative]
            configured = [bool(os.environ.get(key)) for key in required]
            if not any(configured):
                print(f"CONDITIONAL: {relative}: set {' and '.join(required)} to run", flush=True)
                continue
            if not all(configured):
                print(f"Missing environment for {relative}: requires {' and '.join(required)}", file=sys.stderr)
                failed.append(relative)
                continue
        executed += 1
        print(f"=== Script regression: {relative} ===", flush=True)
        command = [sys.executable if path.suffix == ".py" else "bash", str(path)]
        if relative in {"infra/docker/test-g5-dev-dockerd-start.sh", "scripts/test-apt-install-with-retry.sh", "scripts/test-worker-image-guard.sh"} and platform.system() != "Linux":
            # This exercises Linux flock/process behavior. Use the same Linux
            # base as the CI builder instead of interpreting macOS utilities.
            command = ["docker", "run", "--rm", "-v", f"{ROOT}:{ROOT}:ro", "-w", str(ROOT),
                       "g5/base-rust-warm:26.04", "bash", str(path)]
        if subprocess.run(command, cwd=ROOT).returncode:
            failed.append(relative)
    if failed:
        print("FAILED script suites:\n" + "\n".join(failed), file=sys.stderr)
        return 1
    print(f"All {executed} script regression suites passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
