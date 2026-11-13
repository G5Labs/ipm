#!/usr/bin/env python3
"""Verify immutable, versioned formal approval assets, including published bytes."""
import hashlib,json,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
contracts=ROOT/'backend/g5-contracts'
main_ref=subprocess.run(['git','rev-parse','--verify','origin/main^{commit}'],cwd=ROOT,capture_output=True)
if main_ref.returncode != 0:
    print('WARNING: origin/main is unavailable; published-contract immutability comparison is skipped. Fetch origin/main to enable it.',file=sys.stderr)
for path in sorted((contracts/'agent-approval-contracts').glob('*.json')):
    manifest=json.loads(path.read_text())
    assert manifest['contract_version']==f"g5.formal-agent-approval/{path.stem}"
    for name,digest in manifest['assets'].items():
        assert hashlib.sha256((contracts/name).read_bytes()).hexdigest()==digest, f'{name}: frozen contract drift'
    relative=str(path.relative_to(ROOT))
    previous=subprocess.run(['git','show',f'origin/main:{relative}'],cwd=ROOT,capture_output=True)
    if previous.returncode==0:
        assert previous.stdout==path.read_bytes(), f'{relative}: published contract is immutable; add a new version'
        for name in manifest['assets']:
            relative=str((contracts/name).relative_to(ROOT))
            old=subprocess.run(['git','show',f'origin/main:{relative}'],cwd=ROOT,capture_output=True,check=True)
            assert old.stdout==(contracts/name).read_bytes(), f'{relative}: retain published assets for frozen retries'
print('Versioned formal approval assets match their immutable manifests.')
