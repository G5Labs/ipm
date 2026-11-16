#!/usr/bin/env python3
"""List ontology statements whose Evidence cites an IPM test class by X: (not run) or T: (run) tag.
Usage: python3 -I validation/iris/n1/xref_tests.py [ClassShortName ...]"""
import re, sys
from pathlib import Path
ONT = Path(__file__).resolve().parents[3] / "ontology"
want = set(sys.argv[1:])
for f in sorted(ONT.glob("0*.md")):
    for line in f.read_text().splitlines():
        cols = [c.strip() for c in line.split("|")]
        if len(cols) < 8 or not re.match(r"^[A-Z]+-(?!C\d)[A-Z]*\d+$", cols[1]):
            continue
        ev = cols[5]
        hits = set(re.findall(r"X:tests/(?:integration_tests|unit_tests|migration/[^/]+/tests/unit_tests)/Test/PM/(?:Integration|Unit|Migration)/([A-Za-z/]+)\.cls", ev))
        hits |= set(re.findall(r"T:Test\.PM\.(?:Integration|Unit)\.([A-Za-z.]+)", ev))
        hits = {h.replace("/", ".") for h in hits}
        for h in sorted(hits):
            if not want or h in want:
                kind = "T" if f"T:Test.PM" in ev and h in ev.replace("/", ".") and re.search(r"T:Test\.PM\.\w+\."+re.escape(h), ev) else "X"
                print(f"{h}\t{cols[1]}\t{kind}\t{cols[4][:90]}")
