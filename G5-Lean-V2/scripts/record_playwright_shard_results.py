#!/usr/bin/env python3
"""Record value-loop browser selectors proved by the exact-candidate shard reports.

The unfiltered Playwright inventory is compared with the executed reports (one
per planned broad shard, however many) so a grep exclusion, skipped test, retry,
or missing test cannot qualify a selector.
The Preview selector combines its ordinary shard tests with the provider-only
transfer test; all other selectors are proved entirely by the broad shards.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path

from value_loop_receipt import ReceiptError, SelectorKey, load_catalog, record_result


PREVIEW_SELECTOR = "e2e/flows/workspace-preview.spec.ts"
TRANSFER_TITLE = "M9/M12 transfer fences live chat"


def _read_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ReceiptError(f"cannot read Playwright JSON report {path}: {error}") from error
    if (
        not isinstance(value, dict)
        or not isinstance(value.get("suites"), list)
        or not isinstance(value.get("errors"), list)
    ):
        raise ReceiptError(f"invalid Playwright JSON report: {path}")
    if value.get("errors"):
        raise ReceiptError(f"Playwright report has collection errors: {path}")
    return value


def _report_tests(path: Path, selectors_by_basename: dict[str, str]) -> dict[str, dict[str, dict]]:
    report = _read_json(path)
    found: dict[str, dict[str, dict]] = defaultdict(dict)

    def walk(suite: dict) -> None:
        if not isinstance(suite, dict):
            raise ReceiptError(f"invalid Playwright suite in {path}")
        for spec in suite.get("specs", []):
            if not isinstance(spec, dict):
                raise ReceiptError(f"invalid Playwright spec in {path}")
            file = spec.get("file")
            if not isinstance(file, str):
                raise ReceiptError(f"Playwright spec has no file in {path}")
            selector = selectors_by_basename.get(Path(file).name)
            if selector is None:
                continue
            spec_id = spec.get("id")
            if not isinstance(spec_id, str) or not spec_id:
                raise ReceiptError(f"Playwright spec has no stable ID in {path}")
            tests = spec.get("tests")
            chromium = [test for test in tests or [] if test.get("projectName") == "chromium"]
            if len(chromium) != 1:
                raise ReceiptError(f"{selector}:{spec_id} must contain one Chromium test in {path}")
            if spec_id in found[selector]:
                raise ReceiptError(f"duplicate {selector}:{spec_id} in {path}")
            found[selector][spec_id] = {"spec": spec, "test": chromium[0]}
        for child in suite.get("suites", []):
            walk(child)

    for suite in report["suites"]:
        walk(suite)
    return found


def _passed_result(selector: str, spec_id: str, item: dict, path: Path) -> tuple[datetime, datetime]:
    spec, test = item["spec"], item["test"]
    results = test.get("results")
    if (
        spec.get("ok") is not True
        or test.get("expectedStatus") != "passed"
        or test.get("status") != "expected"
        or not isinstance(results, list)
        or len(results) != 1
        or results[0].get("status") != "passed"
        or results[0].get("retry") != 0
        or results[0].get("errors") != []
    ):
        raise ReceiptError(f"{selector}:{spec_id} did not pass once without skip or retry in {path}")
    result = results[0]
    try:
        start = datetime.fromisoformat(result["startTime"].replace("Z", "+00:00"))
        duration = result["duration"]
    except (KeyError, AttributeError, TypeError, ValueError) as error:
        raise ReceiptError(f"{selector}:{spec_id} has invalid timing in {path}") from error
    if start.tzinfo is None or start.utcoffset() != timedelta(0) or not isinstance(duration, (int, float)) or duration < 0:
        raise ReceiptError(f"{selector}:{spec_id} has invalid UTC timing in {path}")
    return start, start + timedelta(milliseconds=duration)


def record_shard_results(
    *, catalog_path: Path, selectors_path: Path, inventory_path: Path,
    shard_paths: list[Path], provider_path: Path, candidate_sha: str,
    results_path: Path, artifact_root: Path,
) -> None:
    catalog = load_catalog(catalog_path)
    lines = selectors_path.read_text(encoding="utf-8").splitlines()
    selectors: dict[str, str] = {}
    for line in lines:
        parts = line.split("|")
        if len(parts) != 2 or not all(parts):
            raise ReceiptError(f"invalid browser selector row: {line!r}")
        selector, source = parts
        key = SelectorKey("playwright", "playwright-workspace-vm", selector)
        if selector in selectors or catalog.source_by_selector.get(key) != source:
            raise ReceiptError(f"browser selector/source disagrees with catalog: {line!r}")
        selectors[selector] = source
    provider_only = {
        "e2e/flows/chat-restart-continuity.spec.ts",
        "e2e/flows/native-requests.spec.ts",
    }
    expected_selectors = {
        key.selector for key in catalog.loops_by_selector
        if key.lane == "playwright" and key.selector not in provider_only
    }
    if set(selectors) != expected_selectors:
        raise ReceiptError(
            "shard receipt must name every catalogued shared selector and Preview; "
            f"missing={sorted(expected_selectors - set(selectors))} "
            f"extra={sorted(set(selectors) - expected_selectors)}"
        )
    by_basename = {Path(selector).name: selector for selector in selectors}
    if len(by_basename) != len(selectors):
        raise ReceiptError("browser selector basenames must be unique")

    inventory = _report_tests(inventory_path, by_basename)
    shards = [_report_tests(path, by_basename) for path in shard_paths]
    provider = _report_tests(provider_path, by_basename)
    if set(provider) != {PREVIEW_SELECTOR} or len(provider[PREVIEW_SELECTOR]) != 1:
        raise ReceiptError("provider Preview run must contain exactly the transfer test")
    provider_item = next(iter(provider[PREVIEW_SELECTOR].values()))
    if TRANSFER_TITLE not in provider_item["spec"].get("title", ""):
        raise ReceiptError("provider Preview run did not execute the transfer test")

    pending = []
    for selector in selectors:
        expected = inventory.get(selector, {})
        if not expected:
            raise ReceiptError(f"unfiltered inventory has no tests for {selector}")
        for spec_id, item in expected.items():
            if item["test"].get("expectedStatus") != "passed":
                raise ReceiptError(f"unfiltered inventory skips {selector}:{spec_id}")
        observed: dict[str, tuple[dict, Path]] = {}
        for path, report in zip(shard_paths, shards):
            for spec_id, item in report.get(selector, {}).items():
                if spec_id in observed:
                    raise ReceiptError(f"duplicate shard test {selector}:{spec_id}")
                observed[spec_id] = (item, path)
        if selector == PREVIEW_SELECTOR:
            for spec_id, item in provider[selector].items():
                if spec_id in observed:
                    raise ReceiptError(f"duplicate broad/provider Preview test {selector}:{spec_id}")
                observed[spec_id] = (item, provider_path)
        if set(observed) != set(expected):
            raise ReceiptError(
                f"{selector} inventory/report mismatch: missing={sorted(set(expected) - set(observed))}, "
                f"extra={sorted(set(observed) - set(expected))}"
            )
        timings = [_passed_result(selector, spec_id, *observed[spec_id]) for spec_id in sorted(expected)]
        pending.append((selector, expected, observed, min(start for start, _ in timings), max(end for _, end in timings)))

    # Only write passing rows after every selector is proved, so a parsing
    # failure cannot leave a partially qualified browser receipt.
    artifact_root.joinpath("logs").mkdir(parents=True, exist_ok=True)
    for selector, expected, observed, started, ended in pending:
        evidence_ref = f"logs/playwright-{Path(selector).stem}-shard.json"
        evidence = {
            "candidateSha": candidate_sha,
            "catalogDigest": catalog.digest,
            "selector": selector,
            "source": selectors[selector],
            "inventoryArtifact": "value-loop-matrix-artifacts/playwright-value-loop-inventory.json",
            "inventorySha256": hashlib.sha256(inventory_path.read_bytes()).hexdigest(),
            "tests": [
                {
                    "id": spec_id,
                    "title": expected[spec_id]["spec"]["title"],
                    "report": str(observed[spec_id][1]),
                    "reportArtifact": (
                        f"playwright-test-results-shard-{shard_paths.index(observed[spec_id][1]) + 1}.tar.gz"
                        if observed[spec_id][1] in shard_paths
                        else "provider-agent-playwright-test-results.tar.gz"
                    ),
                    "reportSha256": hashlib.sha256(observed[spec_id][1].read_bytes()).hexdigest(),
                    "status": "passed",
                    "retry": 0,
                }
                for spec_id in sorted(expected)
            ],
        }
        (artifact_root / evidence_ref).write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        record_result(
            results_path,
            candidate_sha=candidate_sha,
            catalog_digest_value=catalog.digest,
            key=SelectorKey("playwright", "playwright-workspace-vm", selector),
            started_at=started.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
            ended_at=ended.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
            status="passed",
            evidence_artifact=evidence_ref,
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--selectors", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--shard-report", action="append", type=Path, required=True)
    parser.add_argument("--provider-report", type=Path, required=True)
    parser.add_argument("--candidate-sha", required=True)
    parser.add_argument("--results", type=Path, required=True)
    parser.add_argument("--artifact-root", type=Path, required=True)
    args = parser.parse_args()
    try:
        record_shard_results(
            catalog_path=args.catalog, selectors_path=args.selectors,
            inventory_path=args.inventory, shard_paths=args.shard_report,
            provider_path=args.provider_report, candidate_sha=args.candidate_sha,
            results_path=args.results, artifact_root=args.artifact_root,
        )
    except (ReceiptError, OSError) as error:
        parser.exit(1, f"browser shard receipt rejected: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
