#![allow(non_snake_case)]
// Iteration75: neighbour assertions for lifecycle obligations whose inherited models left one side of a
// contrast unexercised, plus two small successors where the inherited token or flag could not represent the
// case.
//
// Source roles at pin 571e04a1:
//   Main.cls::ModuleAction (2830-2849)                 words -> list, IsComplete = '-only
//   Lifecycle/Base.cls::GetCompletePhases (139-162)     one non-standard name returned as-is; otherwise the
//       union of GetCompletePhasesForOne chains (165-189), where a non-standard name contributes ""
//   Lifecycle/Base.cls::MatchSinglePhase (192-214)     -only: $listtostring(list) matched as one name
//   Storage/Module.cls::GetCustomPhases (413-443), ExecutePhases (447-)   custom lookup by lower-cased name
//   ResourceProcessor/CPF.cls::OnBeforePhase (24-32), OnAfterPhase (34-42)   When and Phase decide the merge
//   ExtensionBase/CompositeMethodOverrides.cls::SetPrimaryExtensionClass (81-103)
//   Storage/Module.cls::LifecycleGet (1428-1449)       OverrideLifecycleClass, LifecycleClass, else Module
#[path = "../../consolidated/iter30/phases.rs"]
mod p;
#[path = "../../consolidated/iter48/cpfmerge.rs"]
mod cpf;
#[path = "../../consolidated/iter67/composite.rs"]
mod comp;
#[path = "../../consolidated/iter52/record.rs"]
mod rec;

use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------- lifecycle.custom#4 (inherited phases.rs)

pub open spec fn w0() -> p::World {
    p::World { disk: 0, source: 0, compiled: 0, compile_hooks: 0, activate_hooks: 0, custom_hooks: 0 }
}

// A custom phase runs when it is the only name, with or without -only; beside a standard phase it does not
// run: complete mode drops it (no chain), -only joins both names into one unknown name and fails.
pub proof fn custom_alone_or_beside(w: p::World, disk: int)
    ensures
        p::execute(w, seq![p::CUSTOM()], true, disk).ok,
        p::execute(w, seq![p::CUSTOM()], true, disk).world.custom_hooks == w.custom_hooks + 1,
        p::execute(w, seq![p::CUSTOM()], false, disk).ok,
        p::execute(w, seq![p::CUSTOM()], false, disk).world.custom_hooks == w.custom_hooks + 1,
        !p::execute(w, seq![p::CUSTOM(), 6], true, disk).ok,
        p::execute(w, seq![p::CUSTOM(), 6], true, disk).world.custom_hooks == w.custom_hooks,
        p::execute(w, seq![p::CUSTOM(), 6], true, disk).world.compile_hooks == w.compile_hooks,
        p::execute(w, seq![6, p::CUSTOM()], false, disk).ok,
        p::execute(w, seq![6, p::CUSTOM()], false, disk).world.custom_hooks == w.custom_hooks,
{
    reveal_with_fuel(p::ordered_from, 19);
    reveal_with_fuel(p::any_chain, 3);
    reveal_with_fuel(p::run_steps, 6);
    assert(p::plan(seq![p::CUSTOM()], true) =~= seq![p::CUSTOM()]);
    assert(p::plan(seq![p::CUSTOM(), 6], true) =~= seq![p::INVALID()]);
    assert(p::plan(seq![6, p::CUSTOM()], false) =~= seq![1, 2, 3, 4, 6]) by (compute);
}

// ---------------------------------------------------------------- lifecycle.custom#9 (successor tokens)
// phases.rs has one CUSTOM token, so two distinct declared custom phases cannot be written. Here a name is a
// standard rank 0..17 or any other integer; `customs` is the module's declared custom phase set
// (GetCustomPhases). The chain union is the inherited ordered_from, whose in_chain is false for every
// non-standard name, as GetCompletePhasesForOne returns "" for one.

pub open spec fn BAD() -> int { -1 }

pub open spec fn single_step(t: int, customs: Set<int>) -> int {
    if p::standard(t) { t } else if customs.contains(t) { t } else { BAD() }
}
// Base.cls:139-162.
pub open spec fn complete_plan(tokens: Seq<int>, customs: Set<int>) -> Seq<int> {
    if tokens.len() == 1 && !p::standard(tokens[0]) { seq![single_step(tokens[0], customs)] }
    else { p::ordered_from(tokens, 0) }
}
// ExecutePhases with -only: MatchSinglePhase($listtostring(list)); a comma-joined name is no phase.
pub open spec fn only_plan(tokens: Seq<int>, customs: Set<int>) -> Seq<int> {
    if tokens.len() == 1 { seq![single_step(tokens[0], customs)] } else { seq![BAD()] }
}

pub struct Run { pub custom_runs: Seq<int>, pub phase_runs: Seq<int>, pub ok: bool }

pub open spec fn step(r: Run, s: int, customs: Set<int>) -> Run {
    if !r.ok { r }
    else if s == p::STAR() { r }
    else if s == BAD() { Run { ok: false, ..r } }
    else if customs.contains(s) && !p::standard(s) { Run { custom_runs: r.custom_runs.push(s), ..r } }
    else { Run { phase_runs: r.phase_runs.push(s), ..r } }
}
pub open spec fn run(steps: Seq<int>, customs: Set<int>) -> Run
    decreases steps.len()
{
    if steps.len() == 0 { Run { custom_runs: seq![], phase_runs: seq![], ok: true } }
    else { step(run(steps.drop_last(), customs), steps.last(), customs) }
}
pub open spec fn execute(tokens: Seq<int>, only: bool, customs: Set<int>) -> Run {
    run(if only { only_plan(tokens, customs) } else { complete_plan(tokens, customs) }, customs)
}

pub open spec fn two_customs() -> Set<int> { set![100int, 102int] }

// Two declared custom names run neither and the command succeeds; with -only it fails; one alone runs.
pub proof fn two_custom_names()
    ensures
        execute(seq![100int, 102int], false, two_customs()).ok,
        execute(seq![100int, 102int], false, two_customs()).custom_runs.len() == 0,
        execute(seq![100int, 102int], false, two_customs()).phase_runs.len() == 0,
        !execute(seq![100int, 102int], true, two_customs()).ok,
        execute(seq![100int, 102int], true, two_customs()).custom_runs.len() == 0,
        execute(seq![100int], false, two_customs()).ok,
        execute(seq![100int], false, two_customs()).custom_runs == seq![100int],
        execute(seq![102int], true, two_customs()).custom_runs == seq![102int],
{
    reveal_with_fuel(p::ordered_from, 19);
    reveal_with_fuel(p::any_chain, 3);
    reveal_with_fuel(run, 2);
    assert(p::ordered_from(seq![100int, 102int], 0) =~= Seq::<int>::empty()) by (compute);
    assert(run(seq![100int], two_customs()).custom_runs =~= seq![100int]);
    assert(run(seq![102int], two_customs()).custom_runs =~= seq![102int]);
}

// ---------------------------------------------------------------- module.processor.cpf#2 (inherited cpfmerge.rs)

// When="Before" merges before its phase's work and not after; When="After" the reverse; neither merges at
// another phase.
pub proof fn before_and_after(ph: int, other: int)
    requires ph != other
    ensures
        cpf::merges_in(cpf::Cpf { phase: ph, when: cpf::When::Before, custom: 0, valid: true }, ph, true),
        !cpf::merges_in(cpf::Cpf { phase: ph, when: cpf::When::Before, custom: 0, valid: true }, ph, false),
        !cpf::merges_in(cpf::Cpf { phase: ph, when: cpf::When::After, custom: 0, valid: true }, ph, true),
        cpf::merges_in(cpf::Cpf { phase: ph, when: cpf::When::After, custom: 0, valid: true }, ph, false),
        !cpf::merges_in(cpf::Cpf { phase: ph, when: cpf::When::Before, custom: 0, valid: true }, other, true),
        !cpf::merges_in(cpf::Cpf { phase: ph, when: cpf::When::After, custom: 0, valid: true }, other, false),
{}

// ---------------------------------------------------------------- lifecycle.source-control.editor#7

// CompositeMethodOverrides.cls:81-103. `classes` is the class dictionary (%Dictionary.ClassDefinition) and
// `extensions` the classes extending %Studio.Extension.Base. Premise: the dictionary has no class with the
// empty name, so %ExistsId("") is false (name 0 stands for "").
pub open spec fn set_primary(old: int, name: int, classes: Set<int>, extensions: Set<int>) -> (int, bool) {
    if !classes.contains(name) { (old, true) }
    else if !extensions.contains(name) { (old, false) }
    else { (name, true) }
}

pub proof fn primary_neighbours()
    ensures
        // empty name: previous primary kept, no error
        set_primary(1, 0, set![1int, 2, 3], set![1int, 2]) == (1int, true),
        // a class name extending the base replaces the primary
        set_primary(1, 2, set![1int, 2, 3], set![1int, 2]) == (2int, true),
        // a class that is not an extension: error, primary kept
        set_primary(1, 3, set![1int, 2, 3], set![1int, 2]) == (1int, false),
        // the inherited flag form agrees on the empty and class-name cases
        comp::primary_after_set(1, 0, false) == 1,
        comp::primary_after_set(1, 2, true) == 2,
{}

// The inherited primary_after_set is the successor's primary for names that are either absent from the
// dictionary or extensions.
pub proof fn inherited_agrees(old: int, name: int, classes: Set<int>, extensions: Set<int>)
    requires extensions.subset_of(classes), !classes.contains(name) || extensions.contains(name)
    ensures set_primary(old, name, classes, extensions).0 == comp::primary_after_set(old, name, extensions.contains(name))
{}

// ---------------------------------------------------------------- module.lifecycle-class#1 (inherited record.rs)

// LifecycleGet: no LifecycleClass resolves to Module whatever is compiled; a compiled named class is used.
pub proof fn lifecycle_class_neighbours(n: int)
    ensures
        rec::resolve(None, true) == rec::Life::Module,
        rec::resolve(None, false) == rec::Life::Module,
        rec::resolve(Some(n), true) == rec::Life::Named(n),
        rec::resolve(Some(n), false) == rec::Life::Module,
{}

} // verus!

fn main() {}
