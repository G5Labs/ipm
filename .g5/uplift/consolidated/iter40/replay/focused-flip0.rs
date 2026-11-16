#[path="../../../../Verus_verification/consolidated/iter40/unittests.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(n1().t_def)) == 1) by (compute);assert((b(n1().t_def)) == 0); // n1.tests
}
}
fn main() {}
