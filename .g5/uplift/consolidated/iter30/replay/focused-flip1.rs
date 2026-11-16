#[path="../../../../Verus_verification/consolidated/iter30/phases.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((case(0).starts.len() as int) == 6) by (compute);assert((case(0).starts.len() as int) == 7); // setup.starts
}
}
fn main() {}
