#!/usr/bin/env python3
"""
Verify integrity of intent records and grounding edges after regeneration.

Checks:
1. Label counts across all spec files
2. Intent record JSONL matches actual labels in specs
3. Grounding edge validity across ALL four subdirectories of intent_grounding/
   (ontology, views, code, chat) — orphan files AND missing source/target labels
4. Coverage stats

CLI flags:
  --allow-missing-specs  Exit successfully when the repository intentionally
                         carries no ontology specs directory. An existing but
                         empty directory remains a failure, and all validation
                         stays fail-closed when any spec is present.
  --strict-labels      Exit non-zero when any missing-source/target-label error
                       is recorded. This is the class of error that catches the
                       rev-7 regression family (edges referencing labels that
                       have been deleted from their referenced spec/view file).
  --strict-orphans     Additionally fail on orphan-source/target-file errors
                       (file referenced by an edge no longer exists on disk).
  --strict-all         Fail on every recorded error class, including
                       intent-record drift. Suitable once the legacy 30+ drift
                       entries have been cleaned up.

  Env variables:
    G5_VERIFY_GROUNDING_STRICT=labels    same as --strict-labels (CI invocation)
    G5_VERIFY_GROUNDING_STRICT=orphans   same as --strict-orphans
    G5_VERIFY_GROUNDING_STRICT=all       same as --strict-all

Categorizes errors into:
  1. intent_record_drift  — spec/view label exists but no intent_record entry,
                             or intent_record entry exists but no spec/view label.
  2. orphan_file          — edge references a spec/view/code file that no
                             longer exists on disk.
  3. missing_label        — edge references a label that no longer exists in
                             the referenced spec/view file. THIS is the rev-7
                             PR #112 regression class.

The hardened guard is wired into `infra/ci/cloudbuild-ci-rust.yaml` as the
`verify-grounding-integrity` step. Initial wiring is advisory (no strictness
flag) because current `main` has ~1800 pre-existing entries across all three
categories that need follow-up cleanup before strict mode is feasible.
"""

import argparse
import json
import os
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

WORKSPACE = Path(__file__).parent.parent  # Repository root
REPO_ROOT = WORKSPACE
G5_DOCS_ROOT = Path(os.environ.get("G5_DOCS_ROOT", str(REPO_ROOT / "g5_docs")))


def first_existing(*paths: Path) -> Path:
    """Return the first existing path, or the first candidate for diagnostics."""
    for path in paths:
        if path.exists():
            return path
    return paths[0]


def canonical_file_name(path: Path) -> str:
    """Return the identity used inside intent/grounding JSONL records."""
    for base in (WORKSPACE, G5_DOCS_ROOT, REPO_ROOT):
        try:
            return path.relative_to(base).as_posix()
        except ValueError:
            continue
    return path.as_posix()


SPECS_DIR = first_existing(REPO_ROOT / ".semlift" / "specs",
                           WORKSPACE / ".semlift" / "specs")
VIEWS_DIR = first_existing(G5_DOCS_ROOT / "views",
                           REPO_ROOT / "views")
INTENT_RECORD_DIR = first_existing(REPO_ROOT / "intent_record" / "ontology",
                                   WORKSPACE / "intent_record" / "ontology")
INTENT_RECORD_VIEW_DIR = first_existing(G5_DOCS_ROOT / "intent_record" / "views",
                                        WORKSPACE / "intent_record" / "views",
                                        REPO_ROOT / "intent_record" / "views")
INTENT_RECORD_CHAT_DIR = first_existing(G5_DOCS_ROOT / "intent_record" / "chat",
                                        WORKSPACE / "intent_record" / "chat",
                                        REPO_ROOT / "intent_record" / "chat")
GROUNDING_DIRS = {
    "ontology": first_existing(REPO_ROOT / "intent_grounding" / "ontology",
                               WORKSPACE / "intent_grounding" / "ontology"),
    "views": first_existing(G5_DOCS_ROOT / "intent_grounding" / "views",
                            WORKSPACE / "intent_grounding" / "views",
                            REPO_ROOT / "intent_grounding" / "views"),
    "code": first_existing(REPO_ROOT / "intent_grounding" / "code",
                           WORKSPACE / "intent_grounding" / "code"),
    "chat": first_existing(G5_DOCS_ROOT / "intent_grounding" / "chat",
                           WORKSPACE / "intent_grounding" / "chat",
                           REPO_ROOT / "intent_grounding" / "chat"),
}
GROUNDING_SUBDIRS = tuple(GROUNDING_DIRS)

LABEL_PATTERN = re.compile(r'\[([IDQ]\d+[A-Z]?)\.\s+[^\]]+\]')


def encode_file_name(path: str) -> str:
    return path.replace("/", "__") + ".jsonl"


def scan_labels(path: Path) -> set:
    """Extract [I/D/Q + optional letter + ID. Title] labels from a markdown file."""
    content = path.read_text(encoding="utf-8")
    return set(LABEL_PATTERN.findall(content))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-missing-specs", action="store_true",
                        help="Succeed when the repository intentionally has no ontology specs directory.")
    parser.add_argument("--strict-labels", action="store_true",
                        help="Exit non-zero on any missing-source/target-label error.")
    parser.add_argument("--strict-orphans", action="store_true",
                        help="Additionally fail on orphan-file errors.")
    parser.add_argument("--strict-all", action="store_true",
                        help="Fail on every recorded error class.")
    args = parser.parse_args()

    # Env-var sugar (used by Cloud Build step).
    env_strict = os.environ.get("G5_VERIFY_GROUNDING_STRICT", "")
    if env_strict == "labels":
        args.strict_labels = True
    elif env_strict == "orphans":
        args.strict_labels = args.strict_orphans = True
    elif env_strict == "all":
        args.strict_all = True

    # Categorized error buckets. Each error string is appended to its own bucket
    # and also to a flat `errors` list for the summary print.
    errors = []
    warnings = []
    err_intent_record_drift = []
    err_orphan_file = []
    err_missing_label = []

    # 1. Count labels across all spec files (key: canonical ontology path)
    spec_files = sorted(SPECS_DIR.rglob("*.semlift.md"))
    total_i = 0
    total_d = 0
    total_q = 0
    spec_labels = {}  # canonical ontology file path -> set of label IDs

    for spec_path in spec_files:
        rel_path = canonical_file_name(spec_path)
        labels = scan_labels(spec_path)
        spec_labels[rel_path] = labels

        for l in labels:
            if l.startswith("I"):
                total_i += 1
            elif l.startswith("D"):
                total_d += 1
            elif l.startswith("Q"):
                total_q += 1

    total = total_i + total_d + total_q
    print(f"=== Label Count Audit ===")
    print(f"  Spec files: {len(spec_files)}")
    print(f"  I labels: {total_i}")
    print(f"  D labels: {total_d}")
    print(f"  Q labels: {total_q}")
    print(f"  Total: {total}")
    if not spec_files:
        print("  Avg per file: n/a")
        if args.allow_missing_specs and not SPECS_DIR.exists():
            print(f"\nSKIP: ontology specs directory {SPECS_DIR} does not exist")
            return
        print(f"\nFAIL: no ontology spec files found under {SPECS_DIR}")
        sys.exit(1)
    print(f"  Avg per file: {total / len(spec_files):.1f}")

    # Also index view labels using their canonical grounding identity.
    view_files = sorted(VIEWS_DIR.rglob("*.md")) if VIEWS_DIR.exists() else []
    view_labels = {}  # canonical view file path -> set of label IDs
    for view_path in view_files:
        rel = canonical_file_name(view_path)
        view_labels[rel] = scan_labels(view_path)

    # Files-on-disk set for "code" target validation
    # (we don't try to grep symbols out of code; just verify the file exists)
    code_root = REPO_ROOT

    # Files with very few labels (possible under-labeling)
    under_labeled = [(p, len(ls)) for p, ls in spec_labels.items() if len(ls) < 3 and ls]
    if under_labeled:
        print(f"\n  Under-labeled files (<3 labels): {len(under_labeled)}")
        for p, c in sorted(under_labeled)[:10]:
            print(f"    {p}: {c} labels")

    # 2. Verify intent records match spec labels
    print(f"\n=== Intent Record Audit ===")
    record_mismatches = 0
    missing_records = 0

    for rel_path, labels in spec_labels.items():
        encoded = encode_file_name(rel_path)
        record_path = INTENT_RECORD_DIR / encoded

        if not record_path.exists():
            if labels:
                msg = f"Missing intent record for {rel_path} ({len(labels)} labels)"
                errors.append(msg); err_intent_record_drift.append(msg)
                missing_records += 1
            continue

        # Read record labels
        record_labels = set()
        with open(record_path) as f:
            for line in f:
                line = line.strip()
                if line:
                    rec = json.loads(line)
                    record_labels.add(rec.get("intent_record_number", ""))

        if labels != record_labels:
            in_spec_not_record = labels - record_labels
            in_record_not_spec = record_labels - labels
            if in_spec_not_record or in_record_not_spec:
                record_mismatches += 1
                if in_spec_not_record:
                    msg = f"{rel_path}: labels in spec but not record: {in_spec_not_record}"
                    errors.append(msg); err_intent_record_drift.append(msg)
                if in_record_not_spec:
                    msg = f"{rel_path}: labels in record but not spec: {in_record_not_spec}"
                    errors.append(msg); err_intent_record_drift.append(msg)

    print(f"  Record mismatches: {record_mismatches}")
    print(f"  Missing record files: {missing_records}")

    # 3. Verify grounding edges across ALL four subdirectories
    print(f"\n=== Grounding Edge Audit ===")
    edge_counts = {sub: 0 for sub in GROUNDING_SUBDIRS}
    orphan_edges = 0
    missing_label_edges = 0

    def file_type_index(file_type: str):
        """Return (label_dict_or_None, normalize_fn) for the file_type's lookup."""
        if file_type == "ontology":
            return spec_labels, lambda p: p
        if file_type == "view":
            return view_labels, lambda p: p
        return None, None  # code/chat → existence check only

    def check_endpoint(side, file_type, file_path, label_id, location):
        """side is 'source' or 'target' for error messages."""
        nonlocal orphan_edges, missing_label_edges
        labels, _ = file_type_index(file_type)
        if labels is not None:
            if file_path not in labels:
                msg = f"Orphan {side} ({file_type}): {file_path} in {location}"
                errors.append(msg); err_orphan_file.append(msg)
                orphan_edges += 1
                return
            # File exists → check label
            if label_id and label_id not in labels[file_path]:
                msg = f"Missing {side} label {label_id} in {file_path} (referenced in {location})"
                errors.append(msg); err_missing_label.append(msg)
                missing_label_edges += 1
            return
        # code / chat — file-existence check only
        if file_type == "code":
            on_disk = REPO_ROOT / file_path
            # code paths in grounding JSONL are usually -relative (e.g. "backend/g5d/src/...")
            if not on_disk.exists():
                # Try repo-relative as a fallback (some entries store "...")
                alt = REPO_ROOT / file_path
                if not alt.exists():
                    msg = f"Orphan {side} (code): {file_path} not on disk (in {location})"
                    errors.append(msg); err_orphan_file.append(msg)
                    orphan_edges += 1
            return
        if file_type == "chat":
            # Chat sources reference a chat-intent record by content-hash / slug.
            # We validate that the chat-intent-record file exists under intent_record/chat/
            # if the directory exists; otherwise we skip (no canonical hash format yet).
            if INTENT_RECORD_CHAT_DIR.exists():
                # Look for a chat record file matching this id
                candidates = list(INTENT_RECORD_CHAT_DIR.glob(f"*{file_path}*"))
                if not candidates:
                    warnings.append(
                        f"Chat record not found for {file_path} (in {location}); chat-id format may vary"
                    )
            return

    for sub in GROUNDING_SUBDIRS:
        subdir = GROUNDING_DIRS[sub]
        if not subdir.exists():
            continue
        for jsonl_path in sorted(subdir.glob("*.jsonl")):
            with open(jsonl_path) as f:
                for line_num, line in enumerate(f, 1):
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        edge = json.loads(line)
                    except json.JSONDecodeError as e:
                        errors.append(f"Bad JSON in {jsonl_path.name}:{line_num}: {e}")
                        continue
                    edge_counts[sub] += 1

                    src_file = edge.get("source_file", "")
                    tgt_file = edge.get("target_file", "")
                    src_id = edge.get("source_id", "")
                    tgt_id = edge.get("target_id", "")
                    src_type = edge.get("source_file_type", "")
                    tgt_type = edge.get("target_file_type", "")

                    location = f"{jsonl_path.name}:{line_num}"
                    check_endpoint("source", src_type, src_file, src_id, location)
                    check_endpoint("target", tgt_type, tgt_file, tgt_id, location)

    for sub in GROUNDING_SUBDIRS:
        print(f"  {sub:9s} edges: {edge_counts[sub]}")
    print(f"  Total edges:   {sum(edge_counts.values())}")
    print(f"  Orphan files:  {orphan_edges}")
    print(f"  Missing labels: {missing_label_edges}")

    # 4. Coverage stats (ontology-only — the existing audit signal)
    print(f"\n=== Coverage Stats ===")
    specs_with_incoming = set()
    specs_with_outgoing = set()

    ont_grounding_dir = GROUNDING_DIRS["ontology"]
    if ont_grounding_dir.exists():
        for jsonl_path in ont_grounding_dir.glob("*.jsonl"):
            with open(jsonl_path) as f:
                for line in f:
                    line = line.strip()
                    if line:
                        try:
                            edge = json.loads(line)
                        except json.JSONDecodeError:
                            continue
                        specs_with_outgoing.add(edge.get("source_file", ""))
                        specs_with_incoming.add(edge.get("target_file", ""))

    print(f"  Specs with incoming edges: {len(specs_with_incoming)} / {len(spec_files)}")
    print(f"  Specs with outgoing edges: {len(specs_with_outgoing)} / {len(spec_files)}")

    # Print errors and warnings
    if errors:
        print(f"\n=== ERRORS ({len(errors)}) ===")
        for e in errors[:30]:
            print(f"  ERROR: {e}")
        if len(errors) > 30:
            print(f"  ... and {len(errors) - 30} more errors")

    if warnings:
        print(f"\n=== WARNINGS ({len(warnings)}) ===")
        for w in warnings[:20]:
            print(f"  WARN: {w}")
        if len(warnings) > 20:
            print(f"  ... and {len(warnings) - 20} more warnings")

    if not errors and not warnings:
        print(f"\n  All checks passed!")

    # Per-class summary — surfaces what the guard would gate on at each strictness.
    print(f"\n=== Error class summary ===")
    print(f"  intent_record_drift: {len(err_intent_record_drift)}")
    print(f"  orphan_file:         {len(err_orphan_file)}")
    print(f"  missing_label:       {len(err_missing_label)}")

    # Strict-mode gating (warnings never affect exit code). Flags cascade:
    #   --strict-labels  → fail iff missing_label > 0           (rev-7 regression class)
    #   --strict-orphans → strict-labels + also orphan_file > 0
    #   --strict-all     → strict-orphans + also intent_record_drift > 0
    if args.strict_all:
        args.strict_orphans = True
    if args.strict_orphans:
        args.strict_labels = True

    fail = False
    if args.strict_labels and err_missing_label:
        fail = True
        print(f"\nFAIL: {len(err_missing_label)} missing-label edges (--strict-labels)")
    if args.strict_orphans and err_orphan_file:
        fail = True
        print(f"FAIL: {len(err_orphan_file)} orphan-file edges (--strict-orphans)")
    if args.strict_all and err_intent_record_drift:
        fail = True
        print(f"FAIL: {len(err_intent_record_drift)} intent-record drift entries (--strict-all)")
    if fail:
        sys.exit(1)


if __name__ == "__main__":
    main()
