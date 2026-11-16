#[path="../../../../Verus_verification/consolidated/iter17/report_identity.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
raw_neighbors();fixture_neighbors();assert((b(nested().ok)) == 1); // nested
}
}
fn main() {}
