#[path="../../../../Verus_verification/consolidated/iter52/record.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(modified_changes(false))) == 1);assert((b(modified_changes(false))) == 0); // c1.modchanged
}
}
fn main() {}
