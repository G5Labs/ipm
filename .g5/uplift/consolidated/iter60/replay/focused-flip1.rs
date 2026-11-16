#[path="../../../../Verus_verification/consolidated/iter60/legacymsgs.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(survives_uninstall(Spec::Mapped(seq![2int, 3]), 3))) == 1) by (compute);assert((b(survives_uninstall(Spec::Mapped(seq![2int, 3]), 3))) == 0); // u1.incB.B
}
}
fn main() {}
