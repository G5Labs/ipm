"""Verify the immutable human source and explicitly reviewed canonical transformations."""
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
manifest = json.loads(Path(__file__).with_name("source-map.json").read_text())
source = (ROOT / manifest["source"]).read_bytes()
assert hashlib.sha256(source).hexdigest() == manifest["sha256"], "Source changed"
source_lines = source.decode().splitlines()
edits = json.loads(Path(__file__).with_name("source-edits.json").read_text())["edits"]
assert set(edits) <= {block["id"] for block in manifest["blocks"]}
source_body = source.decode().split("\n---\ngdoc_comments:")[0]
source_bullets = [line for line in source_body.splitlines()
                  if line.startswith("- ") and line.strip() != "- &nbsp;"]
assert len(manifest["blocks"]) == len(source_bullets) == 285
assert len({block["id"] for block in manifest["blocks"]}) == 285
assert len({block["source_line"] for block in manifest["blocks"]}) == 285
for block in manifest["blocks"]:
    text = (ROOT / block["file"]).read_text()
    body, metadata = text.split("\n## Metadata\n", 1)
    marker = f'[{block["type"]} · {block["id"]}]\n\n'
    assert body.count(marker) == 1, block["id"]
    tail = body.split(marker, 1)[1]
    actual = tail.splitlines()[0]
    full_body = re.split(r'(?m)^(?:<a id=|## )', tail, maxsplit=1)[0].strip()
    expected = source_lines[block["source_line"] - 1]
    assert hashlib.sha256(expected.encode()).hexdigest() == block["sha256"], "Source-map drift"
    if block["id"] in edits:
        edit = edits[block["id"]]
        assert edit["source_line"] == block["source_line"]
        assert edit["kind"] and actual == edit["canonical"], f'Undocumented edit: {block["id"]}'
        if "human-authorized-amendment" in edit["kind"]:
            assert edit.get("authority") and edit.get("reason"), f'Missing amendment authority: {block["id"]}'
    else:
        assert actual == expected, f'Altered source: {block["id"]}'
    assert full_body == actual.strip(), f'Unregistered extra source body: {block["id"]}'
    entry = metadata.split(f'### {block["id"]}\n', 1)[1].split("\n### ", 1)[0]
    assert "- human-provided\n" in entry, block["id"]
    assert f'#L{block["source_line"]}' in entry, block["id"]
    if "human-authorized-amendment" in edits.get(block["id"], {}).get("kind", []):
        assert "Human-verified amendment:" in entry, f'Missing amendment lineage: {block["id"]}'
for path in (ROOT / ".g5/ontology").rglob("*.md"):
    body = path.read_text().split("\n## Metadata\n")[0]
    assert not re.search(r"^#{3,} ", body, re.M), path
amendments = sum("human-authorized-amendment" in edit["kind"] for edit in edits.values())
print(f"PASS: immutable source and all 285 human statements mapped; {len(edits) - amendments} editorial/reference transformations and {amendments} human-authorized amendments; unique IDs, lineage and flat sections.")
