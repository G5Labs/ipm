#![allow(non_snake_case)]
// Iteration48: CPF resources merge at their declared phase or custom phase.
//
// Source roles at pin 571e04a1: ResourceProcessor/CPF.cls::OnBeforePhase, OnAfterPhase, OnCustomPhase
// (timing), DoMerge/MergeCPF (external `iris merge`, nonzero exit raises an error with output details),
// OnPhase (Clean handled), OnResolveChildren (relative path). Phase lists come from iteration30.
// Premise (native-qualified): `iris merge` applies a valid [Map.<ns>] section and exits nonzero on an
// invalid section name.
use vstd::prelude::*;

verus! {

pub enum When { Before, After }
pub struct Cpf { pub phase: int, pub when: When, pub custom: int, pub valid: bool }   // custom 0 = none

// Does this CPF merge when phase p runs (custom phases are positive ids)?
pub open spec fn merges_in(c: Cpf, p: int, before: bool) -> bool {
    if c.custom != 0 { p == c.custom && before }
    else { c.phase == p && (match c.when { When::Before => before, When::After => !before }) }
}

pub open spec fn INITIALIZE() -> int { -1 }
pub open spec fn COMPILE() -> int { -6 }

// A command's executed phases: load runs Initialize..Activate; compile -only runs Compile; custom runs itself.
pub open spec fn merged_by(c: Cpf, phases: Seq<int>) -> bool
    decreases phases.len()
{
    if phases.len() == 0 { false } else { merges_in(c, phases.last(), true) || merges_in(c, phases.last(), false) || merged_by(c, phases.drop_last()) }
}
pub open spec fn ok(c: Cpf, phases: Seq<int>) -> bool { !merged_by(c, phases) || c.valid }

pub open spec fn A() -> Cpf { Cpf { phase: INITIALIZE(), when: When::Before, custom: 0, valid: true } }
pub open spec fn B() -> Cpf { Cpf { phase: INITIALIZE(), when: When::Before, custom: 48, valid: true } }
pub open spec fn BAD() -> Cpf { Cpf { phase: INITIALIZE(), when: When::Before, custom: 49, valid: false } }
pub open spec fn load() -> Seq<int> { seq![INITIALIZE(), -2, -4, COMPILE(), -7] }

// Timing: a When="After" CPF merges after its phase's work, never before it (product test: MakeDeployed After).
pub proof fn after_is_not_before(p: int)
    ensures !merges_in(Cpf { phase: p, when: When::After, custom: 0, valid: true }, p, true),
            merges_in(Cpf { phase: p, when: When::After, custom: 0, valid: true }, p, false)
{}

pub proof fn neighbors()
    ensures merged_by(A(), load()), !merged_by(B(), load()), !merged_by(A(), seq![COMPILE()]),
            merged_by(B(), seq![48int]), !ok(BAD(), seq![49int]), ok(BAD(), load())
{
    reveal_with_fuel(merged_by, 6);
}

} // verus!

fn main() {}
