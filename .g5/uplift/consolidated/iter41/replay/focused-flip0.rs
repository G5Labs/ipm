#[path="../../../../Verus_verification/consolidated/iter41/settings.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((c2().slots[9].value) == 5) by (compute);assert((c2().slots[9].value) == 6); // c2.HistoryRetain.value
}
}
fn main() {}
