"""Read-only correspondence check for exact native literals and qualified JSON port."""
from pathlib import Path
import hashlib,json,re
NAMES={'TxtValid':'valid.txt','TxtUnknown':'unknown.txt','TxtRuntime':'runtime.txt','JsonValid':'valid.json','JsonUnknown':'unknown.json','JsonRuntime':'runtime.json','JsonArray':'array.json','JsonWrong':'wrongcommands.json','Unsupported':'unsupported.text'}
def chars(s):
 assert all(ord(c)<128 for c in s),'Only selected ASCII fixture domain is qualified'
 def char(c):return {"'":"'\\\''",'\\':"'\\\\'",'\n':"'\\n'",'\r':"'\\r'",'\t':"'\\t'"}.get(c,"'"+c+"'")
 chunks=['seq!['+','.join(char(c) for c in s[i:i+32])+']' for i in range(0,len(s),32)]
 def balanced(cs):
  if not cs:return 'seq![]'
  if len(cs)==1:return cs[0]
  m=len(cs)//2;return '('+balanced(cs[:m])+'+'+balanced(cs[m:])+')'
 return balanced(chunks)
def command(d):
 return 'JsonCommand{name:'+chars(d.get('command',''))+',params:seq!['+','.join('('+chars(k)+','+chars(str(v))+')' for k,v in d.get('parameters',{}).items())+']}'
def check():
 here=Path(__file__).resolve().parent;files=here/'native/files';source=(here/'script.rs').read_text();manifest=json.loads((here/'model-lane/fixture-inputs.json').read_text())
 assert set(manifest['files'])=={'native/files/'+n for n in NAMES.values()}
 for case,name in NAMES.items():
  p=files/name;assert hashlib.sha256(p.read_bytes()).hexdigest()==manifest['files']['native/files/'+name]
  expected='pub open spec fn raw_'+case.lower()+'()->Text {'+chars(p.read_text())+'}'
  assert source.count(expected)==1,('raw literal differs from native bytes',name)
 fixture='pub open spec fn fixture(c:Case)->File {match c {\n'
 for case,name in NAMES.items():fixture+=' Case::'+case+'=>File{name:'+chars(name)+',exists:true,raw:raw_'+case.lower()+'()},\n'
 fixture+=' Case::Missing=>File{name:'+chars('missing.txt')+',exists:false,raw:seq![]}\n}}'
 assert source.count(fixture)==1,'File name/existence/raw case mapping changed'
 assert not (files/'missing.txt').exists(),'Missing-file provider premise changed'
 decoder='pub open spec fn decode_json(raw:Text)->Option<JsonDoc> {\n'
 for i,(case,name) in enumerate((c,n) for c,n in NAMES.items() if n.endswith('.json')):
  obj=json.loads((files/name).read_text())
  doc='JsonDoc::Array(seq!['+','.join(command(d) for d in obj)+'])' if isinstance(obj,list) else 'JsonDoc::Object('+('CommandField::Items(seq!['+','.join(command(d) for d in obj['commands'])+'])' if isinstance(obj.get('commands'),list) else 'CommandField::WrongType')+')'
  decoder+=(' if' if i==0 else ' else if')+' raw==raw_'+case.lower()+'(){Some('+doc+')}\n'
 decoder+=' else{None}\n}'
 assert source.count(decoder)==1,'Qualified literal JSON tree/ordered parameters differ from actual file'
 return {'checked':True,'raw_files':len(NAMES),'qualified_json_literals':5,'scope':'Mechanical literal/tree binding only; generic SDK JSON parsing and object iteration remain native-qualified.'}
if __name__=='__main__':print(json.dumps(check(),sort_keys=True))
