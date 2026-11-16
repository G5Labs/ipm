#[path="../../../../Verus_verification/consolidated/iter24/snapshot_dependencies.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
lifecycle_staged();literal_fields();literal_membership();classification_neighbor();decision_neighbor();flat_neighbor();assert((b(ok(shallow()))) == 0); // shallow-snap
}
}
fn main() {}
