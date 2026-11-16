#[path="../../../../Verus_verification/consolidated/iter53/generator.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(fixture_author().copyright == 1)) == 1);assert((b(fixture_author().copyright == 1)) == 1); // t2.copyrightIsLicense
}
}
fn main() {}
