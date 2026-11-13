"""Exercise the actual image entrypoint with bootstrap values from an old API."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class WorkerRuntimeIsolation(unittest.TestCase):
    def test_image_owns_host_library_path_during_rolling_upgrade(self):
        with tempfile.TemporaryDirectory() as tmp:
            executable = Path(tmp) / "g5d"
            executable.write_text('''#!/bin/sh
set -eu
test "$1" = host
test "$LD_LIBRARY_PATH" = /opt/g5-host/libs
test "$G5_RUNTIME_LIB_DIR" = /opt/g5-host/libs
test "${PATH%%:*}" = /opt/g5-host/bin
test "$G5D_HOST_RUNTIME_BIN_DIR" = /var/g5d-data/runtime/bin
''')
            executable.chmod(0o755)
            subprocess.run(
                ["bash", str(ROOT / "infra/packer/g5-worker/files/g5d-host")],
                env=dict(os.environ, G5D_BIN=str(executable),
                         G5D_HOST_RUNTIME_BIN_DIR="/var/g5d-data/runtime/bin",
                         G5_RUNTIME_LIB_DIR="/var/g5d-data/runtime/libs",
                         LD_LIBRARY_PATH="/var/g5d-data/runtime/libs:/var/g5d-data/runtime/bin"),
                check=True,
            )


if __name__ == "__main__":
    unittest.main()
