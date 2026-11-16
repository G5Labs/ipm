#!/usr/bin/env python3
from pathlib import Path
import hashlib,json,os,re,sys

def main(here,baseline=None):
 here=Path(here).resolve();root=here.parents[3];cfg=json.loads((here/'bindings.json').read_text());run=json.loads((here/'run.json').read_text());rows=cfg['rows'];problems=[];members=set()
 lines=['# '+here.name+' scoped evidence index','','These are builder cases, not independent holdouts. Proofs verify hand-written specifications. ObjectScript implementation proofs: 0.','','| Case | Canonical owner | Contract | Source decisions → consuming readers |','|---|---|---|---|']
 for row in rows:
  f,anchor=row['owner'].split('#');s=(root/f).read_text();assert f'<a id="{anchor}"></a>' in s,row
  model=cfg['verus_dir']+'/'+row['model'];assert (root/model).is_file()
  for symbol in row['symbols']:assert re.search(r'\bfn '+re.escape(symbol)+r'\b',(root/model).read_text()),symbol
  for role in ['direct','reader']:
   for path,method in row[role]:
    text=(root/path).read_text();assert method in text,(path,method);members.add((path,method))
  lines.append('| '+row['id']+' | ['+anchor+']('+os.path.relpath(root/f,here)+'#'+anchor+') | ['+row['model']+']('+os.path.relpath(root/model,here)+') `'+', '.join(row['symbols'])+'` | '+'; '.join(p+'::'+m for p,m in row['direct'])+' → '+'; '.join(p+'::'+m for p,m in row['reader'])+' |')
 (here/'INDEX.md').write_text('\n'.join(lines)+'\n')
 old=json.loads(Path(baseline).read_text()) if baseline else {}; changed=[];missing=[]
 for rel,row in old.items():
  p=root/rel
  if row['kind']=='file':
   if not p.is_file():missing.append(rel)
   elif hashlib.sha256(p.read_bytes()).hexdigest()!=row['sha256']:changed.append(rel)
 assert not missing,missing
 assert set(changed)<=set(cfg['changed_files']),(set(changed)-set(cfg['changed_files']))
 for row in rows:
  f=row['owner'].split('#')[0]
  for target in re.findall(r'\]\(([^)\s]+)\)',(root/f).read_text()):
   if target.startswith(('http:','https:','mailto:')):continue
   path,_,anchor=target.partition('#');p=(root/f).parent/path if path else root/f
   if not p.exists():problems.append([f,target])
 assert not problems,problems
 r={'ok':True,'scoped_bindings':len(members),'rows':len(rows),'changed_files':changed,'missing':missing,'native_fields':run['native_fields'],'verus_controls':run['controls'],'proof_level':'Specification proofs + source inspection + native correspondence; direct implementation proofs=0'}
 (here/'validation.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
if __name__=='__main__':main(sys.argv[1],sys.argv[2] if len(sys.argv)>2 else None)
