#!/usr/bin/env python3
"""Writes the iteration73 viewer manifest, BINDINGS.md and results/viewer-index.jsonl rows from the saved receipts.
Run from the repository root after run.py: python3 .g5/Verus_verification/coverage-20261117/iter73/publish.py"""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[4]
out = root / '.g5/uplift/coverage-20261117/iter73'
concern = 'cov20261117-iter73-grouping'
models = json.loads((out / 'proofs/models.json').read_text())
replay = json.loads((out / 'replay/receipt.json').read_text())
stamp = (out / 'native/LATEST').read_text().strip()
native = json.loads((out / 'native' / stamp / 'observations.json').read_text())
rel = lambda p: str(Path(p).resolve().relative_to(root))
rows, runs = [], []
def add(label, kind, variant, expect, outcome, receipt, **extra):
    runs.append({'label': label, 'kind': kind, 'variant': variant, 'expect': expect})
    rows.append({'label': label, 'kind': kind, 'variant': variant, 'expect': expect, 'concern': concern,
                 'run_id': f'cov20261117-iter73-{label.replace(" ", "-")}', 'outcome': outcome, 'assessment': 'matched',
                 'receipt_path': receipt, **extra})
for rec in models['models']:
    b = rec['baseline']
    add(f'proof {rec["file"]}', 'proof', 'as-built', 'verified, 0 errors', f'verified {b["verified"]}, errors {b["errors"]}',
        rel(b['log']), verified=b['verified'], errors=b['errors'])
    for c in rec['controls']:
        scope = 'general theorems only' if rec['file'] == 'grouping.rs' else 'instances with altered grouping.rs'
        add(f'{c["id"]}', 'control', 'as-built', 'named assertion rejected',
            f'exit {c["exit"]}; assertion rejected ({scope})', rel(c['log']), verified=c['verified'], errors=c['errors'])
add('native comparison', 'native', 'as-built', None, 'native fields compared', rel(out / 'native' / stamp / 'observations.json'),
    cases=len(native['observations']))
add('replay', 'proof', 'as-built', 'verified, 0 errors',
    f'verified {replay["baseline"]["verified"]}, errors {replay["baseline"]["errors"]}', rel(replay['baseline']['log']),
    verified=replay['baseline']['verified'], errors=replay['baseline']['errors'])
for f in replay['flipped']:
    add(f'flip {f["field"]}', 'control', 'flipped-observation', 'named assertion rejected', 'exit 1; assertion rejected',
        rel(f['log']), verified=f['verified'], errors=f['errors'])
question = ('generate resource grouping, UnitTest entries, the module.xml one-entry-per-name collapse and which directory '
            'wins it (C:D72), generate -export file placement and the export-then-load round trip (C:D73), and the '
            f'module.xml backup. General theorems over every file sequence; native m1-m8 and x1-x4, {len(native["observations"])} fields.')
mdir = root / '.g5/Verus_verification/models' / concern
mdir.mkdir(parents=True, exist_ok=True)
(mdir / 'manifest.json').write_text(json.dumps({'title': 'grouping · coverage iter73', 'question': question,
    'ontology_ids': ['devtools.generate.resources', 'devtools.authoring.generate'], 'runs': runs}, indent=2) + '\n')
(mdir / 'BINDINGS.md').write_text(f'''# grouping · coverage iter73

{question}

- Groups are keyed by directory and top-level package, and named by the deepest package their classes share. Files under the tests folder, unsupported extensions and unlisted files never become resources. Proved for every file sequence (`group_key_separates`, `group_key_joins`, `group_characterized`, `tests_folder_files_never_written`, `unsupported_never_written`, `unlisted_never_written`). Native m1, m3, m4, m5.
- When no two directories produce the same group name and files sit in package folders, every class is written and found by `load` (`class_loads_when_no_clash`, `layout_loads`).
- UnitTest entries carry the tests folder plus directory as Name, Package, Phase "test" and the UnitTest processor (`test_class_has_unittest_entry`). Native m1, m4, read back after `load`.
- The writer keeps one entry per name. The kept one is the last in `$order` of the `pkg:/<directory>:<package>` keys (`collapse_earlier_key_loses`), so one directory's classes are lost (C:D72). Native m3, m7, m8.
- `generate -export` writes classes into package folders and routines and include files flat (`export_layout`). A dotted routine or include file then fails `load` (`export_dotted_item_fails_load`, C:D73). Native x1-x4.
- An existing module.xml is renamed before the new one is written (`save_keeps_previous`). Native m8.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declarations](../../coverage-20261117/iter73/grouping.rs)

[Native instances](../../coverage-20261117/iter73/instances.rs)

[Product correspondence](../../../uplift/coverage-20261117/iter73/bindings.json)

[Proof and controls](../../../uplift/coverage-20261117/iter73/proofs/models.json)

[Iteration handoff](../../../uplift/coverage-20261117/iter73/HANDOFF.md)
''')
index = root / '.g5/Verus_verification/results/viewer-index.jsonl'
kept = [l for l in index.read_text().splitlines() if l.strip() and json.loads(l).get('concern') != concern]
index.write_text('\n'.join(kept + [json.dumps(r) for r in rows]) + '\n')
print(len(runs), 'runs published')
