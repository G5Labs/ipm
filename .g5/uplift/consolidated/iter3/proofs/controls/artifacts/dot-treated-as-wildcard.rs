//! Hand translation of pinned Artifactory decisions. See TRUST.md for representation and ports.
use vstd::prelude::*;
verus! {
pub type Text = Seq<int>;
#[derive(PartialEq, Eq)]
pub enum Op { Eq, Match, Other }
pub enum Condition { Scalar(Text), Object(Seq<(Op, Option<Text>)>), Array }
pub struct Pattern { pub value: Text, pub wildcard: bool }
pub open spec fn condition(c: Condition, allow_match: bool) -> Option<Pattern> {
    match c {
        Condition::Scalar(v) => Some(Pattern {value:v,wildcard:false}),
        Condition::Object(ops) => {
            if ops.len()!=1 { None }
            else { match ops[0].1 {
                None => None,
                Some(v) => if ops[0].0==Op::Eq || (ops[0].0==Op::Match && allow_match) {
                    Some(Pattern {value:v,wildcard:ops[0].0==Op::Match})
                } else {None}
            }}
        },
        Condition::Array => None,
    }
}
pub open spec fn wildcard(value: Text, pattern: Text) -> bool
    decreases value.len()+pattern.len()
{
    if pattern.len()==0 {value.len()==0}
    else if pattern[0]==42 {
        wildcard(value,pattern.drop_first()) || (value.len()>0 && wildcard(value.drop_first(),pattern))
    } else {
        value.len()>0 && (pattern[0]==63 || pattern[0]==46 || pattern[0]==value[0])
            && wildcard(value.drop_first(),pattern.drop_first())
    }
}
pub open spec fn matches(value: Text, p: Option<Pattern>) -> bool {
    match p {None=>true,Some(p)=> if p.wildcard {wildcard(value,p.value)} else {value==p.value}}
}
pub enum Key { Repo, Name, Path, Type, Property(Text), Other }
pub struct Criteria { pub repo:Text, pub name:Option<Pattern>, pub path:Option<Pattern>, pub props:Map<Text,Text> }
pub open spec fn empty_criteria() -> Criteria {
    Criteria {repo:seq![],name:None,path:None,props:Map::empty()}
}
pub open spec fn parse_field(c:Criteria, key:Key, raw:Condition) -> Option<Criteria> {
    let p=condition(raw,matches!(key,Key::Name|Key::Path));
    match p {
        None=>None,
        Some(p)=> match key {
            Key::Repo=>Some(Criteria{repo:p.value,..c}),
            Key::Name=>Some(Criteria{name:Some(p),..c}),
            Key::Path=>Some(Criteria{path:Some(p),..c}),
            Key::Type=>if p.value==seq![102int,105,108,101] {Some(c)} else {None},
            Key::Property(k)=>Some(Criteria{props:c.props.insert(k,p.value),..c}),
            Key::Other=>None,
        }
    }
}
pub open spec fn parse_fields(fields:Seq<(Key,Condition)>, c:Criteria) -> Option<Criteria>
    decreases fields.len()
{
    if fields.len()==0 {if c.name.is_some() || c.props.len()>0 {Some(c)} else {None}}
    else {match parse_field(c,fields[0].0,fields[0].1) {
        None=>None, Some(next)=>parse_fields(fields.drop_first(),next)
    }}
}
pub struct Artifact { pub id:int, pub repo:Text, pub name:Text, pub folder:Text, pub updated:Text, pub props:Map<Text,Seq<Text>> }
pub open spec fn properties(actual:Map<Text,Seq<Text>>, expected:Map<Text,Text>) -> bool {
    forall|k:Text| #[trigger] expected.contains_key(k) ==> actual.contains_key(k) && actual[k].contains(expected[k])
}
pub open spec fn accepted(a:Artifact,c:Criteria) -> bool {
    (c.repo.len()==0 || a.repo==c.repo) && matches(a.name,c.name)
        && matches(if a.folder.len()==0 {seq![46int]} else {a.folder},c.path) && properties(a.props,c.props)
}
pub open spec fn later(a:Text,b:Text) -> bool
    decreases a.len()+b.len()
{
    if a.len()==0 {false} else if b.len()==0 {true}
    else if a[0]!=b[0] {a[0]>b[0]} else {later(a.drop_first(),b.drop_first())}
}
pub open spec fn choose_candidate(best:Option<Artifact>,a:Artifact,c:Criteria) -> Option<Artifact> {
    if !accepted(a,c) {best}
    else {match best {None=>Some(a),Some(old)=>if later(a.updated,old.updated) {Some(a)} else {Some(old)}}}
}
pub open spec fn newest(rows:Seq<Artifact>,c:Criteria,best:Option<Artifact>) -> Option<Artifact>
    decreases rows.len()
{
    if rows.len()==0 {best} else {newest(rows.drop_first(),c,choose_candidate(best,rows[0],c))}
}
pub open spec fn authenticated_first(rows:Seq<Artifact>) -> Option<Artifact> {
    if rows.len()==0 {None} else {Some(rows[0])}
}
pub proof fn authenticated_order_is_a_provider_premise() {
    let old=Artifact{id:1,repo:seq![],name:seq![],folder:seq![],updated:seq![49int],props:Map::empty()};
    let new=Artifact{id:2,updated:seq![50int],..old};
    assert(authenticated_first(seq![old,new])==Some(old));
    assert(authenticated_first(seq![new,old])==Some(new));
    assert(authenticated_first(seq![]).is_none());
}
#[derive(PartialEq, Eq)]
pub enum Auth { Anonymous, Basic, Bearer }
pub open spec fn auth(user:Text,password:Text,token:Text) -> Auth {
    if token.len()>0 {Auth::Bearer} else if user.len()>0 && password.len()>0 {Auth::Basic} else {Auth::Anonymous}
}
pub struct Request { pub authorization:Auth }
pub open spec fn metadata_request(u:Text,p:Text,t:Text) -> Request {Request{authorization:auth(u,p,t)}}
pub open spec fn bytes_request() -> Request {Request{authorization:Auth::Anonymous}}
// This provider is a deliberately scoped server requiring the same credential at both endpoints.
pub open spec fn provider_status(required:Auth,r:Request) -> int {
    if required==Auth::Anonymous || required==r.authorization {200} else {401}
}
pub struct SavedFile {pub exists:bool,pub bytes:Text}
pub open spec fn save_download(old:SavedFile,response:Text,status:int) -> SavedFile {
    if status!=200 || old.exists {old} else {SavedFile{exists:true,bytes:response}}
}
#[derive(PartialEq, Eq)]
pub enum Download { MissingServer, SearchOrFetchError, ExtractError, Complete }
pub struct DownloadResult {pub ok:bool,pub skipped:bool,pub extract_called:bool}
pub open spec fn download(dev:bool,a:Auth,event:Download) -> DownloadResult {
    match event {
        Download::MissingServer=>DownloadResult{ok:dev,skipped:dev,extract_called:false},
        Download::SearchOrFetchError=>DownloadResult{ok:dev && a==Auth::Anonymous,skipped:dev && a==Auth::Anonymous,extract_called:false},
        Download::ExtractError=>DownloadResult{ok:false,skipped:false,extract_called:true},
        Download::Complete=>DownloadResult{ok:true,skipped:false,extract_called:true},
    }
}
// The gzip/header decoder is a named native port. This kernel covers the first-header decision,
// not checksum calculation, PAX, path safety or file-body extraction.
pub struct HeaderRead {pub status_ok:bool,pub name:Text,pub expected:nat,pub stored:nat}
#[derive(PartialEq, Eq)]
pub enum HeaderAction { Stop, Reject, Entry }
pub open spec fn header_action(r:HeaderRead) -> HeaderAction {
    if r.name.len()==0 {HeaderAction::Stop}
    else if r.expected!=r.stored {HeaderAction::Reject} else {HeaderAction::Entry}
}
pub open spec fn artifact_first_header(dev:bool,a:Auth,r:HeaderRead) -> DownloadResult {
    download(dev,a,if header_action(r)==HeaderAction::Reject {Download::ExtractError} else {Download::Complete})
}
// Proposed repairs only; the pinned implementation continues to use the as-built functions.
pub open spec fn repaired_header_action(r:HeaderRead) -> HeaderAction {
    if !r.status_ok {HeaderAction::Reject} else {header_action(r)}
}
pub open spec fn repaired_bytes_request(u:Text,p:Text,t:Text) -> Request {metadata_request(u,p,t)}
pub proof fn header_repair_preserves_successful_reads(r:HeaderRead)
    requires r.status_ok
    ensures repaired_header_action(r)==header_action(r)
{}
pub proof fn header_repair_refuses_failed_reads(r:HeaderRead)
    requires !r.status_ok
    ensures repaired_header_action(r)==HeaderAction::Reject
{}
pub proof fn credential_repair_neighbors() {
    assert(provider_status(Auth::Bearer,repaired_bytes_request(seq![],seq![],seq![1int]))==200);
    assert(provider_status(Auth::Anonymous,repaired_bytes_request(seq![],seq![],seq![1int]))==200);
    assert(repaired_header_action(HeaderRead{status_ok:true,name:seq![],expected:0,stored:0})==HeaderAction::Stop);
}
pub proof fn failed_read_is_not_rejected() {
    let bad=HeaderRead{status_ok:false,name:seq![],expected:0,stored:0};
    assert(header_action(bad)==HeaderAction::Stop);
    assert(artifact_first_header(false,Auth::Anonymous,bad).ok);
    assert(artifact_first_header(true,Auth::Anonymous,bad).ok);
    let checksum=HeaderRead{status_ok:true,name:seq![97int],expected:353,stored:0};
    assert(header_action(checksum)==HeaderAction::Reject);
    assert(!artifact_first_header(true,Auth::Anonymous,checksum).ok);
    assert(header_action(HeaderRead{expected:0,..checksum})==HeaderAction::Entry);
}
pub struct Activation {pub request_download:bool,pub copy_called:bool,pub status_ok:bool}
pub open spec fn activate(phase:bool,local_directory:bool,d:DownloadResult,copy_ok:bool) -> Activation {
    Activation{request_download:phase && !local_directory,
        copy_called:phase && (local_directory || (d.ok && !d.skipped)),
        status_ok:!phase || (if local_directory {copy_ok} else {d.ok && (d.skipped || copy_ok)})}
}
// Overlay replaces equal file names and retains unrelated target files; siblings are untouched.
pub struct ExportTree {pub destination:Map<Text,Text>,pub siblings:Map<Text,Text>}
pub open spec fn copy_export(old:ExportTree,source:Map<Text,Text>) -> ExportTree {
    ExportTree{destination:Map::new(old.destination.dom().union(source.dom()),|k:Text| if source.contains_key(k) {source[k]} else {old.destination[k]}),siblings:old.siblings}
}
pub proof fn filter_boundaries() {
    reveal_with_fuel(parse_fields,5);
    assert(condition(Condition::Object(seq![(Op::Match,Some(seq![42int]))]),false).is_none());
    assert(condition(Condition::Object(seq![(Op::Match,Some(seq![42int]))]),true).is_some());
    assert(condition(Condition::Object(seq![(Op::Eq,Some(seq![1int])),(Op::Eq,Some(seq![2int]))]),true).is_none());
    assert(condition(Condition::Object(seq![(Op::Eq,None)]),true).is_none());
    assert(parse_fields(seq![(Key::Repo,Condition::Scalar(seq![1int]))],empty_criteria()).is_none());
    assert(parse_fields(seq![(Key::Name,Condition::Scalar(seq![]))],empty_criteria()).is_some());
    assert(parse_fields(seq![(Key::Other,Condition::Scalar(seq![1int]))],empty_criteria()).is_none());
    assert(parse_field(empty_criteria(),Key::Repo,Condition::Object(seq![(Op::Match,Some(seq![42int]))])).is_none());
    assert(parse_fields(seq![(Key::Type,Condition::Scalar(seq![1int])),(Key::Name,Condition::Scalar(seq![1int]))],empty_criteria()).is_none());
}
pub proof fn wildcard_neighbors() {
    reveal_with_fuel(wildcard,8);
    assert(wildcard(seq![97int,46,98],seq![97int,42]));
    assert(wildcard(seq![],seq![42int]));
    assert(wildcard(seq![97int,46,98],seq![97int,63,98]));
    assert(!wildcard(seq![97int,120,98],seq![97int,46,98]));
    assert(!wildcard(seq![120int,97],seq![97int,42]));
}
pub proof fn rejected_candidate_frames(best:Option<Artifact>,a:Artifact,c:Criteria)
    requires !accepted(a,c)
    ensures choose_candidate(best,a,c)==best
{}
pub proof fn selection_neighbors() {
    let c=Criteria{repo:seq![1int],name:Some(Pattern{value:seq![97int,42],wildcard:true}),path:None,props:Map::empty()};
    let a=Artifact{id:1,repo:seq![1int],name:seq![97int],folder:seq![],updated:seq![50int],props:Map::empty()};
    let b=Artifact{id:2,updated:seq![51int],..a};
    let wrong=Artifact{id:3,repo:seq![2int],updated:seq![57int],..a};
    reveal_with_fuel(newest,5);reveal_with_fuel(wildcard,5);reveal_with_fuel(later,4);
    assert(wrong.repo[0]!=c.repo[0]);
    assert(accepted(a,c)); assert(accepted(b,c)); assert(!accepted(wrong,c));
    assert(later(b.updated,a.updated));
    assert(choose_candidate(None,a,c)==Some(a));
    assert(choose_candidate(Some(a),wrong,c)==Some(a));
    assert(choose_candidate(Some(a),b,c)==Some(b));
    assert(newest(seq![a,wrong,b],c,None)==Some(b));
    let tie=Artifact{id:4,..b};
    assert(newest(seq![b,tie],c,None)==Some(b));
    assert(newest(seq![wrong],c,None).is_none());
    let need=Map::empty().insert(seq![7int],seq![8int]);
    assert(need.contains_key(seq![7int]));
    assert(!Map::<Text,Seq<Text>>::empty().contains_key(seq![7int]));
    assert(!properties(Map::empty(),need));
    assert(properties(Map::empty().insert(seq![7int],seq![seq![9int],seq![8int]]),need));
    let pc=Criteria{props:need,path:Some(Pattern{value:seq![46int],wildcard:false}),..c};
    let pass=Artifact{props:Map::empty().insert(seq![7int],seq![seq![8int]]),..a};
    let wrong_name=Artifact{name:seq![122int],..pass};
    let wrong_path=Artifact{folder:seq![120int],..pass};
    assert(pass.props[seq![7int]][0]==seq![8int]);
    assert(pass.props[seq![7int]].contains(seq![8int]));
    assert(properties(pass.props,need));
    assert(matches(seq![46int],pc.path));
    assert(accepted(pass,pc));
    assert(!accepted(a,pc));
    assert(!accepted(wrong_name,pc)) by {assert(wrong_name.name[0]!=97);}
    assert(!accepted(wrong_path,pc)) by {assert(wrong_path.folder[0]!=46);}

}
pub proof fn credentials_cross_endpoint() {
    assert(auth(seq![1int],seq![],seq![])==Auth::Anonymous);
    assert(auth(seq![1int],seq![2int],seq![3int])==Auth::Bearer);
    assert(auth(seq![1int],seq![2int],seq![])==Auth::Basic);
    assert(provider_status(Auth::Bearer,metadata_request(seq![],seq![],seq![3int]))==200);
    assert(provider_status(Auth::Bearer,bytes_request())==401);
    assert(provider_status(Auth::Anonymous,bytes_request())==200);
}
pub proof fn cache_and_failure_neighbors() {
    let old=SavedFile{exists:true,bytes:seq![1int]};
    assert(save_download(old,seq![2int],200).bytes==seq![1int]);
    assert(save_download(SavedFile{exists:false,bytes:seq![]},seq![2int],200).bytes==seq![2int]);
    assert(save_download(SavedFile{exists:false,bytes:seq![]},seq![2int],401).exists==false);
    assert(download(true,Auth::Anonymous,Download::SearchOrFetchError).ok);
    assert(!download(true,Auth::Bearer,Download::SearchOrFetchError).ok);
    assert(!download(false,Auth::Anonymous,Download::SearchOrFetchError).ok);
    assert(!download(true,Auth::Anonymous,Download::ExtractError).ok);
    assert(download(true,Auth::Bearer,Download::MissingServer).skipped);
    let error=download(false,Auth::Anonymous,Download::SearchOrFetchError);
    assert(!activate(true,true,error,true).request_download);
    assert(activate(true,true,error,true).copy_called);
    assert(activate(true,true,error,true).status_ok);
    assert(!activate(true,false,error,true).status_ok);
    let old_tree=ExportTree{destination:Map::empty().insert(seq![1int],seq![2int]),siblings:Map::empty().insert(seq![3int],seq![4int])};
    let copied=copy_export(old_tree,Map::empty());
    assert(copied.destination[seq![1int]]==seq![2int]);
    let overwrite=copy_export(old_tree,Map::empty().insert(seq![1int],seq![3int]));
    assert(overwrite.destination[seq![1int]]==seq![3int]);
    assert(copied.siblings==old_tree.siblings);
}
}
fn main() {}
