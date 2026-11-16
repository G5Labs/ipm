#[path="../../../../Verus_verification/consolidated/iter17/report_identity.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
raw_neighbors();fixture_neighbors();assert((b(fail().ok)) == 0); // fail
}
}
fn main() {}
