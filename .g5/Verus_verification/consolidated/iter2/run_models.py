#!/usr/bin/env python3
"""Verify scoped contracts and require each deliberate bad variant to fail an assertion."""
from pathlib import Path
import os, subprocess, tempfile, hashlib, json, time
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
OUT=ROOT/'.g5/uplift/consolidated/iter2/proofs'
VERUS=Path(os.environ.get('VERUS',str(Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')))
MUTANTS={
 'registry':{
  'provider-order-erased':('Some(tags[tags.len()-1])','Some(tags[0])'),
  'accept-second-tar':('layers[0].size == size && layers[0].digest == digest','layers[0].size == size && layers[0].digest == digest || verify_layers(layers.drop_first(),size,digest)'),
  'cached-success-rechecks-enabled':('state:s, available:true, provider_called:false','state:s, available:enabled, provider_called:false'),
  'cached-failure-reprobes':('state:s, available:false, provider_called:false','state:s, available:probe, provider_called:false'),
  'all-auth-denials-are-bad-credentials':('&& configured && probe == Probe::Rejected','&& configured'),
  '401-is-always-suppressed':('&& !rejected',''),
  'token-realm-not-needed':('if !realm || !transport','if !transport'),
  'configured-credentials-always':('user || pass || token','true'),
  'basic-user-alone':('(user && pass) || (token && basic)','user || (token && basic)'),
  'empty-manifest-accepted':('has_xml && trimmed_nonempty','has_xml'),
  'empty-taglist-exists':('provider_tags.len() > 0','provider_tags.len() >= 0'),
  'missing-platform-admitted':('!deployed || platform_nonempty','true'),
  'false-capability-is-error':('status_ok:available && !provider_threw','status_ok:available && !provider_threw && provider_enabled'),
  'remote-publish-accepts201':('remote_authorized(user,password,token) && export_ok && transport_ok && status == 200','remote_authorized(user,password,token) && export_ok && transport_ok && (status == 200 || status == 201)'),
  'false-hash-clears-stream':('stream_set: directory_ok && provider_ok && archive_found && link_ok','stream_set: directory_ok && provider_ok && archive_found && link_ok && hash_ok'),
  'failed-command-compensates-registry':('remote_written:called && provider_written, local_version_changed','remote_written:called && provider_written && local_export_ok, local_version_changed'),
  'failed-provider-has-no-effects':('remote_written:called && provider_written, version_assigned_in_memory:called','remote_written:called && provider_written && provider_ok, version_assigned_in_memory:called'),
  'deletion-removes-all-tags':('delete_prefix(store,tags,(completed-1) as nat).remove(tags[completed-1])','delete_prefix(store,tags,(completed-1) as nat).remove(tags[completed-1]).remove(tags[tags.len()-1])'),
 },
 'readers':{
  'show-includes-scoped-dependents':('deps[0].target == target && !deps[0].scoped','deps[0].target == target'),
  'missing-version-is-satisfied':('if !nonempty_version { Installed::Missing }','if !nonempty_version { Installed::Satisfied }'),
  'requirements-keep-comments':('lines[0].blank_after_trim || lines[0].comment_after_trim','lines[0].blank_after_trim'),
  'method-error-inferred-from-assertions':('if methods[0].passed {0nat}else{1}','if methods[0].failed_assertions == 0 {0nat}else{1}'),
  'ignore-case-hook-error':('Count{methods:if case_error {1nat}else{0}','Count{methods:0nat'),
  'json-file-is-summary':('full_tree:true,summary:false','full_tree:false,summary:true'),
  'quiet-device-includes-all':('all_methods:show_detail && verbose','all_methods:show_detail'),
  'verify-skip-still-loads':('load:!current && !(skip && exists)','load:!current'),
  'test-prefix-is-exact-name':('prefix(package,requested) || prefix(requested,package)','package == requested'),
  'force-masks-error':('status_ok:is_ipm || provider_ok','status_ok:is_ipm || provider_ok || force'),
 }
}
# Rename-safe target keyed to the final field name.
MUTANTS['registry']['failed-command-compensates-registry']=(
 'remote_written:called && provider_written, version_assigned_in_memory:called',
 'remote_written:called && provider_written && local_export_ok, version_assigned_in_memory:called')
def execute(cmd):
 start=time.monotonic();p=subprocess.run(list(map(str,cmd)),capture_output=True,text=True,timeout=120)
 return {'command':list(map(str,cmd)),'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'seconds':round(time.monotonic()-start,4)}
def main():
 OUT.mkdir(parents=True,exist_ok=True)
 receipt={'tool_sha256':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [VERUS,VERUS.parent/'vstd.vir',VERUS.parent/'z3',VERUS.parent/'version.json']},'models':[]}
 with tempfile.TemporaryDirectory(prefix='ipm-iter2-controls-') as td:
  for model,variants in MUTANTS.items():
   path=HERE/(model+'.rs');source=path.read_text()
   row={'model':str(path.relative_to(ROOT)),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'proof':execute([VERUS,path,'--triggers-mode','silent']),'controls':[]}
   assert row['proof']['exit']==0,row['proof']
   for name,(old,new) in variants.items():
    assert source.count(old)==1,(name,source.count(old))
    bad=Path(td)/(name+'.rs');bad.write_text(source.replace(old,new,1))
    r=execute([VERUS,bad,'--triggers-mode','silent'])
    rejected=r['exit']!=0 and ('postcondition not satisfied' in r['stderr'] or 'assertion failed' in r['stderr'])
    row['controls'].append({'id':name,'mutation':{'before':old,'after':new},'rejected_at_assertion':rejected,**r})
    assert rejected,(name,r)
   receipt['models'].append(row)
 (OUT/'models.json').write_text(json.dumps(receipt,indent=2)+'\n')
 print(json.dumps({'proofs':[x['proof']['stdout'].strip() for x in receipt['models']],
  'controls':sum(len(x['controls']) for x in receipt['models']),
  'rejected':sum(y['rejected_at_assertion'] for x in receipt['models'] for y in x['controls'])}))
if __name__=='__main__': main()
