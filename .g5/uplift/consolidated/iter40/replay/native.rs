#[path="../../../../Verus_verification/consolidated/iter40/unittests.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(n1().t_def)) == 1) by (compute);assert((b(n1().t_def)) == 1); // n1.tests
assert((b(n1().v_def)) == 1) by (compute);assert((b(n1().v_def)) == 1); // n1.vtests
assert((b(d0().t_compiled)) == 0) by (compute);assert((b(d0().t_compiled)) == 0); // d0.testscompiled
assert((n0().ran - empty().ran) == 0) by (compute);assert((n0().ran - empty().ran) == 0); // n0.ran
assert((n0().vran - empty().vran) == 0) by (compute);assert((n0().vran - empty().vran) == 0); // n0.vran
assert((b(n0().t_def)) == 0) by (compute);assert((b(n0().t_def)) == 0); // n0.tests
assert((b(n0().v_def)) == 0) by (compute);assert((b(n0().v_def)) == 0); // n0.vtests
assert((b(n0().t_compiled)) == 0) by (compute);assert((b(n0().t_compiled)) == 0); // n0.testscompiled
assert((b(n0().main)) == 1) by (compute);assert((b(n0().main)) == 1); // n0.main
assert((b(n0().record)) == 1) by (compute);assert((b(n0().record)) == 1); // n0.record
assert((n1().ran - n0().ran) == 1) by (compute);assert((n1().ran - n0().ran) == 1); // n1.ran
assert((n1().vran - n0().vran) == 0) by (compute);assert((n1().vran - n0().vran) == 0); // n1.vran
assert((b(n1().t_compiled)) == 1) by (compute);assert((b(n1().t_compiled)) == 1); // n1.testscompiled
assert((b(n1().main)) == 1) by (compute);assert((b(n1().main)) == 1); // n1.main
assert((b(n1().record)) == 1) by (compute);assert((b(n1().record)) == 1); // n1.record
assert((n2().ran - n1().ran) == 0) by (compute);assert((n2().ran - n1().ran) == 0); // n2.ran
assert((n2().vran - n1().vran) == 0) by (compute);assert((n2().vran - n1().vran) == 0); // n2.vran
assert((b(n2().t_def)) == 0) by (compute);assert((b(n2().t_def)) == 0); // n2.tests
assert((b(n2().v_def)) == 0) by (compute);assert((b(n2().v_def)) == 0); // n2.vtests
assert((b(n2().t_compiled)) == 0) by (compute);assert((b(n2().t_compiled)) == 0); // n2.testscompiled
assert((b(n2().main)) == 0) by (compute);assert((b(n2().main)) == 0); // n2.main
assert((b(n2().record)) == 0) by (compute);assert((b(n2().record)) == 0); // n2.record
assert((d0().ran - n2().ran) == 0) by (compute);assert((d0().ran - n2().ran) == 0); // d0.ran
assert((d0().vran - n2().vran) == 0) by (compute);assert((d0().vran - n2().vran) == 0); // d0.vran
assert((b(d0().t_def)) == 1) by (compute);assert((b(d0().t_def)) == 1); // d0.tests
assert((b(d0().v_def)) == 1) by (compute);assert((b(d0().v_def)) == 1); // d0.vtests
assert((b(d0().main)) == 1) by (compute);assert((b(d0().main)) == 1); // d0.main
assert((b(d0().record)) == 1) by (compute);assert((b(d0().record)) == 1); // d0.record
assert((d1().ran - d0().ran) == 1) by (compute);assert((d1().ran - d0().ran) == 1); // d1.ran
assert((d1().vran - d0().vran) == 0) by (compute);assert((d1().vran - d0().vran) == 0); // d1.vran
assert((b(d1().t_def)) == 1) by (compute);assert((b(d1().t_def)) == 1); // d1.tests
assert((b(d1().v_def)) == 1) by (compute);assert((b(d1().v_def)) == 1); // d1.vtests
assert((b(d1().t_compiled)) == 1) by (compute);assert((b(d1().t_compiled)) == 1); // d1.testscompiled
assert((b(d1().main)) == 1) by (compute);assert((b(d1().main)) == 1); // d1.main
assert((b(d1().record)) == 1) by (compute);assert((b(d1().record)) == 1); // d1.record
assert((d2().ran - d1().ran) == 0) by (compute);assert((d2().ran - d1().ran) == 0); // d2.ran
assert((d2().vran - d1().vran) == 0) by (compute);assert((d2().vran - d1().vran) == 0); // d2.vran
assert((b(d2().t_def)) == 0) by (compute);assert((b(d2().t_def)) == 0); // d2.tests
assert((b(d2().v_def)) == 0) by (compute);assert((b(d2().v_def)) == 0); // d2.vtests
assert((b(d2().t_compiled)) == 0) by (compute);assert((b(d2().t_compiled)) == 0); // d2.testscompiled
assert((b(d2().main)) == 0) by (compute);assert((b(d2().main)) == 0); // d2.main
assert((b(d2().record)) == 0) by (compute);assert((b(d2().record)) == 0); // d2.record
}
}
fn main() {}
