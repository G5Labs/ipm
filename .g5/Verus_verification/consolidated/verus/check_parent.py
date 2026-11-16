#!/usr/bin/env python3
"""Verify new models, reject wrong variants at named properties, and check native-field-derived assertions.
All emitted assertions are obligations discharged by Verus, never assumptions.
"""
import json,hashlib,subprocess,tempfile,re,sys,importlib.util
from pathlib import Path
HERE=Path(__file__).resolve().parent; ROOT=HERE.parents[3]
VERUS=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus'
OUT=ROOT/'.g5/uplift/consolidated/extension/parent-checks';OUT.mkdir(parents=True,exist_ok=True)
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def run(stem,source,expected=None):
 with tempfile.TemporaryDirectory(prefix='ipm-formal-') as td:
  f=Path(td)/'model.rs';f.write_text(source)
  p=subprocess.run([str(VERUS),str(f),'--triggers-mode','silent'],cwd=td,capture_output=True,text=True,timeout=90)
  text=p.stdout+p.stderr;(OUT/(stem+'.log')).write_text(text)
  match=re.search(r'verification results:: (\d+) verified, (\d+) errors',text)
  logical=p.returncode!=0 and 'postcondition not satisfied' in text and expected in text and 'could not prove termination' not in text and 'resource limit' not in text if expected else p.returncode==0 and bool(match) and match[2]=='0'
  return {'command':[str(VERUS),'model.rs','--triggers-mode','silent'],'returncode':p.returncode,'verified':int(match[1]) if match else None,'errors':int(match[2]) if match else None,'expected_function':expected,'decisive':logical,'log':str((OUT/(stem+'.log')).relative_to(ROOT))}
controls={
 'temp_repo':[
 ('foreign-repository-deleted','(rows.remove(m.id), !rows.contains(m.id))','(Set::<int>::empty(), !rows.contains(m.id))','foreign_repository_preserved'),
 ('tie-prefers-local','a.repository_id < b.repository_id','a.repository_id > b.repository_id','equal_version_older_repository_wins')],
 'partial_compile':[
 ('descriptor-means-callable','callable:keep && results[i].callable','callable:keep && results[i].descriptor','compiler_distinction_is_preserved'),
 ('rollback-keeps-resources','all_ok || mode != Mode::Transaction','true','rollback_removes_every_resource')],
 'history':[
 ('unfinished-looks-successful','Text::Error00 }','Text::Success }','unfinished_row_displays_error00'),
 ('invalid-key-only-skipped','if fs.len() == 0 || !fs[0].value_present || !valid_column(fs[0].key) { seq![] }','if fs.len() == 0 { seq![] } else if !fs[0].value_present || !valid_column(fs[0].key) { where_prefix(fs.drop_first()) }','invalid_first_key_widens_delete')],
 'loadtx':[
 ('uncommitted-journal-survives','w.at < cut && (!w.journal','w.at < cut && (w.journal','journaled_child_cannot_survive_uncommitted_parent'),
 ('parent-cancels-child-default','if parent.is_some() {parent} else {own}','if own.is_some() {own} else {parent}','explicit_false_parent_overrides_child')],
}
results=[]
for stem,variants in controls.items():
 src=(HERE/(stem+'.rs')).read_text();base=run(stem,src); row={'family':stem,'source_sha256':sha(HERE/(stem+'.rs')),'baseline':base,'controls':[]}
 for name,old,new,expected in variants:
  assert old in src,(stem,name)
  row['controls'].append({'name':name,**run(stem+'-'+name,src.replace(old,new,1),expected)})
 results.append(row)
# Native/Python oracle validation is kept separate from proof status.
sys.path.insert(0,str(HERE.parent/'checks'));import framework
native=[]
for stem in ['temp_repo','partial_compile','history_display']:
 mod=framework.load_family(HERE.parent/'models'/(stem+'.py'));r=framework.check_family(mod)
 native.append({'family':stem,'fields':r['fields_compared'],'matches':r['as_built_matches'],'mismatches':r['mismatches'],'contracts':r['contract'],'receipt_runs':r['runs']})
# Translation assertions use actual inputs to specs; expected values are extracted from independent native fields.
t=framework.load_family(HERE.parent/'models/temp_repo.py');p=framework.load_family(HERE.parent/'models/partial_compile.py');h=framework.load_family(HERE.parent/'models/history_display.py')
native_asserts=[]
# Every compile mode is checked against descriptor/callability/status native projections.
obs=framework.parse_obs(p.RUNS[0]);lines=[]
for case,mode,pkg in [('R2txn','Transaction','Txn'),('R2notxn','NoTransaction','Notxn'),('R2dev','Developer','Dev'),('R2ok','NoTransaction','Ok')]:
 good=case=='R2ok';resources='seq![m::CompilerOutcome{descriptor:true,callable:true},m::CompilerOutcome{descriptor:true,callable:'+str(good).lower()+'}]'
 for i,c in enumerate('AB'):
  f=obs[(case,f'cls.TwoRes{pkg}.{c}')];kv=dict(x.split(':',1) for x in f.split())
  for field,nf in [('definition','def'),('descriptor','compiled'),('callable','runs')]:
   value=kv[nf] not in ('0','err');lines.append(f'assert(m::compile(m::Mode::{mode},{resources},{str(good).lower()},0).resources[{i}].{field} == {str(value).lower()});')
 lines.append(f'assert(m::compile(m::Mode::{mode},{resources},{str(good).lower()},0).status_ok == {str(obs[(case,"status")]=="OK").lower()});')
# Imported module definitions copied verbatim to one disposable source so paths cannot influence evaluation.
src=(HERE/'partial_compile.rs').read_text().replace('fn main() {}','')
src=src.replace('verus! {','mod m {use super::*; verus! {',1)+'}\nverus!{proof fn native_projection_agrees(){\n'+'\n'.join(lines)+'\n}}\nfn main(){}'
native_asserts.append(run('partial-native-projections',src));native_asserts[-1]['fields']=len(lines)
# Concrete repo outcome projections are asserted from OBS on the two failure histories and valid neighbor.
obs=framework.parse_obs(t.RUNS[1]);lines=[]
for case,dev,parent,child in [('Dtxn',False,'dd-parent-txn','dd-dep-txn'),('Ddev',True,'dd-parent-dev','dd-dep-dev'),('Dok',False,'dd-parent-ok','dd-dep-ok')]:
 expr=f'm::dependency_result({str(dev).lower()},{str(case!="Dok").lower()})'
 for field,pkg in [('parent',parent),('child',child)]:
  val=obs[(case,'mod.'+pkg)].startswith('present');lines.append(f'assert({expr}.{field} == {str(val).lower()});')
 lines.append(f'assert({expr}.status_ok == {str(obs[(case,"status")]=="OK").lower()});')
src=(HERE/'temp_repo.rs').read_text().replace('fn main() {}','').replace('verus! {','mod m {use super::*; verus! {',1)+'}\nverus!{proof fn native_repo_agrees(){'+'\n'.join(lines)+'}}\nfn main(){}'
native_asserts.append(run('repo-native-projections',src));native_asserts[-1]['fields']=len(lines)
inputs={str(x.relative_to(ROOT)):sha(x) for stem in controls for x in [HERE/(stem+'.rs')]}
for stem in ['temp_repo','partial_compile','history_display']:
 mod=framework.load_family(HERE.parent/'models'/(stem+'.py'))
 for x in [Path(mod.__file__),HERE.parent/'checks/framework.py']+[HERE.parent/'native/results'/r/'raw.log' for r in mod.RUNS]:inputs[str(x.relative_to(ROOT))]=sha(x)
inputs[str(VERUS)]=sha(VERUS)
result={'models':results,'native_oracle_comparison':native,'native_formal_assertions':native_asserts,'inputs':inputs,'proof_boundary':'Hand translation, disjoint-write-key prefix; compiler/SQL/IRIS environment inputs explicit. No ObjectScript body proof.'}
(OUT/'results.json').write_text(json.dumps(result,indent=2))
ok=all(x['baseline']['decisive'] and all(c['decisive'] for c in x['controls']) for x in results) and all(x['decisive'] for x in native_asserts) and all(x['matches'] for x in native)
print(json.dumps({'ok':ok,'models':len(results),'verified':sum(x['baseline']['verified'] or 0 for x in results),'controls_rejected':sum(c['decisive'] for x in results for c in x['controls']),'native_formal_fields':sum(x['fields'] for x in native_asserts)}));sys.exit(0 if ok else 1)
