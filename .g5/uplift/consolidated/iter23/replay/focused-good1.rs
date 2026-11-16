#[path="../../../../Verus_verification/consolidated/iter23/status_reader.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(utf8(plain_value(Case::Controls,true))))==0) by(compute);assert((b(utf8(plain_value(Case::Controls,true)))) == 0); // controls-detail-status-utf8
}
}
fn main() {}
