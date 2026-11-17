#![allow(non_snake_case)]
// Iteration70: `config set/get/delete/list` after #281 adds the NotaryExecutable setting.
// Revises iteration41's settings.rs (unchanged at ../iter41/settings.rs), whose key count and reset rule the PR changes.
//
// Source roles (see .g5/uplift/consolidated/iter70/bindings.json):
//   Repo/UniversalSettings.cls::CONFIGURABLE              13 keys; NotaryExecutable is the 13th (index 12)
//   Repo/UniversalSettings.cls::UpdateOne, SetValue       unknown key -> "not found", OK; TestReportFormat validated;
//                                                         first write of an index also becomes its default
//   Repo/UniversalSettings.cls::ResetToDefault            delete: TestReportFormat and NotaryExecutable -> ""
//                                                         (no factory value), others -> the stored default
//   Repo/Oras/ContentTrust.cls::NotaryExecutable          reader: "" means find notary on the PATH
//
// A slot abstracts one ^IPM.settings index in %SYS. -1 is empty, other ints are values.
use vstd::prelude::*;

verus! {

pub struct Slot { pub value: int, pub has_default: bool, pub default: int }

// Keys in CONFIGURABLE order; 13 is a key outside the list.
pub open spec fn TRF() -> int { 10 }
pub open spec fn NOTARY() -> int { 12 }
pub open spec fn configurable(k: int) -> bool { 0 <= k < 13 }
pub open spec fn valid_format(v: int) -> bool { v == -1 || v == 1001 }  // empty or json

// Keys whose delete restores the empty value rather than the stored default.
pub open spec fn factory_empty(k: int) -> bool { k == TRF() || k == NOTARY() }

pub open spec fn set_value(s: Slot, v: int) -> Slot {
    Slot { value: v, has_default: true, default: if s.has_default { s.default } else { v } }
}

pub struct Out { pub slots: Seq<Slot>, pub ok: bool, pub not_found: bool, pub updated: bool, pub reset: bool, pub error: bool }
pub open spec fn out(slots: Seq<Slot>) -> Out { Out { slots, ok: true, not_found: false, updated: false, reset: false, error: false } }

// config set <key> <value>
pub open spec fn set(slots: Seq<Slot>, k: int, v: int) -> Out {
    if !configurable(k) { Out { not_found: true, ..out(slots) } }
    else if k == TRF() && !valid_format(v) { Out { error: true, ..out(slots) } }
    else { Out { updated: true, ..out(slots.update(k, set_value(slots[k], v))) } }
}

// config delete <key>
pub open spec fn delete(slots: Seq<Slot>, k: int) -> Out {
    if !configurable(k) { Out { not_found: true, ..out(slots) } }
    else {
        let d = if factory_empty(k) { -1 } else if slots[k].has_default { slots[k].default } else { -1 };
        Out { reset: true, ..out(slots.update(k, set_value(slots[k], d))) }
    }
}

// ContentTrust.NotaryExecutable: the configured path, or "notary" found on the PATH when empty.
pub enum Verifier { OnPath, Configured { path: int } }
pub open spec fn verifier(slots: Seq<Slot>) -> Verifier {
    if slots[NOTARY()].value == -1 { Verifier::OnPath } else { Verifier::Configured { path: slots[NOTARY()].value } }
}

// Inherited neighbours, now excluding both factory-empty keys.
pub proof fn delete_restores_first_write(slots: Seq<Slot>, k: int, v: int)
    requires 0 <= k < slots.len(), configurable(k), !factory_empty(k), !slots[k].has_default
    ensures delete(set(slots, k, v).slots, k).slots[k].value == v
{}
pub proof fn delete_restores_seed(slots: Seq<Slot>, k: int, v: int)
    requires 0 <= k < slots.len(), configurable(k), !factory_empty(k), slots[k].has_default
    ensures delete(set(slots, k, v).slots, k).slots[k].value == slots[k].default
{}
pub proof fn unknown_key_reports_ok(slots: Seq<Slot>, v: int)
    ensures set(slots, 13, v).ok, set(slots, 13, v).not_found, set(slots, 13, v).slots == slots,
            delete(slots, 13).ok, delete(slots, 13).slots == slots
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
