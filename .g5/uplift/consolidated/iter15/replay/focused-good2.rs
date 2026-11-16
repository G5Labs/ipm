#[path="../../../../Verus_verification/consolidated/iter15/oras.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(parse(joined(encode(build_version()),current())).platform==current())) == 1); // codec-platform
}
}
fn main() {}
