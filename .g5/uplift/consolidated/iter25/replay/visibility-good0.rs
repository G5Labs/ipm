#[path="../../../../Verus_verification/consolidated/iter25/developer_sources.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
lifecycle_staged();constructor_neighbor();layout_neighbor();reader_neighbor();constants_neighbor();
hide(normal);hide(developer);hide(refused);hide(explicit_normal);hide(explicit_developer);hide(explicit_refused);
assert((b(ok(developer()))) == 1); // reinstall-dev
}
}
fn main() {}
