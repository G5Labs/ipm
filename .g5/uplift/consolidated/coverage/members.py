"""Freeze the 1,216-member denominator with the unmodified counting rule of
results/experiments/ipm-uplift-comparison-20261113/linkage.py, read from the pinned product commit.

Writes members.tsv: member id (path::name, as linkage.py), kind, first and last line of the member's span
under linkage.py's line-to-member map, and whether the declaration line ends with ';' (no body).
Run from anywhere: python3 -I members.py [--check]
"""
import re, subprocess, sys
from pathlib import Path
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
PIN = '571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
def git(*a): return subprocess.check_output(['git', '-C', str(REPO), *a], text=True, errors='replace')
files = [s for s in git('ls-tree', '-r', '--name-only', PIN, '--', 'src/cls/IPM').splitlines() if s.endswith('.cls')]
rows = {}
for path in files:
    rel = path.split('src/cls/IPM/')[-1]
    text = git('show', f'{PIN}:{path}')
    member = None
    for i, line in enumerate(text.splitlines(), 1):
        m = re.match(r'^(ClassMethod|Method|Query|Trigger)\s+("[^"]+"|[\w%]+)', line, re.I)
        if m:
            member = rel + '::' + m[2]
            if member not in rows:
                rows[member] = {'kind': m[1], 'start': i, 'end': i, 'nobody': line.rstrip().endswith(';')}
        elif re.match(r'^(?:Class|ClassMethod|Method|Query|Trigger|Index|Parameter|Property|Relationship|Projection|ForeignKey|XData|Storage)\s', line, re.I):
            member = None
        if member:
            rows[member]['end'] = i
        if line.startswith('}') or (member and line.rstrip().endswith(';') and m):
            member = None
out = ['member\tkind\tstart\tend\tno_body']
for k in sorted(rows):
    r = rows[k]
    out.append(f"{k}\t{r['kind']}\t{r['start']}\t{r['end']}\t{int(r['nobody'])}")
raw = '\n'.join(out) + '\n'
dest = HERE / 'members.tsv'
if '--check' in sys.argv:
    assert dest.read_text() == raw, 'members.tsv differs from the pinned source'
else:
    dest.write_text(raw)
print(len(rows), 'members in', len({k.split('::')[0] for k in rows}), 'files of', len(files))
assert len(rows) == 1216
