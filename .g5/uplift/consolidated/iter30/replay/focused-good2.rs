#[path="../../../../Verus_verification/consolidated/iter30/phases.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(!case(0).ok)) == 0) by (compute);assert((b(!case(0).ok)) == 0); // setup.failures
}
}
fn main() {}
