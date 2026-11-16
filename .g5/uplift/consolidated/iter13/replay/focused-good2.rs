#[path="../../../../Verus_verification/consolidated/iter13/batch.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
selected_neighbors();
assert((b(other().ok)) == 1); // valid
}
}
fn main() {}
