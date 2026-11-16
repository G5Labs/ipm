from pathlib import Path
import hashlib,json,re
NAMES=['Node','Cfg','empty_v','k_trf','k_mroot','mroot_default','valid_trf','St','get_value','set_value','update_one','config_set']
def declarations(text):
 out=[]
 for name in NAMES:
  m=re.search(r'^pub (?:struct |enum |open spec fn )'+re.escape(name)+r'\b',text,re.M)
  assert m is not None,name
  start=m.start();brace=text.index('{',m.end());depth=0
  for t in re.finditer(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|[{}]',text[brace:],re.S):
   if t.group()=='{':depth+=1
   elif t.group()=='}':
    depth-=1
    if depth==0:end=brace+t.end();break
  block=text[start:end]
  out.append({'name':name,'start':text.count('\n',0,start)+1,'end':text.count('\n',0,end)+1,'sha256':hashlib.sha256(block.encode()).hexdigest(),'text':block})
 return out
def expected(text):
 return 'use vstd::prelude::*;\n// Mechanically projected unchanged declarations; provenance checked before use.\nverus! {\n'+'\n\n'.join(d['text'] for d in declarations(text))+'\n}\n'
def check():
 here=Path(__file__).resolve().parent;repo=here.parents[4];manifest=json.loads((here/'settings-projection-provenance.json').read_text());source=repo/manifest['source'];text=source.read_text()
 assert hashlib.sha256(source.read_bytes()).hexdigest()==manifest['source_sha256'],'reuse source hash changed'
 assert [{k:v for k,v in d.items() if k!='text'} for d in declarations(text)]==manifest['declarations'],'reuse source spans/declarations changed'
 projection=here/'settings_projection.rs';assert projection.read_text()==expected(text),'projection is not unchanged selected declarations'
 assert hashlib.sha256(projection.read_bytes()).hexdigest()==manifest['projection_sha256'],'projection hash changed'
 return {'checked':True,'source':manifest['source'],'projection':str(projection),'declarations':len(NAMES),'inherited_proof_units':0}
if __name__=='__main__':print(json.dumps(check(),sort_keys=True))
