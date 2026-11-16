#![allow(non_snake_case)]
// Iteration41: `config set/get/delete/list` over IPM settings and their readers.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter41/bindings.json):
//   Main.cls::Config                                   dispatch; ignores every returned status
//   Repo/UniversalSettings.cls::UpdateOne, SetValue    unknown key -> "not found", OK; TestReportFormat validated;
//                                                      first write of an index also becomes its default
//   Repo/UniversalSettings.cls::ResetToDefault,
//     GetDefaultValue                                  delete writes the stored default (TestReportFormat -> "")
//   Repo/UniversalSettings.cls::GetAll, PrintList, PrintOne, GetValue   list/get readers
//   Repo/UniversalSettings.cls::GetAnalyticsAvailable  consumer test '(value): text counts as disabled
//
// A slot abstracts one ^IPM.settings index in %SYS. Values are the native reader's integer codes:
// -1 empty, small numbers themselves, 1001 json, 1002 xml, 1003 yes, 2000 other text. Premises
// (native-qualified): numeric coercion of text is 0, and the installer's seeded slots are as observed.
use vstd::prelude::*;

verus! {

pub struct Slot { pub value: int, pub has_default: bool, pub default: int }

// Keys in CONFIGURABLE order; 12 is a key outside the list.
pub open spec fn TRF() -> int { 10 }
pub open spec fn configurable(k: int) -> bool { 0 <= k < 12 }
pub open spec fn valid_format(v: int) -> bool { v == -1 || v == 1001 }  // empty or json in the fixture

pub open spec fn set_value(s: Slot, v: int) -> Slot {
    Slot { value: v, has_default: true, default: if s.has_default { s.default } else { v } }
}

pub struct Out { pub slots: Seq<Slot>, pub ok: bool, pub not_found: bool, pub updated: bool, pub reset: bool, pub error: bool }
pub open spec fn out(slots: Seq<Slot>) -> Out { Out { slots, ok: true, not_found: false, updated: false, reset: false, error: false } }

// config set <key> <value>
pub open spec fn set(slots: Seq<Slot>, k: int, v: int) -> Out {
    if !configurable(k) { Out { not_found: true, ok: false, ..out(slots) } }
    else if k == TRF() && !valid_format(v) { Out { error: true, ..out(slots) } }
    else { Out { updated: true, ..out(slots.update(k, set_value(slots[k], v))) } }
}

// config delete <key>
pub open spec fn delete(slots: Seq<Slot>, k: int) -> Out {
    if !configurable(k) { Out { not_found: true, ..out(slots) } }
    else {
        let d = if k == TRF() { -1 } else if slots[k].has_default { slots[k].default } else { -1 };
        Out { reset: true, ..out(slots.update(k, set_value(slots[k], d))) }
    }
}

pub open spec fn analytics_disabled(v: int) -> bool { v <= 0 || v >= 1000 }

// Generic neighbours.
pub proof fn delete_restores_first_write(slots: Seq<Slot>, k: int, v: int)
    requires 0 <= k < slots.len(), configurable(k), k != TRF(), !slots[k].has_default
    ensures delete(set(slots, k, v).slots, k).slots[k].value == v
{}
pub proof fn delete_restores_seed(slots: Seq<Slot>, k: int, v: int)
    requires 0 <= k < slots.len(), configurable(k), k != TRF(), slots[k].has_default
    ensures delete(set(slots, k, v).slots, k).slots[k].value == slots[k].default
{}
pub proof fn unknown_key_reports_ok(slots: Seq<Slot>, v: int)
    ensures set(slots, 12, v).ok, set(slots, 12, v).not_found, set(slots, 12, v).slots == slots,
            delete(slots, 12).ok, delete(slots, 12).slots == slots
{}
pub proof fn invalid_format_refused_with_ok(slots: Seq<Slot>)
    requires TRF() < slots.len()
    ensures set(slots, TRF(), 1002).ok, set(slots, TRF(), 1002).error, set(slots, TRF(), 1002).slots == slots
{}
pub proof fn text_analytics_counts_as_disabled()
    ensures analytics_disabled(1003), analytics_disabled(0), !analytics_disabled(1)
{}

// Native schedule from the observed fresh-install slots (ModuleRoot index 11 not observed).
pub open spec fn sl(v: int, hd: bool, d: int) -> Slot { Slot { value: v, has_default: hd, default: d } }
pub open spec fn c0() -> Seq<Slot> {
    seq![sl(2000, true, 2000), sl(1, true, 1), sl(-1, true, -1), sl(-1, false, -1), sl(-1, false, -1), sl(-1, true, -1),
         sl(-1, true, -1), sl(0, true, 0), sl(20, true, 20), sl(-1, false, -1), sl(-1, false, -1), sl(-1, false, -1)]
}
pub open spec fn c1() -> Out { set(c0(), 9, 5) }
pub open spec fn c2() -> Out { delete(c1().slots, 9) }
pub open spec fn c3() -> Out { set(c2().slots, 4, 99) }
pub open spec fn c3d() -> Out { delete(c3().slots, 4) }
pub open spec fn c4() -> Out { set(c3d().slots, TRF(), 1002) }
pub open spec fn c4n() -> Out { set(c4().slots, TRF(), 1001) }
pub open spec fn c5() -> Out { set(c4n().slots, 12, 1) }
pub open spec fn c6() -> Out { delete(c5().slots, 12) }
pub open spec fn c7() -> Out { set(c6().slots, 1, 0) }
pub open spec fn c8() -> Out { set(c7().slots, 1, 1003) }

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

pub proof fn schedule()
    ensures
        c2().slots[9].value == 5, c2().reset, c3d().slots[4].value == 99,
        c4().slots[10].value == -1, c4().ok, c4().error, c4n().slots[10].value == 1001,
        c5().ok, c5().not_found, c8().slots[1].value == 1003, analytics_disabled(c8().slots[1].value),
{}

} // verus!

fn main() {}
