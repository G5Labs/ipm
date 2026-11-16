// IPM 571e04a1 transaction/lock owners outside load. Hand translation only.
// Environment ports: IRIS object locks, filesystem enumeration/manifest decoding,
// source-control hooks and routine Save/Compile. They are inputs, not assumed successes.
#[path="semver.rs"] pub mod semver;
use vstd::prelude::*;
verus! {
use semver::{Version,Text,Fields};
pub struct ModuleState {pub record:Version,pub manifest:Version,pub disk:Version}
pub enum Request {Major,Minor,Patch,Current,Explicit(Fields)}
pub enum Error {None,MissingModule,MissingStream,InvalidVersion,NotHigher,Checkout,Save,Rewrite}
pub struct ModuleOut {pub state:ModuleState,pub ok:bool,pub error:Error,pub tlevel:nat}
pub open spec fn bump(v:Version,which:nat) -> Version {
    Version {major:if which==0 {v.major+1} else {v.major},minor:if which==0 {0} else if which==1 {v.minor+1} else {v.minor},
        patch:if which<2 {0} else {v.patch+1},pre:seq![],build:seq![]}
}
pub open spec fn with_pre(v:Version,pre:Text) -> Version {Version{pre,..v}}
pub open spec fn with_build(v:Version,build:Text) -> Version {Version{build,..v}}
pub open spec fn set_record(s:ModuleState,v:Version) -> ModuleState {ModuleState{record:v,..s}}
pub open spec fn update_version(s:ModuleState,v:Version) -> ModuleState {ModuleState{record:s.record,manifest:v,disk:s.disk}}
pub open spec fn module_version(s:ModuleState,r:Request,pre:Option<Text>,build:Option<Text>,force:bool,
    exists:bool,stream:bool,checkout:bool,save:bool,rewrite:bool,outer:nat) -> ModuleOut {
    let candidate=match r {Request::Major=>bump(s.record,0),Request::Minor=>bump(s.record,1),Request::Patch=>bump(s.record,2),Request::Current=>s.record,Request::Explicit(f)=>semver::version(f)};
    let valid=match r {Request::Explicit(f)=>semver::validate(f),_=>true};
    let candidate=match pre {Some(p)=>with_pre(candidate,p),None=>candidate};
    let candidate=match build {Some(b)=>with_build(candidate,b),None=>candidate};
    let err=if !exists {Error::MissingModule} else if !stream {Error::MissingStream} else if !valid {Error::InvalidVersion}
        else if !force && !semver::follows(candidate,s.record,false) {Error::NotHigher}
        else if candidate==s.record {Error::None}
        else if !checkout {Error::Checkout} else if !save {Error::Save} else if !rewrite {Error::Rewrite} else {Error::None};
    let ok=err is None;
    ModuleOut {state:if ok && candidate!=s.record {update_version(set_record(s,candidate),candidate)} else {s},ok,error:err,tlevel:outer}
}

pub enum Manifest {Valid,Unreadable,InvalidName}
pub struct Entry {pub key:nat,pub depth:nat,pub manifest:Manifest}
pub enum CacheAction {Cached,Skipped,Error}
pub struct Repository {pub exists:bool,pub cache:Set<nat>}
pub struct CacheOut {pub repo:Repository,pub ok:bool,pub lock_held:bool,pub tlevel:nat}
pub open spec fn scan(e:Entry,depth:nat) -> bool {depth==0 || e.depth<=depth}
pub open spec fn add_cache(e:Entry) -> CacheAction {
    match e.manifest {Manifest::Valid=>CacheAction::Cached,Manifest::Unreadable=>CacheAction::Skipped,Manifest::InvalidName=>CacheAction::Error}
}
pub open spec fn scanned_error(es:Seq<Entry>,depth:nat,i:nat) -> bool
    decreases es.len()-i
{ i<es.len() && ((scan(es[i as int],depth) && add_cache(es[i as int]) is Error) || scanned_error(es,depth,i+1)) }
pub open spec fn scanned_keys(es:Seq<Entry>,depth:nat,i:nat) -> Set<nat>
    decreases es.len()-i
{ if i>=es.len() {Set::empty()} else {
    let rest=scanned_keys(es,depth,i+1);
    if scan(es[i as int],depth) && add_cache(es[i as int]) is Cached {rest.insert(es[i as int].key)} else {rest}
} }
pub open spec fn lock_class_id(held:Set<nat>,id:nat,other_holds:bool) -> Option<Set<nat>> {
    if other_holds {None} else {Some(held.insert(id))}
}
pub open spec fn unlock_class_id(held:Set<nat>,id:nat) -> Set<nat> {held.remove(id)}
pub open spec fn unlock_all(held:Set<nat>) -> Set<nat> {Set::empty()}
pub open spec fn build_cache(old:Repository,es:Seq<Entry>,depth:nat,purge:bool,other_holds:bool,save_ok:bool,outer:nat) -> CacheOut {
    let acquired=old.exists && purge && !other_holds;
    let ok=!(old.exists && purge && other_holds) && save_ok && !scanned_error(es,depth,0);
    CacheOut {repo:if ok {Repository{exists:true,cache:if purge {scanned_keys(es,depth,0)} else {old.cache.union(scanned_keys(es,depth,0))}}} else {old},
        ok,lock_held:acquired && outer>0,tlevel:outer}
}

pub enum Line {Start,End,User(nat),Generated(nat)}
pub open spec fn skip_count(lines:Seq<Line>,i:nat) -> nat
    decreases if i < lines.len() {(lines.len()-i) as nat} else {0nat}
{ if i>=lines.len() {0} else if lines[i as int] is End {1} else {1+skip_count(lines,i+1)} }
pub open spec fn skip_block(lines:Seq<Line>,i:nat) -> nat {i+skip_count(lines,i)}
pub open spec fn rewrite_from(lines:Seq<Line>,gen:Seq<Line>,i:nat,found:bool) -> Seq<Line>
    decreases if i < lines.len() {(lines.len()-i) as nat} else {0nat}
{ if i>=lines.len() {if found {seq![]} else {gen}} else if lines[i as int] is Start {
    gen + rewrite_from(lines,gen,i+1+skip_count(lines,i+1),true)
} else {seq![lines[i as int]] + rewrite_from(lines,gen,i+1,found)} }
pub open spec fn zlang_one(lines:Seq<Line>,gen:Seq<Line>,test_only:bool) -> Seq<Line> {
    if test_only {lines} else {rewrite_from(lines,gen,0,false)}
}
// The observable user content is the sequence outside the generated tags.
pub open spec fn outside_users(lines:Seq<Line>,i:nat) -> Seq<nat>
    decreases if i < lines.len() {(lines.len()-i) as nat} else {0nat}
{ if i>=lines.len() {seq![]} else {match lines[i as int] {
    Line::Start=>outside_users(lines,i+1+skip_count(lines,i+1)),Line::User(n)=>seq![n]+outside_users(lines,i+1),_=>outside_users(lines,i+1)
}} }

pub proof fn stored_version_does_not_write_disk(s:ModuleState,v:Version)
    ensures update_version(s,v).disk==s.disk,update_version(s,v).record==s.record,update_version(s,v).manifest==v
{}
pub proof fn no_force_same_version_refused(s:ModuleState,outer:nat)
    ensures !module_version(s,Request::Current,None,None,false,true,true,true,true,true,outer).ok,
        module_version(s,Request::Current,None,None,false,true,true,true,true,true,outer).state==s
{
    assert(!semver::follows(s.record,s.record,false)) by {
        if s.record.pre.len()>0 {assert(!semver::compare_dot(s.record.pre,s.record.pre)) by {compare_self(s.record.pre);}}
    }
}
pub proof fn compare_self(s:Text)
    ensures !semver::compare_dot(s,s)
    decreases s.len()
{ if semver::first(s,'.',0)<s.len() {compare_self(semver::after(s,'.'));} }
pub proof fn forced_current_is_noop(s:ModuleState,outer:nat)
    ensures module_version(s,Request::Current,None,None,true,true,true,true,true,true,outer).ok,
        module_version(s,Request::Current,None,None,true,true,true,true,true,true,outer).state==s
{}
pub proof fn failed_cache_preserves_rows(old:Repository,es:Seq<Entry>,depth:nat,purge:bool,other:bool,save:bool,outer:nat)
    requires !build_cache(old,es,depth,purge,other,save,outer).ok
    ensures build_cache(old,es,depth,purge,other,save,outer).repo==old,
        build_cache(old,es,depth,purge,other,save,outer).tlevel==outer
{}
pub proof fn foreign_lock_refuses_refresh(old:Repository,es:Seq<Entry>,depth:nat,outer:nat)
    requires old.exists
    ensures !build_cache(old,es,depth,true,true,true,outer).ok,build_cache(old,es,depth,true,true,true,outer).repo==old
{}
pub proof fn successful_refresh_holds_outer_lock(old:Repository,es:Seq<Entry>,depth:nat,outer:nat)
    requires old.exists,outer>0,build_cache(old,es,depth,true,false,true,outer).ok
    ensures build_cache(old,es,depth,true,false,true,outer).lock_held
{}
pub proof fn cache_item_kinds_and_scan_limit(e:Entry,depth:nat)
    ensures (e.manifest is Unreadable ==> add_cache(e) is Skipped),
        (e.manifest is InvalidName ==> add_cache(e) is Error),
        scan(e,0),(depth>0 && e.depth>depth ==> !scan(e,depth))
{}
pub proof fn bump_preserves_unselected_core(v:Version)
    ensures bump(v,0).major==v.major+1,bump(v,0).minor==0,bump(v,0).patch==0,
        bump(v,1).major==v.major,bump(v,1).minor==v.minor+1,bump(v,1).patch==0,
        bump(v,2).major==v.major,bump(v,2).minor==v.minor,bump(v,2).patch==v.patch+1,
        bump(v,2).pre==Seq::<char>::empty(),bump(v,2).build==Seq::<char>::empty(),
        with_pre(v,seq!['r','c']).build==v.build,with_build(v,seq!['x']).pre==v.pre
{}
pub open spec fn witness_state() -> ModuleState {ModuleState{record:semver::v(seq!['1','.','2','.','3']),manifest:semver::v(seq!['1','.','2','.','3']),disk:semver::v(seq!['1','.','2','.','3'])}}
pub open spec fn witness_entries() -> Seq<Entry> {seq![Entry{key:1,depth:1,manifest:Manifest::Valid},Entry{key:2,depth:2,manifest:Manifest::InvalidName}]}
pub open spec fn witness_gen() -> Seq<Line> {seq![Line::Start,Line::Generated(8),Line::End]}
pub open spec fn witness_lines() -> Seq<Line> {seq![Line::User(1),Line::Start,Line::Generated(3),Line::End,Line::User(2)]}
pub proof fn witnessed_owners_distinctions() {
    assert(module_version(witness_state(),Request::Major,None,None,false,true,true,true,true,true,0).ok) by (compute);
    assert(module_version(witness_state(),Request::Major,None,None,false,true,true,true,true,true,0).state.record.major==2) by (compute);
    assert(module_version(witness_state(),Request::Major,None,None,false,true,true,true,true,true,0).state.manifest.major==2) by (compute);
    assert(module_version(witness_state(),Request::Major,None,None,false,true,true,true,true,true,0).state.disk.major==1) by (compute);
    assert(scanned_error(witness_entries(),0,0)) by (compute);
    assert(!scanned_error(witness_entries(),1,0)) by (compute);
    assert(add_cache(Entry{key:2,depth:1,manifest:Manifest::Unreadable}) is Skipped) by (compute);
    assert(zlang_one(witness_lines(),witness_gen(),false)==seq![Line::User(1),Line::Start,Line::Generated(8),Line::End,Line::User(2)]) by (compute);
    assert(zlang_one(zlang_one(witness_lines(),witness_gen(),false),witness_gen(),false)==zlang_one(witness_lines(),witness_gen(),false)) by (compute);
    assert(outside_users(zlang_one(witness_lines(),witness_gen(),false),0)==outside_users(witness_lines(),0)) by (compute);
}
}
fn main(){}
