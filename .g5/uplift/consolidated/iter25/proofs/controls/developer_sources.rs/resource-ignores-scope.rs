// IPM571e04a1: ordinary same-version FS developer reinstall, two class layouts.
// Typed ^Sources writers and product declared-path readers are DISTINCT branches.
// No confirmed external modern ^Sources consumer, SDK legacy success, queue,
// Invoke/installer behavior, complete load body, generic path/DB/compiler proof.
// Mode/root/MODULE writer is Lifecycle.Base::%Reload, NOT CheckBeforeClean.
// Excluded boundaries remain UNASSESSED/UNPURSUED; private inputs not accessed.
use vstd::prelude::*;
verus! {
pub type Text=Seq<u8>;
#[derive(PartialEq,Eq)]
pub enum Case {Implicit,Explicit}
pub struct Resource {pub internal:Text,pub extension_parameter:Text,pub directory_attribute:Option<Text>,pub postfix:Text}
pub struct Installed {pub name:Text,pub version:Text,pub root:Text,pub sources_root:Text,pub developer:bool,pub resource:Resource}
pub struct Processor {pub name:Text,pub extension:Text,pub directory:Text,pub directory_defined:bool,pub postfix:Text}
pub struct Key {pub category:Text,pub name:Text}
pub struct Entry {pub key:Key,pub value:Text}
pub struct World {pub a:Option<Installed>,pub b:Option<Installed>,pub a_resource:Option<Entry>,pub b_resource:Option<Entry>,pub a_module:Option<Entry>,pub b_module:Option<Entry>,pub no_folders:bool}
pub struct Args {pub developer:Option<bool>,pub top_install:bool,pub permit_downgrade:bool}
#[derive(PartialEq,Eq)]
pub enum ResultKind {Loaded,DeveloperGuard,UnassessedProvider}
pub struct Outcome {pub world:World,pub kind:ResultKind}
pub struct Info {pub aware:bool,pub resource_type:Text,pub prefix:Text,pub relative:Text}
pub open spec fn one()->Text {seq![49u8,46,48,46,48]}
pub open spec fn cls()->Text {seq![99u8,108,115]}
pub open spec fn raw_cls()->Text {seq![67u8,76,83]}
pub open spec fn src()->Text {seq![115u8,114,99]}
pub open spec fn code()->Text {seq![99u8,111,100,101]}
pub open spec fn module_category()->Text {seq![77u8,79,68,85,76,69]}
pub open spec fn ipm()->Text {seq![122u8,112,109]}
pub open spec fn name(c:Case)->Text {if c==Case::Implicit{seq![105u8,116,101,114,50,53,45,115,111,117,114,99,101]}else{seq![105u8,116,101,114,50,53,45,101,120,112,108,105,99,105,116]}}
pub open spec fn class_name(c:Case)->Text {if c==Case::Implicit{seq![73u8,116,101,114,50,53,46,86,97,108,117,101]}else{seq![73u8,116,101,114,50,53,46,69,120,112,108,105,99,105,116]}}
pub open spec fn root(c:Case)->Text {if c==Case::Implicit{seq![47u8,116,109,112,47,105,116,101,114,50,53,47,114,101,112,111,47,105,116,101,114,50,53,45,115,111,117,114,99,101,47]}else{seq![47u8,116,109,112,47,105,116,101,114,50,53,47,114,101,112,111,47,105,116,101,114,50,53,45,101,120,112,108,105,99,105,116,47]}}
pub open spec fn physical_file(c:Case)->Text {if c==Case::Implicit{seq![47u8,116,109,112,47,105,116,101,114,50,53,47,114,101,112,111,47,105,116,101,114,50,53,45,115,111,117,114,99,101,47,115,114,99,47,73,116,101,114,50,53,47,86,97,108,117,101,46,99,108,115]}else{seq![47u8,116,109,112,47,105,116,101,114,50,53,47,114,101,112,111,47,105,116,101,114,50,53,45,101,120,112,108,105,99,105,116,47,115,114,99,47,99,111,100,101,47,73,116,101,114,50,53,47,69,120,112,108,105,99,105,116,46,99,108,115]}}
pub open spec fn actual_import_directory(c:Case)->Text {if c==Case::Implicit{seq![47u8,116,109,112,47,105,116,101,114,50,53,47,114,101,112,111,47,105,116,101,114,50,53,45,115,111,117,114,99,101,47,115,114,99,47]}else{seq![47u8,116,109,112,47,105,116,101,114,50,53,47,114,101,112,111,47,105,116,101,114,50,53,45,101,120,112,108,105,99,105,116,47,115,114,99,47,99,111,100,101,47]}}
pub open spec fn expected_relative(c:Case)->Text {if c==Case::Implicit{seq![115u8,114,99,47,99,108,115,47,73,116,101,114,50,53,47,86,97,108,117,101,46,99,108,115]}else{seq![115u8,114,99,47,99,111,100,101,47,73,116,101,114,50,53,47,69,120,112,108,105,99,105,116,46,99,108,115]}}
pub open spec fn expected_sync(c:Case)->Text {if c==Case::Implicit{seq![115u8,114,99,47,99,108,115]}else{seq![115u8,114,99,47,99,111,100,101]}}
pub open spec fn slash()->Text {seq![47u8]}
pub open spec fn prefix()->Text {seq![8u8]}
pub open spec fn item(c:Case)->Text {class_name(c)+seq![46u8]+raw_cls()}
pub open spec fn fixture_resource(c:Case)->Resource {Resource{internal:item(c),extension_parameter:raw_cls(),directory_attribute:if c==Case::Explicit{Some(code())}else{None},postfix:seq![]}}
pub open spec fn fixture_module(c:Case,developer:bool)->Installed {Installed{name:name(c),version:one(),root:root(c),sources_root:src(),developer,resource:fixture_resource(c)}}
pub open spec fn lower_from(s:Text,i:nat)->Text
 decreases if i<s.len(){(s.len()-i) as nat}else{0nat}
{if i>=s.len(){seq![]}else{let x=s[i as int];seq![if 65<=x && x<=90{(x+32) as u8}else{x}]+lower_from(s,i+1)}}
pub open spec fn lower(s:Text)->Text {lower_from(s,0)}
pub open spec fn last_dot(s:Text,i:nat,last:nat)->nat
 decreases if i<s.len(){(s.len()-i) as nat}else{0nat}
{if i>=s.len(){last}else{last_dot(s,i+1,if s[i as int]==46{i}else{last})}}
pub open spec fn basename(s:Text)->Text {s.take(last_dot(s,0,0) as int)}
pub open spec fn suffix(s:Text)->Text {let i=last_dot(s,0,0);if i<s.len(){s.skip((i+1) as int)}else{seq![]}}
// Ordinary non-directory resource branch; directory resources and arbitrary
// property/default metadata application remain outside this selected contract.
pub open spec fn construct(r:Resource)->Processor {let ext=lower(if r.extension_parameter.len()==0{suffix(r.internal)}else{r.extension_parameter});
 Processor{name:basename(r.internal),extension:ext,directory:match r.directory_attribute{Some(d)=>if d.len()>0{d}else{ext},None=>ext},directory_defined:r.directory_attribute.is_some(),postfix:r.postfix}}
pub open spec fn translate_from(s:Text,i:nat)->Text
 decreases if i<s.len(){(s.len()-i) as nat}else{0nat}
{if i>=s.len(){seq![]}else{(if s[i as int]==46{slash()}else if s[i as int]==37{seq![]}else{seq![s[i as int]]})+translate_from(s,i+1)}}
pub open spec fn translate(s:Text)->Text {translate_from(s,0)}
pub open spec fn source_directory(m:Installed)->Text {m.root+m.sources_root+slash()}
pub open spec fn starts(s:Text,p:Text)->bool {p.len()<=s.len() && s.take(p.len() as int)==p}
// Relevant directory provider domain: one known physical class per source tree,
// no extra category directory. Prefix/file membership qualifies DirectoryExists;
// general filesystem enumeration/normalization is not proved.
// ONLY A:DirectoryDefined0 + missing cls, B:DirectoryDefined1 + existing code.
// DirectoryDefined is constructor/readback only; no missing-explicit-directory
// or arbitrary category-selection/fallback rule is claimed.
pub open spec fn selected_directory(m:Installed,p:Processor,c:Case)->Text {let category=source_directory(m)+p.directory+slash();if starts(physical_file(c),category){category}else{source_directory(m)}}
pub open spec fn import_file(m:Installed,p:Processor,c:Case)->Text {selected_directory(m,p,c)+translate(p.name)+seq![46u8]+cls()}
pub open spec fn resource_key(p:Processor)->Key {Key{category:p.extension,name:p.name+p.postfix}}
pub open spec fn expected_resource_key(c:Case)->Key {Key{category:cls(),name:class_name(c)}}
pub open spec fn expected_resource(c:Case)->Entry {Entry{key:expected_resource_key(c),value:prefix()+actual_import_directory(c)}}
pub open spec fn expected_module(c:Case)->Entry {Entry{key:Key{category:module_category(),name:name(c)},value:prefix()+root(c)}}
pub open spec fn initial()->World {World{a:None,b:None,a_resource:None,b_resource:None,a_module:None,b_module:None,no_folders:false}}
pub open spec fn module(w:World,c:Case)->Option<Installed> {if c==Case::Implicit{w.a}else{w.b}}
pub open spec fn with_module(w:World,c:Case,m:Installed)->World {if c==Case::Implicit{World{a:Some(m),..w}}else{World{b:Some(m),..w}}}
pub open spec fn processor(w:World,c:Case)->Option<Processor> {match module(w,c){Some(m)=>Some(construct(m.resource)),None=>None}}
pub open spec fn normal_args()->Args {Args{developer:None,top_install:true,permit_downgrade:false}}
pub open spec fn developer_args()->Args {Args{developer:Some(true),..normal_args()}}
pub open spec fn requested_developer(a:Args)->bool {a.developer==Some(true)}
pub open spec fn overwrite_guard(w:World,c:Case,a:Args)->bool {!a.permit_downgrade && a.top_install && name(c)!=ipm() && match module(w,c){Some(m)=>m.developer && !requested_developer(a),None=>false}}
pub open spec fn effective_mode(w:World,c:Case,a:Args)->bool {match a.developer{Some(v)=>v,None=>match module(w,c){Some(m)=>m.developer,None=>false}}}
// Conditional successful %Reload metadata/save and MODULE writes. NoFolders is
// GLOBAL and is framed by normal loads, including the later explicit normal case.
pub open spec fn module_reload(w:World,c:Case,a:Args)->World {let mode=effective_mode(w,c,a);let m=fixture_module(c,mode);let w1=with_module(w,c,m);let entry=Entry{key:Key{category:module_category(),name:m.name},value:prefix()+m.root};if mode{if c==Case::Implicit{World{a_module:Some(entry),no_folders:true,..w1}}else{World{b_module:Some(entry),no_folders:true,..w1}}}else{w1}}
pub open spec fn resource_write(w:World,c:Case,p:Processor,developer:bool,eligible:bool,generated:bool,root_exists:bool)->World {
 if developer && true && !generated && root_exists && p.extension.len()>0 && p.name.len()>0 {let m=module(w,c).unwrap();let entry=Entry{key:resource_key(p),value:prefix()+selected_directory(m,p,c)};
 if c==Case::Implicit{World{a_resource:Some(entry),..w}}else{World{b_resource:Some(entry),..w}}}else{w}}
pub open spec fn perform(w:World,c:Case,a:Args)->Outcome {
 if overwrite_guard(w,c,a){Outcome{world:w,kind:ResultKind::DeveloperGuard}}else{
 let m=fixture_module(c,effective_mode(w,c,a));let p=construct(m.resource);
 if import_file(m,p,c)!=physical_file(c){Outcome{world:w,kind:ResultKind::UnassessedProvider}}else{
 let w1=module_reload(w,c,a);Outcome{world:resource_write(w1,c,p,m.developer,true,false,true),kind:ResultKind::Loaded}}}}
pub open spec fn ok(o:Outcome)->bool {o.kind==ResultKind::Loaded}
pub open spec fn normal()->Outcome {perform(initial(),Case::Implicit,normal_args())}
pub open spec fn developer()->Outcome {perform(normal().world,Case::Implicit,developer_args())}
pub open spec fn refused()->Outcome {perform(developer().world,Case::Implicit,normal_args())}
pub open spec fn explicit_normal()->Outcome {perform(refused().world,Case::Explicit,normal_args())}
pub open spec fn explicit_developer()->Outcome {perform(explicit_normal().world,Case::Explicit,developer_args())}
pub open spec fn explicit_refused()->Outcome {perform(explicit_developer().world,Case::Explicit,normal_args())}
pub open spec fn normal_cutpoint()->World {World{a:Some(fixture_module(Case::Implicit,false)),..initial()}}
pub open spec fn dev_cutpoint()->World {World{a:Some(fixture_module(Case::Implicit,true)),a_resource:Some(expected_resource(Case::Implicit)),a_module:Some(expected_module(Case::Implicit)),no_folders:true,..initial()}}
pub open spec fn explicit_normal_cutpoint()->World {World{b:Some(fixture_module(Case::Explicit,false)),..dev_cutpoint()}}
pub open spec fn explicit_dev_cutpoint()->World {World{b:Some(fixture_module(Case::Explicit,true)),b_resource:Some(expected_resource(Case::Explicit)),b_module:Some(expected_module(Case::Explicit)),..dev_cutpoint()}}
pub open spec fn resource_mapping(w:World,c:Case)->Option<Text> {let e=if c==Case::Implicit{w.a_resource}else{w.b_resource};match e{Some(v)=>if v.key==expected_resource_key(c){Some(v.value)}else{None},None=>None}}
pub open spec fn module_mapping(w:World,c:Case)->Option<Text> {let e=if c==Case::Implicit{w.a_module}else{w.b_module};match e{Some(v)=>if v.key==(Key{category:module_category(),name:name(c)}){Some(v.value)}else{None},None=>None}}
pub open spec fn stored_mode(w:World,c:Case)->bool {match module(w,c){Some(m)=>m.developer,None=>false}}
pub open spec fn stored_root(w:World,c:Case)->Text {match module(w,c){Some(m)=>m.root,None=>seq![]}}
pub open spec fn stored_version(w:World,c:Case)->Text {match module(w,c){Some(m)=>m.version,None=>seq![]}}
pub open spec fn source_info(w:World,c:Case)->Option<Info> {match processor(w,c){Some(p)=>Some(Info{aware:true,resource_type:p.extension,prefix:p.name+p.postfix,relative:p.directory}),None=>None}}
// Selected ordinary class/preloadfalse/nonempty declared directory. Path readers
// are descriptors/formatters, not ^Sources consumers or import fallback finders.
pub open spec fn declared_relative(w:World,c:Case)->Text {match module(w,c){Some(m)=>{let p=construct(m.resource);m.sources_root+slash()+p.directory+slash()+translate(basename(m.resource.internal))+seq![46u8]+cls()},None=>seq![]}}
pub open spec fn declared_absolute(w:World,c:Case)->Text {stored_root(w,c)+declared_relative(w,c)}
pub open spec fn sync_directory(w:World,c:Case)->Text {match module(w,c){Some(m)=>m.sources_root+slash()+construct(m.resource).directory,None=>seq![]}}
// Static fixture-file membership only, not a general filesystem existence proof.
pub open spec fn file_exists(path:Text)->bool {path==physical_file(Case::Implicit) || path==physical_file(Case::Explicit)}
pub open spec fn b(v:bool)->int {if v{1}else{0}}
// Independent controls call definitions only; no proof lemmas in selectors.





pub open spec fn other_internal()->Text {seq![79u8,116,104,101,114,46,86,97,108,117,101,46,67,76,83]}
pub open spec fn other_name_world()->World {let m=normal_cutpoint().a.unwrap();World{a:Some(Installed{resource:Resource{internal:other_internal(),..m.resource},..m}),..normal_cutpoint()}}


pub proof fn resource_guard_neighbor()
 ensures resource_write(normal_cutpoint(),Case::Implicit,construct(fixture_resource(Case::Implicit)),false,true,false,true)==normal_cutpoint(),
 resource_write(normal_cutpoint(),Case::Implicit,construct(fixture_resource(Case::Implicit)),true,false,false,true)==normal_cutpoint(),
 resource_write(normal_cutpoint(),Case::Implicit,construct(fixture_resource(Case::Implicit)),true,true,true,true)==normal_cutpoint(),
 resource_write(normal_cutpoint(),Case::Implicit,construct(fixture_resource(Case::Implicit)),true,true,false,false)==normal_cutpoint(),
 resource_write(normal_cutpoint(),Case::Implicit,Processor{extension:seq![],..construct(fixture_resource(Case::Implicit))},true,true,false,true)==normal_cutpoint(),
 resource_write(normal_cutpoint(),Case::Implicit,Processor{name:seq![],..construct(fixture_resource(Case::Implicit))},true,true,false,true)==normal_cutpoint()
{}



}
fn main(){}
