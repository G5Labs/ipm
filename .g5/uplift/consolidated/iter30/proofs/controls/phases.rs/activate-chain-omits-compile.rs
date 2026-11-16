#![allow(non_snake_case)]
// Iteration30: module-action / RunOnePhase phase selection through ExecutePhases to observable phase work.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter30/bindings.json):
//   Main.cls::ModuleAction, Main.cls::RunOnePhase          command words -> phase list, IsComplete = '-only
//   Lifecycle/Base.cls::GetCompletePhases / ForOne          complete-mode union of chains, fixed order, single non-standard kept
//   Lifecycle/Base.cls::MatchSinglePhase                    -only mode: $listtostring of the list matched as ONE phase name
//   Storage/Module.cls::GetCustomPhases, ExecutePhases      custom lookup, "*" refresh, per-phase hooks, START lines, stop on error
//   Lifecycle/Base.cls::%DispatchMethod, Lifecycle/Module.cls::%Activate   Activate's nested Configure phase
//
// Tokens abstract case-insensitive command words: 0..17 are standard phase ranks in
// %IPM.DataType.PhaseName VALUELIST order (3 is "*", never a user token here), CUSTOM is the
// module's declared custom phase, UNKNOWN is any other word. Premises (native-qualified, not proved):
// case folding of ASCII words, no comma in a declared custom phase name, and the ObjectScript runtime
// raising METHOD DOES NOT EXIST for a lifecycle method name that is neither standard nor custom.
use vstd::prelude::*;

verus! {

pub open spec fn CUSTOM() -> int { 100 }
pub open spec fn UNKNOWN() -> int { 101 }
// Step label for a name that is neither a standard phase nor a declared custom phase.
pub open spec fn INVALID() -> int { 101 }
pub open spec fn STAR() -> int { 3 }
pub open spec fn INITIALIZE() -> int { 1 }
pub open spec fn RELOAD() -> int { 2 }
pub open spec fn VALIDATE() -> int { 4 }
pub open spec fn COMPILE() -> int { 6 }
pub open spec fn ACTIVATE() -> int { 7 }
pub open spec fn CONFIGURE() -> int { 14 }

pub open spec fn standard(t: int) -> bool { 0 <= t <= 17 && t != 3 }

// GetCompletePhasesForOne: membership of rank r in the chain of token t ("Register" omitted: not a PHASES member).
pub open spec fn in_chain(t: int, r: int) -> bool {
    let build = r == 1 || r == 2 || r == 3 || r == 4 || r == 6 || r == 7;
    if t == 0 { r == 0 }
    else if t == 1 { r == 1 }
    else if t == 2 { r == 1 || r == 2 || r == 3 }
    else if t == 4 { r == 1 || r == 2 || r == 3 || r == 4 }
    else if t == 5 { r == 5 }
    else if t == 6 { r == 1 || r == 2 || r == 3 || r == 4 || r == 6 }
    else if t == 7 { build && r != 6 }
    else if t == 8 { r == 8 }
    else if t == 9 { r == 9 }
    else if t == 10 { build || r == 10 }
    else if t == 11 { build || r == 11 }
    else if t == 12 { build || r == 11 || r == 12 }
    else if t == 13 { build || r == 13 }
    else if t == 14 { r == 14 }
    else if t == 15 { r == 15 }
    else if t == 16 { build || r == 16 }
    else if t == 17 { r == 17 }
    else { false }
}

pub open spec fn any_chain(tokens: Seq<int>, r: int) -> bool
    decreases tokens.len()
{
    if tokens.len() == 0 { false } else { in_chain(tokens.last(), r) || any_chain(tokens.drop_last(), r) }
}

// Ranks from r up to 17 that some requested chain contains, in PHASES order.
pub open spec fn ordered_from(tokens: Seq<int>, r: int) -> Seq<int>
    decreases 18 - r
{
    if r > 17 || r < 0 { seq![] }
    else if any_chain(tokens, r) { seq![r] + ordered_from(tokens, r + 1) }
    else { ordered_from(tokens, r + 1) }
}

// A single named phase as ExecutePhases will run it after custom lookup.
pub open spec fn single_step(t: int) -> int {
    if standard(t) { t } else if t == CUSTOM() { CUSTOM() } else { INVALID() }
}

// GetCompletePhases: one non-standard name is returned as-is; otherwise union of chains in fixed order.
pub open spec fn complete_plan(tokens: Seq<int>) -> Seq<int> {
    if tokens.len() == 1 && !standard(tokens[0]) { seq![single_step(tokens[0])] }
    else { ordered_from(tokens, 0) }
}

// -only: MatchSinglePhase($listtostring(list)); two or more words become one comma-joined name.
pub open spec fn only_plan(tokens: Seq<int>) -> Seq<int> {
    if tokens.len() == 1 { seq![single_step(tokens[0])] } else { seq![INVALID()] }
}

pub open spec fn plan(tokens: Seq<int>, only: bool) -> Seq<int> {
    if only { only_plan(tokens) } else { complete_plan(tokens) }
}

pub struct World { pub disk: int, pub source: int, pub compiled: int, pub compile_hooks: int, pub activate_hooks: int, pub custom_hooks: int }

pub struct Outcome { pub world: World, pub starts: Seq<int>, pub ok: bool }

// One step of ExecutePhases. "*" refreshes the module object without a START line.
// Activate's lifecycle body runs a nested complete Configure phase (its own START line).
pub open spec fn step(o: Outcome, s: int) -> Outcome {
    let w = o.world;
    if !o.ok { o }
    else if s == STAR() { o }
    else if s == INVALID() { Outcome { world: w, starts: o.starts.push(s), ok: false } }
    else if s == CUSTOM() { Outcome { world: World { custom_hooks: w.custom_hooks + 1, ..w }, starts: o.starts.push(s), ok: true } }
    else if s == RELOAD() { Outcome { world: World { source: w.disk, ..w }, starts: o.starts.push(s), ok: true } }
    else if s == COMPILE() { Outcome { world: World { compiled: w.source, compile_hooks: w.compile_hooks + 1, ..w }, starts: o.starts.push(s), ok: true } }
    else if s == ACTIVATE() { Outcome { world: World { activate_hooks: w.activate_hooks + 1, ..w }, starts: o.starts.push(s).push(CONFIGURE()), ok: true } }
    else { Outcome { world: w, starts: o.starts.push(s), ok: true } }
}

pub open spec fn run_steps(o: Outcome, steps: Seq<int>) -> Outcome
    decreases steps.len()
{
    if steps.len() == 0 { o } else { step(run_steps(o, steps.drop_last()), steps.last()) }
}

pub open spec fn execute(w: World, tokens: Seq<int>, only: bool, disk: int) -> Outcome {
    run_steps(Outcome { world: World { disk: disk, ..w }, starts: seq![], ok: true }, plan(tokens, only))
}

// Position (1-based) of a START label, 0 when absent.
pub open spec fn position(starts: Seq<int>, label: int) -> int
    decreases starts.len()
{
    if starts.len() == 0 { 0 }
    else if position(starts.drop_last(), label) != 0 { position(starts.drop_last(), label) }
    else if starts.last() == label { starts.len() as int }
    else { 0 }
}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

// Native case sequence (developer-mode load, disk value written before each command).
pub open spec fn start_world() -> World { World { disk: 0, source: 0, compiled: 0, compile_hooks: 0, activate_hooks: 0, custom_hooks: 0 } }
pub open spec fn cmd_tokens(i: int) -> Seq<int> {
    if i == 0 { seq![7] }                 // load = complete Activate chain
    else if i == 1 { seq![6] }            // c1  phasecase compile -only
    else if i == 2 { seq![6] }            // c2  phasecase compile
    else if i == 3 { seq![6, 7] }         // c3  phasecase compile activate -only
    else if i == 4 { seq![2, CUSTOM()] }  // c4  phasecase reload mycustom
    else if i == 5 { seq![CUSTOM()] }     // c5  phasecase MYCUSTOM
    else if i == 6 { seq![6, UNKNOWN()] } // c6  phasecase compile bogus
    else if i == 7 { seq![UNKNOWN()] }    // c7  phasecase bogus
    else if i == 8 { seq![7, 1] }         // c8  phasecase activate initialize
    else if i == 9 { seq![6] }            // c9  RunOnePhase: compile phasecase -only
    else if i == 10 { seq![CUSTOM(), 6] } // c10 mycustom compile -only
    else { seq![7] }                      // c11 phasecase activate -only
}
pub open spec fn cmd_only(i: int) -> bool { i == 1 || i == 3 || i == 9 || i == 10 || i == 11 }
pub open spec fn cmd_disk(i: int) -> int { if i == 0 { 1 } else { 10 + i } }

pub open spec fn world_before(i: int) -> World
    decreases i, 0int
{
    if i <= 0 { start_world() } else { case(i - 1).world }
}
pub open spec fn case(i: int) -> Outcome
    decreases i, 1int
{
    if i < 0 { Outcome { world: start_world(), starts: seq![], ok: true } }
    else { execute(world_before(i), cmd_tokens(i), cmd_only(i), cmd_disk(i)) }
}
pub open spec fn delta_compile(i: int) -> int { case(i).world.compile_hooks - world_before(i).compile_hooks }
pub open spec fn delta_activate(i: int) -> int { case(i).world.activate_hooks - world_before(i).activate_hooks }
pub open spec fn delta_custom(i: int) -> int { case(i).world.custom_hooks - world_before(i).custom_hooks }

// The neighbours this iteration distinguishes.

// -only on one standard phase runs just that phase; the complete form adds its chain and reloads disk.
pub proof fn only_versus_chain()
    ensures
        plan(seq![6], true) == seq![6],
        plan(seq![6], false) == seq![1, 2, 3, 4, 6],
        execute(World { disk: 0, source: 5, compiled: 5, compile_hooks: 0, activate_hooks: 0, custom_hooks: 0 }, seq![6], true, 9).world.compiled == 5,
        execute(World { disk: 0, source: 5, compiled: 5, compile_hooks: 0, activate_hooks: 0, custom_hooks: 0 }, seq![6], false, 9).world.compiled == 9,
{
    reveal_with_fuel(ordered_from, 19);
    reveal_with_fuel(any_chain, 2);
    reveal_with_fuel(run_steps, 6);
    assert(plan(seq![6], false) =~= seq![1, 2, 3, 4, 6]) by (compute);
}

// As built, -only with two or more names fails before any phase work; neither named phase runs.
pub proof fn only_multiple_names_fail(w: World, a: int, c: int, disk: int)
    requires standard(a), standard(c)
    ensures
        plan(seq![a, c], true) == seq![INVALID()],
        !execute(w, seq![a, c], true, disk).ok,
        execute(w, seq![a, c], true, disk).world == (World { disk: disk, ..w }),
{
    reveal_with_fuel(run_steps, 2);
}

// As built, a custom phase named beside a standard phase contributes no chain and is dropped;
// alone it runs. An unknown name beside a standard phase is also dropped, and alone it fails.
pub proof fn custom_and_unknown_neighbors(w: World, disk: int)
    ensures
        execute(w, seq![CUSTOM()], false, disk).world.custom_hooks == w.custom_hooks + 1,
        execute(w, seq![CUSTOM()], false, disk).ok,
        execute(w, seq![2, CUSTOM()], false, disk).world.custom_hooks == w.custom_hooks,
        execute(w, seq![2, CUSTOM()], false, disk).ok,
        plan(seq![6, UNKNOWN()], false) == plan(seq![6], false),
        !execute(w, seq![UNKNOWN()], false, disk).ok,
{
    reveal_with_fuel(ordered_from, 19);
    reveal_with_fuel(any_chain, 3);
    reveal_with_fuel(run_steps, 4);
    assert(plan(seq![2, CUSTOM()], false) =~= seq![1, 2, 3]) by (compute);
    assert(plan(seq![6, UNKNOWN()], false) =~= plan(seq![6], false)) by (compute);
}

// Complete mode reorders requested names into the fixed phase order and runs each phase once.
pub proof fn fixed_order()
    ensures plan(seq![7, 1], false) == seq![1, 2, 3, 4, 6, 7],
{
    reveal_with_fuel(ordered_from, 19);
    reveal_with_fuel(any_chain, 3);
    assert(plan(seq![7, 1], false) =~= seq![1, 2, 3, 4, 6, 7]) by (compute);
}

// Activate alone runs its After-Activate hook and a nested Configure phase, without Compile.
pub proof fn activate_nests_configure(w: World, disk: int)
    ensures
        execute(w, seq![7], true, disk).starts == seq![7int, 14int],
        execute(w, seq![7], true, disk).world.activate_hooks == w.activate_hooks + 1,
        execute(w, seq![7], true, disk).world.compile_hooks == w.compile_hooks,
{
    reveal_with_fuel(run_steps, 2);
    assert(execute(w, seq![7], true, disk).starts =~= seq![7int, 14int]);
}

// Replay support: the whole native case sequence evaluates to concrete outcomes.
pub proof fn native_sequence()
    ensures
        case(11).world.compiled == 18,
        case(4).world.custom_hooks == 0,
        case(5).world.custom_hooks == 1,
{
    assert(case(11).world.compiled == 18) by (compute);
    assert(case(4).world.custom_hooks == 0) by (compute);
    assert(case(5).world.custom_hooks == 1) by (compute);
}

} // verus!

fn main() {}
