#[path="../../../../Verus_verification/consolidated/iter65/moderefresh.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
reveal_with_fuel(value, 6); assert((value(3, true, 4)) == 3);assert((value(3, true, 4)) == 4); // c.D.gbl
}
}
fn main() {}
