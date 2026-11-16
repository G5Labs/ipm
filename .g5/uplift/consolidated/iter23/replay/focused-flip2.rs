#[path="../../../../Verus_verification/consolidated/iter23/status_reader.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(utf8(plain_value(Case::Bad,true))))==0) by(compute);assert((b(utf8(plain_value(Case::Bad,true)))) == 1); // bad-detail-status-utf8
}
}
fn main() {}
