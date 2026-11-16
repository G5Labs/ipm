#[path="../../../../Verus_verification/consolidated/iter17/report_identity.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
raw_neighbors();fixture_neighbors();assert((b(pass().ok)) == 1); // pass
}
}
fn main() {}
