#[path="../../../../Verus_verification/consolidated/iter40/unittests.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(n1().v_def)) == 1) by (compute);assert((b(n1().v_def)) == 1); // n1.vtests
}
}
fn main() {}
