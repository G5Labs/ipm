#!/usr/bin/env python3
"""Refresh this review from its saved starting inventory and existing receipts.

Does not run native application tests or Verus. Run those explicit scripts when
new execution evidence is required. Refuses to overwrite a later register edit.
"""
from pathlib import Path
import hashlib,json,subprocess,sys
H=Path(__file__).resolve().parent;R=H.parents[3]
manifest=H/'written-files.json'
if manifest.exists():
 for row in json.loads(manifest.read_text()):
  p=R/row['path']
  assert hashlib.sha256(p.read_bytes()).hexdigest()==row['sha256'],('File changed after audit; reconcile it before refreshing',row['path'])
for name in ['audit_questions.py','reconcile.py','audit_simplifications.py','finalize.py','format_registers.py','validate.py']:
 subprocess.run([sys.executable,str(H/name)],cwd=R,check=True)
files=[p for p in (H/'before').rglob('*') if p.is_file() and (R/p.relative_to(H/'before')).is_file()]
rows=[{'path':str(p.relative_to(H/'before')),'sha256':hashlib.sha256((R/p.relative_to(H/'before')).read_bytes()).hexdigest()} for p in files]
manifest.write_text(json.dumps(rows,indent=2)+'\n')
