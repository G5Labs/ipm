from pathlib import Path
import hashlib,json,re,subprocess,time
from isolate_witness import isolate
r=Path(__file__).resolve().parent.parent;o=r/'model-lane/isolated-controls';o.mkdir(exist_ok=True)
src=r/'purge.rs';base=src.read_text();cfg=json.loads((r/'purge-cases.json').read_text());tool=Path('/Users/ziniuw/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus');runs=[]
receipt={'model_sha256':hashlib.sha256(src.read_bytes()).hexdigest(),'config_sha256':hashlib.sha256((r/'purge-cases.json').read_bytes()).hexdigest(),'isolation_helper_sha256':hashlib.sha256((r/'model-lane/isolate_witness.py').read_bytes()).hexdigest(),'tool_sha256':hashlib.sha256(tool.read_bytes()).hexdigest(),'full_positive_reference':'../baseline-2.json','scope':'All spec/type definitions unchanged; only exact selected independent proof retained. Same transformation for passing original and mutant. No imported models or proof-lemma calls.','runs':runs}
(o/'config.json').write_text((r/'purge-cases.json').read_text())
def save():(o/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
def make(text,name,selector):
 body,meta=isolate(text,selector);target=o/(name+'.rs');target.write_text(body);meta.update({'transformed_sha256':hashlib.sha256(target.read_bytes()).hexdigest(),'input_sha256':hashlib.sha256(text.encode()).hexdigest(),'path':str(target)});return target,meta
def run(name,target,selector,meta,negative=False):
 cmd=[str(tool),str(target),'--triggers-mode','silent','--verify-root','--verify-function',selector]
 if negative:cmd+=cfg['control_options']
 start=time.monotonic();log=o/(name+'.log')
 with log.open('w') as f:
  try:q=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,timeout=180);code=q.returncode
  except subprocess.TimeoutExpired:code=124
 text=log.read_text();counts=re.search(r'verification results:: (\d+) verified, (\d+) errors',text)
 limited=code==124 or any(x in text.lower() for x in ['resource limit','rlimit','timed out','timeout','time limit','time-limit','exceeded maximum recursion'])
 semantic=any(x in text for x in ['assertion failed','postcondition not satisfied','evaluates to false','expression simplifies to false'])
 accepted=(code!=0 and semantic and not limited and not re.search(r'error\[E',text)) if negative else code==0 and counts is not None and int(counts[1])==1
 row={'name':name,'command':cmd,'exit':code,'wall_seconds':time.monotonic()-start,'verified':int(counts[1]) if counts else None,'errors':int(counts[2]) if counts else None,'semantic_rejection':semantic and not limited,'resource_limit':limited,'accepted':accepted,'isolation':meta};runs.append(row);save();print(name,'accepted',accepted,flush=True);assert accepted,text
for selector in sorted(set(cfg['control_functions']['purge.rs'].values())):
 target,meta=make(base,'good-'+selector,selector);run('good-'+selector,target,selector,meta)
for name,(old,new) in cfg['models']['purge.rs'].items():
 assert base.count(old)==1
 selector=cfg['control_functions']['purge.rs'][name];target,meta=make(base.replace(old,new,1),name,selector);run(name,target,selector,meta,True)
receipt['complete']=True;save()
