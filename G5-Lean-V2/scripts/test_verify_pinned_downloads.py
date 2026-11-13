#!/usr/bin/env python3
"""Exercise scripts/verify-pinned-downloads.sh, the static guard for operations
g5.quality.delivery.pinned-third-party-tools (G5Labs/G5-Lean#527), against
temporary recipes and scripts, the historical findings, the real tree, the CI
wiring and the cross-file agreement of the recorded pins."""
from pathlib import Path
import re
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]
SCRIPT = REPO / "scripts/verify-pinned-downloads.sh"
DOCKERFILE = REPO / "infra/docker/Dockerfile.g5d-base"
BOOTSTRAP = REPO / "scripts/self-hosted-azure/bootstrap-ubuntu-host.sh"
SHA256 = re.compile(r"^[0-9a-f]{64}$")

# (file name, content, accepted)
CASES = [
    ("Dockerfile.pipe-sh", "FROM g5/base:26.04\nRUN curl -LsSf https://example.invalid/install.sh | sh\n", False),
    ("Dockerfile.pipe-sh-continued",
     'FROM g5/base:26.04\nRUN set -eux; \\\n    curl -LsSf "https://astral.sh/uv/${UV_VERSION}/install.sh" | sh; \\\n    uv --version\n', False),
    ("Dockerfile.pipe-tar", 'FROM g5/base:26.04\nRUN curl -LsSf "$URL" | tar zxf - -C /usr/local/cargo/bin && echo ok\n', False),
    ("Dockerfile.pipe-chain", "FROM g5/base:26.04\nRUN curl -fsSL https://example.invalid/t.tgz | gunzip | tar x -C /opt\n", False),
    ("Dockerfile.pipe-bash-flags", "FROM g5/base:26.04\nRUN curl -fsSL https://example.invalid/i.sh | bash -s -- --to /usr/local/bin\n", False),
    ("Dockerfile.pipe-abs", "FROM g5/base:26.04\nRUN curl -fsSL https://example.invalid/i.sh | /bin/bash\n", False),
    ("Dockerfile.pipe-env", "FROM g5/base:26.04\nRUN curl -fsSL https://example.invalid/i.sh | env FOO=1 sh\n", False),
    ("Dockerfile.pipe-mount",
     "FROM g5/base:26.04\nRUN --mount=type=cache,target=/root/.cache curl -fsSL https://example.invalid/i.sh | sh\n", False),
    ("Dockerfile.pipe-wget", "FROM g5/base:26.04\nRUN wget -qO- https://example.invalid/i.sh | sh\n", False),
    ("install.sh", "#!/usr/bin/env bash\nwget -qO- https://example.invalid/i.sh | sudo -E bash -\n", False),
    ("install-python.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.py | python3 -\n", False),
    ("install-python-bare.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.py | python3\n", False),
    ("hidden-continuation.sh",
     "#!/usr/bin/env bash\ncurl --proto '=https' --tlsv1.2 -LsSf https://example.invalid/install.sh \\\n  | bash -s -- --to /usr/local/bin\n", False),
    ("semicolon-sink.sh", "#!/usr/bin/env bash\nset -eux; curl -fsSL https://example.invalid/i.sh | sh; echo done\n", False),
    ("bsdtar.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/t.tgz | bsdtar -xf - -C /opt\n", False),
    ("bootstrap", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | sh\n", False),
    ("dash-script", "#!/bin/dash\ncurl -fsSL https://example.invalid/i.sh | sh\n", False),
    ("Dockerfile.verified",
     'FROM g5/base:26.04\nRUN curl -fsSLo /tmp/t.tgz https://example.invalid/t.tgz && \\\n'
     '    echo "abc  /tmp/t.tgz" | sha256sum -c - && \\\n    tar xzf /tmp/t.tgz -C /opt\n', True),
    ("Dockerfile.comment",
     "FROM g5/base:26.04\n# never `curl | sh` of the mutable script\nRUN curl -fsSLo /tmp/t https://example.invalid/t\n", True),
    ("Dockerfile.comment-in-continuation",
     'FROM g5/base:26.04\nRUN curl -fsSLo /tmp/t https://example.invalid/t && \\\n'
     '    # the old `curl | sh` lived here \\\n    tar xzf /tmp/t\n', True),
    ("Dockerfile.or-list", "FROM g5/base:26.04\nRUN curl -fsSLo /tmp/t https://example.invalid/t || bash /opt/fallback.sh\n", True),
    ("parse.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/api | jq -r .tag_name\n", True),
    # An interpreter is a sink whatever its arguments: its program may execute
    # what it reads. Parse a download after saving it to a file instead.
    ("interpreter-c.sh",
     "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/api | python3 -c 'import sys, json; json.load(sys.stdin)'\n", False),
    ("node-e.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.js | node -e 'eval(require(\"fs\").readFileSync(0, \"utf8\"))'\n", False),
    # Tab-indented lines and quoted command bodies (sh -c, ssh) are inspected.
    ("Dockerfile.tab", "FROM g5/base:26.04\nRUN\tcurl -fsSL https://example.invalid/i.sh | sh\n", False),
    ("tab-indented.sh", "#!/usr/bin/env bash\n\tcurl -fsSL https://example.invalid/i.sh | sh\n", False),
    ("sh-c-single.sh", "#!/usr/bin/env bash\nsh -c 'curl -fsSL https://example.invalid/i.sh | sh'\n", False),
    ("bash-c-double.sh", '#!/usr/bin/env bash\nbash -c "curl -fsSL https://example.invalid/i.sh | bash"\n', False),
    ("Dockerfile.sh-c", "FROM g5/base:26.04\nRUN sh -c 'curl -fsSL https://example.invalid/i.sh | sh'\n", False),
    ("ssh-body.sh", "#!/usr/bin/env bash\nssh host 'curl -fsSL https://example.invalid/i.sh | sudo sh'\n", False),
    # A download inside a quoted body still belongs to its enclosing pipeline.
    ("quoted-download-piped.sh", "#!/usr/bin/env bash\nsh -c 'curl -fsSL https://example.invalid/i.sh' | bash\n", False),
    ("quoted-download-piped-double.sh", '#!/usr/bin/env bash\nbash -c "curl -fsSL https://example.invalid/i.sh" | sh\n', False),
    # Launchers that relay stdin, including options with separate values and operands.
    ("sudo-user.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | sudo -u root bash\n", False),
    ("sudo-long-user.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | sudo --user=root -H bash -s\n", False),
    ("doas-user.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | doas -u root sh\n", False),
    ("env-unset.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | env -u FOO -i sh\n", False),
    ("nice-adjust.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | nice -n 10 sh\n", False),
    ("timeout-duration.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | timeout 60 bash\n", False),
    ("busybox-applet.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | busybox sh\n", False),
    ("sudo-tee.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/key | sudo -u root tee /etc/apt/keyrings/k.asc >/dev/null\n", True),
    # GNU env -S / --split-string: the quoted value is the command line.
    ("env-split-string.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | env -S 'bash -s'\n", False),
    ("Dockerfile.env-split-string", "FROM g5/base:26.04\nRUN curl -fsSL https://example.invalid/i.sh | env -S 'bash -s'\n", False),
    ("env-split-string-double.sh", '#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | env -S "bash -s"\n', False),
    ("env-split-string-attached.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | env -S'bash -s'\n", False),
    ("env-split-string-long.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | env --split-string='bash -s'\n", False),
    ("su-c-body.sh", "#!/usr/bin/env bash\nsu root -c 'curl -fsSL https://example.invalid/i.sh | sh'\n", False),
    # Command and process substitutions execute their bodies whatever surrounds them.
    ("substitution-double-quoted.sh", '#!/usr/bin/env bash\necho "$(curl -fsSL https://example.invalid/i.sh | sh)"\n', False),
    ("Dockerfile.substitution", 'FROM g5/base:26.04\nRUN echo "$(curl -fsSL https://example.invalid/i.sh | sh)"\n', False),
    ("backtick-substitution.sh", "#!/usr/bin/env bash\necho `curl -fsSL https://example.invalid/i.sh | sh`\n", False),
    ("eval-substitution.sh", '#!/usr/bin/env bash\neval "$(curl -fsSL https://example.invalid/i.sh)"\n', False),
    ("process-substitution.sh", "#!/usr/bin/env bash\nbash <(curl -fsSL https://example.invalid/i.sh)\n", False),
    ("source-process-substitution.sh", "#!/usr/bin/env bash\nsource <(wget -qO- https://example.invalid/i.sh)\n", False),
    ("tar-process-substitution.sh", "#!/usr/bin/env bash\ntar xzf <(curl -fsSL https://example.invalid/t.tgz) -C /opt\n", False),
    # env -S keeps a quoted assignment together and still runs the shell.
    ("env-split-string-assignment.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/i.sh | env -S 'FOO=\"hello world\" bash -s'\n", False),
    ("Dockerfile.env-split-string-assignment", "FROM g5/base:26.04\nRUN curl -fsSL https://example.invalid/i.sh | env -S 'FOO=\"hello world\" bash -s'\n", False),
    # A download that is only data, or verified before extraction, stays allowed.
    ("echo-substitution-data.sh", '#!/usr/bin/env bash\necho "$(curl -fsSL https://example.invalid/api)"\n', True),
    ("bash-c-verified.sh", '#!/usr/bin/env bash\nbash -c "curl -fsSLo /tmp/f https://example.invalid/t && echo abc  /tmp/f | sha256sum -c - && tar xf /tmp/f"\n', True),
    # Quoted data that merely mentions the pattern is not a pipeline.
    ("quoted-message.sh", "#!/usr/bin/env bash\necho 'never curl | sh here'\ncurl -fsSLo /tmp/t https://example.invalid/t\n", True),
    ("quoted-substitution.sh", '#!/usr/bin/env bash\nTAG="$(curl -fsSL https://example.invalid/api | jq -r .tag)"\n', True),
    ("quoted-pattern.sh", "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/api | grep -E 'sh|bash' | sort\n", True),
    ("tee.sh",
     "#!/usr/bin/env bash\ncurl -fsSL https://example.invalid/key | gpg --dearmor | sudo tee /etc/apt/keyrings/k.gpg >/dev/null\n", True),
    ("retry-helper.sh",
     '#!/usr/bin/env bash\nHTTP="$(curl -fsSL -w \'%{http_code}\' "$1" -o "$2")" || RC=$?\n', True),
    ("notes.md", "curl https://example.invalid/i.sh | sh\n", True),
    ("config.yaml", "run: curl https://example.invalid/i.sh | sh\n", True),
]

# The exact texts the scan must reject at the finding's revision a95ae38d0.
HISTORICAL = {
    "Dockerfile.g5d-base": (
        "FROM g5/base:26.04\n"
        'RUN ARCH=$(case "${TARGETARCH:-$(uname -m)}" in arm64|aarch64) echo "aarch64" ;; *) echo "x86_64" ;; esac) && \\\n'
        "    # cargo-nextest\n"
        '    NEXTEST_VER="0.9.138" && \\\n'
        '    NEXTEST_URL=$(case "${TARGETARCH:-$(uname -m)}" in arm64|aarch64) echo "https://get.nexte.st/${NEXTEST_VER}/linux-arm" ;; *) echo "https://get.nexte.st/${NEXTEST_VER}/linux" ;; esac) && \\\n'
        '    curl -LsSf "$NEXTEST_URL" | tar zxf - -C /usr/local/cargo/bin && \\\n'
        "    # cargo-deny\n"
        '    DENY_VER="0.19.0" && \\\n'
        '    curl -LsSf "https://github.com/EmbarkStudios/cargo-deny/releases/download/${DENY_VER}/cargo-deny-${DENY_VER}-${ARCH}-unknown-linux-musl.tar.gz" | \\\n'
        "    tar zxf - --strip-components=1 -C /usr/local/cargo/bin --wildcards '*/cargo-deny'\n"
        "ARG UV_VERSION=0.12.6\n"
        "RUN set -eux; \\\n"
        '    curl -LsSf "https://astral.sh/uv/${UV_VERSION}/install.sh" | sh; \\\n'
        '    uv --version | grep -F "${UV_VERSION}"\n'
    ),
    "bootstrap-ubuntu-host.sh": (
        "#!/usr/bin/env bash\nset -euo pipefail\n"
        "if ! command -v just >/dev/null 2>&1; then\n"
        "  curl --proto '=https' --tlsv1.2 -LsSf https://just.systems/install.sh \\\n"
        "    | bash -s -- --to /usr/local/bin\n"
        "fi\n"
    ),
}


class PinnedDownloadScanTests(unittest.TestCase):
    def scan(self, *paths):
        return subprocess.run(
            ["bash", str(SCRIPT), *[str(path) for path in paths]],
            text=True, capture_output=True, cwd=REPO,
        )

    def test_missing_path_and_empty_scan_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "notes.md").write_text("curl https://example.invalid/i.sh | sh\n")
            for path in [root / "missing", root]:
                with self.subTest(path=path):
                    result = self.scan(path)
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertNotIn("OK:", result.stdout)

    def test_piped_downloads_are_rejected_and_verified_downloads_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, content, accepted in CASES:
                with self.subTest(name=name):
                    for stale in root.iterdir():
                        stale.unlink()
                    # An accepted recipe beside each case keeps the scan from
                    # failing closed on an empty set (non-recipe files are skipped).
                    (root / "Dockerfile.anchor").write_text(
                        "FROM g5/base:26.04\nRUN curl -fsSLo /tmp/t https://example.invalid/t\n"
                    )
                    (root / name).write_text(content)
                    result = self.scan(root)
                    if accepted:
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertIn("OK:", result.stdout)
                    else:
                        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                        self.assertIn(f"FAIL: {root / name}:", result.stderr)

    def test_findings_name_the_instruction_line(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "hidden-continuation.sh").write_text(
                "#!/usr/bin/env bash\n# a comment\ncurl -fsSL https://example.invalid/i.sh \\\n  | bash -s --\n"
            )
            result = self.scan(root)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn(f"FAIL: {root / 'hidden-continuation.sh'}:3:", result.stderr)

    def test_single_file_paths_are_scanned_as_shell(self):
        with tempfile.TemporaryDirectory() as temporary:
            recipe = Path(temporary) / "Justfile"
            recipe.write_text("fetch:\n    curl -fsSL https://example.invalid/i.sh | sh\n")
            self.assertEqual(self.scan(recipe).returncode, 1)
            recipe.write_text("fetch:\n    curl -fsSLo /tmp/i.sh https://example.invalid/i.sh\n")
            self.assertEqual(self.scan(recipe).returncode, 0)

    def test_historical_findings_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, content in HISTORICAL.items():
                (root / name).write_text(content)
            result = self.scan(root)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn(f"FAIL: {root / 'bootstrap-ubuntu-host.sh'}:4:", result.stderr)
            self.assertIn(f"FAIL: {root / 'Dockerfile.g5d-base'}:2:", result.stderr)
            self.assertIn(f"FAIL: {root / 'Dockerfile.g5d-base'}:12:", result.stderr)

    def test_repository_defaults_pass(self):
        result = self.scan()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("OK: pinned-download scan", result.stdout)

    def test_ci_runs_the_guard_and_its_contracts(self):
        justfile = (REPO / "Justfile").read_text()
        self.assertIn("bash scripts/verify-pinned-downloads.sh", justfile)
        ci_rust = (REPO / "infra/ci/cloudbuild-ci-rust.yaml").read_text()
        step = ci_rust[ci_rust.index("- id: image-build-script-contracts"):]
        step = step[:step.index("waitFor")]
        for command in [
            "scripts/verify-pinned-downloads.sh",
            "python3 scripts/test_verify_pinned_downloads.py",
            "scripts/test-pinned-download-verification.sh",
        ]:
            with self.subTest(command=command):
                self.assertIn(command, step)
        canary = (REPO / "scripts/test-self-hosted-azure-microvm-canary.sh").read_text()
        self.assertIn("reject_text \"$AZURE_DIR/bootstrap-ubuntu-host.sh\" 'just.systems'", canary)

    def test_recorded_pins_agree_across_the_repository(self):
        args = dict(re.findall(r"^ARG ([A-Za-z_][A-Za-z0-9_]*)=(\S+)$", DOCKERFILE.read_text(), re.M))
        pins = {name: value for name, value in args.items() if "_SHA256" in name}
        self.assertTrue(pins, "Dockerfile.g5d-base declares no sha256 pins")
        for name, value in pins.items():
            with self.subTest(pin=name):
                self.assertRegex(value, SHA256)
        for tool in ["GIT_LFS", "CARGO_NEXTEST", "SCCACHE", "CARGO_DENY", "UV", "GH_CLI", "JUST"]:
            with self.subTest(tool=tool):
                self.assertEqual(
                    2, sum(1 for name in pins if name.startswith(f"{tool}_SHA256_")),
                    f"{tool} needs one pin per supported architecture",
                )
        run_text = "\n".join(line for line in DOCKERFILE.read_text().splitlines() if not line.lstrip().startswith("#"))
        for name in pins:
            with self.subTest(pin=name):
                self.assertIn("${" + name + "}", run_text, f"{name} is declared but never checked")

        bootstrap = BOOTSTRAP.read_text()
        bootstrap_version = re.search(r"^JUST_VERSION=(\S+)$", bootstrap, re.M).group(1)
        bootstrap_pin = re.search(r"^JUST_SHA256_X86_64=([0-9a-f]{64})$", bootstrap, re.M).group(1)
        self.assertEqual(bootstrap_version, args["JUST_VERSION"])
        self.assertEqual(bootstrap_pin, pins["JUST_SHA256_X86_64"])
        self.assertEqual(re.search(r"^JUST_SHA256_AARCH64=([0-9a-f]{64})$", bootstrap, re.M).group(1), pins["JUST_SHA256_AARCH64"])

        action = (REPO / ".github/actions/setup-just/action.yml").read_text()
        self.assertIn(f'JUST_VERSION: "{bootstrap_version}"', action)
        self.assertIn(f"JUST_SHA256_X64: {bootstrap_pin}", action)

        packer = (REPO / "infra/ci/cloudbuild-packer-worker.yaml").read_text()
        self.assertIn(f"_JUST_SHA256: '{bootstrap_pin}'", packer)
        self.assertIn(f"_CARGO_NEXTEST_SHA256: '{pins['CARGO_NEXTEST_SHA256_X86_64']}'", packer)

        host_env = (REPO / "scripts/worker-host-env.sh").read_text()
        for variable in ["JUST_VERSION", "CARGO_NEXTEST_VERSION", "GH_CLI_VERSION", "GIT_LFS_VERSION"]:
            with self.subTest(variable=variable):
                match = re.search(rf"{variable}:-([^}}]+)}}", host_env)
                if match is None:
                    self.skipTest(f"{variable} has no default in worker-host-env.sh")
                self.assertEqual(match.group(1), args[variable])

        ci_builder = (REPO / "infra/docker/Dockerfile.ci-builder").read_text()
        self.assertIn(f'G5_CI_CARGO_DENY_PIN="={args["CARGO_DENY_VERSION"]}"', ci_builder)
        self.assertIn(f'G5_CI_CARGO_NEXTEST_PIN="={args["CARGO_NEXTEST_VERSION"]}"', ci_builder)


if __name__ == "__main__":
    unittest.main()
