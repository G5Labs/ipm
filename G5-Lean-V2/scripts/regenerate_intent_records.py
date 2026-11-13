#!/usr/bin/env python3
"""
Regenerate all ontology intent_record JSONL files from actual I/D/Q labels in spec files.
Reads all .semlift.md files, extracts labels via regex, writes one JSONL per spec file.
"""

import json
import os
import re
import sys
from pathlib import Path

WORKSPACE = Path(__file__).parent.parent  # Repository root
SPECS_DIR = WORKSPACE / ".semlift" / "specs"
INTENT_RECORD_DIR = WORKSPACE / "intent_record" / "ontology"

# Regex to find I/D/Q labels: [I1. Title], [D2. Title], [Q3. Title]
LABEL_PATTERN = re.compile(r'\[([IDQ]\d+)\.\s+[^\]]+\]')


def encode_file_name(path: str) -> str:
    """Encode file path to JSONL filename: replace / with __, append .jsonl"""
    return path.replace("/", "__") + ".jsonl"


def extract_labels(spec_path: Path) -> list[str]:
    """Extract all I/D/Q label IDs from a spec file."""
    content = spec_path.read_text(encoding="utf-8")
    labels = LABEL_PATTERN.findall(content)
    # Deduplicate while preserving order
    seen = set()
    unique = []
    for label in labels:
        if label not in seen:
            seen.add(label)
            unique.append(label)
    return unique


def make_record(file_name: str, label_id: str) -> dict:
    """Create an intent record dict."""
    return {
        "file_type": "ontology",
        "file_name": file_name,
        "intent_record_number": label_id,
        "is_human": False,
        "created_by": "backfill",
        "created_at": "2026-04-15T00:00:00Z"
    }


def main():
    # Ensure output directory exists
    INTENT_RECORD_DIR.mkdir(parents=True, exist_ok=True)

    # Find all spec files
    spec_files = sorted(SPECS_DIR.rglob("*.semlift.md"))

    total_files = 0
    total_labels = 0
    empty_files = []

    for spec_path in spec_files:
        # Get relative path from workspace root
        rel_path = str(spec_path.relative_to(WORKSPACE))

        # Extract labels
        labels = extract_labels(spec_path)

        if not labels:
            empty_files.append(rel_path)
            # Still write empty file to match expected behavior

        # Encode filename
        encoded = encode_file_name(rel_path)
        output_path = INTENT_RECORD_DIR / encoded

        # Write JSONL
        with open(output_path, "w", encoding="utf-8") as f:
            for label_id in labels:
                record = make_record(rel_path, label_id)
                f.write(json.dumps(record, separators=(",", ":")) + "\n")

        total_files += 1
        total_labels += len(labels)

    # Clean up orphan JSONL files that don't correspond to any spec
    existing_jsonls = set(f.name for f in INTENT_RECORD_DIR.glob("*.jsonl"))
    expected_jsonls = set(encode_file_name(str(p.relative_to(WORKSPACE))) for p in spec_files)
    orphans = existing_jsonls - expected_jsonls
    for orphan in orphans:
        (INTENT_RECORD_DIR / orphan).unlink()
        print(f"  Removed orphan: {orphan}")

    print(f"\nIntent Record Regeneration Complete:")
    print(f"  Spec files processed: {total_files}")
    print(f"  Total labels found: {total_labels}")
    print(f"  Files with no labels: {len(empty_files)}")
    if empty_files:
        for f in empty_files[:10]:
            print(f"    - {f}")
        if len(empty_files) > 10:
            print(f"    ... and {len(empty_files) - 10} more")
    print(f"  Orphan JSONL files removed: {len(orphans)}")


if __name__ == "__main__":
    main()
