#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_315() {
uninstall_fields();assert((c2_rows()) == 0); // c2.rows
}
}
fn main() {}
