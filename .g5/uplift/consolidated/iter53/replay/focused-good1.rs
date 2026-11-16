#[path="../../../../Verus_verification/consolidated/iter53/generator.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(exported(skip_rout()))) == 1);assert((b(exported(skip_rout()))) == 1); // e1.skiprout
}
}
fn main() {}
