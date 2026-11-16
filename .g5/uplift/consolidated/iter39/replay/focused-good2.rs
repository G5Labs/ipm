#[path="../../../../Verus_verification/consolidated/iter39/document.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((doc_names_other(e3().state)) == 1) by (compute);assert((doc_names_other(e3().state)) == 1); // e3.doctext.other
}
}
fn main() {}
