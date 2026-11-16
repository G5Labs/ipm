use vstd::prelude::*;
#[path="../../../../Verus_verification/consolidated/iter3/artifacts.rs"] mod a;
#[path="../../../../Verus_verification/consolidated/iter3/resources.rs"] mod r;
#[path="../../../../Verus_verification/consolidated/iter3/public_repairs.rs"] mod p;
verus! {
proof fn native_fields() {
    reveal_with_fuel(a::wildcard,12); reveal_with_fuel(a::parse_fields,5);
    reveal_with_fuel(r::apply_defaults,5); reveal_with_fuel(r::check_project,5);
    reveal_with_fuel(p::lower,12); reveal_with_fuel(p::suite_path,8);
    let class=seq![67int,111,110,115,111,108,105,100,97,116,101,100,46,73,116,101,114,51,46,68,101,102,97,117,108,116,115];
    let initial=Map::empty().insert(1int,0int).insert(2int,0int);
    let one=r::DefaultRule{class_name:class,conditions:seq![],values:Map::empty().insert(1int,1int)};
    let two=r::DefaultRule{class_name:class,conditions:seq![(1int,1int)],values:Map::empty().insert(1int,2int).insert(2int,3int)};
    assert(r::satisfies(initial,seq![]));
    let step=r::defaults_step(initial,Set::empty(),one,class);
    assert(step[1]==1); assert(r::satisfies(step,two.conditions));
    let unset=r::Requirements{health:r::Tri::Unset,interoperability:0,sys_namespace:false};
    let health=r::Environment{health:true,interoperability:true,namespace:seq![85int,83,69,82]};
    let first=p::Setting{current:seq![102int,105,114,115,116],first:Some(seq![102int,105,114,115,116])};
    let phases=seq![seq![116int,101,115,116]];
    assert(p::lower(seq![74int,115,79,110]) =~= seq![106int,115,111,110]);
    assert(p::lower(seq![116int,69,115,84]) =~= seq![116int,101,115,116]);
    assert(p::lower(seq![84int,101,115,116]) =~= seq![116int,101,115,116]);
    assert(p::phase_enabled(seq![84int,101,115,116],phases)) by {assert(p::lower(phases[0])==p::lower(seq![84int,101,115,116]));}
    assert(p::phase_enabled(seq![116int,69,115,84],phases)) by {assert(p::lower(phases[0])==p::lower(seq![116int,69,115,84]));}
    assert(seq![65int,66].take(1) =~= seq![65int]);
    assert(seq![65int,46,67].take(2) =~= seq![65int]+seq![46int]);
    assert(seq![65int,66][1] != (seq![65int]+seq![46int])[1]); assert(seq![65int][0]!=seq![90int][0]);
    // secure-bytes
    assert((a::provider_status(a::Auth::Bearer,a::bytes_request())==200) == false);
    // filter-2
    assert((a::parse_fields(seq![(a::Key::Repo,a::Condition::Scalar(seq![114int]))],a::empty_criteria()).is_some()) == false);
    // filter-3
    assert((a::parse_field(a::empty_criteria(),a::Key::Repo,a::Condition::Object(seq![(a::Op::Match,Some(seq![114int,42]))])).is_some()) == false);
    // filter-4
    assert((a::parse_field(a::empty_criteria(),a::Key::Other,a::Condition::Scalar(seq![51int])).is_some()) == false);
    // wild-1
    assert((a::wildcard(seq![97int,46,98],seq![97int,46,98])) == true);
    // wild-2
    assert((a::wildcard(seq![97int,120,98],seq![97int,46,98])) == false);
    // wild-3
    assert((a::wildcard(seq![97int,98],seq![97int,63])) == true);
    // wild-4
    assert((a::wildcard(seq![120int,97,98],seq![97int,42])) == false);
    // cached-bytes
    assert((a::save_download(a::SavedFile{exists:true,bytes:seq![111int,108,100]},seq![112int,114,111,118,105,100,101,114,45,99,111,110,116,101,110,116],200).bytes) == seq![111int,108,100]);
    // activate-local
    assert((a::activate(true,true,a::download(false,a::Auth::Anonymous,a::Download::MissingServer),true).status_ok) == true);
    // export-old
    assert((a::copy_export(a::ExportTree{destination:Map::empty().insert(seq![1int],seq![2int]),siblings:Map::empty()},Map::empty().insert(seq![3int],seq![4int])).destination.contains_key(seq![1int])) == true);
    // export-sibling
    assert((a::copy_export(a::ExportTree{destination:Map::empty(),siblings:Map::empty().insert(seq![1int],seq![2int])},Map::empty()).siblings.contains_key(seq![1int])) == true);
    // default-mode
    assert((r::apply_defaults(initial,Set::empty(),seq![one,two],class)[1]) == 2);
    // default-other
    assert((r::apply_defaults(initial,Set::empty(),seq![one,two],class)[2]) == 3);
    // default-explicit-empty
    assert((r::apply_defaults(initial.insert(1,-1),Set::empty().insert(1),seq![one,two],class)[1]==-1) == true);
    // health-unset
    assert((r::requirement_failures(unset,health).len()==0) == true);
    // health-false
    assert((r::requirement_failures(r::Requirements{health:r::Tri::Off,..unset},health).len()==0) == false);
    // health-implies-interop
    assert((r::requirement_failures(r::Requirements{health:r::Tri::On,interoperability:2,..unset},r::Environment{interoperability:false,..health}).len()==0) == false);
    // disabled-allows-enabled
    assert((r::requirement_failures(r::Requirements{interoperability:2,..unset},health).len()==0) == true);
    // requires-sys
    assert((r::requirement_failures(r::Requirements{sys_namespace:true,..unset},health).len()==0) == false);
    // requires-sys-in-sys
    assert((r::requirement_failures(r::Requirements{sys_namespace:true,..unset},r::Environment{namespace:seq![37int,83,89,83],..health}).len()==0) == true);
    // invoke-default
    assert((p::invoke_ok(p::Tri::Unset,true,false,false)) == false);
    // invoke-unchecked
    assert((p::invoke_ok(p::Tri::Off,true,false,false)) == true);
    // invoke-checked
    assert((p::invoke_ok(p::Tri::On,true,false,false)) == false);
    // setting-no-overwrite
    assert((p::set_value(first,seq![115int,101,99,111,110,100],false).current) == seq![102int,105,114,115,116]);
    // setting-empty-allows-write
    assert((p::set_value(p::set_value(first,seq![],true),seq![116int,104,105,114,100],false).current) == seq![116int,104,105,114,100]);
    // setting-first
    assert((p::set_value(p::set_value(first,seq![],true),seq![116int,104,105,114,100],false).first.unwrap()) == seq![102int,105,114,115,116]);
    // format-case
    assert((p::set_format(first,seq![74int,115,79,110],true).unwrap().current) == seq![74int,115,79,110]);
    // format-invalid
    assert((p::set_format(first,seq![105int,110,118,97,108,105,100],true).is_some()) == false);
    // test-prefix
    assert((p::test_selected(seq![116int,69,115,84],phases,seq![65int],None,seq![65int,66])) == true);
    // owns-sibling
    assert((p::owns_class(seq![65int,66],seq![],seq![65int])) == false);
    // owns-child
    assert((p::owns_class(seq![65int,46,67],seq![],seq![65int])) == true);
    // suite-empty-shadows
    assert((p::test_selected(seq![84int,101,115,116],phases,seq![65int],Some(seq![]),seq![90int])) == true);
    // case-alone
    assert((p::test_selected(seq![84int,101,115,116],phases,seq![65int],None,seq![90int])) == false);
    // class-ignores-case
    assert((p::test_selected(seq![84int,101,115,116],phases,seq![],None,seq![90int])) == true);
    // deploy-parameter-false
    assert((r::deploy(true,true,Some(0),r::Tri::On,false)) == false);
    // deploy-resource-true
    assert((r::deploy(true,true,None,r::Tri::On,false)) == true);
    // deploy-module-default
    assert((r::deploy(true,true,None,r::Tri::Unset,false)) == false);
    // deploy-deployedmodule-default
    assert((r::deploy(true,true,None,r::Tri::Unset,true)) == true);
    // export-overwrites-error
    assert((r::export_item(false,false,true,true).ok) == true);
    // project-after-check
    assert((r::check_project(seq![1int],Set::empty()).len()) == 0);
    // project-deployed-on-error
    assert((r::load_project(true,true,false,false,false,true,false,false,true,false).marked_deployed) == true);
    // project-deployed-stored-on-error
    assert((r::project_database(false,true,false,r::resource_save_allowed(false,seq![65int],Set::empty().insert(seq![65int])))) == true);
    // project-duplicate-store
    assert((r::project_database(false,true,false,r::resource_save_allowed(true,seq![65int],Set::empty().insert(seq![65int])))) == false);
    // project-duplicate-object
    assert((r::load_project(true,true,false,false,false,true,false,false,r::resource_save_allowed(true,seq![65int],Set::empty().insert(seq![65int])),false).marked_deployed) == true);
    // project-marker-on-error
    assert((r::load_project(true,true,false,false,false,true,false,false,true,false).marker) == false);
    // archive-bad-gzip
    assert((a::header_action(a::HeaderRead{status_ok:false,name:seq![],expected:0,stored:0})!=a::HeaderAction::Reject) == true);
    // archive-bad-checksum
    assert((a::header_action(a::HeaderRead{status_ok:true,name:seq![97int],expected:353,stored:0})!=a::HeaderAction::Reject) == false);
    // branch-missing-dev-1794685259577191000
    assert((a::download(true,a::Auth::Anonymous,a::Download::MissingServer).ok) == true);
    // branch-missing-normal-1794685259656940000
    assert((a::download(false,a::Auth::Anonymous,a::Download::MissingServer).ok) == false);
    // branch-auth-failed-dev-1794685259769032000
    assert((a::download(true,a::Auth::Bearer,a::Download::SearchOrFetchError).ok) == false);
    // branch-auth-success-1794685259850828000
    assert((a::download(false,a::Auth::Bearer,a::Download::Complete).ok) == true);
    // branch-anonymous-extract-dev-1794685259923066000
    assert((a::artifact_first_header(true,a::Auth::Anonymous,a::HeaderRead{status_ok:false,name:seq![],expected:0,stored:0}).ok) == true);
    // branch-anonymous-extract-normal-1794685260002427000
    assert((a::artifact_first_header(false,a::Auth::Anonymous,a::HeaderRead{status_ok:false,name:seq![],expected:0,stored:0}).ok) == true);
}
proof fn native_selection() {
    reveal_with_fuel(a::newest,6); reveal_with_fuel(a::wildcard,8); reveal_with_fuel(a::later,5);
    let branch=seq![98int,114,97,110,99,104];let main=seq![109int,97,105,110];let dev=seq![100int,101,118];
    let c=a::Criteria{repo:seq![114int],name:Some(a::Pattern{value:seq![97int,42],wildcard:true}),path:None,props:Map::empty().insert(branch,main)};
    let one=a::Artifact{id:1,repo:seq![114int],name:seq![97int,49],folder:seq![],updated:seq![50int],props:Map::empty().insert(branch,seq![main])};
    let two=a::Artifact{id:2,repo:seq![98int,97,100],name:seq![97int,50],updated:seq![57int],..one};
    let three=a::Artifact{id:3,name:seq![97int,51],updated:seq![51int],props:Map::empty().insert(branch,seq![dev,main]),..one};
    let four=a::Artifact{id:4,name:seq![97int,52],updated:seq![56int],props:Map::empty().insert(branch,seq![dev]),..one};
    assert(c.props.contains_key(branch));
    assert(one.props[branch][0]==main);assert(one.props[branch].contains(main));
    assert(three.props[branch][1]==main);assert(three.props[branch].contains(main));
    assert(four.props[branch].len()==1);assert(four.props[branch][0][0]!=main[0]);
    assert(!four.props[branch].contains(main));
    assert(a::properties(one.props,c.props));assert(a::properties(three.props,c.props));
    assert(a::accepted(one,c));assert(a::accepted(three,c));
    assert(two.repo[0]!=c.repo[0]);assert(!a::accepted(two,c));assert(!a::accepted(four,c));
    // selection
    assert(a::newest(seq![one,two,three,four],c,None).unwrap().name==seq![97int,51]);
    let first=a::Artifact{updated:seq![51int],..one};
    let second=a::Artifact{name:seq![97int,50],..first};
    // tie
    assert(a::newest(seq![first,second],c,None).unwrap().name==seq![97int,49]);
    let older=a::Artifact{name:seq![111int,108,100,101,114,46,116,103,122],..one};
    let newer=a::Artifact{name:seq![110int,101,119,101,114,46,116,103,122],..three};
    // aql-first
    assert(a::authenticated_first(seq![older,newer]).unwrap().name==seq![111int,108,100,101,114,46,116,103,122]);
    // aql-second
    assert(a::authenticated_first(seq![newer,older]).unwrap().name==seq![110int,101,119,101,114,46,116,103,122]);
}
}
fn main() {}
