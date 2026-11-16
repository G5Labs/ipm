#[path="../../../../Verus_verification/consolidated/iter65/moderefresh.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
reveal_with_fuel(value, 6); assert((value(4, true, 3)) == 2);assert((value(4, true, 3)) == 3); // b2.D.filecopy
}
}
fn main() {}
