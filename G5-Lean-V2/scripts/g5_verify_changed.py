#!/usr/bin/env python3
"""g5_verify_changed.py — changed-files G5 gate for CI (Rule 5 / Rule 9 / Rule 6).

Complements `verify_grounding.py` (store-wide integrity) with a diff-scoped
sweep so honor-system session rules become merge-visible:

  rule5        — stub/mock/skip markers in changed production sources
                 (multi-stack union: generic, Rust, TS/JS, Python, Java, Go).
  grounding    — changed production sources under backend/ and frontend/ must have a
                 grounding JSONL (Rule 9 existence; counts are NOT gated —
                 the invariant is block coverage, not edge volume).
  edge-fields  — changed grounding JSONLs must carry canonical relevance
                 (0-100 int) and vocabulary relationships.
  human-diff   — added/removed lines inside <human>...</human> regions of
                 spec/view files must carry a provenance marker
                 (origin="g5-human-supervisor:" or last-modified-by) in the
                 added text. This protects Rule 6's most sacred invariant
                 mechanically.

Modes:
  default                       — report findings, exit 0 (annotation only).
  G5_VERIFY_CHANGED_STRICT=1    — exit 1 on any finding (flip after one
                                  clean week per the rollout plan).

Base ref: --base <ref>, else $G5_VERIFY_BASE, else merge-base with
origin/main, else HEAD~1, else a HEAD tree diff for shallow CI checkouts.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

PRODUCTION_PREFIXES = (
    "backend/g5-contracts/src/",
    "backend/g5-auth/src/",
    "backend/g5-storage/src/",
    "backend/g5-semantic/src/",
    "backend/g5-scm/src/",
    "backend/g5-internal-api/src/",
    "backend/g5-api-foundation/src/",
    "backend/g5-api-data/src/",
    "backend/g5-api-core/src/",
    "backend/g5-api-routes/src/",
    "backend/g5-api/src/",
    "backend/g5-authz/src/",
    "backend/g5d-foundation/src/",
    "backend/g5d-isolation/src/",
    "backend/g5d-agent/src/",
    "backend/g5d-host/src/",
    "backend/g5d-dispatch/src/",
    "backend/g5d/src/",
    "frontend/src/",
)
PRODUCTION_EXTS = (".rs", ".java", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".kt")
EXCLUDE_SUBSTRINGS = (
    "/src/test/", "/tests/", "/e2e/", "/__tests__/", "/target/", "/dist/",
    "/node_modules/", "/generated-sources/",
)
TEST_FILENAME_RX = re.compile(
    r"(^|/)([^/]+\.(test|spec)\.[^/]+|test_[^/]+\.py|[^/]+_test\.go|[^/]+Test\.java|[^/]+Tests\.java)$"
)

# Multi-stack stub/skip markers (Rule 5). The union, not one stack: this
# script also guards the repo-agnostic asset bundle.
RULE5_PATTERNS = (
    re.compile(r"\bTODO\b"),
    re.compile(r"\bFIXME\b"),
    re.compile(r"\bXXX\b"),
    re.compile(r"\btodo!\s*\("),
    re.compile(r"\bunimplemented!\s*\("),
    re.compile(r"#\[ignore\]"),
    re.compile(r"panic!\(\s*\"(?:not |un)implemented"),
    re.compile(r"\b(?:it|test|describe)\.skip\b"),
    re.compile(r"\bxit\("),
    re.compile(r"\bNotImplementedError\b"),
    re.compile(r"\b(?:Mockito|@MockBean|@Mock)\b"),
    re.compile(r"\bthrow\s+new\s+UnsupportedOperationException\b"),
    re.compile(r"panic\(\s*\"(?:TODO|not implemented)"),
)

ALLOWED_RELATIONSHIPS = {
    "refines", "implements", "substantiates", "constrains",
    "motivates", "requests", "describes",
}

HUMAN_OPEN_RX = re.compile(r"<human\b")
HUMAN_CLOSE_RX = re.compile(r"</human>")
PROVENANCE_RX = re.compile(r'origin="g5-human-supervisor:|last-modified-by:')
INLINE_CODE_RX = re.compile(r"`[^`]*`")


def _is_protected_human_file(path: str) -> bool:
    return (
        path.endswith(".semlift.md")
        or (path.startswith("views/") and path.endswith(".md"))
        or (path.startswith("designs/") and path.endswith(".md"))
    )


def _git(*args: str) -> str:
    proc = subprocess.run(
        ["git", "-C", str(REPO_ROOT), *args], capture_output=True, text=True, timeout=60
    )
    if proc.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {proc.stderr.strip()}")
    return proc.stdout


def _warn(message: str) -> None:
    print(f"[g5-verify-changed] WARN: {message}", file=sys.stderr)


def _ref_exists(ref: str) -> bool:
    proc = subprocess.run(
        ["git", "-C", str(REPO_ROOT), "rev-parse", "--verify", "--quiet", f"{ref}^{{commit}}"],
        capture_output=True,
        text=True,
        timeout=30,
    )
    return proc.returncode == 0


def resolve_base(explicit: str | None) -> str | None:
    if explicit:
        if _ref_exists(explicit):
            return explicit
        _warn(f"explicit base {explicit!r} is not available; falling back")
    env = os.environ.get("G5_VERIFY_BASE")
    if env:
        if _ref_exists(env):
            return env
        _warn(f"G5_VERIFY_BASE {env!r} is not available; falling back")
    try:
        base = _git("merge-base", "HEAD", "origin/main").strip()
        if base and _ref_exists(base):
            return base
        if base:
            _warn(f"merge-base {base!r} is not available locally; falling back")
    except RuntimeError as exc:
        _warn(f"could not resolve merge-base with origin/main: {exc}")
    if _ref_exists("HEAD~1"):
        return "HEAD~1"
    _warn("no merge base or HEAD~1 is available; falling back to HEAD tree diff")
    return None


def changed_files(base: str | None) -> list[str]:
    if base is not None:
        try:
            out = _git("diff", "--name-only", "--diff-filter=d", f"{base}...HEAD")
            return [line.strip() for line in out.splitlines() if line.strip()]
        except RuntimeError as exc:
            _warn(f"could not diff against base {base!r}: {exc}; falling back to HEAD tree diff")
    try:
        out = _git("diff-tree", "--no-commit-id", "--name-only", "-r", "--root", "HEAD")
    except RuntimeError as exc:
        _warn(f"could not compute HEAD tree diff: {exc}; treating changed file set as empty")
        return []
    return [line.strip() for line in out.splitlines() if line.strip()]


def is_production_source(path: str) -> bool:
    if not path.startswith(PRODUCTION_PREFIXES):
        return False
    if not path.endswith(PRODUCTION_EXTS):
        return False
    if any(s in path for s in EXCLUDE_SUBSTRINGS):
        return False
    return not TEST_FILENAME_RX.search(path)


def check_rule5(files: list[str]) -> list[str]:
    findings = []
    for rel in files:
        if not is_production_source(rel):
            continue
        try:
            lines = (REPO_ROOT / rel).read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        for lineno, line in enumerate(lines, 1):
            for pattern in RULE5_PATTERNS:
                if pattern.search(line):
                    findings.append(f"{rel}:{lineno}: {line.strip()[:100]}")
                    break
    return findings


def check_grounding_existence(files: list[str]) -> list[str]:
    findings = []
    for rel in files:
        if not is_production_source(rel):
            continue
        flat = rel.replace("/", "__") + ".jsonl"
        if not (REPO_ROOT / "intent_grounding/code" / flat).exists():
            findings.append(f"{rel}: no grounding JSONL (Rule 9 — register + ground, or justify)")
    return findings


def check_edge_fields(files: list[str]) -> list[str]:
    findings = []
    for rel in files:
        if "intent_grounding/" not in rel or not rel.endswith(".jsonl"):
            continue
        try:
            text = (REPO_ROOT / rel).read_text(encoding="utf-8")
        except OSError:
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            if not line.strip():
                continue
            try:
                edge = json.loads(line)
            except ValueError:
                findings.append(f"{rel}:{lineno}: malformed JSONL line")
                continue
            relevance = edge.get("relevance")
            if relevance is not None and (
                isinstance(relevance, bool)
                or not isinstance(relevance, int)
                or not 0 <= relevance <= 100
            ):
                findings.append(f"{rel}:{lineno}: non-canonical relevance {relevance!r}")
            relationship = edge.get("relationship")
            if relationship is not None and relationship not in ALLOWED_RELATIONSHIPS:
                findings.append(f"{rel}:{lineno}: unknown relationship '{relationship}'")
    return findings


def _human_line_spans(text: str) -> list[tuple[int, int]]:
    """1-based inclusive line spans covered by <human>...</human>."""
    spans = []
    open_line = None
    for lineno, line in enumerate(text.splitlines(), 1):
        scan_line = INLINE_CODE_RX.sub("", line)
        if open_line is None and HUMAN_OPEN_RX.search(scan_line):
            open_line = lineno
        if open_line is not None and HUMAN_CLOSE_RX.search(scan_line):
            spans.append((open_line, lineno))
            open_line = None
    if open_line is not None:
        spans.append((open_line, len(text.splitlines())))
    return spans


def check_human_diff(base: str | None, files: list[str]) -> list[str]:
    findings = []
    if base is None:
        if any(_is_protected_human_file(f) for f in files):
            _warn("skipping <human> diff provenance check because no base ref is available")
        return findings
    spec_like = [f for f in files if _is_protected_human_file(f)]
    for rel in spec_like:
        try:
            new_text = (REPO_ROOT / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            new_text = ""
        new_spans = _human_line_spans(new_text)
        try:
            old_text = _git("show", f"{base}:{rel}")
        except RuntimeError:
            old_text = ""
        old_spans = _human_line_spans(old_text)
        if not new_spans and not old_spans:
            continue
        try:
            diff = _git("diff", "--unified=0", f"{base}...HEAD", "--", rel)
        except RuntimeError as exc:
            _warn(f"could not compute <human> diff for {rel}: {exc}")
            continue
        old_lineno = None
        new_lineno = None
        for line in diff.splitlines():
            hunk = re.match(r"@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@", line)
            if hunk:
                old_lineno = int(hunk.group(1))
                new_lineno = int(hunk.group(2))
                continue
            if old_lineno is None or new_lineno is None:
                continue
            if line.startswith("+") and not line.startswith("+++"):
                inside = any(start <= new_lineno <= end for start, end in new_spans)
                if inside and not PROVENANCE_RX.search(line):
                    findings.append(
                        f"{rel}:{new_lineno}: edit inside <human> block without a "
                        f'provenance marker (origin="g5-human-supervisor:..." or a '
                        f"registered human decision) — Rule 6 / §2.4.2"
                    )
                new_lineno += 1
            elif line.startswith("-") and not line.startswith("---"):
                inside = any(start <= old_lineno <= end for start, end in old_spans)
                if inside and not PROVENANCE_RX.search(line):
                    findings.append(
                        f"{rel}:{old_lineno}: deletion inside <human> block without a "
                        f'provenance marker (origin="g5-human-supervisor:..." or a '
                        f"registered human decision) — Rule 6 / §2.4.2"
                    )
                old_lineno += 1
            elif line.startswith(" "):
                old_lineno += 1
                new_lineno += 1
    return findings


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", default=None)
    args = parser.parse_args(argv)

    base = resolve_base(args.base)
    files = changed_files(base)
    base_label = base[:12] if base is not None else "HEAD-tree"
    print(f"[g5-verify-changed] base={base_label} changed_files={len(files)}")

    sections = {
        "rule5": check_rule5(files),
        "grounding": check_grounding_existence(files),
        "edge-fields": check_edge_fields(files),
        "human-diff": check_human_diff(base, files),
    }

    total = 0
    for name, findings in sections.items():
        if not findings:
            continue
        total += len(findings)
        print(f"\n✗ {name} — {len(findings)} finding(s):")
        for finding in findings[:40]:
            print(f"    {finding}")
        if len(findings) > 40:
            print(f"    … and {len(findings) - 40} more")

    if total == 0:
        print("[g5-verify-changed] OK — no findings.")
        return 0

    strict = os.environ.get("G5_VERIFY_CHANGED_STRICT", "").lower() in {"1", "true", "yes", "on"}
    print(
        f"\n[g5-verify-changed] {total} finding(s); "
        + ("STRICT mode — failing the build." if strict
           else "report-only (set G5_VERIFY_CHANGED_STRICT=1 to enforce).")
    )
    return 1 if strict else 0


if __name__ == "__main__":
    sys.exit(main())
