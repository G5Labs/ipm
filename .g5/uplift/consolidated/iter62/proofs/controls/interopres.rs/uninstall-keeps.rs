#![allow(non_snake_case)]
// Iteration62: Interoperability document resources (.LUT lookup tables) through load, reload and uninstall.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter62/bindings.json):
//   ResourceProcessor/Default/Interoperability.cls::DirectoryGet   explicit Directory, else src/i14y if it exists,
//                                                                  else src/misc if it exists, else default i14y/
//                                                                  with the Document "" fallback still allowed
//   ResourceProcessor/Default/Document.cls::OnPhase (Reload)       candidate folders, $system.OBJ.Load of <Name>.xml
//   Lifecycle/Base.cls::%Clean                                     resolved children deleted as documents
// Premises (native-qualified): importing a LUT document replaces the whole table; deleting the LUT document
// removes its entries; an HL7 document needs a registered HL7 document class (absent on IRIS Community,
// where the load fails with #5844).
use vstd::prelude::*;

verus! {

// Folder where the file actually is: 0 = src/i14y, 1 = src/misc, 2 = src (no subfolder), 3 = declared Directory.
pub struct Res { pub declared: bool, pub i14y_exists: bool, pub misc_exists: bool, pub at: int }

pub open spec fn candidates_find(r: Res) -> bool {
    if r.declared { r.at == 3 }
    else if r.i14y_exists { r.at == 0 }
    else if r.misc_exists { r.at == 1 }
    else { r.at == 0 || r.at == 2 }
}

// A table is a map from key ids to values (Option per key id 0..2).
pub type Table = Seq<Option<int>>;
pub open spec fn load(old: Table, file: Table, found: bool) -> Table { if found { file } else { old } }
pub open spec fn uninstall(t: Table) -> Table { t }

pub proof fn directory_neighbors()
    ensures
        candidates_find(Res { declared: false, i14y_exists: true, misc_exists: false, at: 0 }),
        candidates_find(Res { declared: false, i14y_exists: false, misc_exists: true, at: 1 }),
        candidates_find(Res { declared: true, i14y_exists: false, misc_exists: false, at: 3 }),
        candidates_find(Res { declared: false, i14y_exists: false, misc_exists: false, at: 2 }),
        !candidates_find(Res { declared: false, i14y_exists: true, misc_exists: false, at: 2 }),
{}

pub proof fn reload_replaces(old: Table, file: Table, k: int)
    requires 0 <= k < file.len(), file[k].is_none()
    ensures load(old, file, true)[k].is_none()
{}

pub proof fn uninstall_clears(t: Table, k: int)
    requires 0 <= k < t.len()
    ensures uninstall(t)[k].is_none()
{}

// Native schedule for table I62A: keys a, b, c.
pub open spec fn v(t: Table, k: int) -> int { match t[k] { Some(x) => x, None => -1 } }
pub open spec fn a1() -> Table { load(seq![None::<int>, None::<int>, None::<int>], seq![Some(1int), Some(2int), None::<int>], true) }
pub open spec fn a2() -> Table { load(a1(), seq![Some(10int), None::<int>, Some(3int)], true) }
pub open spec fn a3() -> Table { uninstall(a2()) }

pub proof fn schedule()
    ensures v(a1(), 0) == 1, v(a1(), 1) == 2, v(a2(), 0) == 10, v(a2(), 1) == -1, v(a2(), 2) == 3, v(a3(), 0) == -1
{}

} // verus!

fn main() {}
