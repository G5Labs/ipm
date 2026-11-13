#!/usr/bin/env python3
"""Exercise the static image guard against real temporary Dockerfiles."""
from pathlib import Path
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]
SCRIPT = REPO / "scripts/verify-base-digest-identity.sh"
CADDY_SOURCE = "caddy:2.11.4-alpine@sha256:6aeddd44c3078b0f9a35206472a11420648a79c184603ef95957d0a20044cb2b"


class StaticFleetScanTests(unittest.TestCase):
    def scan(self, directory):
        return subprocess.run(
            ["bash", str(SCRIPT), "--static-scan", str(directory)],
            text=True, capture_output=True,
        )

    def test_missing_and_empty_directories_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for directory in [root / "missing", root]:
                with self.subTest(directory=directory):
                    result = self.scan(directory)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertNotIn("every FROM classifies", result.stdout)

    def test_classifies_real_dockerfiles_including_pinned_playwright(self):
        cases = [
            ("g5/base:26.04", True),
            ("mcr.microsoft.com/playwright:v1.58.2-noble@sha256:" + "a" * 64, True),
            ("mcr.microsoft.com/playwright:v1.58.2-noble@sha256:invalid", False),
            ("g5/base-attacker:latest", False),
            ("alpine:latest", False),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for reference, accepted in cases:
                with self.subTest(reference=reference):
                    (root / "Dockerfile.test").write_text(f"FROM {reference}\n")
                    result = self.scan(root)
                    self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_ci_scans_the_relocated_fleet(self):
        recipe = (REPO / "Justfile").read_text()
        self.assertIn("verify-base-digest-identity.sh --static-scan infra/docker", recipe)
        result = self.scan(REPO / "infra/docker")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_caddy_is_a_pinned_binary_source_without_runtime_ancestry(self):
        source = f"ARG CADDY_IMAGE={CADDY_SOURCE}\nFROM ${{CADDY_IMAGE}} AS caddy-source\n"
        runtime = "FROM g5/base:26.04 AS runtime\nCOPY --from=caddy-source /usr/bin/caddy /usr/bin/caddy\n"
        cases = [
            (source + runtime, True),
            (f"FROM {CADDY_SOURCE} AS runtime\n", False),
            (f"FROM {CADDY_SOURCE}\n", False),
            (source + "FROM caddy-source AS runtime\n", False),
            (source.replace(CADDY_SOURCE, "caddy:2.11.4-alpine") + runtime, False),
            (source.replace("6aeddd44", "0aeddd44") + runtime, False),
            (source.replace("caddy:", "example.invalid/caddy:") + runtime, False),
            ("  FROM caddy:latest AS caddy-source\nFROM g5/base:26.04 AS runtime\nCOPY --from=caddy-source / /\n", False),
            (source + runtime.replace("/usr/bin/caddy /usr/bin/caddy", "/ /"), False),
            (source + runtime.replace("--from=caddy-source", "--from=0"), True),
            (source + runtime.replace("--from=caddy-source /usr/bin/caddy /usr/bin/caddy", "--from=0 / /"), False),
            (source + runtime.replace(" /usr/bin/caddy /usr/bin/caddy", " \\\n  / /"), False),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for dockerfile, accepted in cases:
                with self.subTest(dockerfile=dockerfile):
                    (root / "Dockerfile.test").write_text(dockerfile)
                    result = self.scan(root)
                    self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_all_caddy_consumers_share_the_binary_source(self):
        for filename in ["Dockerfile.edge", "Dockerfile.edge-prebuilt", "Dockerfile.g5d-g5dev"]:
            with self.subTest(filename=filename):
                source = (REPO / "infra/docker" / filename).read_text()
                self.assertIn(f"ARG CADDY_IMAGE={CADDY_SOURCE}\n", source)
                self.assertIn("FROM ${CADDY_IMAGE} AS caddy-source\n", source)
                self.assertIn("COPY --from=caddy-source /usr/bin/caddy /usr/bin/caddy\n", source)
                self.assertNotIn("dl.cloudsmith.io/public/caddy", source)

    def test_duplicate_caddy_sources_cannot_hide_an_earlier_stage(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for first_index in [0, 1]:
                source = "FROM g5/base-node:26.04 AS builder\n" if first_index else ""
                source += (
                    f"FROM {CADDY_SOURCE} AS caddy-source\n"
                    "FROM g5/base:26.04 AS intermediate\n"
                    f"FROM {CADDY_SOURCE} AS caddy-source\n"
                    "FROM g5/base:26.04 AS runtime\n"
                )
                for instruction in [
                    f"COPY --from={first_index} / /\n",
                    f"RUN --mount=type=bind,from={first_index},target=/upstream cp -a /upstream/. /\n",
                    "COPY --from=caddy-source /usr/bin/caddy /usr/bin/caddy\n",
                ]:
                    with self.subTest(first_index=first_index, instruction=instruction):
                        (root / "Dockerfile.test").write_text(
                            source + instruction
                            + f"COPY --from={first_index + 2} /usr/bin/caddy /usr/bin/caddy\n"
                        )
                        result = self.scan(root)
                        self.assertNotEqual(result.returncode, 0, result.stdout)
            for base_alias in ["caddy-source", "CADDY-SOURCE"]:
                pinned = f"FROM {CADDY_SOURCE} AS caddy-source\n"
                base = f"FROM g5/base:26.04 AS {base_alias}\n"
                for source in [pinned + base, base + pinned]:
                    with self.subTest(shadowing_source=source):
                        (root / "Dockerfile.test").write_text(
                            source + "FROM g5/base:26.04 AS runtime\n"
                            + "COPY --from=caddy-source /usr/bin/caddy /usr/bin/caddy\n"
                        )
                        result = self.scan(root)
                        self.assertNotEqual(result.returncode, 0, result.stdout)
            (root / "Dockerfile.test").write_text(
                "FROM g5/base-node:26.04 AS builder\n"
                "FROM g5/base:26.04 AS builder\n"
                "FROM g5/base:26.04 AS runtime\n"
                "COPY --from=0 /usr/bin/git /usr/bin/git\n"
            )
            result = self.scan(root)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_caddy_source_cannot_be_mounted_or_referenced_outside_binary_copy(self):
        source = f"FROM {CADDY_SOURCE} AS caddy-source\nFROM g5/base:26.04 AS runtime\n"
        mounts = [
            "RUN --mount=type=bind,from=caddy-source,target=/upstream cp -a /upstream/. /\n",
            "RUN --mount=type=bind,from=0,target=/upstream cp -a /upstream/. /\n",
            "run --mount=type=bind,from=CADDY-SOURCE,target=/upstream cp -a /upstream/. /\n",
            'RUN --mount=type=bind,from="caddy-source",target=/upstream cp -a /upstream/. /\n',
            "RUN --mount=from=caddy-source,type=bind,target=/upstream true\n",
            'RUN --mount="from=caddy-source,type=bind,target=/upstream" true\n',
            "RUN --mount=type=bind,target=/upstream,from=caddy-source true\n",
            "RUN \\\n  --mount=type=bind,from=caddy-source,target=/upstream cp -a /upstream/. /\n",
            "  RUN --mount=type=bind,\\\n  from=caddy-source,target=/upstream cp -a /upstream/. /\n",
            f"RUN --mount=type=bind,from={CADDY_SOURCE},target=/upstream cp -a /upstream/. /\n",
            "COPY --from=caddy:latest /usr/bin/caddy /usr/bin/caddy\n",
            "ARG UPSTREAM=caddy-source\nRUN --mount=type=bind,from=${UPSTREAM},target=/upstream true\n",
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for instruction in mounts:
                with self.subTest(instruction=instruction):
                    (root / "Dockerfile.test").write_text(source + instruction)
                    result = self.scan(root)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
            (root / "Dockerfile.test").write_text(
                "FROM g5/base-node:26.04 AS builder\n" + source
                + "RUN --mount=type=bind,from=1,target=/upstream cp -a /upstream/. /\n"
            )
            result = self.scan(root)
            self.assertNotEqual(result.returncode, 0, result.stdout)
            (root / "Dockerfile.test").write_text(
                source + "RUN --mount=type=cache,target=/tmp/cache true\n"
                + "copy --from=caddy-source \\\n  /usr/bin/caddy /usr/bin/caddy\n"
            )
            result = self.scan(root)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_caddy_stage_indexes_use_normalized_instructions(self):
        sources = [
            "  FROM g5/base-node:26.04 AS builder\n"
            f"FROM {CADDY_SOURCE} AS caddy-source\n"
            "FROM g5/base:26.04 AS runtime\n",
            f"  ARG CADDY_IMAGE={CADDY_SOURCE}\n"
            "  fRoM g5/base-node:26.04 AS builder\n"
            "  FROM \\\n    ${CADDY_IMAGE} AS caddy-source\n"
            "FROM g5/base:26.04 AS runtime\n",
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for source in sources:
                for paths, accepted in [("/ /", False), ("/usr/bin/caddy /usr/bin/caddy", True)]:
                    with self.subTest(source=source, paths=paths):
                        (root / "Dockerfile.test").write_text(source + f"COPY --from=1 {paths}\n")
                        result = self.scan(root)
                        self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_direct_caddy_sources_are_rejected_with_or_without_pinned_stage(self):
        pinned = f"FROM {CADDY_SOURCE} AS caddy-source\n"
        runtime = "FROM g5/base:26.04 AS runtime\n"
        references = [
            "caddy", "caddy:latest", "caddy@sha256:" + "a" * 64,
            "library/caddy", "docker.io/library/caddy",
            "docker.io/library/caddy@sha256:" + "a" * 64,
            "example.invalid/caddy", "example.invalid/caddy:latest",
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for source in ["", pinned]:
                for reference in references:
                    for instruction in [
                        f"COPY --from={reference} /usr/bin/caddy /usr/bin/caddy\n",
                        f"COPY --from={reference} / /\n",
                        f"RUN --mount=type=bind,from={reference},target=/upstream true\n",
                    ]:
                        with self.subTest(pinned=bool(source), instruction=instruction):
                            (root / "Dockerfile.test").write_text(source + runtime + instruction)
                            result = self.scan(root)
                            self.assertNotEqual(result.returncode, 0, result.stdout)

    def test_non_caddy_binary_source_and_cache_mount_are_unchanged(self):
        source = (
            "FROM maven:3.9-eclipse-temurin-17@sha256:" + "a" * 64 + " AS maven-source\n"
            "FROM g5/base:26.04 AS runtime\n"
            "COPY --from=maven-source /usr/share/maven /usr/share/maven\n"
            "RUN --mount=type=cache,target=/tmp/cache true\n"
        )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Dockerfile.test").write_text(source)
            result = self.scan(root)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_actual_copy_and_mount_providers_must_classify(self):
        runtime = "FROM g5/base:26.04 AS runtime\n"
        sources = f"FROM {CADDY_SOURCE} AS caddy-source\n" + runtime
        maven = "maven:3.9-eclipse-temurin-17@sha256:" + "a" * 64
        cases = [
            (sources + "COPY --from=example.invalid/caddy-evil:latest /usr/bin/caddy /usr/bin/caddy\n", False),
            (runtime + "COPY --from=example.invalid/tool:latest /usr/bin/caddy /usr/bin/caddy\n", False),
            (runtime + "COPY --chown=root:root --from=example.invalid/tool:latest / /\n", False),
            (runtime + "RUN --mount=type=bind,from=example.invalid/caddy-evil:latest,target=/upstream cp -a /upstream/. /\n", False),
            (runtime + 'RUN --mount="type=bind,from=example.invalid/tool:latest,target=/upstream" true\n', False),
            (runtime + 'RUN --mount=type=bind,from="example.invalid/tool:latest",target=/upstream true\n', False),
            (runtime + 'RUN --mount=\'type=bind,"from=example.invalid/tool:latest",target=/upstream\' true\n', False),
            (runtime + "RUN printf 'from=example.invalid/tool:latest'\n", True),
            (runtime + 'RUN echo --mount=from=example.invalid/tool:latest\n', True),
            (runtime + "COPY file /tmp/from=example.invalid/tool:latest\n", True),
            (runtime + "RUN printf 'from=caddy:latest'\n", True),
            (sources + "RUN echo --mount=from=caddy-source\n", True),
            (sources + "COPY file /tmp/from=caddy-source\n", True),
            (runtime + "RUN --mount=type=cache,target=/tmp/cache true\n", True),
            (runtime + 'RUN --mount="type=cache,target=/tmp/cache" true\n', True),
            (runtime + "ONBUILD COPY --from=example.invalid/tool:latest / /\n", False),
            (runtime + "ONBUILD RUN --mount=from=example.invalid/tool:latest,target=/upstream true\n", False),
            ("FROM g5/base:26.04 AS builder\n" + runtime
             + "ONBUILD COPY --from=builder /usr/bin/git /usr/bin/git\n", False),
            (runtime + "ONBUILD COPY file /tmp/file\n", True),
            (runtime + "ONBUILD RUN --mount=type=cache,target=/tmp/cache true\n", True),
            (runtime + "COPY --from=g5/base:26.04 /usr/bin/git /usr/bin/git\n", True),
            (runtime + f"COPY --from={maven} /usr/share/maven /usr/share/maven\n", True),
            ("FROM g5/base-node:26.04 AS builder\n" + runtime
             + "COPY --from=BUILDER /app/dist /srv\n"
             + "RUN --mount=from=+00,target=/upstream true\n", True),
            ("FROM g5/base-node:26.04 AS builder\n" + runtime
             + 'RUN --mount="type=bind,from=builder,target=/upstream" true\n', True),
            (runtime + "COPY --from=1 / /\n", False),
            (runtime + "COPY --from= / /\n", False),
            (runtime + "COPY --from='builder / /\n", False),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for source, accepted in cases:
                with self.subTest(source=source):
                    (root / "Dockerfile.test").write_text(source)
                    result = self.scan(root)
                    self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_only_active_initial_escape_directives_change_parsing(self):
        source = f"FROM {CADDY_SOURCE} AS caddy-source\n"
        runtime = "FROM g5/base:26.04 AS runtime\n"
        copy = "COPY --from=caddy-\\\nsource /usr/bin/caddy /usr/bin/caddy\n"
        cases = [
            (source + runtime + "# escape=`\n" + copy, True),
            ("# ordinary comment\n# escape=`\n" + source + runtime + copy, True),
            ("\n# escape=`\n" + source + runtime + copy, True),
            ("# unknown=value\n# escape=`\n" + source + runtime + copy, True),
            ("# syntax=docker/dockerfile:1\n# ordinary comment\n# escape=`\n" + source + runtime + copy, True),
            ("# escape=`\n" + source + runtime + copy, False),
            ("# syntax=docker/dockerfile:1\n# escape=`\n" + source + runtime + copy, False),
            ("# escape=\\\n" + source + runtime + copy, True),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for text, accepted in cases:
                with self.subTest(source=text):
                    (root / "Dockerfile.test").write_text(text)
                    result = self.scan(root)
                    self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_caddy_source_continuations_preserve_parser_concatenation(self):
        source = f"FROM {CADDY_SOURCE} AS caddy-source\n"
        runtime = "FROM g5/base:26.04 AS runtime\n"
        cases = [
            (source + runtime + "COPY --from=caddy-\\\nsource / /\n", False),
            (source + runtime + "COPY --from=caddy-\\\n# continued comment\n\nsource / /\n", False),
            (source + runtime + "RUN --mount=type=bind,from=caddy-\\\nsource,target=/upstream true\n", False),
            (source + runtime + "COPY --from=caddy-\\\nsource /usr/bin/caddy /usr/bin/caddy\n", True),
            (source + runtime + "COPY --from=caddy-\\\n# continued comment\n\nsource /usr/bin/caddy /usr/bin/caddy\n", True),
            (runtime + "COPY --from=ca\\\nddy / /\n", False),
            (source.replace(" AS caddy-source", " AS caddy-\\\nsource") + runtime
             + "COPY --from=caddy-source /usr/bin/caddy /usr/bin/caddy\n", True),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for dockerfile, accepted in cases:
                with self.subTest(dockerfile=dockerfile):
                    (root / "Dockerfile.test").write_text(dockerfile)
                    result = self.scan(root)
                    self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_caddy_numeric_source_references_allow_only_canonical_binary_copy(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for index, references in [(0, ["00", "000", "+0", "+00", "-0", "-00"]),
                                      (1, ["01", "0001", "+1", "+01"])]:
                source = ("FROM g5/base-node:26.04 AS builder\n" if index else "")
                source += f"FROM {CADDY_SOURCE} AS caddy-source\nFROM g5/base:26.04 AS runtime\n"
                for reference in references + [str(index)]:
                    for paths in ["/ /", "/usr/bin/caddy /usr/bin/caddy"]:
                        with self.subTest(index=index, reference=reference, paths=paths):
                            (root / "Dockerfile.test").write_text(source + f"COPY --from={reference} {paths}\n")
                            result = self.scan(root)
                            accepted = reference == str(index) and paths == "/usr/bin/caddy /usr/bin/caddy"
                            self.assertEqual(result.returncode == 0, accepted, result.stderr)
            # A padded reference to a different, permitted stage retains its policy.
            (root / "Dockerfile.test").write_text(
                "FROM g5/base-node:26.04 AS builder\n"
                f"FROM {CADDY_SOURCE} AS caddy-source\n"
                "FROM g5/base:26.04 AS runtime\n"
                "COPY --from=00 / /\n"
                "COPY --from=+0 / /\n"
                "RUN --mount=type=cache,target=/tmp/cache true\n"
                "COPY --from=1 /usr/bin/caddy /usr/bin/caddy\n"
            )
            result = self.scan(root)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_instruction_lexer_cannot_hide_caddy_stage_or_source(self):
        source = f"FROM {CADDY_SOURCE} AS caddy-source\nFROM g5/base:26.04 AS runtime\n"
        copy = "COPY --from=0 /usr/bin/caddy /usr/bin/caddy\n"
        cases = [
            ("\ufeff" + source + "COPY --from=0 / /\n", False),
            ("\ufeff" + source + copy, True),
            ("ARG UNUSED=x" + "\\" * 2 + "\n" + source + "COPY --from=0 / /\n", False),
            ("ARG UNUSED=x" + "\\" * 2 + "\n" + source + copy, True),
            ("FROM g5/base-node:26.04 AS builder\nRUN printf x" + "\\" * 2 + "\n"
             + source + "COPY --from=1 / /\n", False),
            ("FROM g5/base-node:26.04 AS builder\nRUN printf x" + "\\" * 2 + "\n"
             + source + copy.replace("--from=0", "--from=1"), True),
            ("# escape=`\n" + source + "COPY --from=caddy-`\nsource / /\n", False),
            ("\ufeff# EsCaPe = `\n" + source + copy, False),
            ("# escape=invalid\n" + source + copy, False),
            ("# escape=\\\n" + source + copy, True),
            ("# escape=\\\n" + source + "COPY --from=caddy-\\\nsource / /\n", False),
            ("# Ordinary comment mentioning escape=`\n" + source + copy, True),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for dockerfile, accepted in cases:
                with self.subTest(dockerfile=dockerfile):
                    (root / "Dockerfile.test").write_text(dockerfile)
                    result = self.scan(root)
                    self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_heredoc_bodies_cannot_be_counted_as_dockerfile_stages(self):
        source = f"FROM {CADDY_SOURCE} AS caddy-source\nFROM g5/base:26.04 AS runtime\n"
        openings = [
            "COPY <<EOF /tmp/example", "COPY <<'EOF' /tmp/example",
            'COPY <<"EOF" /tmp/example', "COPY <<-EOF /tmp/example",
            "RUN cat <<EOF", "RUN cat 3<<EOF", "RUN cat <<-'EOF'",
            "ADD <<EOF /tmp/example", "ONBUILD RUN cat <<EOF",
            "copy \\\n  <<EOF /tmp/example",
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for opening in openings:
                with self.subTest(opening=opening):
                    (root / "Dockerfile.test").write_text(
                        "# syntax=docker/dockerfile:1\nFROM g5/base:26.04 AS builder\n"
                        + opening + "\nFROM g5/base:26.04 AS decoy\nEOF\n"
                        + source + "COPY --from=1 / /\n"
                    )
                    result = self.scan(root)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn("heredoc", result.stderr)
            (root / "Dockerfile.test").write_text(
                "# Comment mentioning COPY <<EOF is not an instruction\n"
                + source + "RUN echo ordinary\n"
                + "COPY --from=caddy-source /usr/bin/caddy /usr/bin/caddy\n"
            )
            result = self.scan(root)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_from_uses_only_unique_literal_global_arg_defaults(self):
        runtime = "FROM g5/base:26.04 AS runtime\nCOPY --from=caddy-source /usr/bin/caddy /usr/bin/caddy\n"
        pinned = f"ARG CADDY_IMAGE={CADDY_SOURCE}\n"
        source = "FROM ${CADDY_IMAGE} AS caddy-source\n"
        cases = [
            (pinned + "ARG CADDY_IMAGE=caddy:latest\n" + source + runtime, False),
            ("ARG CADDY_IMAGE=caddy:latest\n" + pinned + source + runtime, False),
            (pinned + "ARG CADDY_IMAGE\n" + source + runtime, False),
            (pinned + pinned + source + runtime, False),
            ("FROM g5/base:26.04 AS builder\n" + pinned + source + runtime, False),
            ("ARG CADDY_IMAGE\nFROM g5/base:26.04 AS builder\n" + pinned + source + runtime, False),
            (pinned + "FROM g5/base:26.04 AS builder\nARG CADDY_IMAGE=caddy:latest\n" + source + runtime, True),
            ("ARG CADDY_IMAGE=${CADDY_IMAGE}\n" + source + runtime, False),
            ("ARG CADDY_IMAGE=${OTHER}\nARG OTHER=${CADDY_IMAGE}\n" + source + runtime, False),
            (pinned + "ARG ALIAS=${CADDY_IMAGE}\n" + source.replace("CADDY_IMAGE", "ALIAS") + runtime, False),
            ("ARG CADDY_IMAGE=${OTHER}\n" + pinned.replace("CADDY_IMAGE", "OTHER") + source + runtime, False),
            (pinned.replace(f"={CADDY_SOURCE}", f'="{CADDY_SOURCE}"') + source + runtime, False),
            (pinned.replace(f"={CADDY_SOURCE}", f"='{CADDY_SOURCE}'") + source + runtime, False),
            (pinned.replace("caddy:", "caddy\\:") + source + runtime, False),
            (pinned + source + runtime, True),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for dockerfile, accepted in cases:
                with self.subTest(dockerfile=dockerfile):
                    (root / "Dockerfile.test").write_text(dockerfile)
                    try:
                        result = subprocess.run(
                            ["bash", str(SCRIPT), "--static-scan", str(root)],
                            text=True, capture_output=True, timeout=2,
                        )
                    except subprocess.TimeoutExpired:
                        self.fail("FROM ARG resolution must terminate")
                    self.assertEqual(result.returncode == 0, accepted, result.stderr)


if __name__ == "__main__":
    unittest.main()
