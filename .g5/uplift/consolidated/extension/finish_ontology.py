from pathlib import Path
import json,re,os
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];ONT=ROOT/'.g5/ontology'
# Explain evidence beside each owning block, with exact formal symbol and boundary.
byid={}
for p in (ROOT/'.g5/Verus_verification/consolidated/verus').glob('*.manifest.json'):
 x=json.loads(p.read_text());formal=x.get('formal_file') or x.get('artifact');
 if not formal:continue
 f=ROOT/formal if (ROOT/formal).exists() else p.parent/Path(formal).name
 for b in x.get('bindings',[]):
  if b.get('represented',True) is False or b.get('open',False) is True:continue
  id=b.get('ontology');s=b.get('symbol')
  if id and s:byid.setdefault(id,set()).add((str(f),s))
for p in ONT.rglob('*.g5.md'):
 s=p.read_text()
 for id,refs in byid.items():
  if not re.search(r'^\[[IDQ] · '+re.escape(id)+r'\]',s,re.M):continue
  pat=r'(^### '+re.escape(id)+r'\n)(.*?)(?=\n### |\Z)';m=re.search(pat,s,re.M|re.S)
  if not m:continue
  if 'Extension Verus:' not in m[2]:
   links=', '.join('['+Path(f).name+'::'+sym+']('+os.path.relpath(f,p.parent)+')' for f,sym in sorted(refs))
   body=m[2].rstrip()+'\n\nExtension Verus: '+links+'. These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index]('+os.path.relpath(HERE/'bindings.json',p.parent)+'). They do not mechanically verify the ObjectScript body.\n'
   s=s[:m.start(2)]+body+s[m.end(2):]
 # Every uncertain question has the format-required uncertain metadata tag.
 for id in re.findall(r'^\[Q · ([^\]]+)\]',s,re.M):
  pat=r'(^### '+re.escape(id)+r'\n)(.*?)(?=\n### |\Z)';m=re.search(pat,s,re.M|re.S)
  assert m,('no Q metadata',p,id)
  if not re.search(r'^- uncertain\s*$',m[2],re.M):s=s[:m.start(2)]+m[2].replace('- agent-provided','- uncertain\n- agent-provided',1)+s[m.end(2):] if '- agent-provided' in m[2] else s[:m.start(2)]+'\n- uncertain\n'+m[2]+s[m.end(2):]
 # Long evidence qualifications become short parallel bullets; keep the commitment itself concise.
 def normalize(m):
  header,body=m[1],m[2];paras=body.strip().split('\n\n');out=[];first=True
  for para in paras:
   if not para.strip():continue
   if para.startswith(('- ','|','```','##')):out.append(para);continue
   sentences=re.split(r'(?<=[.!?])\s+(?=[A-Z`])',para.replace('\n',' '))
   if first:
    out.append(' '.join(sentences[:2]));rest=sentences[2:];first=False
   else:rest=sentences
   for i in range(0,len(rest),2):out.append('- '+' '.join(rest[i:i+2]))
  return header+'\n'+'\n\n'.join(out)+'\n'
 # Limit normalization to blocks we materially edited, leave other commitments alone.
 selected=set()
 for item in json.loads((HERE/'ontology-changes.json').read_text()):
  if item['file']==str(p.relative_to(ONT)):selected.add(item['id'])
 selected.update(['history.dependencies.parent-rollback','transaction.rollback-scope','history.committed.side-effects'])
 for id in selected:
  pat=r'(^\[[IDQ] · '+re.escape(id)+r'\]\n)(.*?)(?=\n<a id=|\n## |\Z)'
  s=re.sub(pat,normalize,s,flags=re.M|re.S)
 p.write_text(s)
print('Canonical symbol links, uncertainty tags and concise qualifications updated')
