from pathlib import Path
import json,subprocess,time,hashlib
root=Path('.g5/Verus_verification/consolidated/iter20')
out=root/'model-lane'
cfg=json.loads((root/'ownership-cases.json').read_text())
n=Path('.g5/uplift/consolidated/iter20/native/20261115T213633510810Z/observations.json')
native=json.loads(n.read_text());obs=native['observations']
assert set(obs)==set(cfg['replay'])|set(cfg['native_expected'])
assert all(obs[k]==v for k,v in cfg['native_expected'].items())
s=out/'replay.rs';source=(root/'ownership.rs').read_text()
body='\nverus! {pub proof fn observed_replay() {\n'+cfg['replay_prelude']+'\n'
for k,v in cfg['replay'].items():body+='// '+k+'\nassert(('+v+')=='+str(obs[k])+');\n'
s.write_text(source+body+'}}\n')
cmd=['/Users/ziniuw/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus',str(s),'--triggers-mode','silent']
t=time.monotonic()
with (out/'replay.log').open('w') as log:p=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,timeout=120)
old=json.loads((out/'controls/config.json').read_text())
subset=lambda j:{k:j[k] for k in ['models','dependency_controls','control_functions','control_options']}
assert subset(cfg)==subset(old)
r={'command':cmd,'exit':p.returncode,'wall_seconds':time.monotonic()-t,
 'source_sha256':hashlib.sha256((root/'ownership.rs').read_bytes()).hexdigest(),
 'config_sha256':hashlib.sha256((root/'ownership-cases.json').read_bytes()).hexdigest(),
 'control_config_snapshot':'controls/config.json','control_inputs_unchanged':True,
 'control_inputs_sha256':hashlib.sha256(json.dumps(subset(cfg),sort_keys=True).encode()).hexdigest(),
 'native_observations_path':str(n),'native_observations_sha256':hashlib.sha256(n.read_bytes()).hexdigest(),
 'native_actual_image':native['actual_container_image_id'],
 'replay_fields':len(cfg['replay']),'native_qualifications':len(cfg['native_expected'])}
(out/'replay-receipt.json').write_text(json.dumps(r,indent=2)+'\n')
print(r);print((out/'replay.log').read_text());assert p.returncode==0
