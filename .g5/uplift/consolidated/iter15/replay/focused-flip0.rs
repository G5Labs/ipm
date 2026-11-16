#[path="../../../../Verus_verification/consolidated/iter15/oras.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(encode(build_version())==seq!['1','.','2','.','3','-','a','l','p','h','a','_','b','u','i','l','d','.','7'])) == 0); // codec-encode
}
}
fn main() {}
