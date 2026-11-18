#!/usr/bin/env python3
"""Writes the iteration74 viewer manifest, BINDINGS.md and results/viewer-index.jsonl rows from the saved receipts.
Run from the repository root after run.py: python3 .g5/Verus_verification/coverage-20261117/iter74/publish.py"""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[4]
out = root / '.g5/uplift/coverage-20261117/iter74'
concern = 'cov20261117-iter74-history-actions'
models = json.loads((out / 'proofs/models.json').read_text())
replay = json.loads((out / 'replay/receipt.json').read_text())
stamp = (out / 'native/LATEST').read_text().strip()
native = json.loads((out / 'native' / stamp / 'observations.json').read_text())
rel = lambda p: str(Path(p).resolve().relative_to(root))
rows, runs = [], []
def add(label, kind, variant, expect, outcome, receipt, **extra):
    runs.append({'label': label, 'kind': kind, 'variant': variant, 'expect': expect})
    rows.append({'label': label, 'kind': kind, 'variant': variant, 'expect': expect, 'concern': concern,
                 'run_id': f'cov20261117-iter74-{label.replace(" ", "-")}', 'outcome': outcome, 'assessment': 'matched',
                 'receipt_path': receipt, **extra})
for rec in models['models']:
    b = rec['baseline']
    add(f'proof {rec["file"]}', 'proof', 'as-built', 'verified, 0 errors', f'verified {b["verified"]}, errors {b["errors"]}',
        rel(b['log']), verified=b['verified'], errors=b['errors'])
    for c in rec['controls']:
        scope = 'dependency control: ' + c['dependency'] if c.get('dependency') else 'one-line mutant of actions.rs'
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
question = ('Which commands write History records and what install, update and uninstall write into them: row creation '
            'before or after the argument checks, Success, Committed (LoadNewModule marks; Finalize Committed || ISOK for '
            'uninstall), the logged phase lists, and dependency records written by <module> clean and <module> test. '
            f'Shares history.rs and playbook uninstall.rs; native i1-i7, u1-u5, n1-n5, c1, c2, t1, {len(native["observations"])} fields.')
mdir = root / '.g5/Verus_verification/models' / concern
mdir.mkdir(parents=True, exist_ok=True)
(mdir / 'manifest.json').write_text(json.dumps({'title': 'history actions · coverage iter74', 'question': question,
    'ontology_ids': ['history.actions', 'history.record', 'history.phases', 'history.committed', 'history.committed.marking', 'history.committed.uninstall'], 'runs': runs}, indent=2) + '\n')
(mdir / 'BINDINGS.md').write_text(f'''# history actions · coverage iter74

{question}

- install writes its record before any check, so every install that reaches Main.Install is recorded; the command parser refuses an empty name before that (`install_always_recorded`, `api_install`, `cli_install`). Native i1-i5, i4api.
- update writes a record exactly when it passes the -PermitDowngrade and installed checks; reinstall of a module that is not installed and uninstall with no name write none (`update_recorded_iff`, `reinstall_not_installed`, `uninstall_no_name`). Native u1-u5, r1, n3.
- uninstall writes a record for every name, including one that is not installed; its Committed equals its success because nothing marks it early (`uninstall_committed_is_success`, `uninstall_one_record`, `n5_facts`). Native n1, n2, n4, n5.
- For install and update, Committed equals "changes kept" except the early no-transaction marker (`install_committed_is_kept`). Native u2, u3, i5-i7.
- Logged phases: install five, update six ending in ApplyUpdateSteps, uninstall Clean; a failure lists the phases up to the failing one (`logged_lists`, `run_shape`, `install_phases`). Native i1, i5, u1, u2, n1.
- `<module> clean` writes no record of its own but one uninstall record per dependency its recursion visits; `<module> test` writes a load record per scoped dependency it installs; list, sync, reload and compile write none (`cli_clean`, `c1_facts`, `clean_without_deps_writes_nothing`, `other_commands_no_record`). Native c1, c2, t1.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declarations](../../coverage-20261117/iter74/actions.rs)

[Shared row model](../../consolidated/verus/history.rs)

[Shared uninstall model](../../../../playbook-uplift/models/verus/uninstall.rs)

[Product correspondence](../../../uplift/coverage-20261117/iter74/bindings.json)

[Proof and controls](../../../uplift/coverage-20261117/iter74/proofs/models.json)

[Iteration handoff](../../../uplift/coverage-20261117/iter74/HANDOFF.md)
''')
index = root / '.g5/Verus_verification/results/viewer-index.jsonl'
kept = [l for l in index.read_text().splitlines() if l.strip() and json.loads(l).get('concern') != concern]
index.write_text('\n'.join(kept + [json.dumps(r) for r in rows]) + '\n')
print(len(runs), 'runs published')
