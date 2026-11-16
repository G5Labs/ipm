#[path="../../../../Verus_verification/consolidated/iter30/phases.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(case(0).ok)) == 1) by (compute);assert((b(case(0).ok)) == 0); // setup.ok
}
}
fn main() {}
