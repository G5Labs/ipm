#[path="../../../../Verus_verification/consolidated/iter16/provenance.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(missing().ok)) == 1); // missing
}
}
fn main() {}
