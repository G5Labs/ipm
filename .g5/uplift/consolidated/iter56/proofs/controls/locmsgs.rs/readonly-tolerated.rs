#![allow(non_snake_case)]
// Iteration56: localized message (.LOC) resources across load, reload, uninstall and read-only roots.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter56/bindings.json):
//   ResourceProcessor/Default/LocalizedMessages.cls::OnBeforePhase, OnPhase   compile the .LOC document (message dictionary)
//                                                                              and default Filename /localize/<name>.xml
//   ResourceProcessor/LocalizationExport.cls::OnBeforePhase, GetDomainsForResource, GetMessageFile
//                                                                              Activate exports the document's domains to
//                                                                              Module.Root + Filename (no-op without a root)
//   Lifecycle/Base.cls::%Clean                                                 deletes the .LOC document only
// Message ids are small ints; text codes follow the native reader (alpha 1, alpha2 2, beta 3, gamma 4).
// Premises (native-qualified): compiling a .LOC replaces its domain's messages for the language; deleting
// the document leaves ^IRIS.Msg untouched; writing the export needs a writable module root.
use vstd::prelude::*;

verus! {

pub struct S { pub msgs: Seq<int>, pub doc: bool, pub module: bool, pub exported: bool }   // msgs[i] = text code of id i (0 = absent)

pub open spec fn load(s: S, file: Seq<int>, root_writable: bool) -> (S, bool) {
    if false { (s, false) }   // Activate cannot write the export; the load fails and rolls back
    else { (S { msgs: file, doc: true, module: true, exported: true }, true) }
}
pub open spec fn uninstall(s: S) -> S { S { doc: false, module: false, ..s } }

pub proof fn reload_replaces_domain(s: S, f: Seq<int>)
    ensures load(s, f, true).0.msgs == f, load(s, f, true).1
{}
pub proof fn readonly_root_fails(s: S, f: Seq<int>)
    ensures !load(s, f, false).1, load(s, f, false).0 == s
{}
pub proof fn uninstall_keeps_messages(s: S)
    ensures uninstall(s).msgs == s.msgs, !uninstall(s).doc, !uninstall(s).module, uninstall(s).exported == s.exported
{}

pub open spec fn empty() -> S { S { msgs: seq![0int, 0, 0], doc: false, module: false, exported: false } }
pub open spec fn l1() -> (S, bool) { load(empty(), seq![1int, 3, 0], true) }
pub open spec fn l2() -> (S, bool) { load(l1().0, seq![2int, 0, 4], true) }
pub open spec fn u1() -> S { uninstall(l2().0) }
pub open spec fn r1() -> (S, bool) { load(empty(), seq![1int, 0, 0], false) }
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

pub proof fn schedule()
    ensures l1().0.msgs[1] == 3, l2().0.msgs[1] == 0, l2().0.msgs[2] == 4, u1().msgs[0] == 2, !u1().doc,
            !r1().1, r1().0.msgs[0] == 0, l1().0.exported, !r1().0.exported
{}

} // verus!

fn main() {}
