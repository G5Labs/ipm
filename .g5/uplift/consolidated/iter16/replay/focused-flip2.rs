#[path="../../../../Verus_verification/consolidated/iter16/provenance.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(missing().world.code.is_none())) == 0); // missing-no-code
}
}
fn main() {}
