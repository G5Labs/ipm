#[path="../../../../Verus_verification/consolidated/iter65/moderefresh.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
reveal_with_fuel(value, 6); assert((value(1, false, 2)) == 2);assert((value(1, false, 2)) == 3); // b1.N.inc
}
}
fn main() {}
