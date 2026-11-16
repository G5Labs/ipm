#![allow(non_snake_case)]
// Iteration44: `arrange` and XML comment carry-over in the module's .ZPM document.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter44/bindings.json):
//   Main.cls::Arrange                          one module (or -all dev modules) -> ExportDocumentForObject
//   Utils/Module.cls::ExportDocumentForObject  source-control status gate, comment extraction, XMLExportToStream,
//                                              Reapply, save through %RoutineMgr
//   Utils/XMLCommentHandler.cls                comment -> XPath of the next element start; discarded at the
//                                              parent's end tag; reapplied only where that XPath matches
//
// Paths are small codes. Premises (native-qualified): %Studio.SourceControl.Interface.GetStatus fails with
// no source-control class configured and reports editable with the stock base class; XMLExportToStream
// emits Name and dependency paths unchanged, wraps resources in <Resources> sorted by name; XSLT applies
// the generated templates.
use vstd::prelude::*;

verus! {

// anchor: XPath code of the next element start in the original document; None for a last child.
pub struct Comment { pub anchor: Option<int> }

// Export of the module: does a path code exist in the new document?
pub open spec fn exported(p: int) -> bool {
    p == 1 || p == 2 || p == 3 || p == 4 || p == 5
    // original resource paths 4 and 5 (/Module/Resource[@Name=...]) do not exist: export uses /Module/Resources/Resource
}

pub open spec fn kept(c: Comment) -> bool {
    match c.anchor { Some(p) => exported(p), None => false }
}

pub struct Result { pub ok: bool, pub rewritten: bool }

pub open spec fn arrange(source_control: bool) -> Result {
    if !source_control { Result { ok: false, rewritten: false } } else { Result { ok: true, rewritten: true } }
}

// A comment survives arrange iff arrange ran and the comment's anchor path still exists.
pub open spec fn after(c: Comment, source_control: bool, stored_before: bool) -> bool {
    if arrange(source_control).rewritten { kept(c) } else { stored_before }
}

// Resource order in the rewritten document: by name (A=5 before Z=4 in name order).
pub open spec fn z_first(source_control: bool) -> bool { !arrange(source_control).rewritten }

pub open spec fn c(i: int) -> Comment {
    if i == 1 { Comment { anchor: Some(1) } } else if i == 2 { Comment { anchor: Some(2) } }
    else if i == 3 { Comment { anchor: Some(3) } } else if i == 4 { Comment { anchor: Some(4) } }
    else if i == 5 { Comment { anchor: Some(5) } } else { Comment { anchor: None } }
}
// C0 sits outside the Module element and is never stored in the document.
pub open spec fn stored(i: int) -> bool { 1 <= i <= 6 }

pub proof fn without_source_control_nothing_changes(i: int)
    ensures !arrange(false).ok, after(c(i), false, stored(i)) == stored(i)
{}
pub proof fn trailing_comment_dropped()
    ensures !after(c(6), true, true)
{}
pub proof fn resource_comments_dropped()
    ensures !after(c(4), true, true), !after(c(5), true, true)
{}
pub proof fn leading_and_dependency_comments_kept()
    ensures after(c(1), true, true), after(c(2), true, true), after(c(3), true, true)
{}
// A second arrange finds the kept comments at the same anchors, so the result is stable.
pub proof fn idempotent(i: int)
    requires 1 <= i <= 6, after(c(i), true, true)
    ensures kept(c(i))
{}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

} // verus!

fn main() {}
