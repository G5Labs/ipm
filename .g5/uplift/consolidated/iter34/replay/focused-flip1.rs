#[path="../../../../Verus_verification/consolidated/iter34/versions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((code(installed(repo_c(), E::Any, true))) == 5) by (compute);assert((code(installed(repo_c(), E::Any, true))) == 6); // c1.version
}
}
fn main() {}
