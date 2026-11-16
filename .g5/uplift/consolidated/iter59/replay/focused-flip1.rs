#[path="../../../../Verus_verification/consolidated/iter59/mappings.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((visible_sys(C(), c2())) == 1) by (compute);assert((visible_sys(C(), c2())) == 0); // c2.c.insys
}
}
fn main() {}
