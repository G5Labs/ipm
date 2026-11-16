#!/usr/bin/env python3
"""Replayable proof and native-conformance harness; no product mutation or trusted proof bodies."""
from pathlib import Path
import datetime, hashlib, json, os, re, resource, subprocess, sys, time, tempfile
PIN='571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
TOOL=Path(os.environ.get('VERUS',str(Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def call(cmd, **kw):
 t=time.monotonic(); b=resource.getrusage(resource.RUSAGE_CHILDREN)
 p=subprocess.run(cmd,capture_output=True,text=True,timeout=180,**kw);a=resource.getrusage(resource.RUSAGE_CHILDREN)
 return p,{'command':cmd,'exit':p.returncode,'wall_seconds':time.monotonic()-t,'child_cpu_seconds':a.ru_utime+a.ru_stime-b.ru_utime-b.ru_stime,'raw':p.stdout+p.stderr}
def proof(path,out,options=()):
 p,r=call([str(TOOL),str(path),'--triggers-mode','silent']+list(options));out.parent.mkdir(parents=True,exist_ok=True);out.write_text(r.pop('raw'));raw=out.read_text();attempts=out.parent/'attempts';attempts.mkdir(exist_ok=True);(attempts/(str(time.time_ns())+'.log')).write_text(raw)
 m=re.search(r'verification results:: (\d+) verified, (\d+) errors',raw)
 r.update(verified=int(m[1]) if m else None,errors=int(m[2]) if m else None,log=str(out),assertion_rejected=p.returncode!=0 and ('assertion failed' in raw or 'postcondition not satisfied' in raw or 'evaluates to false' in raw or 'expression simplifies to false' in raw) and not any(t in raw.lower() for t in ['resource limit','rlimit','timed out','time limit exceeded']))
 return r
def output_observations(raw,cfg):
 clean=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',raw).replace('\r\n','\n')
 values={}
 for item in cfg.get('output_observations',[]):
  begin='BEGIN|'+item['command']+'\n';end='END|'+item['command']+'\n'
  assert clean.count(begin)==1,(item['command'],'missing/duplicate command start')
  part=clean.split(begin,1)[1];assert end in part,item['command'];part=part.split(end,1)[0]
  assert item['key'] not in values,item['key']
  values[item['key']]=int(re.search(item['pattern'],part,re.M) is not None)
 if cfg.get('report_json'):
  from report_observer import observe
  report=observe(raw);assert not set(values)&set(report);values.update(report)
 if cfg.get('cache_trace'):
  from cache_observer import observe
  trace=observe(raw);assert not set(values)&set(trace);values.update(trace)
 return values
def main(here):
 here=Path(here).resolve(); root=here.parents[3]; n=here.name; out=root/'.g5/uplift/consolidated'/n;out.mkdir(exist_ok=True)
 cfg=json.loads((here/'cases.json').read_text());native_dir=cfg.get('native_dir',n);assert re.fullmatch(r'iter[0-9]+',native_dir); started=time.monotonic(); records=[]
 for file,mutations in ({} if '--native-only' in sys.argv else cfg['models']).items():
  src=here/file; text=src.read_text(); b=proof(src,out/'proofs'/f'{file}.log');assert b['exit']==0 and b['errors']==0,b
  controls=[];positive_controls={}
  def selected_options(name):
   selected=cfg.get('control_functions',{}).get(file,{}).get(name);options=list(cfg.get('control_options',[]))
   if not selected:return options,None,None
   assert re.fullmatch(r'[A-Za-z_]\w*',selected),selected
   options+=['--verify-root','--verify-function',selected]
   if selected not in positive_controls:
    good=out/'proofs/positive-controls'/file/(selected+'.rs');good.parent.mkdir(parents=True,exist_ok=True)
    good.write_text(re.sub(r'#\[path="([^"]+)"\]',lambda m:'#[path="'+os.path.relpath((src.parent/m[1]).resolve(),good.parent)+'"]',text))
    neighbor=proof(good,good.with_suffix('.log'),options);assert neighbor['exit']==0 and neighbor['errors']==0 and neighbor['verified']==1,neighbor
    positive_controls[selected]=neighbor
   return options,selected,positive_controls[selected]
  for name,(old,new) in mutations.items():
   assert text.count(old)==1,(file,name,text.count(old)); target=out/'proofs/controls'/file/(name+'.rs');target.parent.mkdir(parents=True,exist_ok=True);patched=text.replace(old,new,1)
   patched=re.sub(r'#\[path="([^"]+)"\]',lambda m:'#[path="'+os.path.relpath((src.parent/m[1]).resolve(),target.parent)+'"]',patched)
   target.write_text(patched)
   options,selected,neighbor=selected_options(name)
   r=proof(target,target.with_suffix('.log'),options);assert r['assertion_rejected'],r; controls.append(dict(id=name,source=str(target.relative_to(root)),selected_function=selected,passing_neighbor=neighbor,**r))
  for name,mutation in cfg.get('dependency_controls',{}).get(file,{}).items():
   dep=root/mutation['path'];dep_text=dep.read_text();assert dep_text.count(mutation['old'])==1
   folder=out/'proofs/dependency-controls'/name;folder.mkdir(parents=True,exist_ok=True)
   changed=folder/dep.name;changed.write_text(dep_text.replace(mutation['old'],mutation['new'],1))
   target=folder/file
   def link(m):
    resolved=(src.parent/m[1]).resolve();dest=changed if resolved==dep.resolve() else resolved
    return '#[path="'+os.path.relpath(dest,target.parent)+'"]'
   target.write_text(re.sub(r'#\[path="([^"]+)"\]',link,text))
   options,selected,neighbor=selected_options(name)
   r=proof(target,target.with_suffix('.log'),options);assert r['assertion_rejected'],r
   controls.append(dict(id=name,source=str(target.relative_to(root)),dependency=str(changed.relative_to(root)),selected_function=selected,passing_neighbor=neighbor,**r))
  records.append({'file':file,'sha256':sha(src),'baseline':b,'controls':controls})
 (out/'proofs').mkdir(exist_ok=True)
 (out/'proofs/models.json').write_text(json.dumps({'tool_sha256':sha(TOOL),'models':records,'implementation_proofs':0},indent=2)+'\n')
 if '--no-native' not in sys.argv:
  stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ');dest=out/'native'/stamp;dest.mkdir(parents=True)
  c='ipm-'+n+'-'+stamp.lower();image='ipm-consolidated-iris:571e04a1-fresh';cmds=[]
  def run(cmd,**kw):
   p,r=call(cmd,**kw);cmds.append({**r,**({'stdin':kw['input']} if 'input' in kw else {})});assert not p.returncode,r;return p.stdout
  try:
   run(['docker','run','-d','--name',c]+(['--network',os.environ['IPM_NATIVE_NETWORK']] if 'IPM_NATIVE_NETWORK' in os.environ else ['--network','none'])+[image])
   actual_image_id=run(['docker','inspect',c,'--format','{{.Image}}']).strip()
   for attempt in range(60):
    p,r=call(['docker','exec','-i',c,'iris','session','IRIS','-U','USER'],input='write "READY",!\nhalt\n')
    if p.returncode==0 and 'READY' in p.stdout:break
    time.sleep(.5)
   else:raise RuntimeError('IRIS not ready')
   expected={str(p.relative_to(root)):sha(p) for folder in ['src','preload','tests','modules'] for p in (root/folder).rglob('*') if p.is_file()}
   script='from pathlib import Path;import hashlib,json;r=Path("/home/irisowner/zpm");q='+repr(list(expected))+';print(json.dumps({p:hashlib.sha256((r/p).read_bytes()).hexdigest() if (r/p).is_file() else None for p in q}))'
   actual=json.loads(run(['docker','exec',c,'python3','-c',script])); mismatches=[p for p in expected if expected[p]!=actual[p]]
   (dest/'source-pin.json').write_text(json.dumps({'pin':PIN,'source_sha256':expected,'mismatches':mismatches},indent=2)+'\n');assert not mismatches,mismatches[:5]
   run(['docker','cp',str(here/'native'),c+':/tmp/'+native_dir])
   run(['docker','exec','-u','root',c,'chmod','-R','a+rwX','/tmp/'+native_dir])
   raw=run(['docker','exec','-i',c,'iris','session','IRIS','-U','USER'],input='\n'.join(cfg['session']+['halt'])+'\n');(dest/'session.log').write_text(raw)
   obs={}
   for line in raw.splitlines():
    if line.startswith('OBS|'):
     _,key,value=line.split('|',2);assert key not in obs,key;obs[key]=int(value.strip())
   rendered=output_observations(raw,cfg);assert not set(rendered)&set(obs);obs.update(rendered)
   assert set(cfg['replay'])<=set(obs),(set(cfg['replay'])-set(obs),raw[-3000:])
   assert all(obs.get(k)==v for k,v in cfg.get('native_expected',{}).items()),[(k,v,obs.get(k)) for k,v in cfg.get('native_expected',{}).items() if obs.get(k)!=v]
   (dest/'observations.json').write_text(json.dumps({'pin':PIN,'image':image,'actual_container_image_id':actual_image_id,'observations':obs,'commands':cmds,'fixture_sha256':{str(p.relative_to(root)):sha(p) for p in (here/'native').rglob('*') if p.is_file()}},indent=2)+'\n')
   (out/'native/LATEST').write_text(stamp+'\n')
  finally:subprocess.run(['docker','rm','-f',c],capture_output=True)
 if '--native-only' in sys.argv:
  print(json.dumps({'native_only':True,'stamp':(out/'native/LATEST').read_text().strip(),'formal_gate_run':False}));return
 stamp=(out/'native/LATEST').read_text().strip();native=json.loads((out/'native'/stamp/'observations.json').read_text());obs=native['observations']
 assert native['actual_container_image_id']==cfg['native_image_id'],'Native container image identity differs from configured provider pin'
 assert all(obs.get(k)==val for k,val in output_observations((out/'native'/stamp/'session.log').read_text(),cfg).items()),'CLI output replay changed'
 assert native['fixture_sha256']=={str(p.relative_to(root)):sha(p) for p in (here/'native').rglob('*') if p.is_file()},'Native fixture bytes changed; fresh execution required'
 assert all(obs.get(k)==v for k,v in cfg.get('native_expected',{}).items()),[(k,v,obs.get(k)) for k,v in cfg.get('native_expected',{}).items() if obs.get(k)!=v]
 bridge=out/'replay/native.rs';bridge.parent.mkdir(exist_ok=True)
 head='#[path="'+os.path.relpath(here/cfg['replay_model'],bridge.parent)+'"] mod model;\nuse model::*;\nuse vstd::prelude::*;\nverus! {\npub proof fn observations() {\n'
 assertions=['assert(('+expr+') == '+str(obs[key])+'); // '+key for key,expr in cfg['replay'].items()]
 if cfg.get('replay_grouped'):
  assert not cfg.get('replay_preludes'),'Grouped replay requires one common prelude'
  bridge.write_text(head+cfg.get('replay_prelude','')+'\n'+'\n'.join(assertions)+'\n}\n}\nfn main() {}\n')
 else:
  bridge.write_text(head.replace('pub proof fn observations() {\n','')+''.join('pub proof fn observation_'+str(i)+'() {\n'+cfg.get('replay_preludes',{}).get(key,cfg.get('replay_prelude',''))+assertions[i]+'\n}\n' for i,key in enumerate(cfg['replay']))+'}\nfn main() {}\n')
 replay=proof(bridge,bridge.with_suffix('.log'));assert replay['exit']==0,replay
 flipped=[]
 for i,(key,expr) in enumerate(list(cfg['replay'].items())[:3]):
  prelude=cfg.get('replay_preludes',{}).get(key,cfg.get('replay_prelude',''))
  focused=head.replace('pub proof fn observations() {\n','')+'pub proof fn observation_'+str(i)+'() {\n'+prelude+assertions[i]+'\n}\n}\nfn main() {}\n'
  good=bridge.parent/('focused-good'+str(i)+'.rs');good.write_text(focused)
  passing=proof(good,good.with_suffix('.log'));assert passing['exit']==0,passing
  changed=1-obs[key] if obs[key] in (0,1) else obs[key]+1
  f=bridge.parent/('focused-flip'+str(i)+'.rs');f.write_text(focused.replace('== '+str(obs[key])+'); // '+key,'== '+str(changed)+'); // '+key,1))
  r=proof(f,f.with_suffix('.log'));assert r['assertion_rejected'],r
  flipped.append({'field':key,'observed':obs[key],'changed':changed,'passing_neighbor':passing,**r})
 (out/'replay/receipt.json').write_text(json.dumps({'observations':len(assertions),'baseline':replay,'flipped':flipped},indent=2)+'\n')
 result={'ok':True,'wall_seconds':time.monotonic()-started,'verified':sum(r['baseline']['verified'] for r in records),'controls':sum(len(r['controls']) for r in records),'native_fields':len(obs),'replayed_fields':len(assertions),'model_source_sha256':{f:sha(here/f) for f in cfg['models']},'implementation_proofs':0}
 (out/'run.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
if __name__=='__main__':main(Path(sys.argv[1]))
