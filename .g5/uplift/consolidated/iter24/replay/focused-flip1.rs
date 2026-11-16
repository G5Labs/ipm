#[path="../../../../Verus_verification/consolidated/iter24/snapshot_dependencies.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
lifecycle_staged();literal_fields();literal_membership();classification_neighbor();decision_neighbor();flat_neighbor();assert((b(ok(default_snapshot()))) == 1); // default-snap
}
}
fn main() {}
