#[path="../../../../Verus_verification/consolidated/iter36/installer.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((l1().calls[1].greeting) == 0) by (compute);assert((l1().calls[1].greeting) == 0); // l1.call2.greeting
}
}
fn main() {}
