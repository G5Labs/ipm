#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_6() {
assert((b(has_entry(m7_out().resources, Entry { name: seq![701], ext: PKG, dir: seq![66] }))) == 1) by (compute_only);assert((b(has_entry(m7_out().resources, Entry { name: seq![701], ext: PKG, dir: seq![66] }))) == 0); // m7.Im7.PKG@a
}
}
fn main() {}
