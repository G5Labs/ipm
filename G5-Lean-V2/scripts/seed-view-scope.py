#!/usr/bin/env python3
"""Render the committed view scope from the same catalog used by the SQL seed."""
import json
from pathlib import Path

CATALOG = Path(__file__).resolve().parents[1] / "tests/fixtures/mini-calc-view-scope.json"


def frontmatter_scope() -> str:
    components = json.loads(CATALOG.read_text())
    if not components or len(components) != len(set(components)):
        raise ValueError("seed view scope must be nonempty and contain no duplicates")
    # JSON strings are valid YAML scalars; do not hand-escape component IDs.
    lines = [f"root_spec_ids: [{json.dumps(components[0])}]", "linked_nodes:"]
    lines.extend(f"  - {json.dumps(component)}" for component in components[1:])
    return "\n".join(lines)


if __name__ == "__main__":
    print(frontmatter_scope())
