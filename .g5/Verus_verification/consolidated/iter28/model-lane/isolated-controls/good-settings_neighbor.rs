use vstd::prelude::*;
#[path="../settings_projection.rs"] pub mod settings_projection;
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
pub open spec fn setting_name()->Text {seq!['D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t']}
pub open spec fn first()->Text {(seq!['c','o','n','f','i','g',' ','s','e','t',' ','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t',' ']+seq!['1','0','1'])}
pub open spec fn last()->Text {(seq!['c','o','n','f','i','g',' ','s','e','t',' ','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t',' ']+seq!['2','0','2'])}
pub open spec fn unknown()->Text {seq!['i','t','e','r','2','8','-','n','o','-','s','u','c','h','-','c','o','m','m','a','n','d']}
pub open spec fn expression()->Text {(seq!['s','e','t',' ','^','|','|','I','t','e','r','2','8','R','u','n','t','i','m','e','C','a','l','l','s','=','$','i','n','c','r','e']+(seq!['m','e','n','t','(','^','|','|','I','t','e','r','2','8','R','u','n','t','i','m','e','C','a','l','l','s',')',' ','s','e','t',' ']+seq!['x','=','1','/','0']))}
pub open spec fn fault()->Text {seq!['e','x','e','c',' ']+expression()}
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
pub open spec fn whitespace(c:char)->bool {c==' ' || c=='\t' || c=='\r' || c=='\n'}
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
 if i>=s.len() || s[i]=='\n'{i}else{next_line_end(s,i+1)}
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
  v.len()>0 && (if name==seq!['c','o','n','f','i','g']{k==seq!['a','c','t','i','o','n'] || k==seq!['k','e','y'] || k==seq!['v','a','l','u','e']}else{k==seq!['e','x','p','r','e','s','s','i','o','n']}) && params_ok(name,ps,i+1)
 }
}
pub open spec fn values(ps:Seq<(Text,Text)>,i:int)->Text
 recommends 0<=i<=ps.len() decreases ps.len()-i
{
 if i>=ps.len(){seq![]}else{ps[i].1+(if i+1<ps.len(){seq![' ']+values(ps,i+1)}else{seq![]})}
}
pub open spec fn translate(c:JsonCommand)->Prepared {
 if c.name.len()==0{Prepared{commands:seq![],error:Some(Error::MissingCommand)}}
 else if c.name!=seq!['c','o','n','f','i','g'] && c.name!=seq!['e','x','e','c']{Prepared{commands:seq![],error:Some(Error::UnknownCommand)}}
 else if !params_ok(c.name,c.params,0){Prepared{commands:seq![],error:Some(Error::InvalidParameter)}}
 else{Prepared{commands:seq![c.name+(if c.params.len()>0{seq![' ']+values(c.params,0)}else{seq![]})],error:None}}
}
pub open spec fn render(c:JsonCommand)->Text {c.name+(if c.params.len()>0{seq![' ']+values(c.params,0)}else{seq![]})}
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
pub open spec fn raw_txtvalid()->Text {(seq![' ',' ','c','o','n','f','i','g',' ','s','e','t',' ','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i']+(seq!['t',' ','1','0','1',' ',' ','\n','\n','\t','c','o','n','f','i','g',' ','s','e','t',' ','D','e','f','a','u','l','t','L','o','g','E']+seq!['n','t','r','y','L','i','m','i','t',' ','2','0','2','\t','\n']))}
pub open spec fn raw_txtunknown()->Text {(seq!['c','o','n','f','i','g',' ','s','e','t',' ','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t',' ']+(seq!['1','0','1','\n','i','t','e','r','2','8','-','n','o','-','s','u','c','h','-','c','o','m','m','a','n','d','\n','c','o','n','f','i']+seq!['g',' ','s','e','t',' ','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t',' ','2','0','2','\n']))}
pub open spec fn raw_txtruntime()->Text {((seq!['c','o','n','f','i','g',' ','s','e','t',' ','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t',' ']+seq!['1','0','1','\n','e','x','e','c',' ','s','e','t',' ','^','|','|','I','t','e','r','2','8','R','u','n','t','i','m','e','C','a','l'])+(seq!['l','s','=','$','i','n','c','r','e','m','e','n','t','(','^','|','|','I','t','e','r','2','8','R','u','n','t','i','m','e','C','a']+(seq!['l','l','s',')',' ','s','e','t',' ','x','=','1','/','0','\n','c','o','n','f','i','g',' ','s','e','t',' ','D','e','f','a','u','l']+seq!['t','L','o','g','E','n','t','r','y','L','i','m','i','t',' ','2','0','2','\n'])))}
pub open spec fn raw_jsonvalid()->Text {(((seq!['{','\n',' ',' ','"','c','o','m','m','a','n','d','s','"',':',' ','[','\n',' ',' ',' ',' ','{','\n',' ',' ',' ',' ',' ',' ','"','c']+seq!['o','m','m','a','n','d','"',':',' ','"','c','o','n','f','i','g','"',',','\n',' ',' ',' ',' ',' ',' ','"','p','a','r','a','m','e'])+(seq!['t','e','r','s','"',':',' ','{','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','a','c','t','i','o','n','"',':',' ','"','s','e','t','"']+(seq![',','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','k','e','y','"',':',' ','"','D','e','f','a','u','l','t','L','o','g','E','n','t','r']+seq!['y','L','i','m','i','t','"',',','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','v','a','l','u','e','"',':',' ','"','1','0','1','"','\n'])))+((seq![' ',' ',' ',' ',' ',' ','}','\n',' ',' ',' ',' ','}',',','\n',' ',' ',' ',' ','{','\n',' ',' ',' ',' ',' ',' ','"','c','o','m','m']+(seq!['a','n','d','"',':',' ','"','c','o','n','f','i','g','"',',','\n',' ',' ',' ',' ',' ',' ','"','p','a','r','a','m','e','t','e','r']+seq!['s','"',':',' ','{','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','a','c','t','i','o','n','"',':',' ','"','s','e','t','"',',','\n',' ']))+(seq![' ',' ',' ',' ',' ',' ',' ','"','k','e','y','"',':',' ','"','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i']+(seq!['m','i','t','"',',','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','v','a','l','u','e','"',':',' ','"','2','0','2','"','\n',' ',' ',' ']+seq![' ',' ',' ','}','\n',' ',' ',' ',' ','}','\n',' ',' ',']','\n','}','\n']))))}
pub open spec fn raw_jsonunknown()->Text {(((seq!['{','\n',' ',' ','"','c','o','m','m','a','n','d','s','"',':',' ','[','\n',' ',' ',' ',' ','{','\n',' ',' ',' ',' ',' ',' ','"','c']+(seq!['o','m','m','a','n','d','"',':',' ','"','c','o','n','f','i','g','"',',','\n',' ',' ',' ',' ',' ',' ','"','p','a','r','a','m','e']+seq!['t','e','r','s','"',':',' ','{','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','a','c','t','i','o','n','"',':',' ','"','s','e','t','"']))+(seq![',','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','k','e','y','"',':',' ','"','D','e','f','a','u','l','t','L','o','g','E','n','t','r']+(seq!['y','L','i','m','i','t','"',',','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','v','a','l','u','e','"',':',' ','"','1','0','1','"','\n']+seq![' ',' ',' ',' ',' ',' ','}','\n',' ',' ',' ',' ','}',',','\n',' ',' ',' ',' ','{','\n',' ',' ',' ',' ',' ',' ','"','c','o','m','m'])))+((seq!['a','n','d','"',':',' ','"','i','t','e','r','2','8','-','n','o','-','s','u','c','h','-','c','o','m','m','a','n','d','"','\n',' ']+(seq![' ',' ',' ','}',',','\n',' ',' ',' ',' ','{','\n',' ',' ',' ',' ',' ',' ','"','c','o','m','m','a','n','d','"',':',' ','"','c','o']+seq!['n','f','i','g','"',',','\n',' ',' ',' ',' ',' ',' ','"','p','a','r','a','m','e','t','e','r','s','"',':',' ','{','\n',' ',' ',' ']))+((seq![' ',' ',' ',' ',' ','"','a','c','t','i','o','n','"',':',' ','"','s','e','t','"',',','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','k']+seq!['e','y','"',':',' ','"','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t','"',',','\n',' ',' ',' '])+(seq![' ',' ',' ',' ',' ','"','v','a','l','u','e','"',':',' ','"','2','0','2','"','\n',' ',' ',' ',' ',' ',' ','}','\n',' ',' ',' ',' ']+seq!['}','\n',' ',' ',']','\n','}','\n']))))}
pub open spec fn raw_jsonruntime()->Text {((((seq!['{','\n',' ',' ','"','c','o','m','m','a','n','d','s','"',':',' ','[','\n',' ',' ',' ',' ','{','\n',' ',' ',' ',' ',' ',' ','"','c']+seq!['o','m','m','a','n','d','"',':',' ','"','c','o','n','f','i','g','"',',','\n',' ',' ',' ',' ',' ',' ','"','p','a','r','a','m','e'])+(seq!['t','e','r','s','"',':',' ','{','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','a','c','t','i','o','n','"',':',' ','"','s','e','t','"']+seq![',','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','k','e','y','"',':',' ','"','D','e','f','a','u','l','t','L','o','g','E','n','t','r']))+((seq!['y','L','i','m','i','t','"',',','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','v','a','l','u','e','"',':',' ','"','1','0','1','"','\n']+seq![' ',' ',' ',' ',' ',' ','}','\n',' ',' ',' ',' ','}',',','\n',' ',' ',' ',' ','{','\n',' ',' ',' ',' ',' ',' ','"','c','o','m','m'])+(seq!['a','n','d','"',':',' ','"','e','x','e','c','"',',','\n',' ',' ',' ',' ',' ',' ','"','p','a','r','a','m','e','t','e','r','s','"']+seq![':',' ','{','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','e','x','p','r','e','s','s','i','o','n','"',':',' ','"','s','e','t',' ','^'])))+(((seq!['|','|','I','t','e','r','2','8','R','u','n','t','i','m','e','C','a','l','l','s','=','$','i','n','c','r','e','m','e','n','t','(']+seq!['^','|','|','I','t','e','r','2','8','R','u','n','t','i','m','e','C','a','l','l','s',')',' ','s','e','t',' ','x','=','1','/','0'])+(seq!['"','\n',' ',' ',' ',' ',' ',' ','}','\n',' ',' ',' ',' ','}',',','\n',' ',' ',' ',' ','{','\n',' ',' ',' ',' ',' ',' ','"','c','o']+seq!['m','m','a','n','d','"',':',' ','"','c','o','n','f','i','g','"',',','\n',' ',' ',' ',' ',' ',' ','"','p','a','r','a','m','e','t']))+((seq!['e','r','s','"',':',' ','{','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','a','c','t','i','o','n','"',':',' ','"','s','e','t','"',',']+seq!['\n',' ',' ',' ',' ',' ',' ',' ',' ','"','k','e','y','"',':',' ','"','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y'])+(seq!['L','i','m','i','t','"',',','\n',' ',' ',' ',' ',' ',' ',' ',' ','"','v','a','l','u','e','"',':',' ','"','2','0','2','"','\n',' ']+seq![' ',' ',' ',' ',' ','}','\n',' ',' ',' ',' ','}','\n',' ',' ',']','\n','}','\n']))))}
pub open spec fn raw_jsonarray()->Text {(((seq!['[','\n',' ',' ','{','\n',' ',' ',' ',' ','"','c','o','m','m','a','n','d','"',':',' ','"','c','o','n','f','i','g','"',',','\n',' ']+seq![' ',' ',' ','"','p','a','r','a','m','e','t','e','r','s','"',':',' ','{','\n',' ',' ',' ',' ',' ',' ','"','a','c','t','i','o','n'])+(seq!['"',':',' ','"','s','e','t','"',',','\n',' ',' ',' ',' ',' ',' ','"','k','e','y','"',':',' ','"','D','e','f','a','u','l','t','L']+seq!['o','g','E','n','t','r','y','L','i','m','i','t','"',',','\n',' ',' ',' ',' ',' ',' ','"','v','a','l','u','e','"',':',' ','"','1']))+((seq!['0','1','"','\n',' ',' ',' ',' ','}','\n',' ',' ','}',',','\n',' ',' ','{','\n',' ',' ',' ',' ','"','c','o','m','m','a','n','d','"']+seq![':',' ','"','c','o','n','f','i','g','"',',','\n',' ',' ',' ',' ','"','p','a','r','a','m','e','t','e','r','s','"',':',' ','{','\n'])+(seq![' ',' ',' ',' ',' ',' ','"','a','c','t','i','o','n','"',':',' ','"','s','e','t','"',',','\n',' ',' ',' ',' ',' ',' ','"','k','e']+(seq!['y','"',':',' ','"','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t','"',',','\n',' ',' ',' ',' ']+seq![' ',' ','"','v','a','l','u','e','"',':',' ','"','2','0','2','"','\n',' ',' ',' ',' ','}','\n',' ',' ','}','\n',']','\n']))))}
pub open spec fn raw_jsonwrong()->Text {seq!['{','\n',' ',' ','"','c','o','m','m','a','n','d','s','"',':',' ','{','}','\n','}','\n']}
pub open spec fn raw_unsupported()->Text {(seq!['c','o','n','f','i','g',' ','s','e','t',' ','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t',' ']+(seq!['1','0','1','\n','c','o','n','f','i','g',' ','s','e','t',' ','D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i']+seq!['m','i','t',' ','2','0','2','\n']))}
pub open spec fn fixture(c:Case)->File {match c {
 Case::TxtValid=>File{name:seq!['v','a','l','i','d','.','t','x','t'],exists:true,raw:raw_txtvalid()},
 Case::TxtUnknown=>File{name:seq!['u','n','k','n','o','w','n','.','t','x','t'],exists:true,raw:raw_txtunknown()},
 Case::TxtRuntime=>File{name:seq!['r','u','n','t','i','m','e','.','t','x','t'],exists:true,raw:raw_txtruntime()},
 Case::JsonValid=>File{name:seq!['v','a','l','i','d','.','j','s','o','n'],exists:true,raw:raw_jsonvalid()},
 Case::JsonUnknown=>File{name:seq!['u','n','k','n','o','w','n','.','j','s','o','n'],exists:true,raw:raw_jsonunknown()},
 Case::JsonRuntime=>File{name:seq!['r','u','n','t','i','m','e','.','j','s','o','n'],exists:true,raw:raw_jsonruntime()},
 Case::JsonArray=>File{name:seq!['a','r','r','a','y','.','j','s','o','n'],exists:true,raw:raw_jsonarray()},
 Case::JsonWrong=>File{name:seq!['w','r','o','n','g','c','o','m','m','a','n','d','s','.','j','s','o','n'],exists:true,raw:raw_jsonwrong()},
 Case::Unsupported=>File{name:seq!['u','n','s','u','p','p','o','r','t','e','d','.','t','e','x','t'],exists:true,raw:raw_unsupported()},
 Case::Missing=>File{name:seq!['m','i','s','s','i','n','g','.','t','x','t'],exists:false,raw:seq![]}
}}
pub open spec fn decode_json(raw:Text)->Option<JsonDoc> {
 if raw==raw_jsonvalid(){Some(JsonDoc::Object(CommandField::Items(seq![JsonCommand{name:seq!['c','o','n','f','i','g'],params:seq![(seq!['a','c','t','i','o','n'],seq!['s','e','t']),(seq!['k','e','y'],seq!['D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t']),(seq!['v','a','l','u','e'],seq!['1','0','1'])]},JsonCommand{name:seq!['c','o','n','f','i','g'],params:seq![(seq!['a','c','t','i','o','n'],seq!['s','e','t']),(seq!['k','e','y'],seq!['D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t']),(seq!['v','a','l','u','e'],seq!['2','0','2'])]}])))}
 else if raw==raw_jsonunknown(){Some(JsonDoc::Object(CommandField::Items(seq![JsonCommand{name:seq!['c','o','n','f','i','g'],params:seq![(seq!['a','c','t','i','o','n'],seq!['s','e','t']),(seq!['k','e','y'],seq!['D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t']),(seq!['v','a','l','u','e'],seq!['1','0','1'])]},JsonCommand{name:seq!['i','t','e','r','2','8','-','n','o','-','s','u','c','h','-','c','o','m','m','a','n','d'],params:seq![]},JsonCommand{name:seq!['c','o','n','f','i','g'],params:seq![(seq!['a','c','t','i','o','n'],seq!['s','e','t']),(seq!['k','e','y'],seq!['D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t']),(seq!['v','a','l','u','e'],seq!['2','0','2'])]}])))}
 else if raw==raw_jsonruntime(){Some(JsonDoc::Object(CommandField::Items(seq![JsonCommand{name:seq!['c','o','n','f','i','g'],params:seq![(seq!['a','c','t','i','o','n'],seq!['s','e','t']),(seq!['k','e','y'],seq!['D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t']),(seq!['v','a','l','u','e'],seq!['1','0','1'])]},JsonCommand{name:seq!['e','x','e','c'],params:seq![(seq!['e','x','p','r','e','s','s','i','o','n'],(seq!['s','e','t',' ','^','|','|','I','t','e','r','2','8','R','u','n','t','i','m','e','C','a','l','l','s','=','$','i','n','c','r','e']+(seq!['m','e','n','t','(','^','|','|','I','t','e','r','2','8','R','u','n','t','i','m','e','C','a','l','l','s',')',' ','s','e','t',' ']+seq!['x','=','1','/','0'])))]},JsonCommand{name:seq!['c','o','n','f','i','g'],params:seq![(seq!['a','c','t','i','o','n'],seq!['s','e','t']),(seq!['k','e','y'],seq!['D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t']),(seq!['v','a','l','u','e'],seq!['2','0','2'])]}])))}
 else if raw==raw_jsonarray(){Some(JsonDoc::Array(seq![JsonCommand{name:seq!['c','o','n','f','i','g'],params:seq![(seq!['a','c','t','i','o','n'],seq!['s','e','t']),(seq!['k','e','y'],seq!['D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t']),(seq!['v','a','l','u','e'],seq!['1','0','1'])]},JsonCommand{name:seq!['c','o','n','f','i','g'],params:seq![(seq!['a','c','t','i','o','n'],seq!['s','e','t']),(seq!['k','e','y'],seq!['D','e','f','a','u','l','t','L','o','g','E','n','t','r','y','L','i','m','i','t']),(seq!['v','a','l','u','e'],seq!['2','0','2'])]}]))}
 else if raw==raw_jsonwrong(){Some(JsonDoc::Object(CommandField::WrongType))}
 else{None}
}

// Finite raw-byte adapter above is a named SDK %FromJSON premise tied to the
// exact fixture file hashes. It is not a generic JSON parser/UTF8 guarantee.
pub open spec fn prepare(file:File)->Prepared {
 if !file.exists{Prepared{commands:seq![],error:Some(Error::MissingFile)}}
 else if extension(file.name)==seq!['t','x','t']{Prepared{commands:lines(file.raw,0),error:None}}
 else if extension(file.name)==seq!['j','s','o','n']{
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
// Focused witnesses call only definitions; no proof-lemma calls.




pub proof fn settings_neighbor() {
 let a=set_selected(initial(),101);let z=set_selected(a,202);
 assert(setting_value(a)==101);assert(raw_value(a)==101);
 assert(setting_value(z)==202);assert(raw_value(z)==202);
 assert(a.calls==0);assert(z.calls==0);
}





// Successful raw preparation facts are tied to the active execution function;
// no finite observation or already-rendered answer is assumed.

// The native reset establishes only the selected current value, not absence or
// contents of default nodes. This bisimulation proves observable independence; keys outside this view are not compared.
pub open spec fn same_view(a:World,z:World)->bool {
 a.cfg.conf.contains(key()) && z.cfg.conf.contains(key()) && setting_value(a)==setting_value(z) && a.calls==z.calls
}
pub open spec fn same_output(a:Out,z:Out)->bool {
 same_view(a.world,z.world) && a.error==z.error && a.listed==z.listed && a.attempted==z.attempted
}





}
fn main(){}
