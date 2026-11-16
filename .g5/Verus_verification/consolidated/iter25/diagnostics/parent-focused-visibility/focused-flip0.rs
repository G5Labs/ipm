#[path="../../../../Verus_verification/consolidated/iter25/developer_sources.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
lifecycle_staged();constructor_neighbor();layout_neighbor();reader_neighbor();constants_neighbor();assert((b(ok(developer()))) == 0); // reinstall-dev
}
}
fn main() {}
