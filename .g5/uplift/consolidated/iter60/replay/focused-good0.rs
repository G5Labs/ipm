#[path="../../../../Verus_verification/consolidated/iter60/legacymsgs.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(in_seq(children(Spec::Mapped(seq![2int, 3])), 2))) == 0) by (compute);assert((b(in_seq(children(Spec::Mapped(seq![2int, 3])), 2))) == 0); // p2.child.IncA
}
}
fn main() {}
