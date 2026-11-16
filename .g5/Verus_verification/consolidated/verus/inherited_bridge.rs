#[path="../../../../playbook-uplift/models/verus/load_general.rs"]
mod imported;
use vstd::prelude::*;
verus! {
// Reuse the actual imported theorem unchanged. This proves no heterogeneous-mode extension.
// Sequence folding, total version order, caller/crash premises and excluded late effects stay named.
pub proof fn homogeneous_imported_result(s:imported::L2,c:imported::Choice)
    requires imported::inv(s),imported::enabled(s,c),imported::p1c(s,c),imported::p2(s,c),
        imported::p3(s,c),imported::pc_premise(s,c),imported::pi(s,c),s.clean
    ensures imported::l1_next(imported::abs(s),imported::abs(imported::step(s,c)),false)
{ imported::g_refines(s,c); }
}
fn main() {}
