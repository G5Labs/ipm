#!/usr/bin/env python3
"""Place whole Playwright files on N shards by measured duration, proving exact test-ID coverage.

Ontology: g5.quality.delivery.ci.browser-shards. With fullyParallel off, Playwright
runs each shard's files in discovery order, handing each file to the earliest free
worker; `--test-list` only filters. The planner replays that dispatch with committed
per-file durations and picks the partition whose slowest shard is predicted to finish
first. Whole files keep each spec's serial hooks and per-test fixtures intact.

Playwright groups files by worker configuration before name, so a file with its own
worker-scoped options (for example `test.use({ channel })`) runs after the others.
List reports do not expose that grouping; the replay uses name order, so such a
file's prediction is approximate, off by at most its own weight.
"""

from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path


# Assignment budget for the heaviest files, whose every placement is searched;
# the rest are placed greedily. 12 files on two shards (2^11), 8 on three (3^7).
EXHAUSTIVE_ASSIGNMENTS = 2200
DEFAULT_SHARDS = 2
PLAN_KEYS = {
    "originalFiles",
    "plannedFiles",
    "plannedIds",
    "predictedShardMs",
    "workers",
    "fileWeightsMs",
    "unmeasuredFiles",
}


class PlanError(ValueError):
    pass


def read_report(path: Path) -> dict:
    try:
        report = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PlanError(f"cannot read Playwright JSON report {path}: {error}") from error
    if (
        not isinstance(report, dict)
        or not isinstance(report.get("suites"), list)
        or not isinstance(report.get("errors"), list)
        or report["errors"]
    ):
        raise PlanError(f"invalid Playwright JSON report: {path}")
    return report


def chromium_specs(report: dict, path: Path):
    """Yield (basename, spec, chromium test) in report order."""

    def walk(suite: dict):
        for spec in suite.get("specs", []):
            chromium = [test for test in spec.get("tests", []) if test.get("projectName") == "chromium"]
            if not chromium:
                continue
            if len(chromium) != 1 or not isinstance(spec.get("id"), str):
                raise PlanError(f"invalid Chromium test in {path}")
            file = spec.get("file")
            if not isinstance(file, str) or not file.endswith(".spec.ts"):
                raise PlanError(f"invalid Chromium spec file in {path}")
            yield Path(file).name, spec, chromium[0]
        for child in suite.get("suites", []):
            yield from walk(child)

    for suite in report["suites"]:
        yield from walk(suite)


def report_ids(path: Path) -> dict[str, set[str]]:
    """Map each file to its Chromium test IDs; insertion order is report order."""
    by_file: dict[str, set[str]] = {}
    seen_ids: set[str] = set()
    for basename, spec, _test in chromium_specs(read_report(path), path):
        spec_id = spec["id"]
        if spec_id in seen_ids:
            raise PlanError(f"duplicate test ID {spec_id} in {path}")
        seen_ids.add(spec_id)
        by_file.setdefault(basename, set()).add(spec_id)
    if not seen_ids:
        raise PlanError(f"Playwright report has no Chromium tests: {path}")
    return by_file


def load_durations(path: Path) -> dict[str, int]:
    try:
        table = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PlanError(f"cannot read file durations {path}: {error}") from error
    files = table.get("files") if isinstance(table, dict) else None
    if not isinstance(files, dict) or not files or not all(
        isinstance(name, str) and name.endswith(".spec.ts")
        and isinstance(ms, int) and not isinstance(ms, bool) and ms >= 0
        for name, ms in files.items()
    ):
        raise PlanError(f"invalid file durations {path}: expected files: {{<name>.spec.ts: <ms>}}")
    return files


def predict_ms(files: list[str], order: dict[str, int], weights: dict[str, int], workers: int) -> int:
    """Replay Playwright's dispatch of one shard: discovery order, earliest free worker."""
    free = [0] * workers
    for file in sorted(files, key=order.__getitem__):
        slot = free.index(min(free))
        free[slot] += weights[file]
    return max(free)


def exhaustive_files(shards: int) -> int:
    """How many of the heaviest files are placed exhaustively: the heaviest stays
    on shard 1, each other one tries every shard within the assignment budget."""
    exponent = 0
    while shards ** (exponent + 1) <= EXHAUSTIVE_ASSIGNMENTS:
        exponent += 1
    return exponent + 1


def search(order: dict[str, int], weights: dict[str, int], workers: int, shards: int = DEFAULT_SHARDS) -> list[list[str]]:
    by_weight = sorted(order, key=lambda file: (-weights[file], order[file]))
    heavy, light = by_weight[:exhaustive_files(shards)], by_weight[exhaustive_files(shards):]
    best: tuple[tuple[int, int, int], list[list[str]]] | None = None
    # The heaviest file stays on shard 1; the permuted assignments are equivalent.
    for mask in range(shards ** max(len(heavy) - 1, 0)):
        placed: list[list[str]] = [[heavy[0]] if heavy else []] + [[] for _ in range(shards - 1)]
        for digit, file in enumerate(heavy[1:]):
            placed[(mask // shards ** digit) % shards].append(file)
        for file in light:
            grown = [predict_ms(shard + [file], order, weights, workers) for shard in placed]
            placed[grown.index(min(grown))].append(file)
        predicted = [predict_ms(shard, order, weights, workers) for shard in placed]
        key = (max(predicted), max(predicted) - min(predicted), mask)
        if best is None or key < best[0]:
            best = (key, [list(shard) for shard in placed])
    assert best is not None
    return best[1]


def make_plan(
    original_paths: list[Path], output_dir: Path, durations_path: Path, workers: int = 2,
    shards: int = DEFAULT_SHARDS,
) -> None:
    if shards < 2 or len(original_paths) != shards:
        raise PlanError(f"exactly {shards} original shard inventories are required (one per planned shard)")
    if workers < 1:
        raise PlanError("at least one worker per shard is required")
    original = [report_ids(path) for path in original_paths]
    seen_files: set[str] = set()
    seen_ids: set[str] = set()
    for files in original:
        duplicate_files = seen_files & set(files)
        if duplicate_files:
            raise PlanError(f"test files occur on more than one original shard: {sorted(duplicate_files)}")
        seen_files |= set(files)
        ids = set().union(*files.values())
        duplicate_ids = seen_ids & ids
        if duplicate_ids:
            raise PlanError(f"test IDs occur on more than one original shard: {sorted(duplicate_ids)}")
        seen_ids |= ids

    # Playwright's --shard split is contiguous over discovery order.
    inventory = {file: ids for files in original for file, ids in files.items()}
    order = {file: index for index, file in enumerate(inventory)}
    measured = load_durations(durations_path)
    known = [measured[file] for file in inventory if file in measured]
    if not known:
        raise PlanError("no file in the broad inventory has a measured duration")
    default_ms = round(statistics.fmean(known))
    unmeasured = sorted(file for file in inventory if file not in measured)
    weights = {file: measured.get(file, default_ms) for file in inventory}

    shard_files = search(order, weights, workers, shards)
    planned = [{file: inventory[file] for file in sorted(files)} for files in shard_files]
    predicted = [predict_ms(list(files), order, weights, workers) for files in planned]

    output_dir.mkdir(parents=True, exist_ok=True)
    plan = {
        "originalFiles": [sorted(files) for files in original],
        "plannedFiles": [sorted(files) for files in planned],
        "plannedIds": [sorted(set().union(*files.values())) for files in planned],
        "predictedShardMs": predicted,
        "workers": workers,
        "fileWeightsMs": {file: weights[file] for file in sorted(weights)},
        "unmeasuredFiles": unmeasured,
    }
    for index, files in enumerate(planned, start=1):
        (output_dir / f"shard-{index}.test-list").write_text(
            "".join(f"{file}\n" for file in sorted(files)), encoding="utf-8"
        )
    (output_dir / "plan.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(
        "Playwright shard plan: "
        + ", ".join(
            f"shard {index}: {len(files)} files, {len(plan['plannedIds'][index - 1])} tests, "
            f"predicted {predicted[index - 1] / 60000:.1f} min on {workers} workers"
            for index, files in enumerate(planned, start=1)
        )
    )
    if unmeasured:
        print(
            f"Playwright shard plan: {len(unmeasured)} unmeasured file(s) weighted at the measured mean "
            f"{default_ms} ms; refresh {durations_path.name}: {', '.join(unmeasured)}"
        )


def verify_plan(plan_path: Path, report_paths: list[Path]) -> None:
    try:
        plan = json.loads(plan_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PlanError(f"cannot read Playwright shard plan {plan_path}: {error}") from error
    if (
        not isinstance(plan, dict) or set(plan) != PLAN_KEYS
        or not isinstance(plan["plannedFiles"], list) or not isinstance(plan["plannedIds"], list)
        or len(plan["plannedFiles"]) != len(plan["plannedIds"])
    ):
        raise PlanError("invalid Playwright shard plan")
    if len(report_paths) != len(plan["plannedFiles"]):
        raise PlanError(f"exactly {len(plan['plannedFiles'])} planned shard reports are required")
    for index, path in enumerate(report_paths):
        actual = report_ids(path)
        if sorted(actual) != plan["plannedFiles"][index]:
            raise PlanError(f"shard {index + 1} file inventory changed: {path}")
        if sorted(set().union(*actual.values())) != plan["plannedIds"][index]:
            raise PlanError(f"shard {index + 1} test-ID inventory changed: {path}")
    print("Playwright shard plan verified: every original Chromium test has one planned owner")


def summary_first_attempts(path: Path) -> dict[str, int] | None:
    """Read a report-shard-durations.js summary (its --summary file or its logged
    G5_SHARD_DURATIONS line), or None if the file is a Playwright report."""
    try:
        text = path.read_text(encoding="utf-8").strip()
        data = json.loads(text.removeprefix("G5_SHARD_DURATIONS "))
    except (OSError, json.JSONDecodeError) as error:
        raise PlanError(f"cannot read {path}: {error}") from error
    if not isinstance(data, dict) or data.get("kind") != "g5-shard-durations":
        return None
    files = data.get("files")
    if not isinstance(files, dict):
        raise PlanError(f"invalid shard duration summary {path}")
    per_file = {}
    for name, entry in files.items():
        if not name.endswith(".spec.ts"):
            continue  # never a placed broad-lane file (for example auth.setup.ts)
        first = entry.get("firstAttemptMs") if isinstance(entry, dict) else None
        if not isinstance(first, int) or first < 0:
            raise PlanError(f"invalid firstAttemptMs for {name} in {path}")
        per_file[Path(name).name] = first
    return per_file


def measure_durations(report_paths: list[Path], output: Path, sources: list[str]) -> None:
    """Rebuild the duration table: per-file median of first-attempt time across executed
    Playwright reports or the shard summaries the CI logs and exports."""
    samples: dict[str, list[int]] = {}
    for path in report_paths:
        summary = summary_first_attempts(path)
        if summary is not None:
            for basename, total in summary.items():
                samples.setdefault(basename, []).append(total)
            continue
        per_file: dict[str, int] = {}
        for basename, _spec, test in chromium_specs(read_report(path), path):
            results = test.get("results") or []
            if not results:
                continue
            duration = results[0].get("duration")
            if not isinstance(duration, (int, float)) or duration < 0:
                raise PlanError(f"invalid result duration for {basename} in {path}")
            per_file[basename] = per_file.get(basename, 0) + int(duration)
        for basename, total in per_file.items():
            samples.setdefault(basename, []).append(total)
    if not samples:
        raise PlanError("executed reports contain no Chromium results")
    table = {
        "comment": "Median first-attempt Chromium time per file (ms) from executed CI shard reports; "
        "regenerate with scripts/plan_playwright_shards.py durations.",
        "sources": sources,
        "files": {name: round(statistics.median(values)) for name, values in sorted(samples.items())},
    }
    output.write_text(json.dumps(table, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {len(table['files'])} file durations from {len(report_paths)} report(s) to {output}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    make = sub.add_parser("plan")
    make.add_argument("--original", type=Path, action="append", required=True)
    make.add_argument("--output-dir", type=Path, required=True)
    make.add_argument("--durations", type=Path, required=True)
    make.add_argument("--workers", type=int, default=2)
    make.add_argument("--shards", type=int, default=DEFAULT_SHARDS, help="planned shard count; one --original per shard")
    verify = sub.add_parser("verify")
    verify.add_argument("--plan", type=Path, required=True)
    verify.add_argument("--report", type=Path, action="append", required=True)
    measure = sub.add_parser("durations")
    measure.add_argument("--report", type=Path, action="append", required=True)
    measure.add_argument("--output", type=Path, required=True)
    measure.add_argument("--source", action="append", default=[], help="build or report identity to record")
    args = parser.parse_args()
    try:
        if args.command == "plan":
            make_plan(args.original, args.output_dir, args.durations, args.workers, args.shards)
        elif args.command == "verify":
            verify_plan(args.plan, args.report)
        else:
            measure_durations(args.report, args.output, args.source)
    except PlanError as error:
        parser.exit(1, f"Playwright shard plan rejected: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
