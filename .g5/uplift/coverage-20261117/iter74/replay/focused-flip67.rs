#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_67() {
assert((api_install(i4()).len()) == 1) by (compute_only);assert((api_install(i4()).len()) == 0); // i4api.rows
}
}
fn main() {}
