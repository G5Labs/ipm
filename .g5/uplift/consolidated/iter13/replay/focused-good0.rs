#[path="../../../../Verus_verification/consolidated/iter13/batch.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
selected_neighbors();
assert((b(late().ok)) == 0); // late-bad
}
}
fn main() {}
