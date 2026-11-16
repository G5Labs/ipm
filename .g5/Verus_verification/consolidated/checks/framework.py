"""Differential check framework for the consolidated uplift.

A *family* module in ../models/ describes one product behavior and exposes:
  FAMILY      short name
  RUNS        native run directory names under ../native/results/ whose OBS lines it predicts
  MEMBERS     {member_id: {"slice": "<file>:<a>-<b>", "fn": "<model function>", "behavior": "..."}}
  VARIANTS    {variant_name: {"member": member_id, "wrong": "what the wrong model claims"}}; 'as-built' is implicit
  predict(variant) -> {(case, field): value}   the twin's prediction for every OBS field it claims
  CONTRACT    optional [(name, fn(pred)->(bool, detail), expected_bool, ids)] L1 checks on the as-built prediction

A member is *credited* when (a) the as-built prediction equals every claimed native field, (b) at least one variant
attributed to that member is rejected by a native field (it predicts a different value for a claimed field), and
(c) the member's model function exists in the family source. The runner records which field rejects each variant.
"""
import importlib.util, json, re, sys, hashlib, datetime
from pathlib import Path
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
NATIVE = ROOT / 'native' / 'results'

def parse_obs(run, tags=None):
    """OBS lines -> {(case, field): value}. Also derives (case, 'log.<tag>') from tagged log lines between
    'CMD <case>' and the case's 'status=' line (see LOG_TAGS)."""
    raw = (NATIVE / run / 'raw.log').read_text(errors='replace')
    raw = re.sub(r'\x1b\[[0-9;]*m', '', raw)
    obs, current, buf = {}, None, []
    for line in raw.splitlines():
        m = re.match(r'^CMD (\S+) ', line)
        if m:
            current, buf = m.group(1), []
            continue
        # An OBS line normally starts the line; routine compilation can leave "Compiling routine : X.mac" unterminated.
        m = re.match(r'^OBS (\S+) (.*)$', line) or re.match(r'^Compiling routine : \S+\.mac(?:OBS) (\S+) (.*)$', line)
        if m:
            case, rest = m.groups()
            if rest.startswith('status=') and case == current:
                obs[(case, '_lines')] = '\n'.join(buf)
                for tag, rx in {**LOG_TAGS, **(tags or {})}.items():
                    hits = [h for l in buf for h in re.findall(rx, l)]
                    obs[(case, 'log.' + tag)] = ','.join(hits) if hits else 'none'
            for part in re.split(r' (?=[\w.\-]+=)', rest) if rest.startswith(('status=', 'tlevel')) else [rest]:
                k, _, v = part.partition('=')
                obs[(case, k)] = v
            continue
        if current:
            buf.append(line)
    obs[('_run', 'end_marker')] = str('NATIVE-END' in raw)
    return obs

LOG_TAGS = {'cleanup_failed': r"Failed to clean up repository '([^']+)'"}

def load_family(path):
    spec = importlib.util.spec_from_file_location(path.stem, path)
    mod = importlib.util.module_from_spec(spec); sys.path.insert(0, str(path.parent)); spec.loader.exec_module(mod)
    return mod

def check_family(mod):
    obs = {}
    for run in mod.RUNS:
        o = parse_obs(run, getattr(mod, 'LOG_TAGS', None))
        assert o[('_run', 'end_marker')] == 'True', f'{run}: no NATIVE-END marker'
        for k, v in o.items():
            if k[0] == '_run':
                continue
            if k[1] == '_lines':
                # A family may derive fields from the transcript of a command (e.g. what `history details` printed).
                if hasattr(mod, 'log_fields'):
                    for f, val in mod.log_fields(k[0], v).items():
                        obs[(run, k[0], f)] = val
                continue
            obs[(run, *k)] = v
    def keyed(pred):
        out = {}
        pick = getattr(mod, 'case_run', lambda case: None)
        for (case, field), v in pred.items():
            run = pick(case)
            hits = [k for k in obs if k[1] == case and k[2] == field and (run is None or k[0] == run)]
            out[(case, field)] = (v, [obs[h] for h in hits])
        return out
    base = keyed(mod.predict('as-built'))
    mismatches = [{'case': c, 'field': f, 'predicted': v, 'observed': o} for (c, f), (v, o) in base.items() if o != [v]]
    variants = {}
    for name, meta in mod.VARIANTS.items():
        pred = mod.predict(name)
        # Compare the variant with every observed field the as-built twin claims (a field the variant does not
        # produce, e.g. an error where a value was observed, counts as a disagreement).
        rejected_by = [{'case': c, 'field': f, 'wrong': pred.get((c, f), '<not produced>'), 'observed': obs_v}
                       for (c, f), (_, obs_v) in base.items() if obs_v and obs_v != [pred.get((c, f), object())]]
        variants[name] = {**meta, 'rejected': bool(rejected_by), 'rejected_by': rejected_by[:3]}
    contract = []
    pred = mod.predict('as-built')
    for name, fn, expected, ids in getattr(mod, 'CONTRACT', []):
        ok, detail = fn(pred)
        contract.append({'name': name, 'holds': ok, 'expected': expected, 'as_expected': ok == expected, 'ids': ids, 'detail': detail})
    src = Path(mod.__file__).read_text()
    members = {}
    for mid, meta in mod.MEMBERS.items():
        fn_ok = bool(re.search(r'^def ' + re.escape(meta['fn']) + r'\(', src, re.M))
        killed = [n for n, v in variants.items() if v['member'] == mid and v['rejected']]
        members[mid] = {**meta, 'fn_present': fn_ok, 'variants_rejected': killed,
                        'credited': fn_ok and not mismatches and bool(killed)}
    return {'family': mod.FAMILY, 'runs': mod.RUNS, 'fields_compared': len(base),
            'as_built_matches': not mismatches, 'mismatches': mismatches, 'variants': variants,
            'contract': contract, 'members': members,
            'source_sha256': hashlib.sha256(src.encode()).hexdigest()}

def main(argv):
    files = [Path(a) for a in argv] or sorted((ROOT / 'models').glob('*.py'))
    results = [check_family(load_family(f)) for f in files if not f.name.startswith('_')]
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    out = {'when': stamp, 'families': results}
    d = ROOT / 'results' / 'checks'; d.mkdir(parents=True, exist_ok=True)
    (d / f'{stamp}.json').write_text(json.dumps(out, indent=1, default=str))
    if not argv:
        (d / 'latest.json').write_text(json.dumps(out, indent=1, default=str))
    bad = 0
    for r in results:
        cred = [m for m, v in r['members'].items() if v['credited']]
        surv = [n for n, v in r['variants'].items() if not v['rejected']]
        unexpected = [c['name'] for c in r['contract'] if not c['as_expected']]
        print(f"[{r['family']}] fields={r['fields_compared']} as-built={'MATCH' if r['as_built_matches'] else 'MISMATCH'} "
              f"variants rejected={sum(v['rejected'] for v in r['variants'].values())}/{len(r['variants'])} "
              f"credited={len(cred)}/{len(r['members'])} contract-unexpected={unexpected}")
        for m in r['mismatches'][:5]: print('   mismatch', m)
        if surv: print('   surviving variants:', surv)
        bad |= (not r['as_built_matches']) or bool(unexpected)
    print('results:', d / f'{stamp}.json')
    return 1 if bad else 0

if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
