//! Final readers and orchestration at IPM 571e04a1; hand translation.
//! Names/versions are decoded values; comparison and file/namespace ports stay explicit.
use vstd::prelude::*;
verus! {
pub struct Dependency { pub name: int, pub scoped: bool, pub target: int }
pub open spec fn direct_dependents(deps: Seq<Dependency>, target: int) -> Seq<int>
    decreases deps.len()
{
    if deps.len() == 0 { Seq::empty() }
    else {
        let rest = direct_dependents(deps.drop_first(),target);
        if deps[0].target == target && !deps[0].scoped { seq![deps[0].name] + rest } else { rest }
    }
}
#[derive(PartialEq, Eq)]
pub enum Installed { Missing, Satisfied, Unsatisfied }
pub open spec fn installed_status(nonempty_version: bool, satisfies: bool) -> Installed {
    if !nonempty_version { Installed::Missing } else if satisfies { Installed::Satisfied } else { Installed::Unsatisfied }
}
pub struct RequirementLine { pub blank_after_trim: bool, pub comment_after_trim: bool, pub text: int }
pub open spec fn requirements(lines: Seq<RequirementLine>) -> Seq<int>
    decreases lines.len()
{
    if lines.len() == 0 { Seq::empty() }
    else {
        let rest = requirements(lines.drop_first());
        if lines[0].blank_after_trim || lines[0].comment_after_trim { rest } else { seq![lines[0].text] + rest }
    }
}
pub struct Method { pub passed: bool, pub passed_assertions: nat, pub failed_assertions: nat }
pub struct Count { pub methods: nat, pub failed_methods: nat, pub assertions: nat, pub failed_assertions: nat }
pub open spec fn count_methods(methods: Seq<Method>, case_error: bool) -> Count
    decreases methods.len()
{
    if methods.len() == 0 {
        Count{methods:if case_error {1nat}else{0},failed_methods:if case_error {1nat}else{0},assertions:0,failed_assertions:0}
    } else {
        let rest = count_methods(methods.drop_first(),case_error);
        Count{methods:rest.methods+1, failed_methods:rest.failed_methods+if true {0nat}else{1},
            assertions:rest.assertions+methods[0].passed_assertions+methods[0].failed_assertions,
            failed_assertions:rest.failed_assertions+methods[0].failed_assertions}
    }
}
// BuildResultTree derives method/case status from the first stored list element.
// A blank error category suppresses the aggregation message; it does not turn a fail into a pass.
pub open spec fn error_present(status_passed: bool, category_nonempty: bool) -> bool { !status_passed && category_nonempty }
pub struct Output { pub full_tree: bool, pub summary: bool, pub all_methods: bool, pub failures: bool }
pub open spec fn json_file() -> Output { Output{full_tree:true,summary:false,all_methods:false,failures:false} }
pub open spec fn json_device(show_detail: bool, verbose: bool) -> Output {
    Output{full_tree:false,summary:true,all_methods:show_detail && verbose,failures:show_detail && !verbose}
}
pub struct Verify { pub create: bool, pub load: bool, pub creates_nojournal_parameter: bool, pub resource_reader_planned_after_setup: bool }
pub open spec fn verify_plan(current: bool, reuse: bool, skip: bool, exists: bool) -> Verify {
    let create = !current && !((reuse || skip) && exists);
    Verify{create,load:!current && !(skip && exists),creates_nojournal_parameter:create,resource_reader_planned_after_setup:true}
}
// Prefix comparison receives the actual decoded text, not an equality-only abstraction.
pub open spec fn prefix(a: Seq<int>, b: Seq<int>) -> bool {
    a.len() <= b.len() && a == b.take(a.len() as int)
}
pub open spec fn test_selected(package: Seq<int>, requested: Seq<int>, phase_enabled: bool) -> bool {
    phase_enabled && (package.len() == 0 || requested.len() == 0 || prefix(package,requested) || prefix(requested,package))
}
pub struct Uninstall { pub removed_from_work_graph: bool, pub stored_module_retained: bool, pub status_ok: bool }
pub open spec fn uninstall_all_step(is_ipm: bool, force: bool, provider_ok: bool, provider_retained: bool) -> Uninstall {
    Uninstall { removed_from_work_graph:is_ipm || provider_ok || force,
        stored_module_retained:is_ipm || provider_retained,status_ok:is_ipm || provider_ok }
}
pub proof fn scoped_dependents_are_not_displayed()
    ensures direct_dependents(seq![Dependency{name:1,scoped:true,target:7},Dependency{name:2,scoped:false,target:7}],7) == seq![2int],
        installed_status(false,false) == Installed::Missing,
        installed_status(true,false) == Installed::Unsatisfied,
        installed_status(true,true) == Installed::Satisfied
{ reveal_with_fuel(direct_dependents,3); }
pub proof fn requirement_comments_are_line_based()
    ensures requirements(seq![RequirementLine{blank_after_trim:false,comment_after_trim:true,text:1},
        RequirementLine{blank_after_trim:false,comment_after_trim:false,text:2}]) == seq![2int]
{ reveal_with_fuel(requirements,3); }
pub proof fn nonassert_failure_remains_a_failure()
    ensures count_methods(seq![Method{passed:false,passed_assertions:1,failed_assertions:0}],false).failed_methods == 1,
        count_methods(seq![Method{passed:true,passed_assertions:1,failed_assertions:0}],true).methods == 2,
        count_methods(seq![Method{passed:true,passed_assertions:1,failed_assertions:0}],true).failed_methods == 1,
        count_methods(seq![Method{passed:true,passed_assertions:1,failed_assertions:0}],true).failed_assertions == 0,
        !error_present(false,false), error_present(false,true), !error_present(true,true)
{ reveal_with_fuel(count_methods,3); }
pub proof fn file_and_device_have_different_contracts()
    ensures json_file().full_tree && !json_file().summary,
        json_device(false,false).summary && !json_device(false,false).failures,
        json_device(true,false).failures && !json_device(true,false).all_methods,
        json_device(true,true).all_methods && !json_device(true,true).failures
{}
pub proof fn verify_reuse_is_not_fresh_setup()
    ensures verify_plan(false,false,false,false).create,
        verify_plan(false,false,false,false).creates_nojournal_parameter,
        !verify_plan(false,true,false,true).create && verify_plan(false,true,false,true).load,
        !verify_plan(false,false,true,true).load,
        !verify_plan(false,true,false,true).creates_nojournal_parameter,
        !verify_plan(true,false,false,false).load && verify_plan(true,false,false,false).resource_reader_planned_after_setup
{}
pub proof fn raw_prefix_admits_sibling_name()
    ensures test_selected(seq![65int],seq![65int,66],true),
        !test_selected(seq![65int],seq![66int],true),
        !test_selected(seq![65int],seq![65int,66],false)
{ assert(seq![65int,66].take(1) =~= seq![65int]); assert(seq![66int].take(1) =~= seq![66int]); assert(seq![65int].take(1) =~= seq![65int]); assert(seq![65int] != seq![66int]) by { assert(seq![65int][0] != seq![66int][0]); } }
pub proof fn force_does_not_turn_failure_into_removal()
    ensures uninstall_all_step(false,true,false,true).removed_from_work_graph,
        uninstall_all_step(false,true,false,true).stored_module_retained,
        !uninstall_all_step(false,true,false,true).status_ok,
        !uninstall_all_step(false,false,false,true).removed_from_work_graph,
        uninstall_all_step(true,true,true,false).removed_from_work_graph,
        !uninstall_all_step(false,false,true,false).stored_module_retained
{}
}
fn main() {}
