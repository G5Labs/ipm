#[path="../../../../Verus_verification/consolidated/iter23/status_reader.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(utf8(plain_value(Case::Good,true))))==1) by(compute);assert((b(utf8(plain_value(Case::Good,true)))) == 0); // good-detail-status-utf8
}
}
fn main() {}
