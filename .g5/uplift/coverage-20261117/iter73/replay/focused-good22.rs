#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_22() {
assert((b(exported(seq![901, 27, 28], MAC).path == seq![2int])) == 1) by (compute_only);assert((b(exported(seq![901, 27, 28], MAC).path == seq![2int])) == 1); // x1.file.Ixa.R.Rtn.flat
}
}
fn main() {}
