#![allow(non_snake_case)]
// Iteration39: editing an installed module's manifest through the <name>.ZPM document.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter39/bindings.json):
//   StudioDocument/Module.cls::Save, ImportStream      clear and write the stored stream, then (inside a transaction)
//                                                      save it and update the module record; roll back on error
//   StudioDocument/Module.cls::UpdatePersistentFromStream   empty stream -> version 0.0.1+snapshot on the existing record;
//                                                      otherwise parse onto the existing record and compare names
//   StudioDocument/Module.cls::Delete                  delete the stream and the module record
//
// Text kinds abstract the saved content. Names and versions are small codes (name 1 = m39, 2 = m39other;
// version codes follow the native reader). Premises (native-qualified): the stream's Clear takes
// effect outside the transaction and the written content is rolled back with it; importing onto an
// existing record does not change its Name; XML parsing and XSLT are providers.
use vstd::prelude::*;

verus! {

pub enum Text { Empty, Malformed, Valid { name: int, version: int } }
pub struct Record { pub name: int, pub version: int, pub resources: int }
pub struct State { pub record: Option<Record>, pub doc: Option<Text>, pub classes: bool }

pub open spec fn SNAP() -> int { 4 }   // 0.0.1+snapshot

// UpdatePersistentFromStream on an existing record: Some(new record) on success, None on error.
pub open spec fn update(r: Record, t: Text) -> Option<Record> {
    match t {
        Text::Empty => Some(Record { version: SNAP(), ..r }),
        Text::Malformed => None,
        Text::Valid { name, version } => {
            // The name is imported onto the existing object without replacing it, so the comparison
            // with the stream name always passes.
            let imported_name = r.name;
            if imported_name != r.name { None } else if version > r.version { Some(Record { version: version, ..r }) } else { None }
        }
    }
}

pub struct Saved { pub state: State, pub ok: bool }

pub open spec fn save(s: State, t: Text) -> Saved {
    match s.record {
        None => Saved { state: s, ok: false },
        Some(r) => match update(r, t) {
            Some(r2) => Saved { state: State { record: Some(r2), doc: Some(t), ..s }, ok: true },
            // The rollback restores the record and discards the written text, but the earlier Clear stays.
            None => Saved { state: State { doc: Some(Text::Empty), ..s }, ok: false },
        },
    }
}

pub open spec fn delete(s: State) -> State { State { record: None, doc: None, ..s } }

// Generic neighbours.
pub proof fn valid_edit_updates_record(s: State, r: Record, v: int)
    requires s.record == Some(r)
    ensures save(s, Text::Valid { name: r.name, version: v }).ok,
            save(s, Text::Valid { name: r.name, version: v }).state.record == Some(Record { version: v, ..r })
{}
pub proof fn refused_save_erases_text(s: State, r: Record)
    requires s.record == Some(r)
    ensures !save(s, Text::Malformed).ok,
            save(s, Text::Malformed).state.record == Some(r),
            save(s, Text::Malformed).state.doc == Some(Text::Empty)
{}
pub proof fn other_name_accepted_under_old_name(s: State, r: Record, other: int, v: int)
    requires s.record == Some(r), other != r.name
    ensures save(s, Text::Valid { name: other, version: v }).ok,
            save(s, Text::Valid { name: other, version: v }).state.record == Some(Record { version: v, ..r }),
            save(s, Text::Valid { name: other, version: v }).state.doc == Some(Text::Valid { name: other, version: v })
{}
pub proof fn empty_resets_version_keeps_resources(s: State, r: Record)
    requires s.record == Some(r)
    ensures save(s, Text::Empty).state.record == Some(Record { version: SNAP(), ..r })
{}
pub proof fn delete_keeps_classes(s: State)
    ensures delete(s).record.is_none(), delete(s).classes == s.classes
{}

// Native schedule: setup m39 1.0.0 with one resource, then e1..e6.
pub open spec fn s0() -> State { State { record: Some(Record { name: 1, version: 1, resources: 1 }), doc: Some(Text::Valid { name: 1, version: 1 }), classes: true } }
pub open spec fn e1() -> Saved { save(s0(), Text::Valid { name: 1, version: 2 }) }
pub open spec fn e2() -> Saved { save(e1().state, Text::Malformed) }
pub open spec fn e3() -> Saved { save(e2().state, Text::Valid { name: 2, version: 5 }) }
pub open spec fn e4() -> Saved { save(e3().state, Text::Valid { name: 1, version: 3 }) }
pub open spec fn e5() -> Saved { save(e4().state, Text::Empty) }
pub open spec fn e6() -> State { delete(e5().state) }

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }
pub open spec fn has_record(s: State) -> int { b(s.record.is_some()) }
pub open spec fn version(s: State) -> int { match s.record { Some(r) => r.version, None => 0 } }
pub open spec fn resources(s: State) -> int { match s.record { Some(r) => r.resources, None => -1 } }
pub open spec fn has_doc(s: State) -> int { b(s.doc.is_some()) }
pub open spec fn doc_names_other(s: State) -> int { match s.doc { Some(Text::Valid { name, version }) => b(name == 2), _ => 0 } }
pub open spec fn doc_version_is(s: State, v: int) -> int { match s.doc { Some(Text::Valid { name, version }) => b(version == v), _ => 0 } }
pub open spec fn doc_empty(s: State) -> int { match s.doc { Some(Text::Empty) => 1, Some(_) => 0, None => 1 } }

pub proof fn schedule()
    ensures
        version(e1().state) == 2, !e2().ok, doc_empty(e2().state) == 1, version(e2().state) == 2,
        e3().ok, version(e3().state) == 5, doc_names_other(e3().state) == 1,
        version(e4().state) == 3, version(e5().state) == 4, resources(e5().state) == 1,
        has_record(e6()) == 0, e6().classes,
{}

} // verus!

fn main() {}
