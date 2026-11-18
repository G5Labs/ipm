#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_251() {
uninstall_fields();assert((n2_rs().len()) == 1); // n2.rows
}
}
fn main() {}
