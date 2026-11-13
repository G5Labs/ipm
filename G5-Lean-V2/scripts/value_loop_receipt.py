#!/usr/bin/env python3
"""Build and validate the fail-closed workspace MVP execution receipt."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Iterable


CATALOG_FIELDS = ["loop", "lane", "profile", "selector", "source", "proof"]
PROFILE_LANES = {
    "api-integration": "api",
    "rust-e2e-broad": "e2e",
    "playwright-workspace-vm": "playwright",
}
PROFILE_ORDER = {profile: index for index, profile in enumerate(PROFILE_LANES)}
TERMINAL_STATUSES = {"passed", "failed", "skipped", "ignored", "not_run"}
EXPECTED_LOOPS = {
    # Core shipping, first-run, issue and alignment paths (G5Labs/G5-Lean#976).
    "SH1",
    "SH2",
    "FR1",
    "IS1",
    "ORG1",
    "AP1",
    "DC1",
    "DC2",
    "DC3",
    "ON1",
    "ON2",
    "ON3",
    "M15",
    "M16",
    "RV1",
    "RV2",
    "RV3",
    "RV4",
    "RV5",
    "RV6",
    "RV7",
    "RV8",
    "RV9",
    "RV10",

    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "M1",
    "M2",
    "M3",
    "M4",
    "M5",
    "M6",
    "M7",
    "M8",
    "M9",
    "M10",
    "M11",
    "M12",
    "M13",
    "M14",
    "P2.1",
    "P2.2",
    "P2.4",
    "P4.3a",
    "P4.3b",
    "P4.4a",
    "P4.4b",
    "P4.5",
    "P4.6",
    "P4.7",
    "P4.8",
    "P4.9a",
    "P4.9b",
    "P4.10",
    "P4.11",
    "P4.13",
    "P5.1",
    "P5.2",
}
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
RESULT_FIELDS = {
    "candidateSha",
    "catalogDigest",
    "lane",
    "profile",
    "selector",
    "startedAt",
    "endedAt",
    "status",
    "evidenceArtifact",
}


class ReceiptError(ValueError):
    """The catalog or receipt violated a fail-closed invariant."""


@dataclass(frozen=True, order=True)
class SelectorKey:
    lane: str
    profile: str
    selector: str


@dataclass(frozen=True)
class Catalog:
    digest: str
    loops_by_selector: dict[SelectorKey, tuple[str, ...]]
    source_by_selector: dict[SelectorKey, str]

    def ordered_selectors(self) -> list[SelectorKey]:
        return sorted(
            self.loops_by_selector,
            key=lambda key: (PROFILE_ORDER[key.profile], key.selector),
        )


def catalog_digest(path: Path) -> str:
    return f"sha256:{hashlib.sha256(path.read_bytes()).hexdigest()}"


def _parse_timestamp(value: object, label: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        raise ReceiptError(f"{label} must be a UTC RFC3339 timestamp ending in Z")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise ReceiptError(f"{label} is not a valid RFC3339 timestamp") from error
    if parsed.utcoffset() != timezone.utc.utcoffset(parsed):
        raise ReceiptError(f"{label} must be UTC")
    return parsed


def _validate_sha(value: object, label: str = "candidate SHA") -> str:
    if not isinstance(value, str) or SHA_RE.fullmatch(value) is None:
        raise ReceiptError(f"{label} must be one full lowercase 40-character Git SHA")
    return value


def _validate_evidence(value: object, label: str) -> str:
    if not isinstance(value, str) or not value.strip() or value != value.strip():
        raise ReceiptError(f"{label} must be a non-empty trimmed artifact reference")
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or "." in path.parts:
        raise ReceiptError(f"{label} must be a normalized relative artifact reference")
    return value


def load_catalog(path: Path, generation_root: Path | None = None) -> Catalog:
    if not path.is_file():
        raise ReceiptError(f"value-loop catalog does not exist: {path}")
    # The shipped catalog lives at tests/e2e/value_loop_matrix.tsv. Resolve
    # its source paths (backend/..., frontend/...) from the repository root
    # when callers use the CLI default.
    generation_root = generation_root or path.parent.parent.parent
    loops_by_selector: dict[SelectorKey, set[str]] = defaultdict(set)
    source_by_selector: dict[SelectorKey, str] = {}
    profile_by_lane_selector: dict[tuple[str, str], str] = {}
    seen_rows: set[tuple[str, SelectorKey]] = set()

    with path.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        if reader.fieldnames != CATALOG_FIELDS:
            raise ReceiptError(
                f"catalog header must be exactly {CATALOG_FIELDS}, got {reader.fieldnames}"
            )
        for line_number, row in enumerate(reader, start=2):
            if None in row or any(value is None for value in row.values()):
                raise ReceiptError(f"catalog line {line_number} has the wrong field count")
            if any(value != value.strip() or not value for value in row.values()):
                raise ReceiptError(f"catalog line {line_number} contains an empty or untrimmed field")

            loop_id = row["loop"]
            profile = row["profile"]
            lane = row["lane"]
            selector = row["selector"]
            if loop_id not in EXPECTED_LOOPS:
                raise ReceiptError(f"catalog line {line_number} has unknown loop {loop_id!r}")
            expected_lane = PROFILE_LANES.get(profile)
            if expected_lane is None:
                raise ReceiptError(f"catalog line {line_number} has unknown profile {profile!r}")
            if lane != expected_lane:
                raise ReceiptError(
                    f"catalog line {line_number} profile {profile!r} requires lane {expected_lane!r}"
                )

            key = SelectorKey(lane=lane, profile=profile, selector=selector)
            row_key = (loop_id, key)
            if row_key in seen_rows:
                raise ReceiptError(f"catalog line {line_number} duplicates {loop_id} {key}")
            seen_rows.add(row_key)

            lane_selector = (lane, selector)
            prior_profile = profile_by_lane_selector.setdefault(lane_selector, profile)
            if prior_profile != profile:
                raise ReceiptError(
                    f"selector/profile ambiguity for {lane} {selector}: "
                    f"{prior_profile!r} versus {profile!r}"
                )
            loops_by_selector[key].add(loop_id)
            prior_source = source_by_selector.setdefault(key, row["source"])
            if prior_source != row["source"]:
                raise ReceiptError(
                    f"selector {key} maps to conflicting sources {prior_source!r} and {row['source']!r}"
                )

            source_path = generation_root / row["source"]
            if not source_path.is_file():
                raise ReceiptError(
                    f"catalog line {line_number} names missing source {source_path}"
                )
            if lane != "playwright" and selector not in source_path.read_text(encoding="utf-8"):
                raise ReceiptError(
                    f"catalog line {line_number} selector {selector!r} is absent from {source_path}"
                )

    actual_loops = {loop_id for loops in loops_by_selector.values() for loop_id in loops}
    if actual_loops != EXPECTED_LOOPS:
        raise ReceiptError(
            f"catalog must cover exactly the current required loops; missing={sorted(EXPECTED_LOOPS - actual_loops)} "
            f"extra={sorted(actual_loops - EXPECTED_LOOPS)}"
        )
    observed_profiles = {key.profile for key in loops_by_selector}
    if not set(PROFILE_LANES) <= observed_profiles:
        raise ReceiptError(
            f"catalog must use every execution profile; got {sorted(observed_profiles)}"
        )

    return Catalog(
        digest=catalog_digest(path),
        loops_by_selector={
            key: tuple(sorted(loop_ids)) for key, loop_ids in loops_by_selector.items()
        },
        source_by_selector=source_by_selector,
    )


def record_result(
    path: Path,
    *,
    candidate_sha: str,
    catalog_digest_value: str,
    key: SelectorKey,
    started_at: str,
    ended_at: str,
    status: str,
    evidence_artifact: str,
) -> None:
    _validate_sha(candidate_sha)
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", catalog_digest_value):
        raise ReceiptError("catalog digest must be sha256:<64 lowercase hex characters>")
    if PROFILE_LANES.get(key.profile) != key.lane:
        raise ReceiptError(f"profile {key.profile!r} does not belong to lane {key.lane!r}")
    started = _parse_timestamp(started_at, "startedAt")
    ended = _parse_timestamp(ended_at, "endedAt")
    if ended < started:
        raise ReceiptError("endedAt must not precede startedAt")
    if status not in TERMINAL_STATUSES:
        raise ReceiptError(f"unknown terminal status {status!r}")
    _validate_evidence(evidence_artifact, "evidenceArtifact")

    path.parent.mkdir(parents=True, exist_ok=True)
    result = {
        "candidateSha": candidate_sha,
        "catalogDigest": catalog_digest_value,
        "lane": key.lane,
        "profile": key.profile,
        "selector": key.selector,
        "startedAt": started_at,
        "endedAt": ended_at,
        "status": status,
        "evidenceArtifact": evidence_artifact,
    }
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n")


def _read_results(path: Path) -> list[dict[str, object]]:
    if not path.is_file():
        raise ReceiptError(f"selector results do not exist: {path}")
    results = []
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not raw_line.strip():
            raise ReceiptError(f"selector results line {line_number} is empty")
        try:
            value = json.loads(raw_line)
        except json.JSONDecodeError as error:
            raise ReceiptError(f"selector results line {line_number} is invalid JSON") from error
        if not isinstance(value, dict) or set(value) != RESULT_FIELDS:
            raise ReceiptError(
                f"selector results line {line_number} must contain exactly {sorted(RESULT_FIELDS)}"
            )
        results.append(value)
    return results


def fill_missing_results(
    catalog: Catalog,
    results_path: Path,
    candidate_sha: str,
    artifact_root: Path,
) -> None:
    existing = _read_results(results_path) if results_path.is_file() else []
    existing_keys = {
        SelectorKey(str(row["lane"]), str(row["profile"]), str(row["selector"]))
        for row in existing
    }
    now = datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
    for key in catalog.ordered_selectors():
        if key in existing_keys:
            continue
        slug = re.sub(r"[^A-Za-z0-9._-]+", "-", f"{key.profile}-{key.selector}").strip("-")
        evidence = f"logs/not-run-{slug}.log"
        evidence_path = artifact_root / evidence
        evidence_path.parent.mkdir(parents=True, exist_ok=True)
        evidence_path.write_text(
            f"candidate_sha={candidate_sha}\n"
            f"catalog_digest={catalog.digest}\n"
            f"profile={key.profile}\n"
            f"lane={key.lane}\n"
            f"selector={key.selector}\n"
            "status=not_run\n",
            encoding="utf-8",
        )
        record_result(
            results_path,
            candidate_sha=candidate_sha,
            catalog_digest_value=catalog.digest,
            key=key,
            started_at=now,
            ended_at=now,
            status="not_run",
            evidence_artifact=evidence,
        )


def build_receipt(catalog: Catalog, results: Iterable[dict[str, object]], candidate_sha: str) -> dict:
    candidate_sha = _validate_sha(candidate_sha)
    by_key: dict[SelectorKey, dict[str, object]] = {}
    for index, result in enumerate(results, start=1):
        if result["candidateSha"] != candidate_sha:
            raise ReceiptError(f"result {index} candidate SHA does not match {candidate_sha}")
        if result["catalogDigest"] != catalog.digest:
            raise ReceiptError(f"result {index} catalog digest does not match {catalog.digest}")
        key = SelectorKey(
            lane=str(result["lane"]),
            profile=str(result["profile"]),
            selector=str(result["selector"]),
        )
        if key not in catalog.loops_by_selector:
            raise ReceiptError(f"result {index} names an extra selector {key}")
        if key in by_key:
            raise ReceiptError(f"duplicate result for selector {key}")
        started = _parse_timestamp(result["startedAt"], f"result {index} startedAt")
        ended = _parse_timestamp(result["endedAt"], f"result {index} endedAt")
        if ended < started:
            raise ReceiptError(f"result {index} endedAt precedes startedAt")
        if result["status"] not in TERMINAL_STATUSES:
            raise ReceiptError(f"result {index} has unknown status {result['status']!r}")
        _validate_evidence(result["evidenceArtifact"], f"result {index} evidenceArtifact")
        by_key[key] = result

    missing = set(catalog.loops_by_selector) - set(by_key)
    if missing:
        raise ReceiptError(f"missing selector results: {sorted(missing)}")

    selector_rows = []
    for key in catalog.ordered_selectors():
        result = by_key[key]
        selector_rows.append(
            {
                "lane": key.lane,
                "profile": key.profile,
                "selector": key.selector,
                "loopIds": list(catalog.loops_by_selector[key]),
                "startedAt": result["startedAt"],
                "endedAt": result["endedAt"],
                "status": result["status"],
                "evidenceArtifact": result["evidenceArtifact"],
            }
        )

    loop_rows = []
    for loop_id in sorted(EXPECTED_LOOPS):
        mapped = [
            row
            for row in selector_rows
            if loop_id in row["loopIds"]
        ]
        status = "passed" if mapped and all(row["status"] == "passed" for row in mapped) else "failed"
        loop_rows.append(
            {
                "loopId": loop_id,
                "status": status,
                "selectors": [
                    {
                        "lane": row["lane"],
                        "profile": row["profile"],
                        "selector": row["selector"],
                    }
                    for row in mapped
                ],
            }
        )

    selector_counts = Counter(row["status"] for row in selector_rows)
    loop_counts = Counter(row["status"] for row in loop_rows)
    return {
        "schemaVersion": 1,
        "candidateSha": candidate_sha,
        "catalogDigest": catalog.digest,
        "generatedAt": datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
        "selectors": selector_rows,
        "loops": loop_rows,
        "summary": {
            "selectorCount": len(selector_rows),
            "selectorStatuses": dict(sorted(selector_counts.items())),
            "loopCount": len(loop_rows),
            "loopStatuses": dict(sorted(loop_counts.items())),
        },
    }


def write_receipt(path: Path, receipt: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(temporary, path)


def validate_receipt(
    catalog: Catalog,
    receipt: object,
    candidate_sha: str,
    artifact_root: Path | None = None,
) -> None:
    candidate_sha = _validate_sha(candidate_sha)
    if not isinstance(receipt, dict):
        raise ReceiptError("receipt must be a JSON object")
    expected_top_level = {
        "schemaVersion",
        "candidateSha",
        "catalogDigest",
        "generatedAt",
        "selectors",
        "loops",
        "summary",
    }
    if set(receipt) != expected_top_level:
        raise ReceiptError(f"receipt must contain exactly {sorted(expected_top_level)}")
    if receipt["schemaVersion"] != 1:
        raise ReceiptError("receipt schemaVersion must be 1")
    if receipt["candidateSha"] != candidate_sha:
        raise ReceiptError("receipt candidate SHA mismatch")
    if receipt["catalogDigest"] != catalog.digest:
        raise ReceiptError("receipt catalog digest mismatch")
    _parse_timestamp(receipt["generatedAt"], "generatedAt")

    selector_rows = receipt["selectors"]
    expected_selector_count = len(catalog.loops_by_selector)
    if not isinstance(selector_rows, list) or len(selector_rows) != expected_selector_count:
        raise ReceiptError(
            f"receipt must contain exactly {expected_selector_count} selector rows"
        )
    reconstructed_results = []
    for index, row in enumerate(selector_rows, start=1):
        expected_fields = {
            "lane",
            "profile",
            "selector",
            "loopIds",
            "startedAt",
            "endedAt",
            "status",
            "evidenceArtifact",
        }
        if not isinstance(row, dict) or set(row) != expected_fields:
            raise ReceiptError(f"selector row {index} has an invalid shape")
        key = SelectorKey(row["lane"], row["profile"], row["selector"])
        expected_loop_ids = catalog.loops_by_selector.get(key)
        if expected_loop_ids is None:
            raise ReceiptError(f"selector row {index} names an extra selector {key}")
        if row["loopIds"] != list(expected_loop_ids):
            raise ReceiptError(f"selector row {index} loopIds do not match the catalog")
        evidence = _validate_evidence(
            row["evidenceArtifact"], f"selector row {index} evidenceArtifact"
        )
        if artifact_root is not None and not (artifact_root / evidence).is_file():
            raise ReceiptError(
                f"selector row {index} evidence artifact does not exist: {artifact_root / evidence}"
            )
        reconstructed_results.append(
            {
                "candidateSha": candidate_sha,
                "catalogDigest": catalog.digest,
                "lane": row["lane"],
                "profile": row["profile"],
                "selector": row["selector"],
                "startedAt": row["startedAt"],
                "endedAt": row["endedAt"],
                "status": row["status"],
                "evidenceArtifact": row["evidenceArtifact"],
            }
        )
    rebuilt = build_receipt(catalog, reconstructed_results, candidate_sha)
    if receipt["loops"] != rebuilt["loops"]:
        raise ReceiptError("receipt loop aggregation does not match selector results")
    if receipt["summary"] != rebuilt["summary"]:
        raise ReceiptError("receipt summary does not match selector results")
    non_passing = [
        f"{row['profile']}:{row['selector']}={row['status']}"
        for row in selector_rows
        if row["status"] != "passed"
    ]
    if non_passing:
        raise ReceiptError(f"mandatory selectors are not all passed: {', '.join(non_passing)}")
    if any(row["status"] != "passed" for row in receipt["loops"]):
        raise ReceiptError("all workspace MVP package rows must be passed")


def _load_receipt(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise ReceiptError(f"receipt does not exist: {path}") from error
    except json.JSONDecodeError as error:
        raise ReceiptError(f"receipt is invalid JSON: {path}") from error


def _shell_command_at(text: str, offset: int) -> str:
    """The rest of one backslash-continued shell command starting at `offset`."""
    lines = []
    for line in text[offset:].splitlines():
        lines.append(line)
        if not line.rstrip().endswith("\\"):
            break
    return "\n".join(lines)


def _pipeline_producers(
    text: str, source_root: Path | None = None,
) -> tuple[set[str], set[str], set[str]]:
    """What a pipeline names as value-loop producers: the profiles and
    selectors its `record` calls pass literally, the selectors it loops over
    through a heredoc or a checked-in shard manifest, and the profiles its
    catalog-driven router routes."""
    profiles: set[str] = set()
    selectors: set[str] = set()
    for match in re.finditer(r"value_loop_receipt\.py record\b", text):
        command = _shell_command_at(text, match.end())
        profiles.update(re.findall(r"--profile[ \t]+(\S+)", command))
        selectors.update(re.findall(r"--selector[ \t]+(\S+)", command))
    for match in re.finditer(r"<<'([A-Z_]*VALUE_LOOPS?)'\n(.*?)\n\s*\1\b", text, re.DOTALL):
        for line in match.group(2).splitlines():
            row = line.strip()
            if row:
                selectors.add(row.split("|", 1)[0].strip())
    for match in re.finditer(r"record_playwright_shard_results\.py\b", text):
        command = _shell_command_at(text, match.end())
        manifests = re.findall(r"--selectors[ \t]+(\S+)", command)
        if len(manifests) != 1:
            raise ReceiptError("shard producer must name one literal selector manifest")
        path = manifests[0].strip("\"'")
        if not path.startswith("/workspace/") or "$" in path:
            raise ReceiptError(f"shard selector manifest must be a literal /workspace path: {path}")
        relative = Path(path.removeprefix("/workspace/"))
        if ".." in relative.parts:
            raise ReceiptError(f"shard selector manifest escapes the checkout: {path}")
        root = source_root if source_root is not None else Path(__file__).resolve().parents[1]
        try:
            lines = (root / relative).read_text(encoding="utf-8").splitlines()
        except OSError as error:
            raise ReceiptError(f"cannot read shard selector manifest {path}: {error}") from error
        manifest_selectors: set[str] = set()
        for row in lines:
            parts = row.split("|")
            if len(parts) != 2 or not all(parts) or parts[0] in manifest_selectors:
                raise ReceiptError(f"invalid shard selector manifest row: {row!r}")
            manifest_selectors.add(parts[0])
        selectors.update(manifest_selectors)
    router = re.search(
        r"read -r profile lane selector loop_ids source; do\n(.*?)\n\s*esac\b",
        text,
        re.DOTALL,
    )
    if router is None:
        raise ReceiptError("pipeline has no catalog-driven value-loop router")
    routed: set[str] = set()
    for arm in re.finditer(r"^[ \t]+([a-z0-9|-]+)\)[ \t]*$", router.group(1), re.MULTILINE):
        routed.update(arm.group(1).split("|"))
    strip = lambda values: {value.strip("\"'") for value in values}
    literal = lambda values: {value for value in strip(values) if "$" not in value}
    return literal(profiles), literal(selectors), routed


def verify_producers(catalog: Catalog, pipeline: Path, source_root: Path | None = None) -> None:
    """Fail closed when a producer names a profile or selector the catalog
    does not carry.

    `build_receipt` rejects such a row as an extra selector, but only at
    finalization, after the lane has run. This is the same invariant checked
    before a Cloud Build run pays for it.
    """
    if not pipeline.is_file():
        raise ReceiptError(f"pipeline does not exist: {pipeline}")
    profiles, selectors, routed = _pipeline_producers(pipeline.read_text(encoding="utf-8"), source_root)
    catalog_profiles = {key.profile for key in catalog.loops_by_selector}
    catalog_selectors = {key.selector for key in catalog.loops_by_selector}
    extra_profiles = sorted(profiles - catalog_profiles)
    if extra_profiles:
        raise ReceiptError(
            f"{pipeline} records profiles the catalog does not carry: {extra_profiles}"
        )
    extra_selectors = sorted(selectors - catalog_selectors)
    if extra_selectors:
        raise ReceiptError(
            f"{pipeline} records selectors the catalog does not carry: {extra_selectors}"
        )
    required_playwright = {
        key.selector for key in catalog.loops_by_selector if key.lane == "playwright"
    }
    missing_playwright = sorted(required_playwright - selectors)
    if missing_playwright:
        raise ReceiptError(
            f"{pipeline} does not produce every catalogued Playwright selector: "
            f"{missing_playwright}"
        )
    if routed != set(PROFILE_LANES):
        raise ReceiptError(
            "the value-loop router and this tool disagree on the execution profiles: "
            f"router={sorted(routed)} tool={sorted(PROFILE_LANES)}"
        )


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    def catalog_command(name: str) -> argparse.ArgumentParser:
        command = subparsers.add_parser(name)
        command.add_argument("--catalog", type=Path, required=True)
        command.add_argument("--generation-root", type=Path)
        return command

    catalog_command("plan")
    catalog_command("digest")

    producers = catalog_command("producers")
    producers.add_argument("--pipeline", type=Path, required=True)

    record = subparsers.add_parser("record")
    record.add_argument("--results", type=Path, required=True)
    record.add_argument("--candidate-sha", required=True)
    record.add_argument("--catalog-digest", required=True)
    record.add_argument("--lane", required=True)
    record.add_argument("--profile", required=True)
    record.add_argument("--selector", required=True)
    record.add_argument("--started-at", required=True)
    record.add_argument("--ended-at", required=True)
    record.add_argument("--status", choices=sorted(TERMINAL_STATUSES), required=True)
    record.add_argument("--evidence-artifact", required=True)

    finalize = catalog_command("finalize")
    finalize.add_argument("--results", type=Path, required=True)
    finalize.add_argument("--candidate-sha", required=True)
    finalize.add_argument("--output", type=Path, required=True)
    finalize.add_argument(
        "--fill-missing",
        action="store_true",
        help="materialize missing selector rows as non-passing not_run results",
    )

    validate = catalog_command("validate")
    validate.add_argument("--receipt", type=Path, required=True)
    validate.add_argument("--candidate-sha", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        if args.command == "record":
            record_result(
                args.results,
                candidate_sha=args.candidate_sha,
                catalog_digest_value=args.catalog_digest,
                key=SelectorKey(args.lane, args.profile, args.selector),
                started_at=args.started_at,
                ended_at=args.ended_at,
                status=args.status,
                evidence_artifact=args.evidence_artifact,
            )
            return 0

        catalog = load_catalog(args.catalog, args.generation_root)
        if args.command == "plan":
            for key in catalog.ordered_selectors():
                print(
                    "\t".join(
                        [
                            key.profile,
                            key.lane,
                            key.selector,
                            ",".join(catalog.loops_by_selector[key]),
                            catalog.source_by_selector[key],
                        ]
                    )
                )
            return 0
        if args.command == "producers":
            verify_producers(catalog, args.pipeline)
            print(f"value-loop producers match the catalog: {args.pipeline}")
            return 0
        if args.command == "digest":
            print(catalog.digest)
            return 0
        if args.command == "finalize":
            if args.fill_missing:
                fill_missing_results(
                    catalog,
                    args.results,
                    args.candidate_sha,
                    args.output.parent,
                )
            receipt = build_receipt(catalog, _read_results(args.results), args.candidate_sha)
            write_receipt(args.output, receipt)
            validate_receipt(catalog, receipt, args.candidate_sha, args.output.parent)
            return 0
        if args.command == "validate":
            validate_receipt(
                catalog,
                _load_receipt(args.receipt),
                args.candidate_sha,
                args.receipt.parent,
            )
            return 0
        raise AssertionError(f"unhandled command {args.command}")
    except ReceiptError as error:
        print(f"value-loop receipt error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
