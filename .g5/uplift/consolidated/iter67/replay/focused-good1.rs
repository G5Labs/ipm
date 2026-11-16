#[path="../../../../Verus_verification/consolidated/iter67/composite.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(create_ok(false))) == 0);assert((b(create_ok(false))) == 0); // s1.new
}
}
fn main() {}
