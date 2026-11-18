#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_5() {
assert((b(has_entry(m3_out().resources, Entry { name: seq![301, 12], ext: PKG, dir: seq![62] }))) == 1) by (compute_only);assert((b(has_entry(m3_out().resources, Entry { name: seq![301, 12], ext: PKG, dir: seq![62] }))) == 1); // m3.Id1.B.PKG@y
}
}
fn main() {}
