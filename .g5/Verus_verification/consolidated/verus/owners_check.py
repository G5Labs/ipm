"""Checks typed command transitions against recorded native pre/post observations."""
import datetime,json,re,sys,tempfile
from pathlib import Path
from semver_check import HERE,ROOT,VERUS,load,text,verify,sha

def run():
 m=load(ROOT/'models/owners.py','ow');fw=load(ROOT/'checks/framework.py','owfw');obs={r:fw.parse_obs(r) for r in m.RUNS};tests=[];cases=[]
 def add(case,field,formula,expected):
  tests.append('assert(('+formula+') == '+expected+') by(compute);');cases.append({'case':case,'field':field,'formula':formula,'expected':expected,'receipt':str(ROOT/'native/results'/m.case_run(case)/'raw.log')})
 def version(raw):
  v=m.sv(raw);return 'semver::Version{major:'+str(v['Major'])+',minor:'+str(v['Minor'])+',patch:'+str(v['Patch'])+',pre:'+text(v['Pre'])+',build:'+text(v['Build'])+'}'
 def state(raw): return 'owners::ModuleState{record:'+version(raw)+',manifest:'+version(raw)+',disk:'+version('1.2.3')+'}'
 old='1.2.3'
 requests=[('MV1','Minor',False,None,None,True),('MV2','Explicit(semver::parse('+text('1.0.0')+'))',False,None,None,True),('MV3','Explicit(semver::parse('+text('1.0.0')+'))',True,None,None,True),('MV4','Patch',False,'rc.1','b7',True),('MV5','Current',False,None,None,True),('MV6','Explicit(semver::parse('+text('2.0.0-beta')+'))',False,None,None,True),('MV7','Explicit(semver::parse('+text('01.0.0')+'))',False,None,None,True),('MV8','Major',False,None,None,True),('MV9','Current',False,None,None,False)]
 for case,req,force,pre,build,exists in requests:
  o=obs[m.case_run(case)];cur,doc=o[case,'mv'].split(' doc=');base='owners::module_version('+state(old)+',owners::Request::'+req+','+('None' if pre is None else 'Some('+text(pre)+')')+','+('None' if build is None else 'Some('+text(build)+')')+','+str(force).lower()+','+str(exists).lower()+',true,true,true,true,0)'
  add(case,'status',base+'.ok',str(o[case,'status']=='OK').lower())
  for kind,raw in [('record',cur),('manifest',doc)]:
   expected=m.sv(raw)
   for field,key in [('major','Major'),('minor','Minor'),('patch','Patch'),('pre','Pre'),('build','Build')]:
    add(case,'stored.'+kind+'.'+field,base+'.state.'+kind+'.'+field,str(expected[key]) if field in ('major','minor','patch') else text(expected[key]))
  add(case,'tlevel.after',base+'.tlevel',o[case,'tlevel.after']);old=cur
 add('MV10','disk','semver::version_text('+state(old)+'.disk)',text(obs[m.case_run('MV10')]['MV10','file']))
 def entry(key,depth,kind):return f'owners::Entry{{key:{key},depth:{depth},manifest:owners::Manifest::{kind}}}'
 es='seq!['+','.join([entry(1,1,'Valid'),entry(2,1,'Valid'),entry(3,1,'Unreadable')])+']';one='seq!['+','.join([entry(1,1,'Valid'),entry(3,1,'Unreadable')])+']';repo='owners::Repository{exists:true,cache:set![1nat,2nat]}'
 for case,call in [('LK1','owners::build_cache(owners::Repository{exists:false,cache:Set::empty()},'+es+',0,true,false,true,0)'),('LK2','owners::build_cache('+repo+','+es+',0,true,true,true,0)'),('LK3','owners::build_cache('+repo+','+one+',0,true,false,true,0)'),('LK5','owners::build_cache('+repo+','+es+',0,true,false,true,1)')]:
  expected=obs[m.case_run(case)][case,'build'].split()[0]=='1';add(case,'build',call+'.ok',str(expected).lower())
  if case in ('LK1','LK3'):add(case,'cached.lk-a',call+'.repo.cache.contains(1)', 'true');add(case,'cached.lk-b',call+'.repo.cache.contains(2)',str(case=='LK1').lower())
  if case=='LK5':add(case,'tlevel.inside',call+'.tlevel','1');add(case,'lockable',call+'.lock_held','true')
 deep='seq!['+','.join([entry(1,1,'Valid'),entry(2,2,'InvalidName')])+']'
 for depth in (0,1):
  case=f'DC{depth}';expected=obs[m.case_run(case)][case,'buildcache']=='1';add(case,'buildcache','!owners::scanned_error('+deep+f',{depth},0)',str(expected).lower())
 add('LK1','unreadable','owners::add_cache('+entry(3,1,'Unreadable')+') is Skipped','true');add('TR7','badname','owners::add_cache('+entry(4,1,'InvalidName')+') is Error','true')
 lines='seq![owners::Line::Start,owners::Line::Generated(3),owners::Line::End,owners::Line::User(1)]';gen='seq![owners::Line::Start,owners::Line::Generated(8),owners::Line::End]'
 for case,raw in [('LE1','owners::zlang_one('+lines+','+gen+',false)'),('LE2','owners::zlang_one(owners::zlang_one('+lines+','+gen+',false),'+gen+',false)')]:
  got=obs[m.case_run(case)][case,'zlang'];u=int(re.search(r'user:(\d+)',got)[1]);add(case,'user.count','owners::outside_users('+raw+',0).len()',str(u));add(case,'user.identity','owners::outside_users('+raw+',0)[0]','1')
 src='#[path="owners.rs"] mod owners;\nuse owners::semver;\nuse vstd::prelude::*;\nverus!{\n'+''.join(f'proof fn native_{i}(){{{s}}}\n' for i,s in enumerate(tests))+'}\nfn main(){}\n';p=HERE/'owners_native.rs';p.write_text(src);baseline=verify(HERE/'owners.rs');native=verify(p)
 controls={}
 for name,before,after in [('writes-disk','disk:s.disk','disk:v'),('same-version-skips-before-guard','else if !force && !semver::follows(candidate,s.record,false)','else if !force && candidate!=s.record && !semver::follows(candidate,s.record,false)'),('skip-invalid-name','Manifest::InvalidName=>CacheAction::Error','Manifest::InvalidName=>CacheAction::Skipped'),('ignore-scan-depth','depth==0 || e.depth<=depth','true'),('drop-user-lines','seq![lines[i as int]] + rewrite_from','seq![] + rewrite_from')]:
  original=(HERE/'owners.rs').read_text();assert before in original,name
  with tempfile.TemporaryDirectory(prefix='ipm-owners-control-') as d:
   d=Path(d);(d/'semver.rs').write_bytes((HERE/'semver.rs').read_bytes());q=d/'owners.rs';q.write_text(original.replace(before,after,1));r=verify(q);controls[name]={'rejected':r['exit_code']!=0 and bool(re.search(r'postcondition not satisfied|expression simplifies|assertion failed',r['stderr'])) and 'recursion depth' not in r['stderr'],**r}
 input_files=[Path(__file__),HERE/'semver_check.py',HERE/'owners.rs',HERE/'semver.rs',p,Path(m.__file__),Path(fw.__file__),HERE/'owners.manifest.json']
 for receipt in m.RUNS:
  input_files += [q for q in (ROOT/'native/results'/receipt).iterdir() if q.is_file()]
 input_files += [q for q in [VERUS,VERUS.with_name('rust_verify'),VERUS.with_name('z3'),VERUS.with_name('vstd.vir'),VERUS.with_name('libvstd.rlib')] if q.exists()]
 report={'family':m.FAMILY,'timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat(),'model':baseline,'native':native,'attempted_assertions':len(tests),'matched_assertions':len(tests) if native['exit_code']==0 else 0,'cases':cases,'negative_controls':controls,'tool_sha256':sha(VERUS),'fingerprints':{str(p.relative_to(ROOT)):sha(p) for p in [HERE/'owners.rs',HERE/'semver.rs',Path(m.__file__)]},'receipt_sha256':{r:sha(ROOT/'native/results'/r/'raw.log') for r in m.RUNS},'input_sha256':{str(p):sha(p) for p in input_files},'python_version':sys.version,'open':['Constructor release-mode distinction remains open; real transaction LK5 does not distinguish modes.','Typed command actions compare observed prestate to poststate; CLI token parsing and XML/routine/environment ports stay explicit boundary contracts.']};(HERE/'owners-results.json').write_text(json.dumps(report,indent=2));print(json.dumps({k:report[k] for k in ['family','matched_assertions','attempted_assertions']}));print('model',baseline['exit_code'],'native',native['exit_code'],'controls',{k:v['rejected'] for k,v in controls.items()});print(native['stderr'][:4000]);return int(baseline['exit_code']!=0 or native['exit_code']!=0 or not all(c['rejected'] for c in controls.values()))
if __name__=='__main__':sys.exit(run())
