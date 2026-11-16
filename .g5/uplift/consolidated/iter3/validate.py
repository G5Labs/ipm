#!/usr/bin/env python3
"""Fail on lost baseline files, edits outside owners, stale receipts or broken changed-owner links."""
from pathlib import Path
import difflib,hashlib,json,os,re,subprocess,tarfile
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
EXP=ROOT.parent/'results/experiments/uplift-measurement-20261114/ipm-iter3'
ALLOWED={'.g5/ontology/root.g5.md','.g5/ontology/concepts/modules.g5.md','.g5/ontology/concepts/repositories.g5.md','.g5/ontology/concepts/settings.g5.md','.g5/ontology/features/build-and-publish.g5.md','.g5/uplift/consolidated/BUGS.md','.g5/uplift/consolidated/DESIGN-SIMPLIFICATIONS.md','.g5/uplift/consolidated/QUESTIONS.md'}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def git(*args):return subprocess.check_output(['git',*args],cwd=ROOT)
def main():
    before=json.loads((EXP/'baseline/members.json').read_text());changed=[];missing=[]
    for rel,old in before.items():
        p=ROOT/rel
        if old['kind']=='symlink':
            if not p.is_symlink() or os.readlink(p)!=old['target']:missing.append(rel)
        elif not p.is_file():missing.append(rel)
        elif sha(p)!=old['sha256']:changed.append(rel)
    unexpected=sorted(set(changed)-ALLOWED)
    index_path=Path(git('rev-parse','--git-path','index').decode().strip())
    if not index_path.is_absolute():index_path=ROOT/index_path
    index_unchanged=index_path.read_bytes()==(EXP/'baseline/git-index').read_bytes()
    product_diff=git('diff','571e04a19aba9f9b4bc0ea23fbe09182038f4e55','--','src','preload','modules','tests','README.md','CHANGELOG.md')
    canonical={}
    for p in (ROOT/'.g5/ontology').rglob('*.g5.md'):
        for id in re.findall(r'^\[[IDQ] · ([^\]]+)\]',p.read_text(),re.M):
            assert id not in canonical,id
            canonical[id]=p
    broken=[];changed_pages=[ROOT/p for p in changed if p.startswith('.g5/ontology/')]+[ROOT/'.g5/ontology/features/artifact-download.g5.md']
    for p in changed_pages:
        for target in re.findall(r'\]\(([^)]+)\)',p.read_text()):
            if '://' in target or target.startswith('mailto:'):continue
            file,_,anchor=target.partition('#');dest=(p.parent/file).resolve() if file else p
            if not dest.exists():broken.append((str(p.relative_to(ROOT)),target,'missing path'))
            elif anchor and dest.is_file() and anchor not in re.findall(r'<a id="([^"]+)"',dest.read_text(errors='replace')) and anchor not in ['how-to-read-this-ontology']:
                # Existing heading anchors are allowed if generated from an actual heading.
                heads={re.sub(r'[^a-z0-9 -]','',h.lower()).replace(' ','-') for h in re.findall(r'^#+ (.+)$',dest.read_text(errors='replace'),re.M)}
                if anchor not in heads:broken.append((str(p.relative_to(ROOT)),target,'missing anchor'))
    proofs=json.loads((HERE/'proofs/models.json').read_text())
    for r in proofs['models']:
        assert sha(ROOT/r['model'])==r['sha256'],r['model']
        assert r['baseline']['exit']==0 and all(x['assertion_rejected'] for x in r['controls'])
    native=json.loads((HERE/'native/receipt.json').read_text());bridge=json.loads((HERE/'proofs/native-bridge.json').read_text())
    assert all(x['pass'] for x in native['checks'])
    assert len(native['fixture_compilation'])==4 and all(v=='1' for v in native['fixture_compilation'].values())
    assert sha(HERE/'native/receipt.json')==bridge['native_receipt_sha256']
    assert all(x['pass'] for x in bridge['results'])
    for rel,digest in native['input_sha256'].items():assert sha(ROOT/rel)==digest,rel
    bindings=json.loads((HERE/'INDEX.json').read_text())
    for g in bindings['groups']:assert sha(ROOT/'.g5/Verus_verification/consolidated/iter3'/g['model'])==g['formal_sha256'],g['id']
    for b in bindings['bindings']:assert b['owner'] in canonical
    known=ROOT/'.g5/uplift/consolidated/iter2/known-panel-review.json'
    known_unchanged=sha(known)==before[str(known.relative_to(ROOT))]['sha256']
    withdrawals=ROOT/'.g5/uplift/consolidated/iter2/boundaries/withdrawals.json'
    withdrawals_unchanged=sha(withdrawals)==before[str(withdrawals.relative_to(ROOT))]['sha256']
    diffs=[];old_ids=set()
    with tarfile.open(EXP/'baseline/actual-before.tar.gz') as tf:
        for m in tf:
            if m.isfile() and m.name.startswith('ipm/.g5/ontology/') and m.name.endswith('.g5.md'):
                raw=tf.extractfile(m).read().decode();old_ids.update(re.findall(r'^\[[IDQ] · ([^\]]+)\]',raw,re.M))
                rel=m.name.removeprefix('ipm/')
                if rel in changed:diffs.extend(difflib.unified_diff(raw.splitlines(True),(ROOT/rel).read_text().splitlines(True),fromfile='actual-before/'+rel,tofile='iter3/'+rel))
    (HERE/'canonical.diff').write_text(''.join(diffs))
    inherited=json.loads((HERE/'inherited-replay/receipt.json').read_text())
    required_inherited={'check_env_attr.py','semver_check.py','owners_check.py','check_parent.py','check_thin_surfaces.py','check_flag_overrides.py','iter2-models','iter2-boundaries','G-kernel-baseline','G-sanity-false'} | {'P-baseline-'+x for x in ['dep_resolve','load_general','ns_mapping','scopes','settings','sync_baseline','uninstall','update_steps']} | {'G-control-'+str(i).zfill(2) for i in range(1,14)}
    inherited_complete=required_inherited <= {x['name'] for x in inherited['checks']} and all(x['pass'] for x in inherited['checks'])
    native_pin=json.loads((HERE/'native/source-pin.json').read_text());assert not native_pin['mismatches']
    result={'baseline_members':len(before),'changed_existing':changed,'unexpected_changes':unexpected,'missing_baseline':missing,'git_index_unchanged':index_unchanged,'product_diff_empty':not product_diff,'canonical_blocks':len(canonical),'baseline_ids_preserved':old_ids<=canonical.keys(),'new_ids':sorted(set(canonical)-old_ids),'broken_changed_owner_links':broken,'known_41coverage_26precision_reference_unchanged':known_unchanged,'known_cases_regraded':False,'withdrawals_unchanged':withdrawals_unchanged,'original_models_and_receipts':'Every baseline file outside the 8 declared canonical/register owners is hash-identical, including prior models, native evidence and exact withdrawal ledgers.','new_verified_units':sum(x['baseline']['verified'] for x in proofs['models']),'new_controls':sum(len(x['controls']) for x in proofs['models']),'native_contrasts_passed':sum(x['pass'] for x in native['checks']),'native_bridge_assertions':bridge['native_assertions'],'native_source_files_matching_pin':native_pin['paths'],'inherited_replay_commands':len(inherited['checks']),'implementation_proofs':0,'inherited_replay_complete':inherited_complete,'optional_full_playbook_mutant_campaign':'INCOMPLETE: separate 600-second timeout, no pass credit'}
    (HERE/'validation.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2));assert not missing and not unexpected and index_unchanged and not product_diff and not broken and old_ids<=canonical.keys() and known_unchanged and withdrawals_unchanged and inherited_complete
if __name__=='__main__':main()
