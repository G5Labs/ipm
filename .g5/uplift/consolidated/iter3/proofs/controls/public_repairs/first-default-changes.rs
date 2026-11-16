//! Repairs to now-public distinctions; these are not independent discoveries.
use vstd::prelude::*;
verus! {
pub type Text=Seq<int>;
#[derive(PartialEq, Eq)]
pub enum Tri { Unset, Off, On }
pub open spec fn invoke_checked(check:Tri,status_return:bool) -> bool {
    check!=Tri::Off && (status_return || check==Tri::On)
}
pub open spec fn invoke_ok(check:Tri,status_return:bool,returned_ok:bool,threw:bool) -> bool {
    !threw && (!invoke_checked(check,status_return) || returned_ok)
}
pub open spec fn invoke_selected(actual:Text,normal:Text,custom:Text,before:bool,declared_before:bool) -> bool {
    actual==(if custom.len()>0 {custom} else {normal}) && before==declared_before
}
pub open spec fn repo_visited(enabled:bool,registry_match:bool,filesystem:bool,keywords:Seq<Text>,rejected_in_command:bool) -> bool {
    enabled && registry_match && !(filesystem && keywords.len()>0) && !rejected_in_command
}
pub struct Setting {pub current:Text,pub first:Option<Text>}
pub open spec fn set_value(old:Setting,value:Text,overwrite:bool) -> Setting {
    if !overwrite && old.current.len()>0 {old}
    else {Setting{current:value,first:Some(value)}}
}
pub open spec fn lower(s:Text) -> Text
    decreases s.len()
{
    if s.len()==0 {s} else {
        seq![if 65<=s[0] && s[0]<=90 {s[0]+32} else {s[0]}] + lower(s.drop_first())
    }
}
pub open spec fn format_valid(value:Text) -> bool {
    value.len()==0 || lower(value)==seq![106int,115,111,110] || lower(value)==seq![121int,97,109,108] || lower(value)==seq![116int,111,111,110]
}
pub open spec fn set_format(old:Setting,value:Text,overwrite:bool) -> Option<Setting> {
    if format_valid(value) {Some(set_value(old,value,overwrite))} else {None}
}
pub open spec fn prefix(value:Text,start:Text) -> bool {
    start.len()<=value.len() && value.take(start.len() as int)==start
}
pub open spec fn phase_enabled(phase:Text,declared:Seq<Text>) -> bool {
    exists|i:int| 0<=i<declared.len() && lower(declared[i])==lower(phase)
}
pub open spec fn suite_path(s:Text) -> Text
    decreases s.len()
{
    if s.len()==0 {s} else {seq![if s[0]==47 || s[0]==92 {46int} else {s[0]}]+suite_path(s.drop_first())}
}
pub open spec fn test_selected(phase:Text,declared:Seq<Text>,package:Text,suite:Option<Text>,case:Text) -> bool {
    let requested=match suite {Some(s)=>suite_path(s),None=>case};
    phase_enabled(phase,declared) && (package.len()==0 || requested.len()==0 || prefix(package,requested) || prefix(requested,package))
}
pub open spec fn valid_test_target(class:Text,package:Text) -> bool {
    (class.len()>0)!=(package.len()>0)
}
pub open spec fn owns_class(class:Text,declared_class:Text,package:Text) -> bool {
    class.len()>0 && (if declared_class.len()>0 {class==declared_class}
        else {package.len()>0 && (class==package || prefix(class,package+seq![46int]))})
}
pub proof fn invoke_neighbors() {
    assert(!invoke_ok(Tri::Unset,true,false,false));
    assert(invoke_ok(Tri::Off,true,false,false));
    assert(!invoke_ok(Tri::On,false,false,false));
    assert(invoke_ok(Tri::Unset,false,false,false));
    assert(!invoke_ok(Tri::Off,true,true,true));
    assert(invoke_selected(seq![2int],seq![1int],seq![2int],false,false));
    assert(!invoke_selected(seq![1int],seq![1int],seq![2int],false,false)) by {assert(seq![1int][0]!=seq![2int][0]);}
}
pub proof fn search_neighbors() {
    assert(!repo_visited(true,true,true,seq![seq![1int]],false));
    assert(repo_visited(true,true,true,seq![],false));
    assert(repo_visited(true,true,false,seq![seq![1int]],false));
    assert(!repo_visited(true,true,false,seq![],true));
}
pub proof fn setting_neighbors() {
    let empty=Setting{current:seq![],first:None};
    let first=set_value(empty,seq![1int],false);
    assert(first.current==seq![1int] && first.first==Some(seq![1int]));
    assert(set_value(first,seq![2int],false).current==seq![1int]);
    assert(set_value(first,seq![2int],true).current==seq![2int]);
    assert(set_value(first,seq![2int],true).first==Some(seq![1int]));
    let reset=set_value(first,seq![],true);
    assert(set_value(reset,seq![2int],false).current==seq![2int]);
    assert(set_value(reset,seq![2int],false).first==Some(seq![1int]));
    reveal_with_fuel(lower,8);
    assert(lower(seq![74int,83,79,78]) =~= seq![106int,115,111,110]);
    assert(set_format(first,seq![74int,83,79,78],true).unwrap().current==seq![74int,83,79,78]);
    assert(set_format(first,seq![88int],true).is_none());
    assert(set_format(first,seq![],true).is_some());
}
pub proof fn test_neighbors() {
    reveal_with_fuel(lower,5);
    let phase=seq![84int];let phases=seq![seq![116int]];
    assert(phase_enabled(phase,phases)) by {assert(lower(phases[0])==lower(phase));}
    assert(test_selected(phase,phases,seq![],Some(seq![9int]),seq![8int]));
    assert(valid_test_target(seq![1int],seq![]));
    assert(!valid_test_target(seq![1int],seq![2int]));
    assert(!valid_test_target(seq![],seq![]));
    let package=seq![65int];let sibling=seq![65int,66];let child=seq![65int,46,67];
    assert(sibling.take(1) =~= package);
    assert(test_selected(phase,phases,package,None,sibling));
    assert(!owns_class(sibling,seq![],package)) by {assert(sibling[1]!=(package+seq![46int])[1]);}
    assert(child.take(2) =~= package+seq![46int]);
    assert(owns_class(child,seq![],package));
    assert(test_selected(phase,phases,package,Some(seq![]),seq![90int]));
    assert(!test_selected(phase,phases,package,None,seq![90int])) by {assert(package[0]!=seq![90int][0]);}
    assert(!owns_class(seq![],seq![],seq![]));
}
}
fn main() {}
