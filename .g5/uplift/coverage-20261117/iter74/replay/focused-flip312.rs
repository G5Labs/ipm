#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_312() {
uninstall_fields();assert((c1_rs().len()) == 0); // c1.rows
}
}
fn main() {}
