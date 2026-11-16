from pathlib import Path
import json,re,subprocess,time,hashlib,os
root=Path('.g5/Verus_verification/consolidated/iter16');out=root/'model-lane/controls';out.mkdir(parents=True,exist_ok=True)
source=root/'provenance.rs';cfg=json.loads((root/'provenance-cases.json').read_text());tool=Path('/Users/ziniuw/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')
base=source.read_text();runs=[]
def run(name,path,expect_reject):
 t=time.monotonic();cmd=[str(tool),str(path),'--triggers-mode','silent'];p=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 raw=p.stdout;(out/(name+'.log')).write_text(raw)
 sem=any(s in raw for s in ['assertion failed','postcondition not satisfied','evaluates to false','expression simplifies to false']) and not any(s in raw.lower() for s in ['resource limit','timed out','exceeded maximum recursion']) and not re.search(r'error\[E',raw)
 counts=re.search(r'verification results:: (\d+) verified, (\d+) errors',raw)
 runs.append({'name':name,'command':cmd,'exit':p.returncode,'wall_seconds':time.monotonic()-t,'verified':int(counts[1]) if counts else None,'errors':int(counts[2]) if counts else None,'semantic_rejection':sem})
 print(name,p.returncode,sem,flush=True)
 assert (p.returncode!=0 and sem) if expect_reject else p.returncode==0,raw
run('baseline',source,False)
imported=out/'inherited-only.rs';imported.write_text('#[path="'+os.path.relpath((root/'../verus/history.rs').resolve(),out)+'"] mod inherited_history;\nfn main() {}\n')
run('inherited-only',imported,False)
for name,(old,new) in cfg['models']['provenance.rs'].items():
 assert base.count(old)==1,(name,base.count(old));target=out/(name+'.rs');text=base.replace(old,new,1)
 text=re.sub(r'#\[path="([^"]+)"\]',lambda m:'#[path="'+os.path.relpath((source.parent/m[1]).resolve(),target.parent)+'"]',text)
 target.write_text(text);run(name,target,True)
for name,m in cfg['dependency_controls']['provenance.rs'].items():
 folder=out/name;folder.mkdir(exist_ok=True);dep=Path(m['path']);dep_text=dep.read_text();assert dep_text.count(m['old'])==1
 changed=folder/dep.name;changed.write_text(dep_text.replace(m['old'],m['new'],1));target=folder/'model.rs'
 def link(match):
  actual=(source.parent/match[1]).resolve();dest=changed.resolve() if actual==dep.resolve() else actual
  return '#[path="'+os.path.relpath(dest,target.parent)+'"]'
 target.write_text(re.sub(r'#\[path="([^"]+)"\]',link,base));run(name,target,True)
(out/'receipt.json').write_text(json.dumps({'model_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'import_sha256':hashlib.sha256((root/'../verus/history.rs').read_bytes()).hexdigest(),'tool_sha256':hashlib.sha256(tool.read_bytes()).hexdigest(),'runs':runs,'implementation_proofs':0},indent=2)+'\n')
