#!/usr/bin/env python3
"""Run maintained contracts and discriminating wrong variants; all receipts are additive iter3 files."""
from pathlib import Path
import hashlib,json,os,re,resource,subprocess,time,datetime
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
OUT=ROOT/'.g5/uplift/consolidated/iter3/proofs'
TOOL=Path(os.environ.get('VERUS',str(Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')))
MUTATIONS={
'artifacts':{
 'authenticated-response-is-sorted-locally':('if rows.len()==0 {None} else {Some(rows[0])}','if rows.len()==0 {None} else {Some(rows[rows.len()-1])}'),
 'multi-operator-object-admitted':('if ops.len()!=1','if ops.len()==0'),
 'match-allowed-on-repository':('condition(raw,matches!(key,Key::Name|Key::Path))','condition(raw,true)'),
 'repo-only-filter-admitted':('c.name.is_some() || c.props.len()>0','c.name.is_some() || c.repo.len()>0 || c.props.len()>0'),
 'nonfile-type-admitted':('if p.value==seq![102int,105,108,101]','if true'),
 'dot-treated-as-wildcard':('pattern[0]==63 || pattern[0]==value[0]','pattern[0]==63 || pattern[0]==46 || pattern[0]==value[0]'),
 'star-cannot-be-empty':('wildcard(value,pattern.drop_first()) ||','false ||'),
 'repository-guard-lost':('(c.repo.len()==0 || a.repo==c.repo)','true'),
 'name-guard-lost':('&& matches(a.name,c.name)',''),
 'path-guard-lost':('&& matches(if a.folder.len()==0 {seq![46int]} else {a.folder},c.path)',''),
 'properties-guard-lost':('&& properties(a.props,c.props)',''),
 'selects-first-not-newest':('if later(a.updated,old.updated)','if false'),
 'tie-selects-last':('if later(a.updated,old.updated)','if later(a.updated,old.updated) || a.updated==old.updated'),
 'incomplete-basic-is-credential':('user.len()>0 && password.len()>0','user.len()>0'),
 'token-loses-precedence':('if token.len()>0 {Auth::Bearer}','if token.len()>0 {Auth::Basic}'),
 'bytes-request-reuses-token-as-built':('fn bytes_request() -> Request {Request{authorization:Auth::Anonymous}}','fn bytes_request() -> Request {Request{authorization:Auth::Bearer}}'),
 'cached-file-overwritten':('if status!=200 || old.exists','if status!=200'),
 'developer-suppresses-auth-error':('ok:dev && a==Auth::Anonymous,skipped:dev && a==Auth::Anonymous','ok:dev,skipped:dev'),
 'developer-suppresses-extraction':('Download::ExtractError=>DownloadResult{ok:false','Download::ExtractError=>DownloadResult{ok:dev'),
 'existing-directory-is-refetched':('request_download:phase && !local_directory','request_download:phase'),
 'overlay-deletes-unrelated-files':('destination:Map::new(old.destination.dom().union(source.dom()),|k:Text| if source.contains_key(k) {source[k]} else {old.destination[k]})','destination:source'),
 'as-built-checks-read-status':('if r.name.len()==0 {HeaderAction::Stop}','if !r.status_ok {HeaderAction::Reject} else if r.name.len()==0 {HeaderAction::Stop}'),
 'repair-still-ignores-read-status':('if !r.status_ok {HeaderAction::Reject} else {header_action(r)}','header_action(r)'),
 'credential-repair-still-anonymous':('fn repaired_bytes_request(u:Text,p:Text,t:Text) -> Request {metadata_request(u,p,t)}','fn repaired_bytes_request(u:Text,p:Text,t:Text) -> Request {bytes_request()}'),
},
'resources':{
 'duplicate-same-owner-resource-is-accepted':('!new_object || unique.len()==0 || !existing.contains(unique)','true'),
 'failed-save-writes-database-flag':('if eligible && !already_marked && save_ok {true}','if eligible && !already_marked {true}'),
 'false-class-parameter-ignored':('parameter.unwrap()==1','parameter.unwrap()==1 || resource==Tri::On'),
 'nonone-class-parameter-is-true':('parameter.unwrap()==1','parameter.unwrap()!=0'),
 'generated-uses-project':('&& !generated && !preload','&& !preload'),
 'preload-uses-project':('&& !generated && !preload','&& !generated'),
 'all-types-use-project':('(deployable_type || path_name)','true'),
 'missing-project-still-used':('&& project_exists',''),
 'install-error-records-marker':('marker:installed_marker || (can_install && install_ok)','marker:installed_marker || can_install'),
 'deployed-flag-proves-installation':('marked_deployed:already_marked || eligible','marked_deployed:already_marked || (eligible && install_ok)'),
 'export-propagates-helper-as-built':('if deployed_handled {ExportResult{ok:true','if deployed_handled {ExportResult{ok:deployed_ok'),
 'helper-error-blocks-source-as-built':('if deployed_handled {ExportResult','if !deployed_ok || deployed_handled {ExportResult'),
 'repair-still-exports-after-helper-error':('if !deployed_ok {ExportResult{ok:false,handled:deployed_handled,source_called:false}}','if false {ExportResult{ok:false,handled:deployed_handled,source_called:false}}'),
 'project-check-keeps-missing-items':('(if exists.contains(items[0]) {seq![items[0]]} else {seq![]})','seq![items[0]]'),
 'path-resources-sort-with-documents':('a.document && !a.path_name','a.document'),
 'default-overrides-explicit':('rule.values.contains_key(k) && !explicit.contains(k) {rule.values[k]}','rule.values.contains_key(k) {rule.values[k]}'),
 'default-conditions-ignored':('&& satisfies(props,rule.conditions)',''),
 'unset-health-means-false':('r.health==Tri::Off && e.health','r.health!=Tri::On && e.health'),
 'health-does-not-imply-interop':('(r.interoperability==1 || r.health==Tri::On)','r.interoperability==1'),
 'disabled-means-negative-requirement':('let interop=(r.interoperability==1 || r.health==Tri::On) && !e.interoperability;','let interop=((r.interoperability==1 || r.health==Tri::On) && !e.interoperability) || (r.interoperability==2 && e.interoperability);'),
},
'public_repairs':{
 'explicit-unchecked-still-checks':('check!=Tri::Off && (status_return || check==Tri::On)','status_return || check==Tri::On'),
 'unchecked-swallows-exception':('!threw && (!invoke_checked(check,status_return) || returned_ok)','!invoke_checked(check,status_return) || (!threw && returned_ok)'),
 'custom-phase-does-not-replace':('if custom.len()>0 {custom} else {normal}','normal'),
 'keyword-search-visits-filesystem':('&& !(filesystem && keywords.len()>0)',''),
 'overwrite-flag-ignored':('if !overwrite && old.current.len()>0','if false'),
 'first-default-changes':('first:if old.first.is_some() {old.first} else {Some(value)}','first:Some(value)'),
 'setter-normalizes-spelling':('Some(set_value(old,value,overwrite))','Some(set_value(old,lower(value),overwrite))'),
 'invalid-format-accepted':('if format_valid(value) {Some','if true {Some'),
 'phase-list-is-case-sensitive':('lower(declared[i])==lower(phase)','declared[i]==phase'),
 'empty-suite-falls-back-to-case':('Some(s)=>suite_path(s),None=>case','Some(s)=>if s.len()==0 {case} else {suite_path(s)},None=>case'),
 'class-resource-filters-by-case':('package.len()==0 || requested.len()==0','requested.len()==0'),
 'two-test-targets-admitted':('(class.len()>0)!=(package.len()>0)','class.len()>0 || package.len()>0'),
}
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
