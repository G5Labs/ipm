#!/usr/bin/env python3
"""Prove actual native flag/depth/surviving-write observations against the existing model.
Full interpreter/composition and status restoration stay separate premises; assertions are not axioms.
"""
from pathlib import Path
import json,subprocess,hashlib,tempfile,re
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];N=ROOT/'.g5/uplift/consolidated/extension/flag-overrides-native/results/20261114T160126Z';TOOL=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus'
native_check=json.loads((N/'checks.json').read_text());assert not native_check['unexpected'];assert native_check['preservation']['same'];case_data=native_check['cases'];raw_cases={}
for case,rest in re.findall(r'^OBS (\S+) (.*)$',(N/'raw.log').read_text(),re.M):
    if case not in case_data:continue
    target=raw_cases.setdefault(case,{})
    for k,v in re.findall(r'(\S+)=([^ ]+)',rest):
        assert k not in target or target[k]==v,('conflicting native field',case,k)
        target[k]=v
assert raw_cases==case_data,'checked fields must be decoded from the raw consumer transcript'
lines=[];fields=[]
for case in ['JA','J0','TA','T0','JS','TS']:
 own_j='Some(true)' if case.startswith('J') else 'None';own_t='Some(true)' if case.startswith('T') else 'None';par_j='Some(false)' if case=='J0' else 'None';par_t='Some(false)' if case=='T0' else 'None';ok=case.endswith('S');commit='Some(1nat)' if ok else 'None';d=case_data[case]
 assert d['entry.tlevel']=='0' and d['entry.journal']=='1' and d['parent.tlevel']=='1' and d['parent.journal']=='1','native bridge domain is a journaled parent with no caller transaction'
 lines.append(f'let f=m::effective(m::Flags{{no_journal:{par_j},no_transaction:{par_t}}},m::Flags{{no_journal:{own_j},no_transaction:{own_t}}});')
 lines.append(f'let w=m::Write{{owner:1,key:1,value:1,at:0,journal:!m::enabled(f.no_journal),depth:m::entry_depth(1,f),commit_at:{commit}}};')
 lines.append(f'let parent=m::Write{{owner:0,key:0,value:1,at:0,journal:true,depth:1,commit_at:{commit}}};')
 for field,expr,typ in [('child.tlevel','m::entry_depth(1,f)','int'),('child.journal','!m::enabled(f.no_journal)','bool'),('stored.child','m::survived(w,2)','bool'),('stored.childwrite','m::survived(w,2)','bool'),('stored.root','m::survived(parent,2)','bool'),('stored.parentwrite','m::survived(parent,2)','bool'),('exit.tlevel','m::settle(m::End::Return,true,true,true).transaction_depth','int')]:
  expected=d[field] if typ=='int' else str(d[field]=='1').lower();lines.append(f'assert({expr} == {expected}); // {case} {field}');fields.append({'case':case,'field':field,'expression':expr,'observed':d[field]})
source='#[path="loadtx.rs"] mod m;\nuse vstd::prelude::*;\nverus!{ proof fn flags_match_native(){\n'+'\n'.join(lines)+'\n}}\nfn main(){}\n';(HERE/'flags_native_bridge.rs').write_text(source)
with tempfile.TemporaryDirectory(prefix='ipm-flags-proof-') as td:
 td=Path(td);(td/'loadtx.rs').write_text((HERE/'loadtx.rs').read_text());(td/'flags_native_bridge.rs').write_text(source);cmd=[str(TOOL),str(td/'flags_native_bridge.rs'),'--triggers-mode','silent'];p=subprocess.run(cmd,capture_output=True,text=True,timeout=90);assert p.returncode==0,(p.stdout+p.stderr)
 # Reuse the existing parent priority mutant; this additionally requires a native observation to reject it.
 q=td/'loadtx.rs';q.write_text(q.read_text().replace('if parent.is_some() {parent} else {own}','if own.is_some() {own} else {parent}',1));bad=subprocess.run(cmd,capture_output=True,text=True,timeout=90);assert bad.returncode!=0 and 'assertion failed' in bad.stderr and 'flags_native_bridge.rs' in bad.stderr,bad.stderr
 result={'model_sha256':hashlib.sha256((HERE/'loadtx.rs').read_bytes()).hexdigest(),'native_sha256':{f.name:hashlib.sha256(f.read_bytes()).hexdigest() for f in [N/'raw.log',N/'checks.json',N/'receipt.json',N/'script.cos']},'command':cmd,'returncode':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'native_projection_fields':fields,'field_count':len(fields),'native_checks':90,'native_mismatches':0,'wrong_priority_bridge_rejected':True,'wrong_priority_stdout':bad.stdout,'wrong_priority_stderr':bad.stderr,'boundary':'Exact six disjoint named-write cases. Parent status/failure point and restored journal are native observations; no whole-program refinement is claimed.'}
(HERE/'results/flag-overrides.json').write_text(json.dumps(result,indent=2));print('Flag/depth/prefix bridge:',p.stdout.strip(),len(fields),'matched native fields; existing priority mutant rejected at native assertions')
