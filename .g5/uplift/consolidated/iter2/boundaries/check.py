#!/usr/bin/env python3
"""Reproduce finite boundary proofs and sharpness controls; no old files rewritten."""
from pathlib import Path
import subprocess,json,hashlib,time
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[4]
SRC=ROOT/'.g5/Verus_verification/consolidated/iter2/boundaries/boundaries.rs'
TOOL=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus'
OUT=HERE/'proofs';OUT.mkdir(exist_ok=True)
s=SRC.read_text()
mutants={
 'restart-keeps-temp':s.replace('History{temp:false,row:h.row && durable_database','History{temp:h.temp,row:h.row && durable_database'),
 'close-does-not-replay-buffer':s.replace('h.buffered && replay_on_close','false'),
 'constructor-flag-ignored':s.replace('!l.immediate && l.depth>0','l.depth>0'),
 'cache-default-is-deferred':s.replace('..new_lock(outer+1,true)','..new_lock(outer+1,false)'),
 'cpf-rollback-compensates':s.replace('CPF{database_sentinel:false,..c}','CPF{database_sentinel:false,config_role:false,..c}'),
 'old-no-resurrection-claim':s.replace('fn main() {}','verus! { proof fn old_claim() {assert(!locked_schedule().row);} }\nfn main() {}'),
 'old-cache-holds-outer-lock-claim':s.replace('fn main() {}','verus! { proof fn old_claim() {assert(cache_return(1).held);} }\nfn main() {}')
}
results=[]
for name,body in [('baseline',s),*mutants.items()]:
 p=OUT/(name+'.rs');p.write_text(body);start=time.monotonic();cmd=[str(TOOL),str(p)];r=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 (OUT/(name+'.log')).write_text(r.stdout)
 results.append({'id':name,'command':cmd,'returncode':r.returncode,'expected_returncode':0 if name=='baseline' else 1,'pass':r.returncode==0 if name=='baseline' else (r.returncode!=0 and any(t in r.stdout for t in ['assertion failed','postcondition not satisfied','expression simplifies to false'])),'wall_seconds':time.monotonic()-start,'source_sha256':hashlib.sha256(body.encode()).hexdigest(),'raw':str((OUT/(name+'.log')).relative_to(ROOT))})
receipt={'scope':'bounded source hand translation + observed provider contracts, not ObjectScript proof','model_sha256':hashlib.sha256(SRC.read_bytes()).hexdigest(),'results':results,'pass':all(x['pass'] for x in results)}
(OUT/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2));assert receipt['pass']
