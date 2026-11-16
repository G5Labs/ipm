#[path="../../../../Verus_verification/consolidated/iter39/document.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((doc_empty(e2().state)) == 1) by (compute);assert((doc_empty(e2().state)) == 1); // e2.doctext.empty
}
}
fn main() {}
