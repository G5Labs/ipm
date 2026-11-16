#[path="../../../../Verus_verification/consolidated/iter32/sync.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((s1().not_applied) == 1) by (compute);assert((s1().not_applied) == 0); // s1.notapplied
}
}
fn main() {}
