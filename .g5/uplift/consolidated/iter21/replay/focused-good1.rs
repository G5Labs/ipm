#[path="../../../../Verus_verification/consolidated/iter21/temporal.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
snapshot_neighbor();stable_neighbor();stale_neighbor();refresh_neighbor();readers_neighbor();reveal_with_fuel(model::inherited_duplicates::materialize,2);reveal_with_fuel(model::inherited_duplicates::fold,2);assert(version(Version::One)!=version(Version::Two)) by(compute);assert((b(requested_version_preserved(stale_one(),Version::One))) == 0); // stale-requested-version-preserved
}
}
fn main() {}
