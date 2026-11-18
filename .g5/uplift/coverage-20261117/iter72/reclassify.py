#!/usr/bin/env python3
"""Reclassify mutation records logged as `invalid` whose Verus log shows a proof failure.

mutate.py first classed a failed `by (compute)` assertion as invalid, because Verus stops before the
results line. This reads each invalid record's scratch log (/tmp/ipmcov72-<group>/<id>/verus.log) and
rewrites the record as killed with a `reclassified` note. Records whose log is a type or parse error stay
invalid. Run once after all groups finish."""
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
MARKS = ('simplifies to false', 'evaluates to false', 'assertion failed', 'postcondition not satisfied', 'precondition not satisfied')
changed = 0
for f in sorted((HERE / 'mutations').glob('*.jsonl')):
    out = []
    for line in f.read_text().splitlines():
        r = json.loads(line)
        if r['verdict'] == 'invalid':
            log = Path(f'/tmp/ipmcov72-{r["group"]}') / r['id'] / 'verus.log'
            text = log.read_text() if log.exists() else ''
            if any(m in text for m in MARKS):
                r['verdict'] = 'killed'
                r['reclassified'] = 'by (compute) proof failure; Verus printed no results line'
                changed += 1
        out.append(json.dumps(r))
    f.write_text('\n'.join(out) + '\n')
print(f'{changed} records reclassified invalid -> killed')
