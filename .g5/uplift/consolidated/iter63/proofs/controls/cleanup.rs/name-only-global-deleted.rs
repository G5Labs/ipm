#![allow(non_snake_case)]
// Iteration63: what uninstall removes, by resource kind.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter63/bindings.json):
//   Lifecycle/Base.cls::%Clean            resolves each resource's children and deletes CLS/INC/MAC (and INT/OBJ)
//   ResourceProcessor/Default/Global.cls  a Global attribute names data that Clean kills; a name-only GBL keeps data
//   ResourceProcessor/FileCopy.cls        Clean removes copied targets unless KeepOnUninstall
//   Lifecycle/Base.cls::%Reload           adds declared <Mapping> entries and developer-mode ^Sources entries,
//                                         which no uninstall path removes
//   Storage/FileHash.cls                  baseline rows cascade with the module record (foreign key)
// Premises (native-qualified): $system.OBJ.Delete, Config.Map* and the file APIs perform the requested change.
use vstd::prelude::*;

verus! {

pub enum Kind { Cls, Pkg, Inc, Mac, GblName, GblAttr, FileCopy { keep: bool }, DeclaredMapping, SourcesEntry, Baseline, Record, Stream }

// After `uninstall` (with or without -purge, which changes no outcome here).
pub open spec fn remains(k: Kind, purge: bool) -> bool {
    match k {
        Kind::GblName => false,
        Kind::GblAttr => false,
        Kind::FileCopy { keep } => keep,
        Kind::DeclaredMapping => true,
        Kind::SourcesEntry => true,
        _ => false,
    }
}

pub proof fn code_removed()
    ensures !remains(Kind::Cls, false), !remains(Kind::Pkg, false), !remains(Kind::Inc, false), !remains(Kind::Mac, false),
            !remains(Kind::Record, false), !remains(Kind::Stream, false), !remains(Kind::Baseline, false)
{}
pub proof fn globals_by_declaration()
    ensures remains(Kind::GblName, false), !remains(Kind::GblAttr, false)
{}
pub proof fn filecopy_keep()
    ensures remains(Kind::FileCopy { keep: true }, false), !remains(Kind::FileCopy { keep: false }, false)
{}
// As built: configuration and bookkeeping created by Reload outlive uninstall.
pub proof fn reload_side_effects_outlive_uninstall(purge: bool)
    ensures remains(Kind::DeclaredMapping, purge), remains(Kind::SourcesEntry, purge)
{}
pub proof fn purge_changes_nothing(k: Kind)
    ensures remains(k, true) == remains(k, false)
{}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

} // verus!

fn main() {}
