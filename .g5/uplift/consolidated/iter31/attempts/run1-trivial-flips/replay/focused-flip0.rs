#[path="../../../../Verus_verification/consolidated/iter31/installed.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
resolution_neighbor();reference_neighbor();directory_neighbor();assert((bi(empty_outcome().ok)) == 0); // repo.ok
}
}
fn main() {}
