#!/usr/bin/env bash
# Retired rollout gates must not re-enter deployment configuration.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - <<'PYCODE'
from pathlib import Path
import json
import re
import subprocess
import sys
import tempfile
keys = ['G5_FORK_CHAT_ENABLED', 'G5_CODEX_UI_ENABLED', 'G5_MAKE_REMOTE_ENABLED', 'G5_TASK_CRDT_PERSISTENCE_ENABLED', 'G5_TASK_POLL_ZERO_SLOT_V1_ENABLED', 'G5_EXECUTION_LIFECYCLE_STRICT_IDENTITY', 'G5_STICKY_RUNTIME_DISCARD_FENCE_V2_ENABLED', 'G5_STICKY_RUNTIME_DISCARD_FENCE_V2_DRAIN_ENABLED']
paths = list(Path("infra/helm/g5").glob("values*.yaml")) + [
    Path("infra/ci/cloudbuild-ci-e2e.yaml"), Path("infra/self-hosted/compose.yml"),
    Path("infra/self-hosted/env.template"), Path(".env.example")]
for path in paths:
    text = path.read_text()
    for key in keys:
        assert key not in text, f"{path}: retired deployment gate {key}"
for path in Path("frontend/e2e").rglob("*.ts"):
    assert "process.env.G5_E2E_CODEX_UI_ENABLED" not in path.read_text(), f"{path}: Codex tests must not be gated"
# Exercise the smoke script's actual instance-info validator without installing
# a stack. Standard Codex publication must pass; its absence must fail.
smoke = Path("scripts/test-self-hosted-smoke.sh").read_text()
validator = re.search(r"python3 - \"\$TMP/instance-info.json\" <<'PY'\n(.*?)\nPY", smoke, re.S)
assert validator, "self-hosted smoke instance-info validator not found"
with tempfile.TemporaryDirectory() as tmp:
    response = Path(tmp) / "instance-info.json"
    for features, succeeds in [(["chat", "codex_ui"], True), (["chat"], False)]:
        response.write_text(json.dumps({"data": {
            "authProviders": ["github"], "devLoginEnabled": False,
            "selfHosted": True, "features": features,
        }}))
        result = subprocess.run(
            [sys.executable, "-c", validator.group(1), str(response)],
            capture_output=True, text=True,
        )
        assert (result.returncode == 0) == succeeds, (
            f"self-hosted Codex capability validation disagrees for {features}: {result.stderr}"
        )
print("deployment gate retirement contract passed; Codex browser and self-hosted coverage enabled")
PYCODE
