"""Native observations become assertions, never axioms. No product changes."""
import datetime, hashlib, importlib.util, json, re, subprocess, sys, tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parent
VERUS=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus'
def load(path,name):
 s=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(s);sys.path.insert(0,str(path.parent));s.loader.exec_module(m);return m
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def text(s):
 return 'seq!['+','.join("'"+c.replace('\\','\\\\').replace("'","\\'").replace('\n','\\n').replace('\r','\\r').replace('\t','\\t')+"'" for c in s)+']'
def verify(path):
 cmd=[str(VERUS),str(path),'--triggers-mode','silent','--no-cheating']
 p=subprocess.run(cmd,capture_output=True,text=True)
 return {'command':cmd,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
def run():
 m=load(ROOT/'models/semver.py','sv');fw=load(ROOT/'checks/framework.py','svfw')
 obs={r:fw.parse_obs(r) for r in m.RUNS}
 tests=[];cases=[]
 def add(case,field,formula,expected,run=None):
  tests.append('assert(('+formula+') == '+expected+') by(compute);')
  cases.append({'case':case,'field':field,'formula':formula,'expected':expected,'native_run':run or m.case_run(case)})
 for i,raw in enumerate(m.PL,1):
  case=f'V{i}';got=obs[m.case_run(case)][case,'parse'];parts,rest=got.split(' str=');major,minor,patch,pre,build=parts.split('|');out,snap,valid=re.fullmatch(r'(.*) snap=(\d) valid=(\d)',rest).groups()
  for key,val in zip(('major','minor','patch','pre','build'),(major,minor,patch,pre,build)): add(case,'parse.'+key,f'semver::parse({text(raw)}).{key}',text(val))
  add(case,'str',f'semver::fields_string(semver::parse({text(raw)}))',text(out))
  add(case,'snapshot',f'semver::snapshot(semver::v({text(raw)}))',str(snap=='1').lower())
  add(case,'validate',f'semver::validate(semver::parse({text(raw)}))',str(valid=='1').lower())
 for post in (0,1):
  for i,a in enumerate(m.FL,1):
   case=f'F{post}.{i}';bits=obs[m.case_run(case)][case,'row']
   for j,b in enumerate(m.FL):add(case,f'row.{j+1}',f'semver::follows(semver::v({text(a)}),semver::v({text(b)}),{str(bool(post)).lower()})',str(bits[j]=='1').lower())
 def resolved_range(raw):
  if raw=='':return 'seq![]',[]
  atoms=[];meta=[]
  for token in raw.split(' '):
   mm=re.fullmatch(r'(<=|>=|<|>|=)([0-9]+)\.([0-9]+)\.([0-9]+)(?:-([^+]+))?(?:\+(.+))?',token)
   assert mm,token
   op,ma,mi,pa,pre,build=mm.groups();op={'=':'Eq','<':'Lt','<=':'Le','>':'Gt','>=':'Ge'}[op]
   vv='semver::Version{major:'+ma+',minor:'+mi+',patch:'+pa+',pre:'+text(pre or '')+',build:'+text(build or '')+'}'
   atoms.append('semver::Cmp{op:semver::Op::'+op+',v:'+vv+'}');meta.append((op,ma,mi,pa,pre or '',build or ''))
  return 'seq!['+','.join(atoms)+']',meta
 # Separate lexical expansion and semantic satisfaction; both are checked against the same native row.
 # This prevents recursive compute's fixed-depth limit from hiding correspondence failures.
 for i,raw in enumerate(m.XL,1):
  if any(c in raw for c in ' &|()') or raw in ('a.b.c','01.2.3','1.2.3-01'):continue
  case=f'X{i}';got=obs[m.case_run(case)].get((case,'res'))
  if got is None:continue
  resolved,bits=got.split(' sat=');rr,metadata=resolved_range(resolved)
  add(case,'resolved.render','semver::range_text('+rr+',0)',text(resolved))
  add(case,'expanded.length',f'semver::expand({text(raw)}).len()',str(len(metadata)))
  for k,(op,ma,mi,pa,pre,build) in enumerate(metadata):
   add(case,f'expanded.{k}.op',f'semver::expand({text(raw)})[{k}].op is {op}','true')
   for field,val in [('major',ma),('minor',mi),('patch',pa),('pre',text(pre)),('build',text(build))]:
    add(case,f'expanded.{k}.{field}',f'semver::expand({text(raw)})[{k}].v.{field}',val)
  for j,ver in enumerate(m.VL):add(case,f'sat.{j+1}',f'semver::range_holds({rr},semver::v({text(ver)}),false,0)',str(bits[j]=='1').lower())
 for pre,old,new in (('HB','2.0.0-beta','1.0.0-rc'),('HR','2.0.0','1.0.0'),('HM','2.0.0-beta','1.0.0')):
  case=pre+'2';got=obs[m.case_run(case)][case,'status'];add(case,'status',f'semver::load_refused(semver::v({text(old)}),semver::v({text(new)}),false)',str(got=='ERR').lower())
 # Range intersection is separately input by its parsed comparators; parser endpoints above are independently tested.
 def rng(raw):
  r=m.range_from_string(raw);return 'seq!['+','.join('semver::Cmp{op:semver::Op::'+{'=':'Eq','<':'Lt','<=':'Le','>':'Gt','>=':'Ge'}[c['Operator']]+',v:semver::cv('+','.join(str(int(c[x])) for x in ('Major','Minor','Patch'))+','+text(c['Prerelease'])+')}' for c in r)+']'
 for i,(a,b) in enumerate(m.AL[:4],1):
  case=f'A{i}';bits=obs[m.case_run(case)][case,'res'].split(' sat=')[1]
  for j,ver in enumerate(m.VL):add(case,f'sat.{j+1}',f'semver::range_holds(semver::good_range(semver::intersect({rng(a)},{rng(b)},false)),semver::v({text(ver)}),false,0)',str(bits[j]=='1').lower())
 # Command-level S1-S3 use actual offered versions with the native parsed range.
 for case,raw,offered in [('S1','<2',['2.5.0','1.5.0']),('S2','>=1.2',['1.3.0']),('S3','>=1.2',['1.5.0','1.2.0'])]:
  rr=rng(raw);offer='seq!['+','.join('semver::cv('+','.join(v.split('.'))+',seq![])' for v in offered)+']'
  got=obs[m.case_run(case)][case,'status'];add(case,'status',f'semver::chosen({rr},{offer},false,0) is Some',str(got=='OK').lower())
 # Diamond uses separately checked current-first search and name-order final reader.
 for case,a,b in [('S4ab','^1.0.0','<2.0.0'),('S4ba','^1.0.0','<2.0.0'),('S5','<2.0.0','^1.0.0')]:
  search='semver::good_range(semver::intersect('+rng(b)+','+rng(a)+',false))';check='semver::good_range(semver::intersect('+rng(a)+','+rng(b)+',false))'
  native_case=obs[m.case_run(case)]
  if case!='S5': selected_text=re.search(r'Requested version \([^ ]+ ([^)]+)\)',native_case[case,'error'])[1]
  else: selected_text=re.search(r'version=([^ ]+)',native_case[case,'mod.shared-zz'])[1]
  selected=m.sv_from_string(selected_text)
  target='semver::Version{major:'+selected['Major']+',minor:'+selected['Minor']+',patch:'+selected['Patch']+',pre:'+text(selected['Prerelease'])+',build:'+text(selected['Build'])+'}'
  choice='semver::chosen('+search+',semver::native_offered(),false,0)'
  add(case,'selected.exists',choice+' is Some','true')
  for f,key in [('major','Major'),('minor','Minor'),('patch','Patch'),('pre','Prerelease'),('build','Build')]:
   val=selected[key] if f in ('major','minor','patch') else text(selected[key]);add(case,'selected.'+f,choice+'.unwrap().'+f,val)
  got=obs[m.case_run(case)][case,'status'];add(case,'final.validation','semver::range_holds('+check+','+target+',false,0)',str(got=='OK').lower())
 # Check lexical expression composition directly, including hyphen ranges, unions and inline &&.
 for i,raw in enumerate(m.XL,1):
  case=f'X{i}';o=obs[m.case_run(case)]
  if raw=='01.2.3' or (case,'res') not in o:continue
  resolved=o[case,'res'].split(' sat=')[0];base='semver::parsed(semver::expression_set('+text(raw)+'))'
  add(case,'expression.parsed','semver::expression_set('+text(raw)+') is Parsed','true')
  ranges=[] if resolved=='' else [r.strip('()') for r in resolved.split(' || ')]
  add(case,'expression.options',base+'.len()',str(len(ranges)))
  typed_expr='seq!['+','.join(resolved_range(r)[0] for r in ranges)+']'
  add(case,'expression.render','semver::expr_text('+typed_expr+',0)',text(resolved))
  for j,r in enumerate(ranges):
   rr,meta=resolved_range(r);add(case,f'expression.{j}.length',base+f'[{j}].len()',str(len(meta)))
   for k,(op,ma,mi,pa,pre,build) in enumerate(meta):
    add(case,f'expression.{j}.{k}.op',base+f'[{j}][{k}].op is '+op,'true')
    for field,val in [('major',ma),('minor',mi),('patch',pa),('pre',text(pre)),('build',text(build))]:
     add(case,f'expression.{j}.{k}.{field}',base+f'[{j}][{k}].v.'+field,val)
 add('X42','mixed.error','semver::expression_set('+text(m.XL[41])+') is Mixed','true')
 add('X33','empty.error','semver::expression_set('+text(m.XL[32])+') is NoOverlapInline','true')
 add('X34','exact.error','semver::expression_set('+text(m.XL[33])+') is ExactConflict','true')
 add('X33','diagnostic','semver::and_error(semver::Intersection::Empty,true)',text(obs[m.case_run('X33')]['X33','err'].removeprefix('ERROR #5001: ')))
 source='#[path="semver.rs"] mod semver;\nuse vstd::prelude::*;\nverus!{\n'+''.join(f'proof fn native_{i}(){{{s}}}\n' for i,s in enumerate(tests))+'}\nfn main(){}\n'
 native=HERE/'semver_native.rs';native.write_text(source)
 baseline=verify(HERE/'semver.rs');native_result=verify(native)
 controls={}
 for name,before,after in [
  ('total-prerelease-order','if a.pre.len()>0 && b.pre.len()>0 {false}','if a.pre.len()>0 && b.pre.len()>0 {core_gt(a,b)}'),
  ('last-bound-tie','compare_versions(old,c)>0)','compare_versions(old,c)>=0)'),
  ('caret-major-for-partial','if wildcard(minor) {',"if wildcard(minor) || (s.len()>0 && s[0]=='^') {"),
  ('build-hyphen-preserved',"pre: before(after(s,'-'),'+')","pre: before(after(before(s,'+'),'-'),'+')")]:
  src=(HERE/'semver.rs').read_text();assert before in src,name
  with tempfile.TemporaryDirectory(prefix='ipm-semver-control-') as d:
   p=Path(d)/'semver.rs';p.write_text(src.replace(before,after,1));r=verify(p);controls[name]={'rejected':r['exit_code']!=0 and bool(re.search(r'postcondition not satisfied|expression simplifies|assertion failed',r['stderr'])) and 'recursion depth' not in r['stderr'],**r}
 input_files=[Path(__file__),HERE/'semver.rs',native,Path(m.__file__),Path(fw.__file__),HERE/'semver.manifest.json']
 for receipt in m.RUNS:
  input_files += [q for q in (ROOT/'native/results'/receipt).iterdir() if q.is_file()]
 input_files += [q for q in [VERUS,VERUS.with_name('rust_verify'),VERUS.with_name('z3'),VERUS.with_name('vstd.vir'),VERUS.with_name('libvstd.rlib')] if q.exists()]
 report={'family':'semantic-versions','timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat(),'product_pin':'571e04a19aba9f9b4bc0ea23fbe09182038f4e55','model':baseline,'native':native_result,'matched_assertions':len(tests) if native_result['exit_code']==0 else 0,'attempted_assertions':len(tests),'cases':cases,'negative_controls':controls,'fingerprints':{str(p.relative_to(ROOT)):sha(p) for p in [HERE/'semver.rs',Path(m.__file__),VERUS] if p.is_relative_to(ROOT)},'tool_sha256':sha(VERUS),'receipt_sha256':{r:sha(ROOT/'native/results'/r/'raw.log') for r in m.RUNS},'input_sha256':{str(p):sha(p) for p in input_files},'python_version':sys.version,'open':['Accepted lexical union/inline AND/hyphen forms are represented and fixture-checked; malformed token validation and complete diagnostics remain open.','The P dep_refines theorem retains its exact-ISet-algebra, nat-order premises; no equivalence theorem to that whole resolver is claimed.','Unicode/locale/numeric-sign or decimal-string collation outside unsigned canonical ASCII forms is open.']}
 (HERE/'semver-results.json').write_text(json.dumps(report,indent=2));print(json.dumps({k:report[k] for k in ['family','matched_assertions','attempted_assertions']}));print('model',baseline['exit_code'],'native',native_result['exit_code'],'controls',{k:v['rejected'] for k,v in controls.items()});print(native_result['stderr'][:4000]);return int(baseline['exit_code']!=0 or native_result['exit_code']!=0 or not all(c['rejected'] for c in controls.values()))
if __name__=='__main__':sys.exit(run())
