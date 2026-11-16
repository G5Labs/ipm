#!/usr/bin/env python3
"""Use Python 3.11 where the local 3.14 expat extension is unavailable.
Compare retained deployed XML exports against exact production member bodies.
Whitespace normalization removes line indentation only; this is identity evidence,
not a correctness proof. Export commands and statuses are in native/deployed-export.log.
"""
from pathlib import Path
import xml.etree.ElementTree as ET,csv,json,hashlib
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[4];out=HERE/'native'
rows={r['member']:r for r in csv.DictReader((ROOT/'.g5/uplift/consolidated/coverage/members.tsv').open(),delimiter='\t')};res=[]
for deployed,file,names in [('DeployedLockManager.cls','Utils/LockManager.cls',['%OnNew','%OnClose','LockClassId','UnlockClassId','UnlockAll']),('DeployedHistoryTemp.cls','General/HistoryTemp.cls',['Init','PersistToTwin','%OnClose']),('DeployedFilesystem.cls','Repo/Filesystem/Definition.cls',['BuildCache']),('DeployedCPF.cls','ResourceProcessor/CPF.cls',['DoMerge','GetSourceFile','MergeCPF'])]:
 tree=ET.parse(out/deployed);source=(ROOT/'src/cls/IPM'/file).read_text().splitlines()
 for name in names:
  row=rows[file+'::'+name];sl=source[int(row['start'])-1:int(row['end'])];i=sl.index('{');body='\n'.join(sl[i+1:-1]);imp=tree.find('.//Method[@name="'+name+'"]/Implementation');actual='' if imp is None else imp.text or '';normalize=lambda s:'\n'.join(x.strip() for x in s.strip().splitlines())
  res.append({'member':file+'::'+name,'deployed':deployed,'body_equal_ignoring_line_indent':normalize(body)==normalize(actual),'deployed_implementation_sha256':hashlib.sha256(actual.encode()).hexdigest(),'source_body_sha256':hashlib.sha256(body.encode()).hexdigest()})
assert all(r['body_equal_ignoring_line_indent'] for r in res)
(out/'deployed-source-correspondence.json').write_text(json.dumps(res,indent=2)+'\n');print('12/12 deployed source method bodies agree ignoring line indentation.')
