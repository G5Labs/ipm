#[path="../../../../Verus_verification/consolidated/iter56/locmsgs.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(r1().1)) == 0);assert((b(r1().1)) == 0); // r1.ok
assert((u1().msgs[0]) == 2);assert((u1().msgs[0]) == 2); // u1.msg.A
assert((l2().0.msgs[1]) == 0);assert((l2().0.msgs[1]) == 0); // l2.msg.B
assert((l1().0.msgs[0]) == 1);assert((l1().0.msgs[0]) == 1); // l1.msg.A
assert((l1().0.msgs[1]) == 3);assert((l1().0.msgs[1]) == 3); // l1.msg.B
assert((l1().0.msgs[2]) == 0);assert((l1().0.msgs[2]) == 0); // l1.msg.C
assert((b(l1().0.doc)) == 1);assert((b(l1().0.doc)) == 1); // l1.doc
assert((b(l1().0.module)) == 1);assert((b(l1().0.module)) == 1); // l1.module
assert((b(l1().0.exported)) == 1);assert((b(l1().0.exported)) == 1); // l1.exportfile
assert((b(l1().1)) == 1);assert((b(l1().1)) == 1); // l1.ok
assert((l2().0.msgs[0]) == 2);assert((l2().0.msgs[0]) == 2); // l2.msg.A
assert((l2().0.msgs[2]) == 4);assert((l2().0.msgs[2]) == 4); // l2.msg.C
assert((b(l2().0.doc)) == 1);assert((b(l2().0.doc)) == 1); // l2.doc
assert((b(l2().0.module)) == 1);assert((b(l2().0.module)) == 1); // l2.module
assert((b(l2().0.exported)) == 1);assert((b(l2().0.exported)) == 1); // l2.exportfile
assert((b(l2().1)) == 1);assert((b(l2().1)) == 1); // l2.ok
assert((u1().msgs[1]) == 0);assert((u1().msgs[1]) == 0); // u1.msg.B
assert((u1().msgs[2]) == 4);assert((u1().msgs[2]) == 4); // u1.msg.C
assert((b(u1().doc)) == 0);assert((b(u1().doc)) == 0); // u1.doc
assert((b(u1().module)) == 0);assert((b(u1().module)) == 0); // u1.module
assert((b(u1().exported)) == 1);assert((b(u1().exported)) == 1); // u1.exportfile
assert((r1().0.msgs[0]) == 0);assert((r1().0.msgs[0]) == 0); // r1.msg.A
assert((r1().0.msgs[1]) == 0);assert((r1().0.msgs[1]) == 0); // r1.msg.B
assert((r1().0.msgs[2]) == 0);assert((r1().0.msgs[2]) == 0); // r1.msg.C
assert((b(r1().0.doc)) == 0);assert((b(r1().0.doc)) == 0); // r1.doc
assert((b(r1().0.module)) == 0);assert((b(r1().0.module)) == 0); // r1.module
assert((b(r1().0.exported)) == 0);assert((b(r1().0.exported)) == 0); // r1.exportfile
}
}
fn main() {}
