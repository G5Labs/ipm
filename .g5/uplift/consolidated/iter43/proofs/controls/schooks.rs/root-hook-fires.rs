#![allow(non_snake_case)]
// Iteration43: IPM source-control extension hooks.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter43/bindings.json):
//   ExtensionBase/SourceControl/Interface.cls::%GetClass       hooks are active only when the namespace source-control
//                                                              class extends %IPM.ExtensionBase.Composite and its stored
//                                                              primary extension extends the IPM interface
//   Interface.cls::%OnBeforePhase/%OnAfterPhase/%OnModifyResources/%OnChangeModuleRoot   delegate to that primary
//   ExtensionBase/CompositeMethodOverrides.cls::SetExtensionClasses/GetPrimaryExtensionClass   primary stored in ^SYS
//   Storage/Module.cls::ExecutePhases                          Before before the phase body (error stops the phase),
//                                                              After after it; nested phases nest their hooks
//   Lifecycle/Base.cls::%Reload                                root-change hook compares the already-updated root
//
// Phases: Clean 0, Initialize 1, Reload 2, Validate 4, Compile 6, Activate 7, Configure 14, Unconfigure 15.
// Events: Before p = 2p, After p = 2p+1. Premises (native-qualified): the number of OnModifyResources calls an
// operation makes comes from its resource/manifest saves (load 4, document save 1, reload 2 in the fixture),
// and both checked reload paths store the new root before %Reload compares it.
use vstd::prelude::*;

verus! {

pub enum Config { NoneSet, Direct, Composite { primary_ipm: bool } }
pub open spec fn active(c: Config) -> bool { match c { Config::Composite { primary_ipm } => primary_ipm, _ => false } }

pub enum Op { Load, CompileOnly, DocSave, ReloadOnly, Uninstall }
pub open spec fn op_phases(op: Op) -> Seq<int> {
    match op { Op::Load => seq![1, 2, 4, 6, 7], Op::CompileOnly => seq![6], Op::ReloadOnly => seq![2], Op::Uninstall => seq![0], Op::DocSave => seq![] }
}
pub open spec fn modify_calls(op: Op) -> int { match op { Op::Load => 4, Op::DocSave => 1, Op::ReloadOnly => 2, _ => 0 } }
pub open spec fn nested(p: int) -> int { if p == 7 { 14 } else if p == 0 { 15 } else { -1 } }

pub struct Run { pub events: Seq<int>, pub ok: bool }

// ExecutePhases over the phase list; `fail` is the phase whose Before hook returns an error (-1 for none).
pub open spec fn run_phases(ps: Seq<int>, fail: int) -> Run
    decreases ps.len()
{
    if ps.len() == 0 { Run { events: seq![], ok: true } }
    else {
        let p = ps[0];
        if p == fail { Run { events: seq![2 * p], ok: false } }
        else {
            let inner = if nested(p) >= 0 { seq![2 * nested(p), 2 * nested(p) + 1] } else { seq![] };
            let rest = run_phases(ps.drop_first(), fail);
            Run { events: seq![2 * p] + inner + seq![2 * p + 1] + rest.events, ok: rest.ok }
        }
    }
}

pub struct Obs { pub events: Seq<int>, pub modify: int, pub root: int, pub ok: bool }

pub open spec fn observe(c: Config, op: Op, fail: int) -> Obs {
    let r = run_phases(op_phases(op), if active(c) { fail } else { -1 });
    Obs { events: if active(c) { r.events } else { seq![] }, modify: if active(c) { modify_calls(op) } else { 0 }, root: match op { Op::Load => b(active(c)), Op::ReloadOnly => b(active(c)), _ => 0 }, ok: r.ok }
}

pub open spec fn count(s: Seq<int>, e: int) -> int
    decreases s.len()
{
    if s.len() == 0 { 0 } else { count(s.drop_last(), e) + if s.last() == e { 1int } else { 0int } }
}
pub open spec fn total(o: Obs) -> int { o.events.len() + o.modify + o.root }
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

// Generic neighbours.
pub proof fn inactive_no_calls(c: Config, op: Op)
    requires !active(c)
    ensures observe(c, op, -1).events.len() == 0, observe(c, op, -1).modify == 0
{}
pub proof fn direct_or_secondary_inactive()
    ensures !active(Config::Direct), !active(Config::Composite { primary_ipm: false }), active(Config::Composite { primary_ipm: true })
{}
pub proof fn before_error_stops_phase()
    ensures !observe(Config::Composite { primary_ipm: true }, Op::CompileOnly, 6).ok,
        observe(Config::Composite { primary_ipm: true }, Op::CompileOnly, 6).events == seq![12int],
        observe(Config::Composite { primary_ipm: false }, Op::CompileOnly, 6).ok
{
    reveal_with_fuel(run_phases, 2);
    assert(observe(Config::Composite { primary_ipm: true }, Op::CompileOnly, 6).events =~= seq![12int]);
}
pub proof fn nested_hooks_inside_parent()
    ensures observe(Config::Composite { primary_ipm: true }, Op::Uninstall, -1).events == seq![0int, 30, 31, 1]
{
    reveal_with_fuel(run_phases, 2);
    assert(observe(Config::Composite { primary_ipm: true }, Op::Uninstall, -1).events =~= seq![0int, 30, 31, 1]);
}
// As built, both checked reload paths store the new root first, so the root-change hook is not reached.
pub proof fn root_hook_not_reached()
    ensures observe(Config::Composite { primary_ipm: true }, Op::Load, -1).root == 0,
        observe(Config::Composite { primary_ipm: true }, Op::ReloadOnly, -1).root == 0
{}
pub proof fn load_schedule()
    ensures total(observe(Config::Composite { primary_ipm: true }, Op::Load, -1)) == 16
{
    assert(total(observe(Config::Composite { primary_ipm: true }, Op::Load, -1)) == 16) by (compute);
}

} // verus!

fn main() {}
