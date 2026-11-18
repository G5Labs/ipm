#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_263() {
uninstall_fields();assert((n3_rs().len()) == 1); // n3.rows
}
}
fn main() {}
