from pathlib import Path
import json,os,subprocess,time,hashlib
root=Path('.g5/Verus_verification/consolidated/iter22');out=root/'model-lane'
cfg=json.loads((root/'reinstall-cases.json').read_text())
n=Path('.g5/uplift/consolidated/iter22/native/20261115T222936542560Z/observations.json')
native=json.loads(n.read_text());obs=native['observations']
assert set(obs)==set(cfg['replay'])|set(cfg['native_expected'])
assert all(obs[k]==v for k,v in cfg['native_expected'].items())
s=out/'replay.rs'
body='use vstd::prelude::*;\n#[path="'+os.path.relpath(root/'reinstall.rs',s.parent)+'"] pub mod model;\nuse model::*;\nverus! {pub proof fn observed_group() {\n'+cfg['replay_prelude']+'\n'
for k,v in cfg['replay'].items():body+='// '+k+'\nassert(('+v+')=='+str(obs[k])+');\n'
s.write_text(body+'}}\nfn main() {}\n')
cmd=['/Users/ziniuw/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus',str(s),'--triggers-mode','silent']
t=time.monotonic()
with (out/'replay.log').open('w') as log:p=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,timeout=120)
old=json.loads((out/'controls/config.json').read_text());subset=lambda j:{k:j[k] for k in ['models','dependency_controls','control_functions','control_options']}
assert subset(cfg)==subset(old)
imports=[root/'../iter18/reloading.rs',root/'../iter16/provenance.rs',root/'../verus/history.rs',root/'../verus/temp_repo.rs']
r={'command':cmd,'exit':p.returncode,'wall_seconds':time.monotonic()-t,
 'source_sha256':hashlib.sha256((root/'reinstall.rs').read_bytes()).hexdigest(),
 'config_sha256':hashlib.sha256((root/'reinstall-cases.json').read_bytes()).hexdigest(),
 'import_sha256':{str(v):hashlib.sha256(v.read_bytes()).hexdigest() for v in imports},
 'control_config_snapshot':'controls/config.json','control_inputs_unchanged':True,
 'control_inputs_sha256':hashlib.sha256(json.dumps(subset(cfg),sort_keys=True).encode()).hexdigest(),
 'native_observations_path':str(n),'native_observations_sha256':hashlib.sha256(n.read_bytes()).hexdigest(),
 'native_actual_image':native['actual_container_image_id'],'replay_grouped':True,'wrapper':'public model module plus use model::*',
 'replay_fields':len(cfg['replay']),'native_qualifications':len(cfg['native_expected'])}
(out/'replay-receipt.json').write_text(json.dumps(r,indent=2)+'\n')
print(r);print((out/'replay.log').read_text());assert p.returncode==0
