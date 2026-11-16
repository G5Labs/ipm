#!/usr/bin/env python3
"""Run iteration 5 contracts and discriminating wrong variants; receipts are additive iter5 files."""
from pathlib import Path
import hashlib,json,os,re,resource,subprocess,time,datetime
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
OUT=ROOT/'.g5/uplift/consolidated/iter5/proofs'
TOOL=Path(os.environ.get('VERUS',str(Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')))
MUTATIONS={
'webapp':{
 'adoption-overwrites-undeclared':('|k: int| if declared.contains_key(k) { declared[k] } else { old[k] }','|k: int| if declared.contains_key(k) { declared[k] } else { 0 }'),
 'existing-app-not-adopted':('if apps.contains_key(name) { apps.insert(name, merged(apps[name], declared)) }','if apps.contains_key(name) { apps }'),
 'modify-always-called':('exists|k: int| declared.contains_key(k) && (!old.contains_key(k) || old[k] != declared[k])','true'),
 'validate-dispatches':('pub open spec fn validate_dispatches() -> bool { false }','pub open spec fn validate_dispatches() -> bool { true }'),
 'transaction-keeps-app':('if transaction { FailedInstall { apps: before, module_recorded: false } }','if transaction { FailedInstall { apps: after_activate, module_recorded: false } }'),
 'blocked-uninstall-keeps-app':('let after = match declared_name { Some(n) => apps.remove(n), None => apps };','let after = if blocked(dependents, force, level) { apps } else { match declared_name { Some(n) => apps.remove(n), None => apps } };'),
 'blocked-uninstall-fails':('CleanResult { apps: after, module_kept: blocked(dependents, force, level), ok: true }','CleanResult { apps: after, module_kept: blocked(dependents, force, level), ok: !blocked(dependents, force, level) }'),
 'repair-still-deletes':('if blocked(dependents, force, level) { CleanResult { apps, module_kept: true, ok: false } }','if false { CleanResult { apps, module_kept: true, ok: false } }'),
 'upgrade-removes-app':('pub open spec fn upgrade_drops_declaration(apps: Apps) -> Apps { apps }','pub open spec fn upgrade_drops_declaration(apps: Apps) -> Apps { Map::empty() }'),
},
'entry_forms':{
 'matcher-case-insensitive':('-> bool { body == Spelling::Capitalized }','-> bool { true }'),
 'pinned-body-capitalized':('pub open spec fn pinned_body() -> Spelling { Spelling::Lowercase }','pub open spec fn pinned_body() -> Spelling { Spelling::Capitalized }'),
 'command-flag-terminates':('Form::ZpmCommandWithFlag => Outcome { process_exit: 0, continues: true,','Form::ZpmCommandWithFlag => Outcome { process_exit: 1, continues: false,'),
 'plain-shell-terminates':('Form::ShellPlain => Outcome { process_exit: 0,','Form::ShellPlain => Outcome { process_exit: 1,'),
},
'redirect':{
 'redirect-drops-credentials-as-built':('-> bool {\n    0 <= i < hops.len()\n}','-> bool {\n    0 <= i < hops.len() && i == 0\n}'),
 's14-checks-every-hop':('0 <= i < hops.len() && hops[0] == config','0 <= i < hops.len() && hops[i] == config'),
 's14b-checks-first-hop':('0 <= i < hops.len() && hops[i] == config','0 <= i < hops.len() && hops[0] == config'),
},
}
def run(path):
    before=resource.getrusage(resource.RUSAGE_CHILDREN);start=time.monotonic()
    cmd=[str(TOOL),str(path),'--triggers-mode','silent']
    p=subprocess.run(cmd,capture_output=True,text=True,timeout=120)
    after=resource.getrusage(resource.RUSAGE_CHILDREN);raw=p.stdout+p.stderr
    match=re.search(r'verification results:: (\d+) verified, (\d+) errors',raw)
    row={'command':cmd,'exit':p.returncode,'wall_seconds':time.monotonic()-start,
        'child_user_seconds':after.ru_utime-before.ru_utime,'child_system_seconds':after.ru_stime-before.ru_stime,
        'peak_child_rss_bytes_cumulative_max':after.ru_maxrss,'verified':int(match[1]) if match else None,
        'errors':int(match[2]) if match else None,'raw':raw}
    (OUT/'attempts').mkdir(parents=True,exist_ok=True)
    stamp=str(time.time_ns()); log=OUT/'attempts'/(stamp+'.log');log.write_text(raw)
    with (OUT/'commands.jsonl').open('a') as f:f.write(json.dumps({**{k:v for k,v in row.items() if k!='raw'},'log':str(log.relative_to(ROOT)),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()})+'\n')
    return row
def main():
    OUT.mkdir(parents=True,exist_ok=True);rows=[]
    for model,mutations in MUTATIONS.items():
        source=(HERE/(model+'.rs')).read_text();base=run(HERE/(model+'.rs'))
        (OUT/(model+'.log')).write_text(base.pop('raw'))
        assert base['exit']==0 and base['errors']==0,(model,base)
        row={'model':str((HERE/(model+'.rs')).relative_to(ROOT)),'sha256':hashlib.sha256(source.encode()).hexdigest(),'baseline':base,'controls':[]}
        for name,(old,new) in mutations.items():
            assert source.count(old)==1,(model,name,source.count(old))
            path=OUT/'controls'/model/(name+'.rs');path.parent.mkdir(parents=True,exist_ok=True)
            path.write_text(source.replace(old,new,1));r=run(path);raw=r.pop('raw');path.with_suffix('.log').write_text(raw)
            r.update(id=name,mutation={'before':old,'after':new},source=str(path.relative_to(ROOT)),
                assertion_rejected=r['exit']!=0 and ('assertion failed' in raw or 'postcondition not satisfied' in raw) and 'resource limit' not in raw and 'could not prove termination' not in raw)
            row['controls'].append(r)
            if not r['assertion_rejected']:
                (OUT/'failed-control.json').write_text(json.dumps(r,indent=2));raise AssertionError((model,name,raw[-1600:]))
        rows.append(row);print(model,base['verified'],len(mutations),flush=True)
    receipt={'tool_sha256':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [TOOL,TOOL.parent/'vstd.vir',TOOL.parent/'z3',TOOL.parent/'version.json']},'models':rows,'implementation_proofs':0}
    (OUT/'models.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps({'verified':sum(x['baseline']['verified'] for x in rows),'controls':sum(len(x['controls']) for x in rows)}))
if __name__=='__main__':main()
