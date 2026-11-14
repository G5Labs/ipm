#!/usr/bin/env python3
"""For each observation name given, list ontology statements citing it as O:<name>, and whether the statement also carries F: (finding = contradiction).
Usage: python3 -I validation/iris/n1/xref_obs.py name1 name2 ..."""
import re, sys
from pathlib import Path
ONT = Path(__file__).resolve().parents[3] / "ontology"
want = sys.argv[1:]
rows = []
for f in sorted(ONT.glob("0*.md")):
    for line in f.read_text().splitlines():
        cols = [c.strip() for c in line.split("|")]
        if len(cols) >= 8 and re.match(r"^[A-Z]+-(?!C\d)[A-Z]*\d+$", cols[1]):
            rows.append((cols[1], cols[5]))
for n in want:
    hits = []
    for sid, ev in rows:
        tags = [t.strip() for t in ev.split(";")]
        if f"O:{n}" in tags:
            fs = [t for t in tags if t.startswith("F:")]
            hits.append(f"{sid}{'(' + ','.join(fs) + ')' if fs else ''}")
    print(f"{n}\t{' '.join(hits) if hits else '-'}")
