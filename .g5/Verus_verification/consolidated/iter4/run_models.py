#!/usr/bin/env python3
"""Run iteration 4 contracts and discriminating wrong variants; receipts are additive iter4 files."""
from pathlib import Path
import hashlib,json,os,re,resource,subprocess,time,datetime
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
OUT=ROOT/'.g5/uplift/consolidated/iter4/proofs'
TOOL=Path(os.environ.get('VERUS',str(Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')))
MUTATIONS={
'test_outcome':{
 'method-error-not-counted':('r == Row::CaseError || r == Row::MethodError || r == Row::AssertFailed','r == Row::CaseError || r == Row::AssertFailed'),
 'case-error-not-counted':('r == Row::CaseError || r == Row::MethodError || r == Row::AssertFailed','r == Row::MethodError || r == Row::AssertFailed'),
 'nested-clears-parent':('after: if start == 0 { None } else { Some(slots.take(start as int)) },','after: None,'),
 'nested-counts-parent-slots':('let c = suffix_failures(slots, start);','let c = suffix_failures(slots, 0);'),
 'display-includes-start-slot':('-> nat {\n    suffix_failures(slots, start)\n}','-> nat {\n    suffix_failures(slots, if start > 0 { (start - 1) as nat } else { 0 })\n}'),
 'not-fatal-still-fails':('(!o.fatal || failures(r.log) == 0)','failures(r.log) == 0'),
 'empty-package-fails':('else if !r.has_classes { true }','else if !r.has_classes { false }'),
 'missing-suite-dir-skipped':('else if load_fails(r) { false }','else if load_fails(r) { true }'),
 'extension-not-checked':('(!o.report_requested || o.report_ext_ok) && ',''),
 'lifecycle-stops-at-first-failure':('-> bool {\n    0 <= i < rs.len()\n}','-> bool {\n    0 <= i < rs.len() && forall|j: int| 0 <= j < i ==> resource_ok(rs[j], o)\n}'),
 'sync-runs-all-resources':('0 <= i < rs.len() && forall|j: int| 0 <= j < i ==> resource_ok(rs[j], o)','0 <= i < rs.len()'),
 'exit-ignores-terminate-flag':('if !ok && terminate { 1 }','if !ok { 1 }'),
},
'registry':{
 'bytes-sent-to-configured-origin':('Request { target: url_origin, basic: basic_complete(c), apikey: c.token.len() > 0 }','Request { target: c.origin, basic: basic_complete(c), apikey: c.token.len() > 0 }'),
 'bytes-drop-credentials-as-built':('Request { target: url_origin, basic: basic_complete(c), apikey: c.token.len() > 0 }','Request { target: url_origin, basic: false, apikey: false }'),
 'repair-still-forwards':('basic: same && basic_complete(c), apikey: same && c.token.len() > 0','basic: basic_complete(c), apikey: c.token.len() > 0'),
 'hash-compared-case-insensitively':('registry_hash.len() == 0 || registry_hash == digest_lower','registry_hash.len() == 0 || registry_hash == digest_lower || registry_hash == upper(digest_lower)'),
 'empty-hash-rejected':('registry_hash.len() == 0 || registry_hash == digest_lower','registry_hash == digest_lower'),
 'token-stored-before-integrity':('d == Download::Accepted && token.len() > 0','(d == Download::Accepted || d == Download::Mismatch) && token.len() > 0'),
 'mismatch-sends-analytics':('InstallResult { installed: false, analytics_sent: false, error: InstallError::Download }','InstallResult { installed: false, analytics_sent: true, error: InstallError::Download }'),
 'junk-refused-at-download':('InstallResult { installed: false, analytics_sent: true, error: InstallError::ManifestMissing }','InstallResult { installed: false, analytics_sent: false, error: InstallError::Download }'),
 'bytes-failure-reports-status':('Download::MetaFailed(_) => true, _ => false','Download::MetaFailed(_) => true, Download::BytesFailed => true, _ => false'),
},
'sync_compile':{
 'resolution-failure-fails-step':('ok: !called || compile_ok,','ok: (!called || compile_ok) && warnings(rs) == 0,'),
 'compile-called-without-survivors':('let called = compile_set_size(rs) > 0;','let called = true;'),
 'unresolved-resource-still-compiled':('if eligible(r) && r.resolve_ok { kept(r.children) } else { 0 }','if eligible(r) { kept(r.children) } else { 0 }'),
 'warning-always-retained':('retained: if summary_active { warnings(rs) } else { 0 },','retained: warnings(rs),'),
 'undefined-class-compiled':('!cs.last().is_cls || cs.last().defined','true'),
 'project-resource-warned':('-> bool { eligible(r) && !r.resolve_ok }','-> bool { !r.resolve_ok }'),
},
'repairs4':{
 'lifecycle-defaults-swapped':('Lifecycle::Module => false, Lifecycle::DeployedModule => true,','Lifecycle::Module => true, Lifecycle::DeployedModule => false,'),
 'explicit-false-ignored':('Explicit::Value(b) => b }','Explicit::Value(b) => b || deploy_by_default(l) }'),
 'explicit-overrides-class-parameter':('Some(v) => v == 1,','Some(v) => v == 1 || explicit == Explicit::Value(true),'),
 'stored-phase-folded':('&& declared[i] == lower(phase)\n}','&& lower(declared[i]) == lower(phase)\n}'),
 'request-not-lowercased':('&& declared[i] == lower(phase)\n}','&& declared[i] == phase\n}'),
 'empty-tokens-kept':('(if ts[0].len() > 0 { seq![ts[0]] } else { Seq::<Text>::empty() })','seq![ts[0]]'),
 'dollar-list-not-filtered':('KeywordInput::DollarList(l) => nonempty(l),','KeywordInput::DollarList(l) => l,'),
 'object-filtered':('KeywordInput::Object(l) => l,','KeywordInput::Object(l) => nonempty(l),'),
 'keywords-ignored-at-gate':('enabled && registry_match && keywords(input).len() == 0','enabled && registry_match'),
 'outside-path-returns-empty':('{ rel.skip(dir.len() as int) } else { rel };','{ rel.skip(dir.len() as int) } else { seq![] };'),
 'prefix-requires-boundary':('let r = if has_prefix(rel, dir) {','let r = if has_prefix(rel, dir.push(47)) {'),
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
