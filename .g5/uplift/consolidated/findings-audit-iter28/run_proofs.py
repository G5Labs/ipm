#!/usr/bin/env python3
"""Repeat the five scoped model checks, preserving every attempt."""
from pathlib import Path
import datetime,hashlib,json,re,subprocess,sys,time
H=Path(__file__).resolve().parent;R=H.parents[3]
tool=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus'
stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
out=H/'proof-runs'/stamp;out.mkdir(parents=True)
targets=[('status','.g5/Verus_verification/consolidated/iter3/resources.rs'),('clean','.g5/Verus_verification/consolidated/iter5/webapp.rs'),('error','.g5/Verus_verification/consolidated/iter10/errors.rs'),('identity','.g5/Verus_verification/consolidated/iter17/report_identity.rs'),('durable','playbook-uplift/models/verus/load_general.rs')]
rows=[]
for name,path in targets:
 cmd=[str(tool),path,'--triggers-mode','silent'];start=time.monotonic()
 try:
  p=subprocess.run(cmd,cwd=R,capture_output=True,timeout=180);raw=p.stdout+p.stderr;code=p.returncode
 except subprocess.TimeoutExpired as e:raw=(e.stdout or b'')+(e.stderr or b'');code='timeout'
 (out/(name+'.log')).write_bytes(raw);match=re.search(rb'verification results:: (\d+) verified, (\d+) errors',raw)
 row={'name':name,'path':path,'source_sha256':hashlib.sha256((R/path).read_bytes()).hexdigest(),'tool_sha256':hashlib.sha256(tool.read_bytes()).hexdigest(),'command':cmd,'exit':code,'seconds':time.monotonic()-start,'verified':int(match[1]) if match else None,'errors':int(match[2]) if match else None}
 rows.append(row);print(json.dumps(row),flush=True)
(out/'receipt.json').write_text(json.dumps(rows,indent=2)+'\n')
print('Saved',out)
sys.exit(0 if all(x['exit']==0 for x in rows) else 1)
