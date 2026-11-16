#[path="../../../../Verus_verification/consolidated/iter52/record.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((code(first_install(2, false))) == 1);assert((code(first_install(2, false))) == 1); // l1.life
}
}
fn main() {}
