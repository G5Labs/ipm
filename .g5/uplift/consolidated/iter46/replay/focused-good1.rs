#[path="../../../../Verus_verification/consolidated/iter46/summary.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((0int) == 0);assert((0int) == 0); // a.defaults.mode
}
}
fn main() {}
