#[path="../../../../Verus_verification/consolidated/iter41/settings.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(analytics_disabled(c8().slots[1].value))) == 1) by (compute);assert((b(analytics_disabled(c8().slots[1].value))) == 1); // c8.consumerdisabled
}
}
fn main() {}
