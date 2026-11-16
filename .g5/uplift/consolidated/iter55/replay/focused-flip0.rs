#[path="../../../../Verus_verification/consolidated/iter55/nsinit.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(match init_quiet(Sc::Plain, false, false).active { Sc::Plain => true, _ => false })) == 1);assert((b(match init_quiet(Sc::Plain, false, false).active { Sc::Plain => true, _ => false })) == 0); // i2.sc.base
}
}
fn main() {}
