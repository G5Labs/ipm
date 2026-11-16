#!/usr/bin/env python3
from pathlib import Path
import hashlib,json,re,subprocess,sys
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];PIN='571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
p=subprocess.run([sys.executable,str(HERE.parent/'iter6/check.py'),str(HERE)]+sys.argv[1:],capture_output=True,text=True);print(p.stdout,end='');assert not p.returncode,p.stderr
cfg=json.loads((HERE/'bindings.json').read_text());bindings=[];lines=['# Pinned source decisions and consuming readers','','These are scoped source bindings, not direct implementation proofs.','','| Case | Role | Exact pinned member |','|---|---|---|']
for row in cfg['rows']:
 for role in ['direct','reader']:
  for path,method in row[role]:
   src=(ROOT/path).read_text();m=re.search(r'^(?:ClassMethod|Method)\s+'+re.escape(method)+r'\s*\(',src,re.M);assert m,(path,method)
   line=src[:m.start()].count('\n')+1;url=f'https://github.com/intersystems/ipm/blob/{PIN}/{path}#L{line}'
   bindings.append({'case':row['id'],'role':role,'path':path,'member':method,'line':line,'url':url,'source_sha256':hashlib.sha256(src.encode()).hexdigest()});lines.append(f'| {row["id"]} | {role} | [{path}::{method}]({url}) |')
(HERE/'SOURCE-LINKS.md').write_text('\n'.join(lines)+'\n');(HERE/'exact-bindings.json').write_text(json.dumps({'pin':PIN,'bindings':bindings,'distinct_valid_members':len({(r['path'],r['member']) for r in bindings})},indent=2)+'\n')
run=json.loads((HERE/'run.json').read_text());models=json.loads((ROOT/'.g5/Verus_verification/consolidated/iter13/cases.json').read_text())['models']
assert all(hashlib.sha256((ROOT/'.g5/Verus_verification/consolidated/iter13'/f).read_bytes()).hexdigest()==run['model_source_sha256'][f] for f in models)
x=json.loads((HERE/'validation.json').read_text());x.update(exact_named_members_checked=True,distinct_valid_named_members=len({(r['path'],r['member']) for r in bindings}),proof_source_hashes_match=True);(HERE/'validation.json').write_text(json.dumps(x,indent=2)+'\n');print(json.dumps(x))
