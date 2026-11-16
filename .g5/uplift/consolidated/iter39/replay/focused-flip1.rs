#[path="../../../../Verus_verification/consolidated/iter39/document.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((version(e3().state)) == 5) by (compute);assert((version(e3().state)) == 6); // e3.version
}
}
fn main() {}
