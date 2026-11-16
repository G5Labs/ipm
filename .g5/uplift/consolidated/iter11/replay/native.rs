#[path="../../../../Verus_verification/consolidated/iter11/all.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(!a().locks.contains(1))) == 0); // a-lock-free
}
pub proof fn observation_1() {
assert((b(!a().locks.contains(2))) == 1); // b-lock-free-before
}
pub proof fn observation_2() {
assert((b(!both().locks.contains(2))) == 0); // b-lock-free-during
}
pub proof fn observation_3() {
assert((both().count as int) == 2); // both-count
}
pub proof fn observation_4() {
assert((after_a().count as int) == 2); // final-count
}
pub proof fn observation_5() {
assert((after_a().value) == 1); // final-value
}
pub proof fn observation_6() {
assert((b(same_attempt().started)) == 0); // same-b-entered
}
pub proof fn observation_7() {
assert((same_attempt().state.count as int) == 1); // same-count
}
pub proof fn observation_8() {
assert((b(same_attempt().ok)) == 1); // same-b-status
}
pub proof fn observation_9() {
assert((b(serial().count>1)) == 0); // serial-b-entered
}
pub proof fn observation_10() {
assert((serial().count as int) == 1); // serial-count
}
pub proof fn observation_11() {
assert((serial().value) == 1); // serial-value
}
pub proof fn observation_12() {
assert((b(a().row.unwrap().home==1)) == 1); // before-b-home-a
}
pub proof fn observation_13() {
assert((b(a().row.unwrap().home==2)) == 0); // before-b-home-b
}
pub proof fn observation_14() {
assert((b(a().row.unwrap().complete)) == 0); // before-b-complete
}
pub proof fn observation_15() {
assert((b(a().row.unwrap().ok)) == 1); // before-b-ok
}
pub proof fn observation_16() {
assert((b(after_b().row.unwrap().home==1)) == 0); // after-b-home-a
}
pub proof fn observation_17() {
assert((b(after_b().row.unwrap().home==2)) == 1); // after-b-home-b
}
pub proof fn observation_18() {
assert((b(after_b().row.unwrap().complete)) == 1); // after-b-complete
}
pub proof fn observation_19() {
assert((b(after_b().row.unwrap().ok)) == 1); // after-b-ok
}
pub proof fn observation_20() {
assert((b(after_a().row.unwrap().home==1)) == 1); // after-a-home-a
}
pub proof fn observation_21() {
assert((b(after_a().row.unwrap().home==2)) == 0); // after-a-home-b
}
pub proof fn observation_22() {
assert((b(after_a().row.unwrap().complete)) == 1); // after-a-complete
}
pub proof fn observation_23() {
assert((b(after_a().row.unwrap().ok)) == 1); // after-a-ok
}
pub proof fn observation_24() {
assert((b(serial().row.unwrap().home==1)) == 0); // serial-final-home-a
}
pub proof fn observation_25() {
assert((b(serial().row.unwrap().home==2)) == 1); // serial-final-home-b
}
pub proof fn observation_26() {
assert((b(serial().row.unwrap().complete)) == 1); // serial-final-complete
}
pub proof fn observation_27() {
assert((b(serial().row.unwrap().ok)) == 1); // serial-final-ok
}
pub proof fn observation_28() {
assert((del::b(del::delete_class(del::before(true)).ok)) == 0); // delete-direct-status
}
pub proof fn observation_29() {
assert((del::b(del::delete_class(del::before(true)).class)) == 1); // delete-direct-retained
}
pub proof fn observation_30() {
assert((del::b(del::uninstall(del::before(true)).ok)) == 1); // delete-locked-uninstall
}
pub proof fn observation_31() {
assert((del::b(del::uninstall(del::before(true)).state.module)) == 0); // delete-locked-module
}
pub proof fn observation_32() {
assert((del::b(del::uninstall(del::before(true)).state.manifest)) == 0); // delete-locked-manifest
}
pub proof fn observation_33() {
assert((del::b(del::uninstall(del::before(true)).state.class)) == 1); // delete-locked-code
}
pub proof fn observation_34() {
assert((del::b(del::uninstall(del::before(false)).ok)) == 1); // delete-unlocked-uninstall
}
pub proof fn observation_35() {
assert((del::b(del::uninstall(del::before(false)).state.class)) == 0); // delete-unlocked-code
}
pub proof fn observation_36() {
assert((b(after_a().results[10])) == 1); // a-status
}
pub proof fn observation_37() {
assert((b(after_a().results[20])) == 1); // b-status
}
pub proof fn observation_38() {
assert((b(serial().results[10])) == 1); // serial-a-status
}
pub proof fn observation_39() {
assert((b(serial().results[20])) == 1); // serial-b-status
}
pub proof fn observation_40() {
assert((b(finish(same_attempt().state,10,Save::Snapshot).results[10])) == 1); // same-a-status
}
}
fn main() {}
