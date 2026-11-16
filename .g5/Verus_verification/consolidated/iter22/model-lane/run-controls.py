from pathlib import Path
import hashlib,json,os,re,subprocess,time

root=Path('.g5/Verus_verification/consolidated/iter22')
out=root/'model-lane/controls'
out.mkdir(parents=True,exist_ok=True)
source=root/'reinstall.rs'
cfg=json.loads((root/'reinstall-cases.json').read_text())
tool=Path('/Users/ziniuw/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')
base=source.read_text()
imports=re.findall(r'#\[path="([^"]+)"\]\s*pub mod ([A-Za-z_][A-Za-z_0-9]*);',base)
runs=[]
receipt={'model_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),
 'config_sha256':hashlib.sha256((root/'reinstall-cases.json').read_bytes()).hexdigest(),
 'import_sha256':{path:hashlib.sha256((root/path).read_bytes()).hexdigest() for path,name in imports},
 'tool_sha256':hashlib.sha256(tool.read_bytes()).hexdigest(),
 'implementation_proofs':0,'runs':runs}

def save():
 (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')

def run(name,path,reject=False,selector=None):
 cmd=[str(tool),str(path),'--triggers-mode','silent']
 if reject: cmd+=cfg['control_options']
 if selector: cmd+=['--verify-root','--verify-function',selector]
 started=time.monotonic()
 log=out/(name+'.log')
 with log.open('w') as stream:
  try:
   p=subprocess.run(cmd,stdout=stream,stderr=subprocess.STDOUT,text=True,timeout=120)
  except subprocess.TimeoutExpired:
   runs.append({'name':name,'command':cmd,'wall_seconds':time.monotonic()-started,
    'verified':None,'errors':None,'selector':selector,'resource_limit':True,'accepted':False,'timeout_seconds':120})
   save()
   raise
 raw=log.read_text()
 semantic=any(s in raw for s in ['assertion failed','postcondition not satisfied','evaluates to false','expression simplifies to false'])
 limited=any(s in raw.lower() for s in ['resource limit','rlimit','timed out','timeout','time limit','time-limit','exceeded maximum recursion'])
 counts=re.search(r'verification results:: (\d+) verified, (\d+) errors',raw)
 accepted=(p.returncode!=0 and semantic and not limited and not re.search(r'error\[E',raw)) if reject else p.returncode==0
 if selector and not reject: accepted=accepted and counts is not None and int(counts[1])==1
 runs.append({'name':name,'command':cmd,'exit':p.returncode,'wall_seconds':time.monotonic()-started,
  'verified':int(counts[1]) if counts else None,'errors':int(counts[2]) if counts else None,
  'selector':selector,'semantic_rejection':semantic and not limited,'resource_limit':limited,'accepted':accepted})
 save()
 print(name,p.returncode,'accepted='+str(accepted),flush=True)
 assert accepted,raw

run('baseline',source)
for selector in sorted(set(cfg['control_functions']['reinstall.rs'].values())):
 run('focused-good-'+selector,source,selector=selector)
inherited=out/'inherited-only.rs'
inherited.write_text('use vstd::prelude::*;\n'+'\n'.join('#[path="'+os.path.relpath((root/path).resolve(),out)+'"] mod '+name+';' for path,name in imports)+'\nfn main() {}\n')
run('inherited-only',inherited)
for name,(old,new) in cfg['models']['reinstall.rs'].items():
 assert base.count(old)==1,(name,base.count(old))
 target=out/(name+'.rs')
 changed=base.replace(old,new,1)
 changed=re.sub(r'#\[path="([^"]+)"\]',lambda m:'#[path="'+os.path.relpath((source.parent/m[1]).resolve(),target.parent)+'"]',changed)
 target.write_text(changed)
 run(name,target,True,cfg['control_functions']['reinstall.rs'][name])
receipt['complete']=True
save()
