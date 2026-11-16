from pathlib import Path
import hashlib,json,os,re,subprocess,time
from isolate_witness import isolate
root=Path('.g5/Verus_verification/consolidated/iter24');out=root/'model-lane/isolated-controls';out.mkdir(exist_ok=True)
src=root/'snapshot_dependencies.rs';base=src.read_text();cfg=json.loads((root/'snapshot-dependency-cases.json').read_text());tool=Path('/Users/ziniuw/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus');runs=[]
receipt={'model_sha256':hashlib.sha256(src.read_bytes()).hexdigest(),'config_sha256':hashlib.sha256((root/'snapshot-dependency-cases.json').read_bytes()).hexdigest(),'isolation_helper_sha256':hashlib.sha256((root/'model-lane/isolate_witness.py').read_bytes()).hexdigest(),'tool_sha256':hashlib.sha256(tool.read_bytes()).hexdigest(),'full_positive_reference':'../positive-receipt.json','scope':'all type/spec declarations unchanged, only selected local proof retained, same transform for good/mutant; imported module unchanged; selected witness definition-only','runs':runs}
(out/'config.json').write_text((root/'snapshot-dependency-cases.json').read_text())
def save():(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
def make(text,name,selector):
 body,meta=isolate(text,selector);target=out/(name+'.rs')
 body=re.sub(r'#\[path="([^"]+)"\]',lambda m:'#[path="'+os.path.relpath((root/m[1]).resolve(),out)+'"]',body)
 target.write_text(body);meta.update({'transformed_sha256':hashlib.sha256(target.read_bytes()).hexdigest(),'input_sha256':hashlib.sha256(text.encode()).hexdigest(),'path':str(target)})
 return target,meta
def run(name,target,selector,meta,negative=False):
 cmd=[str(tool),str(target),'--triggers-mode','silent','--verify-root','--verify-function',selector]
 if negative:cmd+=cfg['control_options']
 t=time.monotonic();log=out/(name+'.log')
 with log.open('w') as f:q=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,timeout=180)
 text=log.read_text();counts=re.search(r'verification results:: (\d+) verified, (\d+) errors',text)
 limited=any(x in text.lower() for x in ['resource limit','rlimit','timed out','timeout','time limit','time-limit','exceeded maximum recursion'])
 semantic=any(x in text for x in ['assertion failed','postcondition not satisfied','evaluates to false','expression simplifies to false'])
 ok=(q.returncode!=0 and semantic and not limited and not re.search(r'error\[E',text)) if negative else q.returncode==0 and counts is not None and int(counts[1])==1
 row={'name':name,'command':cmd,'exit':q.returncode,'wall_seconds':time.monotonic()-t,'verified':int(counts[1]) if counts else None,'errors':int(counts[2]) if counts else None,'semantic_rejection':semantic and not limited,'resource_limit':limited,'accepted':ok,'isolation':meta};runs.append(row);save();print(name,'accepted',ok,flush=True);assert ok,text
for sel in sorted(set(cfg['control_functions']['snapshot_dependencies.rs'].values())):
 target,meta=make(base,'good-'+sel,sel);run('good-'+sel,target,sel,meta)
for name,(old,new) in cfg['models']['snapshot_dependencies.rs'].items():
 assert base.count(old)==1
 sel=cfg['control_functions']['snapshot_dependencies.rs'][name];target,meta=make(base.replace(old,new,1),name,sel);run(name,target,sel,meta,True)
receipt['complete']=True;save()
