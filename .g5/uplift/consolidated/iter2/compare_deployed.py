#!/usr/bin/env python3
"""Export compared production methods; run with python3.11 for the local XML library."""
from pathlib import Path
import csv, json, subprocess, hashlib, xml.etree.ElementTree as ET
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
OUT=HERE/'native/deployed'
def main():
 OUT.mkdir(parents=True,exist_ok=True)
 bindings=json.loads((HERE/'inventory/new-binding-review.json').read_text())['bindings']
 rows={r['member']:r for r in csv.DictReader((ROOT/'.g5/uplift/consolidated/coverage/members.tsv').open(),delimiter='\t')}
 files=sorted({b['member'].split('::')[0] for b in bindings})
 pairs=[(file,'%IPM.'+file[:-4].replace('/','.')) for file in files]
 script='write "ZV=",$zversion,!\n'
 for i,(file,cls) in enumerate(pairs):
  script+=f'set sc=$system.OBJ.Export("{cls}.cls","/tmp/g5-iter2/deployed-{i}.xml","-d") write "EXPORT-{i}=",$system.Status.IsOK(sc),!\n'
 script+='halt\n'
 cmd=['docker','exec','-i','ipm-consolidated-iris','iris','session','IRIS','-U','USER']
 p=subprocess.run(cmd,input=script,text=True,capture_output=True,check=True,timeout=120)
 (OUT/'export.cos').write_text(script);(OUT/'export.log').write_text(p.stdout+p.stderr)
 result=[]
 for i,(file,cls) in enumerate(pairs):
  xml=OUT/f'deployed-{i}.xml'
  subprocess.run(['docker','cp',f'ipm-consolidated-iris:/tmp/g5-iter2/deployed-{i}.xml',str(xml)],check=True,capture_output=True)
  tree=ET.parse(xml);source=(ROOT/'src/cls/IPM'/file).read_text().splitlines()
  for b in bindings:
   if b['member'].split('::')[0]!=file:continue
   name=b['member'].split('::')[1];row=rows[b['member']]
   lines=source[int(row['start'])-1:int(row['end'])]
   body='\n'.join(lines[lines.index('{')+1:-1])
   imp=tree.find('.//Method[@name="'+name+'"]/Implementation');actual='' if imp is None else imp.text or ''
   normalize=lambda s:'\n'.join(x.strip() for x in s.strip().splitlines())
   result.append({'member':b['member'],'same_body_ignoring_indentation':normalize(body)==normalize(actual),
      'source_body_sha256':hashlib.sha256(body.encode()).hexdigest(),'deployed_body_sha256':hashlib.sha256(actual.encode()).hexdigest()})
 (OUT/'receipt.json').write_text(json.dumps({'command':cmd,'methods':result,'scope':'Deployed source identity only, not implementation proof.'},indent=2)+'\n')
 assert all(x['same_body_ignoring_indentation'] for x in result),result
 print(len(result),'production member bodies agree ignoring indentation')
if __name__=='__main__':main()
