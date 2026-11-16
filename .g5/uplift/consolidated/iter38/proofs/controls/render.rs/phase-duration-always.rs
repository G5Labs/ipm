#![allow(non_snake_case)]
// Iteration38: `history find` and `history details -phases` rendering of stored history rows.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter38/bindings.json):
//   General/History.cls::ShowColumns            per-column <Column>ToString dispatch, summary vs detail
//   General/History.cls::VersionToString        stored version text "0.0.0-0" -> "<unset>", otherwise "v"_version
//   General/History.cls::TimeToString           summary: start plus "(N sec)" only when an end time exists
//   General/History.cls::SourceToString         "n/a" unless Action is install; "<no source set>" for an empty install source
//   General/History.cls::SuccessToString        summary decodes the status ("Success" or error text); detail is raw
//   General/History.cls::CommittedToString      "Committed" / "Uncommitted"
//   General/History.cls::ShowPhases             stored phase order; start plus duration only when ended; decoded status
//   General/History.cls::DisplayQueryResult, DisplayOneRecord   find rows and details record
//
// The stored row is the input. Premises (native-qualified): $system.Status decoding of a stored
// status, SQL row retrieval by ID, DATEDIFF of the stored times. An empty phase status is outside
// the modelled domain (never observed here).
use vstd::prelude::*;

verus! {

pub struct Phase { pub name: int, pub ended: bool, pub ok: bool }
// action: 1 load, 2 install, 3 uninstall.
pub struct Rec { pub action: int, pub package: bool, pub vunset: bool, pub ok: bool, pub committed: bool, pub ended: bool, pub source: bool, pub phases: Seq<Phase> }

pub struct Summary { pub shows_unset: bool, pub shows_version: bool, pub success: bool, pub committed: bool, pub uncommitted: bool, pub na: bool, pub nosource: bool, pub duration: bool }

pub open spec fn summary(r: Rec) -> Summary {
    Summary {
        shows_unset: r.vunset,
        shows_version: !r.vunset,
        success: r.ok,
        committed: r.committed,
        uncommitted: !r.committed,
        na: r.action != 2,
        nosource: r.action == 2 && !r.source,
        duration: r.ended,
    }
}

pub struct DetailPhase { pub name: int, pub success: bool, pub duration: bool }
pub open spec fn detail_phase(p: Phase) -> DetailPhase { DetailPhase { name: p.name, success: p.ok, duration: true } }
pub open spec fn detail_phases(r: Rec) -> Seq<DetailPhase> { Seq::new(r.phases.len(), |i: int| detail_phase(r.phases[i])) }
pub open spec fn detail_source_na(r: Rec) -> bool { r.action != 2 }
pub open spec fn detail_unset(r: Rec) -> bool { r.vunset }
pub open spec fn detail_committed(r: Rec) -> bool { r.committed }

// Generic rendering facts over any stored row.
pub proof fn placeholder_hides_version(r: Rec)
    requires r.vunset
    ensures summary(r).shows_unset, !summary(r).shows_version, detail_unset(r)
{}
pub proof fn source_only_for_install(r: Rec)
    ensures summary(r).na == (r.action != 2), detail_source_na(r) == (r.action != 2)
{}
pub proof fn phase_rendering_keeps_order_and_status(r: Rec, i: int)
    requires 0 <= i < r.phases.len()
    ensures detail_phases(r).len() == r.phases.len(), detail_phases(r)[i].name == r.phases[i].name,
        detail_phases(r)[i].success == r.phases[i].ok, detail_phases(r)[i].duration == r.phases[i].ended
{}
// A stored version that is the placeholder renders the same whether or not the module is known.
pub proof fn known_placeholder_indistinguishable(r: Rec, s: Rec)
    requires r.vunset, s.vunset, r.package, !s.package
    ensures summary(r).shows_unset == summary(s).shows_unset, summary(r).shows_version == summary(s).shows_version
{}

// Fixture rows as stored natively (iteration38 st.* observations).
pub open spec fn ph(n: int) -> Phase { Phase { name: n, ended: true, ok: true } }
pub open spec fn load_phases() -> Seq<Phase> { seq![ph(1), ph(2), ph(4), ph(6), ph(7)] }
pub open spec fn rec_a() -> Rec { Rec { action: 1, package: true, vunset: false, ok: true, committed: true, ended: true, source: false, phases: load_phases() } }
pub open spec fn rec_b() -> Rec { Rec { action: 1, package: false, vunset: true, ok: false, committed: false, ended: true, source: false, phases: seq![] } }
pub open spec fn rec_e() -> Rec { Rec { action: 1, package: true, vunset: false, ok: false, committed: false, ended: true, source: false,
    phases: seq![ph(1), ph(2), ph(4), ph(6), Phase { name: 7, ended: false, ok: false }] } }
pub open spec fn rec_z() -> Rec { Rec { action: 1, package: true, vunset: true, ok: true, committed: true, ended: true, source: false, phases: load_phases() } }
pub open spec fn rec_i() -> Rec { Rec { action: 2, package: true, vunset: false, ok: true, committed: true, ended: true, source: true, phases: load_phases() } }
pub open spec fn rec_u() -> Rec { Rec { action: 3, package: true, vunset: false, ok: true, committed: true, ended: true, source: false, phases: seq![ph(0)] } }

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

pub proof fn fixture_rendering()
    ensures
        summary(rec_z()).shows_unset, !summary(rec_z()).shows_version,
        !summary(rec_i()).na, summary(rec_u()).na,
        detail_phases(rec_e())[0].success, !detail_phases(rec_e())[4].success, !detail_phases(rec_e())[4].duration,
        detail_phases(rec_b()).len() == 0,
{
    assert(detail_phases(rec_e())[0].success) by (compute);
    assert(!detail_phases(rec_e())[4].success) by (compute);
    assert(!detail_phases(rec_e())[4].duration) by (compute);
    assert(detail_phases(rec_b()).len() == 0) by (compute);
}

} // verus!

fn main() {}
