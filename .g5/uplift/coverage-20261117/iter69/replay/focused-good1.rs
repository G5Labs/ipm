#[path="../../../../Verus_verification/coverage-20261117/iter69/grouping.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(has_entry(m3_out().resources, pkg(seq![301, 12], seq![61])))) == 0) by (compute_only);assert((b(has_entry(m3_out().resources, pkg(seq![301, 12], seq![61])))) == 0); // m3.Id1.B.PKG@x
}
}
fn main() {}
