#[path="../../../../Verus_verification/coverage-20261117/iter69/grouping.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(has_entry(m1_out().resources, pkg(seq![201, 12], seq![1])))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, pkg(seq![201, 12], seq![1])))) == 1); // m1.Ia1.B.PKG@cls
}
}
fn main() {}
