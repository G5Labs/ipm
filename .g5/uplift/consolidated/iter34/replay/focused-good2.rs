#[path="../../../../Verus_verification/consolidated/iter34/versions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((pos(repo_d(), v200(), true)) == 1) by (compute);assert((pos(repo_d(), v200(), true)) == 1); // q5.pos.7
}
}
fn main() {}
