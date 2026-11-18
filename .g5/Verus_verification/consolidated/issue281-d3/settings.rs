#![allow(non_snake_case)]
// #281 D3+: `config set/delete` with the NotaryExecutable key, and every key whose delete has its own reset rule.
// Revises the #281 iteration70 settings.rs, whose `delete_restores_first_write` covered ModuleRoot although
// ResetToDefault computes ModuleRoot's value.
//
// Source roles (bindings in .g5/uplift/consolidated/issue281-d3/bindings.json):
//   Repo/UniversalSettings.cls::CONFIGURABLE     13 keys; TestReportFormat index 10, ModuleRoot 11, NotaryExecutable 12
//   Main.cls::Config                             dispatch; ignores the returned status
//   Repo/UniversalSettings.cls::UpdateOne        unknown key -> "not found", OK; TestReportFormat validated; ModuleRoot
//                                                refuses empty and is normalized; others stored through SetValue
//   Repo/UniversalSettings.cls::SetValue         the first write of an index also becomes its stored default
//   Repo/UniversalSettings.cls::ResetToDefault   TestReportFormat and NotaryExecutable -> "", ModuleRoot -> the
//                                                computed <data directory>/ipm/, every other key -> its stored default
//   Repo/Oras/ContentTrust.cls::NotaryExecutable "" means find notary on the PATH
//
// A slot abstracts one ^IPM.settings index in %SYS. -1 is empty, other ints are values. ROOT() is the computed
// ModuleRoot of this instance, a fixed value the reset reads from the instance rather than from storage.
use vstd::prelude::*;

verus! {

pub struct Slot { pub value: int, pub has_default: bool, pub default: int }

pub open spec fn TRF() -> int { 10 }
pub open spec fn MODULE_ROOT() -> int { 11 }
pub open spec fn NOTARY() -> int { 12 }
pub open spec fn ROOT() -> int { 4242 }
pub open spec fn configurable(k: int) -> bool { 0 <= k < 13 }
pub open spec fn valid_format(v: int) -> bool { v == -1 || v == 1001 }  // empty or json

// The value ResetToDefault writes for a key with its own reset rule. Other keys write their stored default.
pub open spec fn explicit_reset(k: int) -> Option<int> {
    if k == TRF() || k == NOTARY() { Some(-1) } else if k == MODULE_ROOT() { Some(ROOT()) } else { None }
}

pub open spec fn set_value(s: Slot, v: int) -> Slot {
    Slot { value: v, has_default: true, default: if s.has_default { s.default } else { v } }
}

pub struct Out { pub slots: Seq<Slot>, pub ok: bool, pub not_found: bool, pub updated: bool, pub reset: bool, pub error: bool }
pub open spec fn out(slots: Seq<Slot>) -> Out { Out { slots, ok: true, not_found: false, updated: false, reset: false, error: false } }

// config set <key> <value>
pub open spec fn set(slots: Seq<Slot>, k: int, v: int) -> Out {
    if !configurable(k) { Out { not_found: true, ..out(slots) } }
    else if k == TRF() && !valid_format(v) { Out { error: true, ..out(slots) } }
    else if k == MODULE_ROOT() && v == -1 { Out { error: true, ..out(slots) } }
    else { Out { updated: true, ..out(slots.update(k, set_value(slots[k], v))) } }
}

// config delete <key>
pub open spec fn delete(slots: Seq<Slot>, k: int) -> Out {
    if !configurable(k) { Out { not_found: true, ..out(slots) } }
    else {
        let d = match explicit_reset(k) {
            Some(v) => v,
            None => if slots[k].has_default { slots[k].default } else { -1 },
        };
        Out { reset: true, ..out(slots.update(k, set_value(slots[k], d))) }
    }
}

// ContentTrust.NotaryExecutable: the configured path, or "notary" found on the PATH when empty.
pub enum Verifier { OnPath, Configured { path: int } }
pub open spec fn verifier(slots: Seq<Slot>) -> Verifier {
    if slots[NOTARY()].value == -1 { Verifier::OnPath } else { Verifier::Configured { path: slots[NOTARY()].value } }
}

// Keys without a reset rule of their own restore the first value ever written (the inherited defect P:F-REPO-4).
pub proof fn delete_restores_first_write(slots: Seq<Slot>, k: int, v: int)
    requires 0 <= k < slots.len(), configurable(k), explicit_reset(k) is None, !slots[k].has_default,
             k != TRF() || valid_format(v)
    ensures delete(set(slots, k, v).slots, k).slots[k].value == v
{}
pub proof fn delete_restores_seed(slots: Seq<Slot>, k: int, v: int)
    requires 0 <= k < slots.len(), configurable(k), explicit_reset(k) is None, slots[k].has_default
    ensures delete(set(slots, k, v).slots, k).slots[k].value == slots[k].default
{}
pub proof fn unknown_key_reports_ok(slots: Seq<Slot>, v: int)
    ensures set(slots, 13, v).ok, set(slots, 13, v).not_found, set(slots, 13, v).slots == slots,
            delete(slots, 13).ok, delete(slots, 13).slots == slots
{}

// ModuleRoot's delete computes the data directory's ipm/ folder, whatever was set first or last.
pub proof fn module_root_delete_computes(slots: Seq<Slot>, first: int, last: int)
    requires MODULE_ROOT() < slots.len(), first != -1, last != -1
    ensures
        set(set(slots, MODULE_ROOT(), first).slots, MODULE_ROOT(), last).slots[MODULE_ROOT()].value == last,
        delete(set(set(slots, MODULE_ROOT(), first).slots, MODULE_ROOT(), last).slots, MODULE_ROOT())
            .slots[MODULE_ROOT()].value == ROOT(),
{}

// An empty ModuleRoot is refused and changes nothing. Main.cls::Config ignores the returned status, as for every key.
pub proof fn module_root_refuses_empty(slots: Seq<Slot>)
    requires MODULE_ROOT() < slots.len()
    ensures set(slots, MODULE_ROOT(), -1).error, set(slots, MODULE_ROOT(), -1).slots == slots
{}

// TestReportFormat's delete empties it.
pub proof fn report_format_delete_empties(slots: Seq<Slot>, v: int)
    requires TRF() < slots.len(), valid_format(v)
    ensures delete(set(slots, TRF(), v).slots, TRF()).slots[TRF()].value == -1
{}

// NotaryExecutable is configurable, and `config delete` returns verification to the notary on the PATH,
// whatever path was set first or last.
pub proof fn notary_set_then_delete_finds_on_path(slots: Seq<Slot>, first: int, last: int)
    requires NOTARY() < slots.len(), first != -1, last != -1
    ensures
        set(slots, NOTARY(), first).updated,
        verifier(set(set(slots, NOTARY(), first).slots, NOTARY(), last).slots) == (Verifier::Configured { path: last }),
        verifier(delete(set(set(slots, NOTARY(), first).slots, NOTARY(), last).slots, NOTARY()).slots) == Verifier::OnPath,
{}

} // verus!

fn main() {}
