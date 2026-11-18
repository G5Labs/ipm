#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_66() {
assert((cli_install(i4()).len()) == 0) by (compute_only);assert((cli_install(i4()).len()) == 1); // i4.rows
}
}
fn main() {}
