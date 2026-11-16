from pathlib import Path
import hashlib,json,re
r=Path(__file__).resolve().parent.parent;files=r/'native/files'
lit=lambda s:json.dumps(s,ensure_ascii=False)+'@'
names={'TxtValid':'valid.txt','TxtUnknown':'unknown.txt','TxtRuntime':'runtime.txt','JsonValid':'valid.json','JsonUnknown':'unknown.json','JsonRuntime':'runtime.json','JsonArray':'array.json','JsonWrong':'wrongcommands.json','Unsupported':'unsupported.text'}
literal=''
for c,n in names.items():literal+='pub open spec fn raw_'+c.lower()+'()->Text {'+lit((files/n).read_text())+'}\n'
literal+='pub open spec fn fixture(c:Case)->File {match c {\n'
for c,n in names.items():literal+=' Case::'+c+'=>File{name:'+lit(n)+',exists:true,raw:raw_'+c.lower()+'()},\n'
literal+=' Case::Missing=>File{name:'+lit('missing.txt')+',exists:false,raw:seq![]}\n}}\n'
def jc(d):return 'JsonCommand{name:'+lit(d.get('command',''))+',params:seq!['+','.join('('+lit(k)+','+lit(str(v))+')' for k,v in d.get('parameters',{}).items())+']}'
literal+='pub open spec fn decode_json(raw:Text)->Option<JsonDoc> {\n'
for i,(c,n) in enumerate((c,n) for c,n in names.items() if n.endswith('.json')):
 obj=json.loads((files/n).read_text());doc='JsonDoc::Array(seq!['+','.join(jc(d) for d in obj)+'])' if isinstance(obj,list) else 'JsonDoc::Object('+('CommandField::Items(seq!['+','.join(jc(d) for d in obj['commands'])+'])' if isinstance(obj.get('commands'),list) else 'CommandField::WrongType')+')'
 literal+=(' if' if i==0 else ' else if')+' raw==raw_'+c.lower()+'(){Some('+doc+')}\n'
literal+=' else{None}\n}\n'
source='''use vstd::prelude::*;
#[path="model-lane/settings_projection.rs"] pub mod settings_projection;
verus! {
// Selected ASCII files and known config/exec leaves only. File reads, %FromJSON,
// object iteration order, Shell lexer, numeric/text ranks and runtime fault are
// explicitly qualified provider ports. No generic parser or interpreter proof.
pub type Text=Seq<char>;
pub enum Case {TxtValid,TxtUnknown,TxtRuntime,JsonValid,JsonUnknown,JsonRuntime,JsonArray,JsonWrong,Unsupported,Missing}
pub struct File {pub name:Text,pub exists:bool,pub raw:Text}
pub struct JsonCommand {pub name:Text,pub params:Seq<(Text,Text)>}
pub enum CommandField {Items(Seq<JsonCommand>),WrongType}
pub enum JsonDoc {Object(CommandField),Array(Seq<JsonCommand>)}
pub enum Error {MissingFile,UnsupportedExtension,ExpectedObject,ExpectedCommandsArray,JsonDecode,UnknownCommand,MissingCommand,InvalidParameter,Runtime}
pub struct Prepared {pub commands:Seq<Text>,pub error:Option<Error>}
pub struct World {pub cfg:settings_projection::Cfg,pub calls:nat}
pub struct Step {pub world:World,pub error:Option<Error>}
pub struct Out {pub world:World,pub error:Option<Error>,pub listed:Seq<Text>,pub attempted:Seq<Text>}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
pub open spec fn key()->nat {3}
pub open spec fn setting_name()->Text {"DefaultLogEntryLimit"@}
pub open spec fn first()->Text {"config set DefaultLogEntryLimit 101"@}
pub open spec fn last()->Text {"config set DefaultLogEntryLimit 202"@}
pub open spec fn unknown()->Text {"iter28-no-such-command"@}
pub open spec fn expression()->Text {"set ^||Iter28RuntimeCalls=$increment(^||Iter28RuntimeCalls) set x=1/0"@}
pub open spec fn fault()->Text {"exec "@+expression()}
// A current-cell representative, not exact whole %SYS store. All selected
// sequencing results are independent of other keys and existing default nodes.
pub open spec fn initial_with_default(d:Option<nat>)->World {
 World{cfg:settings_projection::Cfg{nodes:map![key()=>settings_projection::Node{val:Some(7nat),dflt:d}],conf:set![key()],seed:Map::empty()},calls:0}
}
pub open spec fn initial()->World {initial_with_default(None)}
pub open spec fn setting_value(w:World)->nat {settings_projection::get_value(w.cfg,key())}
pub open spec fn raw_value(w:World)->nat {if w.cfg.nodes.contains_key(key()) && w.cfg.nodes[key()].val.is_some(){w.cfg.nodes[key()].val.unwrap()}else{0}}
// Existing unchanged settings contract: Main.Config ignores helper status,
// and this selected configurable non-special key goes through SetValue.
pub open spec fn set_selected(w:World,v:nat)->World {World{cfg:settings_projection::config_set(w.cfg,1,key(),v).0,..w}}
pub open spec fn shell(w:World,command:Text)->Step {
 if command==first(){Step{world:set_selected(w,101),error:None}}
 else if command==last(){Step{world:set_selected(w,202),error:None}}
 else if command==fault(){Step{world:World{calls:w.calls+1,..w},error:Some(Error::Runtime)}}
 else{Step{world:w,error:Some(Error::UnknownCommand)}}
}
pub open spec fn whitespace(c:char)->bool {c==' ' || c=='\\t' || c=='\\r' || c=='\\n'}
pub open spec fn left(s:Text,i:int)->int recommends 0<=i<=s.len() decreases s.len()-i {
 if i>=s.len() || !whitespace(s[i]){i}else{left(s,i+1)}
}
pub open spec fn right(s:Text,n:int)->int recommends 0<=n<=s.len() decreases n {
 if n<=0 || !whitespace(s[n-1]){n}else{right(s,n-1)}
}
pub open spec fn trim(s:Text)->Text {let a=left(s,0);let z=right(s,s.len() as int);if a>=z{seq![]}else{s.subrange(a,z)}}
pub open spec fn line_item(s:Text)->Seq<Text> {let t=trim(s);if t.len()==0{seq![]}else{seq![t]}}
pub open spec fn next_line_end(s:Text,i:int)->int
 recommends 0<=i<=s.len()
 decreases s.len()-i
{
 if i>=s.len() || s[i]=='\\n'{i}else{next_line_end(s,i+1)}
}
pub open spec fn lines(s:Text,start:int)->Seq<Text>
 recommends 0<=start<=s.len()
 decreases s.len()-start
{
 // Totalize the pure line-index helper outside its valid provider range; this
 // is not a product refusal/selection guard and has no native guard credit.
 let n=next_line_end(s,start);let end=if n<start{start}else if n>s.len(){s.len() as int}else{n};
 if end>=s.len(){line_item(s.subrange(start,s.len() as int))}
 else{line_item(s.subrange(start,end))+lines(s,end+1)}
}
pub open spec fn dot(s:Text,n:int)->int recommends 0<=n<=s.len() decreases n {
 if n<=0{-1}else if s[n-1]=='.'{n-1}else{dot(s,n-1)}
}
pub open spec fn extension(name:Text)->Text {let d=dot(name,name.len() as int);name.subrange(d+1,name.len() as int)}
pub open spec fn params_ok(name:Text,ps:Seq<(Text,Text)>,i:int)->bool
 recommends 0<=i<=ps.len() decreases ps.len()-i
{
 if i>=ps.len(){true}else{
  let k=ps[i].0;let v=ps[i].1;
  v.len()>0 && (if name=="config"@{k=="action"@ || k=="key"@ || k=="value"@}else{k=="expression"@}) && params_ok(name,ps,i+1)
 }
}
pub open spec fn values(ps:Seq<(Text,Text)>,i:int)->Text
 recommends 0<=i<=ps.len() decreases ps.len()-i
{
 if i>=ps.len(){seq![]}else{ps[i].1+(if i+1<ps.len(){" "@+values(ps,i+1)}else{seq![]})}
}
pub open spec fn translate(c:JsonCommand)->Prepared {
 if c.name.len()==0{Prepared{commands:seq![],error:Some(Error::MissingCommand)}}
 else if c.name!="config"@ && c.name!="exec"@{Prepared{commands:seq![],error:Some(Error::UnknownCommand)}}
 else if !params_ok(c.name,c.params,0){Prepared{commands:seq![],error:Some(Error::InvalidParameter)}}
 else{Prepared{commands:seq![c.name+(if c.params.len()>0{" "@+values(c.params,0)}else{seq![]})],error:None}}
}
pub open spec fn render(c:JsonCommand)->Text {c.name+(if c.params.len()>0{" "@+values(c.params,0)}else{seq![]})}
pub open spec fn validate_all(cs:Seq<JsonCommand>,i:int)->Option<Error>
 recommends 0<=i<=cs.len() decreases cs.len()-i
{
 if i>=cs.len(){None}else{
  let head=translate(cs[i]);
  if head.error.is_some(){head.error}else{validate_all(cs,i+1)}
 }
}
pub open spec fn render_all(cs:Seq<JsonCommand>,i:int)->Seq<Text>
 recommends 0<=i<=cs.len() decreases cs.len()-i
{if i>=cs.len(){seq![]}else{seq![render(cs[i])]+render_all(cs,i+1)}}
pub open spec fn translate_all(cs:Seq<JsonCommand>,i:int)->Prepared
 recommends 0<=i<=cs.len()
{
 let error=validate_all(cs,i);
 Prepared{commands:if error.is_none(){render_all(cs,i)}else{seq![]},error}
}
'''+literal+'''
// Finite raw-byte adapter above is a named SDK %FromJSON premise tied to the
// exact fixture file hashes. It is not a generic JSON parser/UTF8 guarantee.
pub open spec fn prepare(file:File)->Prepared {
 if !file.exists{Prepared{commands:seq![],error:Some(Error::MissingFile)}}
 else if extension(file.name)=="txt"@{Prepared{commands:lines(file.raw,0),error:None}}
 else if extension(file.name)=="json"@{
  match decode_json(file.raw) {
   None=>Prepared{commands:seq![],error:Some(Error::JsonDecode)},
   Some(JsonDoc::Array(_))=>Prepared{commands:seq![],error:Some(Error::ExpectedObject)},
   Some(JsonDoc::Object(CommandField::WrongType))=>Prepared{commands:seq![],error:Some(Error::ExpectedCommandsArray)},
   Some(JsonDoc::Object(CommandField::Items(cs)))=>translate_all(cs,0),
  }
 }else{Prepared{commands:seq![],error:Some(Error::UnsupportedExtension)}}
}
pub open spec fn execute(w:World,commands:Seq<Text>,i:int,dry:bool)->Out
 recommends 0<=i<=commands.len() decreases commands.len()-i
{
 if i>=commands.len(){Out{world:w,error:None,listed:seq![],attempted:seq![]}}else{
  let command=commands[i];
  let step=if dry{Step{world:w,error:None}}else{shell(w,command)};
  let attempted=if dry{seq![]}else{seq![command]};
  if step.error.is_some(){Out{world:step.world,error:step.error,listed:seq![command],attempted}}
  else{let rest=execute(step.world,commands,i+1,dry);Out{listed:seq![command]+rest.listed,attempted:attempted+rest.attempted,..rest}}
 }
}
// ProcessRunFromFile prepares before reaching its dry-run execution branch.
// This factor is unconditional source order, not a discovered flag predicate.
pub open spec fn planned(file:File,dry:bool)->Prepared {prepare(file)}
pub open spec fn run(w:World,file:File,dry:bool)->Out {
 let p=planned(file,dry);
 if p.error.is_some(){Out{world:w,error:p.error,listed:seq![],attempted:seq![]}}else{execute(w,p.commands,0,dry)}
}
pub open spec fn named(c:Case,dry:bool)->Out {run(initial(),fixture(c),dry)}
pub open spec fn ok(o:Out)->bool {o.error.is_none()}
pub open spec fn last_listed(o:Out)->bool {o.listed.contains(last())}
pub open spec fn first_listed(o:Out)->bool {o.listed.contains(first())}
}
fn main(){}
'''
proofs=(r/'model-lane/proofs.txt').read_text() if (r/'model-lane/proofs.txt').exists() else ''
source=source.rsplit('}\nfn main(){}\n',1)[0]+proofs+'}\nfn main(){}\n'
def explicit_chars(m):
 s=json.loads(m.group()[:-1])
 def char(c):
  return {"'":"'\\\''",'\\':"'\\\\'",'\n':"'\\n'",'\r':"'\\r'",'\t':"'\\t'"}.get(c,"'"+c+"'")
 chunks=['seq!['+','.join(char(c) for c in s[i:i+32])+']' for i in range(0,len(s),32)]
 def balanced(cs):
  if not cs:return 'seq![]'
  if len(cs)==1:return cs[0]
  m=len(cs)//2;return '('+balanced(cs[:m])+'+'+balanced(cs[m:])+')'
 return balanced(chunks)
source=re.sub(r'"(?:\\.|[^"\\])*"@',explicit_chars,source)
(r/'script.rs').write_text(source)
(r/'model-lane/fixture-inputs.json').write_text(json.dumps({'files':{str((files/n).relative_to(r)):hashlib.sha256((files/n).read_bytes()).hexdigest() for n in names.values()},'scope':'Exact ASCII raw literal→explicit JSON tree binding; generic SDK parsing/iteration is provider-qualified. No supplied parsed-success flag; execution effects derived from prepared commands.'},indent=2)+'\n')
print('wrote',r/'script.rs')
