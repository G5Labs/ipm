#[path="../../../../Verus_verification/consolidated/iter15/oras.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(decode(parse(joined(encode(build_version()),current())).encoded)==build_version())) == 1); // codec-roundtrip
}
}
fn main() {}
