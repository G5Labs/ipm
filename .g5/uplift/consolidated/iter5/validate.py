#!/usr/bin/env python3
"""Validate the iteration 5 index, links and before-image preservation.

Usage: python3 validate.py [baseline-members.json]
Without an argument it uses the experiment's ipm-iter5/baseline/members.json when present.
"""
from pathlib import Path
import hashlib, json, re, subprocess, sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
PIN = '571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
DECLARED_CHANGED = {
    '.g5/ontology/root.g5.md', '.g5/ontology/features/build-and-publish.g5.md', '.g5/ontology/features/uninstall.g5.md',
    '.g5/ontology/concepts/repositories.g5.md', '.g5/ontology/concepts/modules.g5.md',
    '.g5/uplift/consolidated/BUGS.md', '.g5/uplift/consolidated/QUESTIONS.md', '.g5/uplift/consolidated/DESIGN-SIMPLIFICATIONS.md',
}
NEW_PREFIXES = ('.g5/uplift/consolidated/iter5/', '.g5/Verus_verification/consolidated/iter5/')


def sha(b):
    return hashlib.sha256(b).hexdigest()


GIT = subprocess.run(['git', 'rev-parse', '--git-dir'], cwd=ROOT, capture_output=True).returncode == 0


def pinned(path):
    # In an extracted seal there is no .git; the archived worktree product files are read instead.
    if GIT:
        return subprocess.run(['git', 'show', PIN + ':' + path], cwd=ROOT, capture_output=True).stdout
    f = ROOT / path
    return f.read_bytes() if f.is_file() else b''


def anchors(path):
    text = (ROOT / path).read_text()
    slugs = {re.sub(r'[^a-z0-9 -]', '', h.strip().lower()).replace(' ', '-') for h in re.findall(r'^#+ (.+)$', text, re.M)}
    return set(re.findall(r'<a id="([^"]+)"></a>', text)) | slugs


def main():
    problems, checks = [], {'rows': 0, 'sources': 0, 'verus': 0, 'controls': 0, 'native': 0, 'links': 0}
    index = json.loads((HERE / 'INDEX.json').read_text())
    models = json.loads((HERE / 'proofs/models.json').read_text())
    rejected = {m['model'].split('/')[-1][:-3] + '/' + c['id']: c['assertion_rejected'] for m in models['models'] for c in m['controls']}
    stamp = (HERE / 'native/LATEST').read_text().strip()
    obs = json.loads((HERE / 'native' / stamp / 'observations.json').read_text())
    slices = {}
    for row in index['rows']:
        checks['rows'] += 1
        path, _, anchor = row['owner'].partition('#')
        if anchor not in anchors(path):
            problems.append(('missing anchor', row['owner']))
        for role in ('direct', 'reader'):
            for src, token in row[role]:
                data = pinned(src)
                checks['sources'] += 1
                if not data or token.encode() not in data:
                    problems.append(('source token not at pin', src, token))
                slices[src] = sha(data)
        for sym in row['verus']:
            f, _, name = sym.partition('::')
            checks['verus'] += 1
            if not re.search(r'\bfn ' + re.escape(name) + r'\b', (ROOT / f).read_text()):
                problems.append(('missing verus symbol', sym))
        for c in row['controls']:
            checks['controls'] += 1
            if not rejected.get(c):
                problems.append(('control not rejected', c))
        for n in row['native']:
            checks['native'] += 1
            ok = n[5:] in obs['entry_forms'] if n.startswith('form:') else n in obs['observations']
            if not ok:
                problems.append(('missing native case', n))
    # Relative markdown links in every changed canonical/register file and the new iteration docs.
    docs = sorted(DECLARED_CHANGED) + [str(p.relative_to(ROOT)) for p in HERE.glob('*.md')]
    for doc in docs:
        text = (ROOT / doc).read_text()
        for target in re.findall(r'\]\(([^)\s]+)\)', text):
            if target.startswith(('http', 'mailto:', '#')) and not target.startswith('#'):
                continue
            file_part, _, frag = target.partition('#')
            dest = (ROOT / doc).parent / file_part if file_part else ROOT / doc
            checks['links'] += 1
            if not dest.exists():
                problems.append(('broken link', doc, target))
            elif frag and dest.suffix == '.md' and '.g5/ontology' in str(dest) and frag not in anchors(str(dest.resolve().relative_to(ROOT.resolve()))):
                problems.append(('broken anchor', doc, target))
    # Before-image preservation.
    base_arg = sys.argv[1] if len(sys.argv) > 1 else str(ROOT.parent / 'results/experiments/uplift-measurement-20261114/ipm-iter5/baseline/members.json')
    preservation = None
    if Path(base_arg).exists():
        base = json.loads(Path(base_arg).read_text())
        changed, missing, excluded = [], [], []
        for rel, row in base.items():
            p = ROOT / rel
            if row['kind'] == 'symlink':
                if not p.is_symlink():
                    target = row['target']
                    escapes = Path(target).is_absolute() or not str((p.parent / target).resolve()).startswith(str(ROOT.resolve()))
                    (excluded if escapes else missing).append(rel)
                continue
            if not p.is_file():
                missing.append(rel)
            elif sha(p.read_bytes()) != row['sha256']:
                changed.append(rel)
        undeclared = sorted(set(changed) - DECLARED_CHANGED)
        preservation = {'baseline': base_arg, 'baseline_members': len(base), 'changed': sorted(changed), 'missing': missing, 'portable_excluded_links': len(excluded), 'undeclared_changes': undeclared}
        if missing or undeclared:
            problems.append(('before image not preserved', missing[:5], undeclared[:5]))
    product = subprocess.run(['git', 'diff', '--quiet', PIN, '--', 'src', 'preload', 'modules', 'wheels', 'tests'], cwd=ROOT).returncode == 0 if GIT else None
    if product is False:
        problems.append(('product differs from pin',))
    result = {'checks': checks, 'problems': problems, 'pinned_source_sha256': slices, 'preservation': preservation,
              'product_matches_pin': product, 'source_mode': 'git pin' if GIT else 'extracted worktree (no .git)', 'native_observations': str((HERE / 'native' / stamp / 'observations.json').relative_to(ROOT)),
              'models_sha256': sha((HERE / 'proofs/models.json').read_bytes()), 'ok': not problems}
    (HERE / 'validation.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'ok': result['ok'], 'checks': checks, 'problems': problems[:10],
                      'changed': preservation and preservation['changed']}, indent=1))
    sys.exit(0 if result['ok'] else 1)


if __name__ == '__main__':
    main()
