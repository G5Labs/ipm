#[path="../../../../Verus_verification/consolidated/iter34/versions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((code(installed(repo_d(), E::Any, true))) == 3) by (compute);assert((code(installed(repo_d(), E::Any, true))) == 3); // d1.version
}
}
fn main() {}
