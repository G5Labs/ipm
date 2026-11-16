from pathlib import Path
import hashlib,json,subprocess
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
s=json.loads((HERE/'initial-state.json').read_text());changed=[];missing=[]
for rel,old in s['files'].items():
 f=ROOT/rel
 if not f.exists():missing.append(rel)
 elif hashlib.sha256(f.read_bytes()).hexdigest()!=old:changed.append(rel)
index=subprocess.check_output(['git','-C',str(ROOT),'ls-files','--stage'])
allowed=lambda x:x.startswith('.g5/ontology/') or x.startswith('.g5/uplift/consolidated/') or x in ['.g5/Verus_verification/consolidated/models/env_config.py','.g5/Verus_verification/consolidated/README.md']
r={'initial_files':len(s['files']),'index_unchanged':hashlib.sha256(index).hexdigest()==s['index_sha256'],'changed_initial_files':changed,'missing_initial_files':missing,'inherited_unchanged':all(allowed(x) for x in changed),'product_worktree_matches_pin':subprocess.run(['git','-C',str(ROOT),'diff','--quiet','571e04a1','--','src','preload','tests','modules','wheels','module.xml']).returncode==0,'original_env_twin_preserved':hashlib.sha256((HERE/'before-models/env_config.py').read_bytes()).hexdigest()==s['files']['.g5/Verus_verification/consolidated/models/env_config.py'],'baseline_scope':'Initial permanent files, staged index and product. Runtime __pycache__ files are excluded from fingerprints.'}
(HERE/'preservation.json').write_text(json.dumps(r,indent=2));assert r['index_unchanged'] and r['inherited_unchanged'] and r['product_worktree_matches_pin'] and r['original_env_twin_preserved'] and not missing
print('Initial index, product, inherited artifacts and original twin preserved')
