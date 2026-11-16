#!/usr/bin/env python3
"""Content-based base/head binding drift, exercised on disposable Git commits.
A product edit reopens correspondence without rerunning a formally unchanged theorem.
"""
from pathlib import Path
import re,json,hashlib,shutil,tempfile,subprocess,sys,platform,os
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];V=ROOT/'.g5/Verus_verification/consolidated/verus'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def import_closure(path,seen=None):
 seen=seen or set();path=path.resolve()
 if path in seen:return seen
 seen.add(path)
 for rel in re.findall(r'#\[path\s*=\s*"([^"]+)"\]',path.read_text()):import_closure(path.parent/rel,seen)
 return seen
checks={}
for name in ['env_config','attr_evaluator','semver','owners','temp_repo','partial_compile','history','loadtx','thin_surfaces','inherited_bridge']:
 p=V/(name+'.rs'); deps=import_closure(p)
 driver=V/('check_env_attr.py' if name in ['env_config','attr_evaluator'] else name+'_check.py' if name in ['semver','owners'] else 'check_thin_surfaces.py' if name=='thin_surfaces' else 'check_parent.py')
 if driver.exists():deps.add(driver)
 if name!='inherited_bridge':
  model=ROOT/'.g5/Verus_verification/consolidated/models'/('history_display.py' if name=='history' else name+'.py')
  if model.exists():deps.add(model)
  deps.add(ROOT/'.g5/Verus_verification/consolidated/checks/framework.py')
 # Follow exact receipt/run references in manifests; ignored prose is not a formal edge.
 manifest=V/(name+'.manifest.json')
 if manifest.exists():
  x=json.loads(manifest.read_text())
  def walk(o):
   if isinstance(o,dict):
    for k,z in o.items():walk(z)
   elif isinstance(o,list):
    for z in o:walk(z)
   elif isinstance(o,str):
    for prefix in ['', '.g5/Verus_verification/consolidated/native/results/']:
     p=ROOT/(prefix+o)
     if p.is_file():deps.add(p)
     elif p.is_dir():
      for f in p.rglob('*'):
       if f.is_file() and f.name in ['raw.log','script.cos','receipt.json','zversion.txt']:deps.add(f)
  walk(x.get('bindings',[]))
 # Actual source-family fixtures (not another family's corpus).
 fixture={'temp_repo':['c1','c1e'],'attr_evaluator':['c1e'],'semver':['c2'],'owners':['c4'],'env_config':['c5'],'partial_compile':['c1e']}.get(name,[])
 for fdir in fixture:
  dp=ROOT/'.g5/Verus_verification/consolidated/native/fixtures'/fdir
  if dp.exists():deps.update(p for p in dp.rglob('*') if p.is_file())
 checks[name]={str(p.relative_to(ROOT)):sha(p) for p in sorted(deps) if p.is_file()}
tool=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos'
tools={str(p):sha(p) for p in tool.rglob('*') if p.is_file() and not p.is_symlink()}
# The preserved Python companions are different checks from the formal proofs.
for name,paths in {'G-loadtx-oracle':['.g5/Verus_verification/followup/models/loadtx.py'],'G-history-reader-oracle':['.g5/Verus_verification/followup/models/history_readers.py'],'P-load-explorer':['playbook-uplift/models/explorer'],'P-install-resolver-explorer':['playbook-uplift/models/explorer/inst','playbook-uplift/validation/iris/inst/fixtures'],'P-namespace-sync-explorer':['playbook-uplift/models/explorer/nsp'],'P-repository-explorer':['playbook-uplift/models/explorer/repo']}.items():
    deps=[]
    for rel in paths:
        p=ROOT/rel
        deps.extend([p] if p.is_file() else [f for f in p.rglob('*') if f.is_file() and '__pycache__' not in str(f)])
    if name.startswith('P-'):
        deps.extend(p for p in (ROOT/'playbook-uplift/models/explorer').glob('*.py'))
        deps.extend(p for p in (ROOT/'playbook-uplift/validation/logs').glob('*.log'))
    checks[name]={str(p.relative_to(ROOT)):sha(p) for p in deps}
py311=Path('/opt/homebrew/bin/python3.11').resolve()
if py311.exists():tools[str(py311)]=sha(py311)
# Original proof gates are dependencies too; their model definitions must not fall outside invalidation.
gsrc=ROOT/'.g5/Verus_verification/crates/ipm-load-history/src'
checks['G-inherited-proof']={str(p.relative_to(ROOT)):sha(p) for p in [gsrc/'lib.rs',gsrc/'model.rs',gsrc/'contract.rs',gsrc/'cases.rs',gsrc/'proofs.rs',HERE/'reproduce_baselines.py']}
for p in (ROOT/'playbook-uplift/models/verus').glob('*.rs'):
    if p.stem=='load_history':continue
    control=p.parent/'controls'/(p.stem+'.json')
    if not control.exists():continue
    inputs=[p,control,p.parent/'controls.py',HERE/'reproduce_baselines.py']
    checks['P-inherited-'+p.stem]={str(f.relative_to(ROOT)):sha(f) for f in inputs}

flag_deps=[V/'loadtx.rs',V/'check_flag_overrides.py',V/'flags_native_bridge.rs']
flag_deps.extend(f for f in (HERE/'flag-overrides-native/results/20261114T160126Z').rglob('*') if f.is_file())
flag_deps.extend(f for f in (HERE/'flag-overrides-native/fixtures').rglob('*') if f.is_file())
flag_deps.extend([HERE/'flag-overrides-native/run.py',HERE/'flag-overrides-native/Probe.cls'])
checks['flag-overrides-correspondence']={str(p.relative_to(ROOT)):sha(p) for p in flag_deps}

actual=HERE/'P-explorer-rerun/actual-inputs.json'
if actual.exists():
    audited=json.loads(actual.read_text())['checks']
    for old,new in {'load':'P-load-explorer','install-resolve':'P-install-resolver-explorer','namespace-sync':'P-namespace-sync-explorer','repository':'P-repository-explorer'}.items():
        checks[new]={}
        for name,digest in audited[old].items():
            p=Path(name)
            if ROOT in p.parents:checks[new][str(p.relative_to(ROOT))]=digest
            else:tools[name]=digest

(HERE/'check-inputs.json').write_text(json.dumps({'checks':checks,'tools':tools,'runtime':{'python':sys.version,'companion_XML_runtime':'Python3.11.10; Python3.14 pyexpat linkage error retained as incomplete initial attempt','platform':platform.platform(),'configuration':{'Verus':'0.2026.09.27.3cf1832','IRIS':'2026.1 build234U ARM64','NoNetworkDuringFormalChecks':True}},'limitations':['Native tools/platform receipts are observations; this manifest does not establish the completeness of dynamic environment dependencies.','Imported specs are transitive; native fixtures and driver inputs are content-bound. Unrecognized new runtime inputs require investigation.']},indent=2))
if '--inputs-only' in sys.argv:raise SystemExit(0)
edges=json.loads((HERE/'bindings.json').read_text());bound={e['member']:[] for e in edges if e['formal_symbol']}
for e in edges:
 if e['formal_symbol']:bound[e['member']].append(e['id'])
def members(tree):
 out={}
 for p in (tree/'src/cls/IPM').rglob('*.cls'):
  lines=p.read_text().splitlines(keepends=True);current=None;buf=[]
  def flush():
   if current:out[current]=''.join(buf)
  for line in lines:
   m=re.match(r'^(ClassMethod|Method|Query|Trigger)\s+("[^"]+"|[\w%]+)',line,re.I)
   if m:flush();current=str(p.relative_to(tree/'src/cls/IPM'))+'::'+m[2];buf=[]
   elif re.match(r'^(?:Class|Index|Parameter|Property|Relationship|Projection|ForeignKey|XData|Storage)\s',line,re.I):flush();current=None;buf=[]
   if current:buf.append(line)
   if line.startswith('}') or (m and line.rstrip().endswith(';')):flush();current=None;buf=[]
  flush()
 return out
def compare(a,b):
 old,new=members(a),members(b)
 # Inspect declarations and storage outside named member slices. A property initializer can change
 # constructor behavior without changing any bound method body.
 declaration_changes=[]
 for p in (a/'src/cls/IPM').rglob('*.cls'):
  rel=p.relative_to(a);q=b/rel
  if not q.exists():continue
  left=p.read_text();right=q.read_text();class_id=str(p.relative_to(a/'src/cls/IPM'))
  for mid,body in old.items():
   if mid.split('::')[0]==class_id:left=left.replace(body,'',1)
  for mid,body in new.items():
   if mid.split('::')[0]==class_id:right=right.replace(body,'',1)
  clean=lambda text:'\n'.join(z.strip() for z in text.splitlines() if z.strip() and not z.strip().startswith('//'))
  if clean(left)!=clean(right):declaration_changes.append(class_id)
 oldfiles={str(p.relative_to(a)):p for p in (a/'src').rglob('*') if p.is_file() and p.suffix!='.cls'}
 newfiles={str(p.relative_to(b)):p for p in (b/'src').rglob('*') if p.is_file() and p.suffix!='.cls'}
 outside_member_changes=[rel for rel in sorted(set(oldfiles)|set(newfiles)) if rel not in oldfiles or rel not in newfiles or sha(oldfiles[rel])!=sha(newfiles[rel])]
 changed={m for m in old.keys()&new.keys() if old[m]!=new[m]}
 removed=sorted(old.keys()-new.keys());added=sorted(new.keys()-old.keys());reopened={m:bound[m] for m in sorted(changed|set(removed)) if m in bound}
 for m in bound:
  if m.split('::')[0] in declaration_changes:reopened.setdefault(m,bound[m])
 formal_changed=set()
 for inputs in checks.values():
  for rel in inputs:
   x,y=a/rel,b/rel
   if x.exists()!=y.exists() or x.exists() and sha(x)!=sha(y):formal_changed.add(rel)
 selected={name:sorted(set(inputs)&formal_changed) for name,inputs in checks.items() if set(inputs)&formal_changed}
 writers=[m for m in added if re.search(r'\b(set|kill|merge|INSERT|UPDATE|DELETE)\b|%Save|Write\(',new[m],re.I)]
 registration_changes=[]
 for p in (a/'src/cls/IPM').rglob('*.cls'):
  rel=p.relative_to(a);q=b/rel
  oldregs=re.findall(r'(?sm)^XData\s+.*?^}',p.read_text());newregs=re.findall(r'(?sm)^XData\s+.*?^}',q.read_text()) if q.exists() else []
  if oldregs!=newregs:registration_changes.append(str(rel))
 old_env=json.loads((a/'verification-environment.json').read_text());new_env=json.loads((b/'verification-environment.json').read_text())
 if old_env!=new_env:
  for check in checks:selected.setdefault(check,[]).append('verification-environment.json')
 formal_id_changes={}
 for rel in formal_changed:
  if rel.endswith('.rs'):
   x,y=a/rel,b/rel
   oldf=set(re.findall(r'\bfn\s+(\w+)',x.read_text())) if x.exists() else set()
   newf=set(re.findall(r'\bfn\s+(\w+)',y.read_text())) if y.exists() else set()
   if oldf!=newf:formal_id_changes[rel]={'removed':sorted(oldf-newf),'added':sorted(newf-oldf)}
 affected_formal_edges={e['id'] for e in edges if e['formal_symbol'] and e['formal_symbol'].rsplit('::',1)[0] in formal_changed}
 return {'changed_members':sorted(changed),'removed_ids':removed,'added_ids':added,'reopened_product_correspondence':reopened,'selected_formal_checks':selected,'changed_check_inputs':sorted(formal_changed),'formal_id_changes':formal_id_changes,'affected_formal_binding_ids':sorted(affected_formal_edges),'new_writer_candidates':writers,'registration_changes':registration_changes,'declaration_changes':declaration_changes,'outside_member_source_changes':outside_member_changes,'uncertain_scope':bool(added or removed or registration_changes or formal_id_changes or declaration_changes or outside_member_changes),'body_proof_claim':False}
results=[]
with tempfile.TemporaryDirectory(prefix='ipm-drift-base-head-') as td:
 d=Path(td);base=d/'base';base.mkdir();(base/'src/cls').mkdir(parents=True)
 shutil.copytree(ROOT/'src/cls/IPM',base/'src/cls/IPM')
 for inputs in checks.values():
  for rel in inputs:
   target=base/rel;target.parent.mkdir(parents=True,exist_ok=True)
   if not target.exists():shutil.copyfile(ROOT/rel,target)
 def git(cwd,*args):return subprocess.check_output(['git','-C',str(cwd),*args],stderr=subprocess.STDOUT,text=True).strip()
 (base/'verification-environment.json').write_text(json.dumps({'tools':tools,'runtime':sys.version},sort_keys=True))
 git(base,'init','-q');git(base,'config','user.name','Disposable Binding Check');git(base,'config','user.email','binding-check@example.invalid');git(base,'add','.');git(base,'commit','-qm','frozen bound base');base_id=git(base,'rev-parse','HEAD')
 cases=['member-body','line-shift-outside','formal-definition','unrelated-edit','renamed-writer','new-writer-registration','fixture-change','formal-premise','tool-configuration','inherited-formal-definition','property-initializer','unbound-include']
 for name in cases:
  head=d/name;shutil.copytree(base,head)
  p=head/'src/cls/IPM/General/History.cls'
  if name=='property-initializer':
   p=head/'src/cls/IPM/Utils/JournalManager.cls';p.write_text(p.read_text().replace('InitialExpression = {..Enabled()}','InitialExpression = {0}',1))
  if name=='unbound-include':
   p=head/'src/inc/IPM/Common.inc';p.parent.mkdir(parents=True,exist_ok=True);p.write_text('#define NewUnclassifiedWriter ^NewWriter\n')
  if name=='inherited-formal-definition':
   p=head/'playbook-uplift/models/verus/settings.rs';p.write_text(p.read_text().replace('pub open spec fn default_value(', 'pub open spec fn default_value_renamed(',1))
  if name=='tool-configuration':
   p=head/'verification-environment.json';e=json.loads(p.read_text());e['tools']['simulated-bound-tool-version']='changed';p.write_text(json.dumps(e,sort_keys=True))
  if name=='member-body':s=p.read_text();i=s.index('ClassMethod SuccessToString');s=s[:i]+s[i:].replace('{','{\n    // changed bound member input',1);p.write_text(s)
  if name=='line-shift-outside':p.write_text('// unrelated prefix line shifts locations\n'+p.read_text())
  if name=='formal-definition':
   p=head/'.g5/Verus_verification/consolidated/verus/env_config.rs';p.write_text(p.read_text().replace('pub open spec fn clear(_old: Json) -> Json { empty(false) }','pub open spec fn clear(_old: Json) -> Json { _old }',1))
  if name=='unrelated-edit':(head/'unrelated.txt').write_text('No bound inputs changed.\n')
  if name=='renamed-writer':p.write_text(p.read_text().replace('ClassMethod SetHistoryLastPurge(', 'ClassMethod SetHistoryLastPurgeRenamed(',1))
  if name=='new-writer-registration':p.write_text(p.read_text()+'\nClassMethod NewlyIntroducedWriter()\n{\n set ^NewState=1\n}\nXData NewRegistration\n{\n<command name="newly-written"/>\n}\n')
  if name=='formal-premise':
   p=head/'.g5/Verus_verification/consolidated/verus/loadtx.rs';p.write_text(p.read_text().replace('requires w.journal,w.depth>0,w.commit_at.is_none()','requires w.journal,w.depth==0,w.commit_at.is_none()',1))
  if name=='fixture-change':
   inp=next((x for x in checks['env_config'] if '/fixtures/' in x),None);assert inp
   p=head/inp;p.write_bytes(p.read_bytes()+b'\n')
  git(head,'add','.');git(head,'commit','-qm',name);r=compare(base,head);r.update({'scenario':name,'base':base_id,'head':git(head,'rev-parse','HEAD')})
  if name=='member-body':assert 'General/History.cls::SuccessToString' in r['reopened_product_correspondence'] and not r['selected_formal_checks']
  if name in ['unrelated-edit','line-shift-outside']:assert not r['reopened_product_correspondence'] and not r['selected_formal_checks']
  if name=='formal-definition':assert 'env_config' in r['selected_formal_checks'] and 'attr_evaluator' in r['selected_formal_checks']
  if name=='renamed-writer':assert r['removed_ids'] and r['added_ids'] and r['uncertain_scope']
  if name=='new-writer-registration':assert r['new_writer_candidates'] and r['registration_changes']
  if name=='property-initializer':assert 'Utils/JournalManager.cls' in r['declaration_changes'] and r['reopened_product_correspondence'] and r['uncertain_scope']
  if name=='unbound-include':assert r['outside_member_source_changes'] and r['uncertain_scope']
  if name=='inherited-formal-definition':assert 'P-inherited-settings' in r['selected_formal_checks']
  if name=='tool-configuration':assert len(r['selected_formal_checks'])==len(checks)
  if name=='formal-premise':assert 'loadtx' in r['selected_formal_checks']
  if name=='fixture-change':assert 'env_config' in r['selected_formal_checks']
  results.append(r)
(HERE/'drift-results.json').write_text(json.dumps(results,indent=2));print(len(results),'base/head cases passed; correspondence reopening separate from formal rechecks')
