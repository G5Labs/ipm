#![allow(non_snake_case)]
// Iteration58: DeepSeeItem (.DFI) resources across load, reload and uninstall.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter58/bindings.json):
//   ResourceProcessor/Default/DeepSeeItem.cls::OnPhase        a name containing "*" is loaded by searching the dfi folder
//                                                             with wildcard patterns; on UNIX the uppercase patterns are
//                                                             appended without a ";" separator
//   ResourceProcessor/Default/DeepSeeItem.cls::%OnNew, OnResolveChildren, OnGetUniqueName
//   ResourceProcessor/Default/Document.cls (explicit names: load and Clean)
// File extensions are encoded 0 ".xml", 1 ".dfi", 2 ".XML", 3 ".DFI". Premise (native-qualified): IPM's
// FindFiles splits patterns on ";" and matches case-sensitively on Linux; folder-item import is a provider.
use vstd::prelude::*;

verus! {

// The four patterns as built on UNIX: "n.xml;n.dfi" + "n.XML;n.DFI" -> "n.xml", "n.dfin.XML", "n.DFI".
pub open spec fn matched_unix(ext: int) -> bool { ext == 0 || ext == 3 }
pub open spec fn matched_intended(ext: int) -> bool { 0 <= ext <= 3 }

pub struct Item { pub wildcard: bool, pub ext: int }
pub open spec fn loaded(i: Item) -> bool { !i.wildcard || matched_unix(i.ext) }
// Clean deletes the explicitly named items; wildcard-loaded items are not resolved at uninstall.
pub open spec fn removed_by_uninstall(i: Item) -> bool { true }

pub proof fn wildcard_extension_neighbors()
    ensures loaded(Item { wildcard: true, ext: 0 }), !loaded(Item { wildcard: true, ext: 1 }),
            !loaded(Item { wildcard: true, ext: 2 }), loaded(Item { wildcard: true, ext: 3 }),
            matched_intended(1), matched_intended(2), loaded(Item { wildcard: false, ext: 0 })
{}

pub proof fn uninstall_neighbors()
    ensures removed_by_uninstall(Item { wildcard: false, ext: 0 }), !removed_by_uninstall(Item { wildcard: true, ext: 0 })
{}

// After uninstall an item is present iff it was loaded and not removed.
pub open spec fn present_after_uninstall(i: Item) -> bool { loaded(i) && !removed_by_uninstall(i) }
pub proof fn leftover_after_uninstall()
    ensures present_after_uninstall(Item { wildcard: true, ext: 0 }), !present_after_uninstall(Item { wildcard: false, ext: 0 }),
            !present_after_uninstall(Item { wildcard: true, ext: 1 })
{}

} // verus!

fn main() {}
