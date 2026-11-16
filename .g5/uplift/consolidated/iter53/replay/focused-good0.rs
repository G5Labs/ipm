#[path="../../../../Verus_verification/consolidated/iter53/generator.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(fixture_author().license == 2)) == 1);assert((b(fixture_author().license == 2)) == 1); // t2.licenseIsYear
}
}
fn main() {}
