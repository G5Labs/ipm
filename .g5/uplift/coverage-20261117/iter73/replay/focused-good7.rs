#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_7() {
assert((b(has_entry(m8_out().resources, Entry { name: seq![801], ext: PKG, dir: seq![68] }))) == 1) by (compute_only);assert((b(has_entry(m8_out().resources, Entry { name: seq![801], ext: PKG, dir: seq![68] }))) == 1); // m8.Im8.PKG@ab
}
}
fn main() {}
