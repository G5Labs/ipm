#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_23() {
assert((b(has_entry(xout(x1(), 901).resources, Entry { name: seq![901, 27, 28], ext: MAC, dir: Seq::<int>::empty() }))) == 1) by (compute_only);assert((b(has_entry(xout(x1(), 901).resources, Entry { name: seq![901, 27, 28], ext: MAC, dir: Seq::<int>::empty() }))) == 1); // x1.Ixa.R.Rtn.MAC
}
}
fn main() {}
