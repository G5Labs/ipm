#[path="../../../../Verus_verification/consolidated/iter60/legacymsgs.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(in_seq(children(Spec::Mapped(seq![2int, 3])), 2))) == 0) by (compute);assert((b(in_seq(children(Spec::Mapped(seq![2int, 3])), 2))) == 0); // p2.child.IncA
assert((b(survives_uninstall(Spec::Mapped(seq![2int, 3]), 3))) == 1) by (compute);assert((b(survives_uninstall(Spec::Mapped(seq![2int, 3]), 3))) == 1); // u1.incB.B
assert(((if survives_uninstall(Spec::Single(1), 1) { 1int } else { -1int })) == -1) by (compute);assert(((if survives_uninstall(Spec::Single(1), 1) { 1int } else { -1int })) == -1); // u2.inc1.one
assert((b(in_seq(children(Spec::Single(1)), 1))) == 1) by (compute);assert((b(in_seq(children(Spec::Single(1)), 1))) == 1); // p1.child.Inc1
assert((b(in_seq(children(Spec::Mapped(seq![2int, 3])), 3))) == 0) by (compute);assert((b(in_seq(children(Spec::Mapped(seq![2int, 3])), 3))) == 0); // p2.child.IncB
assert((b(in_seq(children(Spec::Mapped(seq![2int, 3])), GARBAGE()))) == 1) by (compute);assert((b(in_seq(children(Spec::Mapped(seq![2int, 3])), GARBAGE()))) == 1); // p2.child.commaname
assert((b(survives_uninstall(Spec::Mapped(seq![2int, 3]), 2))) == 1) by (compute);assert((b(survives_uninstall(Spec::Mapped(seq![2int, 3]), 2))) == 1); // u1.incA.A
assert((b(after_load(true, true, false))) == 1) by (compute);assert((b(after_load(true, true, false))) == 1); // p3.msg.Dom1.1
assert((b(after_load(true, false, false))) == 0) by (compute);assert((b(after_load(true, false, false))) == 0); // p3.msg.Dom1.2
assert((b(after_load(true, false, false))) == 0) by (compute);assert((b(after_load(true, false, false))) == 0); // p3.msg.Dom1.98
assert((b(after_load(false, true, true))) == 1) by (compute);assert((b(after_load(false, true, true))) == 1); // p4.msg.DomM.1
assert((b(after_load(true, false, true))) == 1) by (compute);assert((b(after_load(true, false, true))) == 1); // p4.msg.DomM.99
}
}
fn main() {}
