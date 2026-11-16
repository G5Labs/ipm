#[path="../../../../Verus_verification/consolidated/iter13/batch.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
selected_neighbors();
assert((b(late().ok)) == 0); // late-bad
}
pub proof fn observation_1() {
selected_neighbors();
assert((b(retry().ok)) == 1); // retry
}
pub proof fn observation_2() {
selected_neighbors();
assert((b(other().ok)) == 1); // valid
}
pub proof fn observation_3() {
selected_neighbors();
assert((b(preview_bad().state.cfg.contains(3))) == 0); // preview-pkg
}
pub proof fn observation_4() {
selected_neighbors();
assert((b(preview_bad().state.cfg.contains(3))) == 0); // preview-rtn
}
pub proof fn observation_5() {
selected_neighbors();
assert((b(preview_bad().state.active.contains(3))) == 0); // preview-active
}
pub proof fn observation_6() {
selected_neighbors();
assert((b(preview_bad().state.active.contains(3))) == 0); // preview-callable
}
pub proof fn observation_7() {
selected_neighbors();
assert((b(preview_bad().state.active.contains(3))) == 0); // preview-shell
}
pub proof fn observation_8() {
selected_neighbors();
assert((b(!preview_bad().state.active.contains(3))) == 1); // preview-exception
}
pub proof fn observation_9() {
selected_neighbors();
assert((b(first_bad().state.cfg.contains(2))) == 0); // first-bad-pkg
}
pub proof fn observation_10() {
selected_neighbors();
assert((b(first_bad().state.cfg.contains(2))) == 0); // first-bad-rtn
}
pub proof fn observation_11() {
selected_neighbors();
assert((b(first_bad().state.active.contains(2))) == 0); // first-bad-active
}
pub proof fn observation_12() {
selected_neighbors();
assert((b(first_bad().state.active.contains(2))) == 0); // first-bad-callable
}
pub proof fn observation_13() {
selected_neighbors();
assert((b(first_bad().state.active.contains(2))) == 0); // first-bad-shell
}
pub proof fn observation_14() {
selected_neighbors();
assert((b(!first_bad().state.active.contains(2))) == 1); // first-bad-exception
}
pub proof fn observation_15() {
selected_neighbors();
assert((b(late().state.cfg.contains(1))) == 1); // stranded-pkg
}
pub proof fn observation_16() {
selected_neighbors();
assert((b(late().state.cfg.contains(1))) == 1); // stranded-rtn
}
pub proof fn observation_17() {
selected_neighbors();
assert((b(late().state.active.contains(1))) == 0); // stranded-active
}
pub proof fn observation_18() {
selected_neighbors();
assert((b(late().state.active.contains(1))) == 0); // stranded-callable
}
pub proof fn observation_19() {
selected_neighbors();
assert((b(late().state.active.contains(1))) == 0); // stranded-shell
}
pub proof fn observation_20() {
selected_neighbors();
assert((b(!late().state.active.contains(1))) == 1); // stranded-exception
}
pub proof fn observation_21() {
selected_neighbors();
assert((b(retry().state.cfg.contains(1))) == 1); // retry-pkg
}
pub proof fn observation_22() {
selected_neighbors();
assert((b(retry().state.cfg.contains(1))) == 1); // retry-rtn
}
pub proof fn observation_23() {
selected_neighbors();
assert((b(retry().state.active.contains(1))) == 1); // retry-active
}
pub proof fn observation_24() {
selected_neighbors();
assert((b(retry().state.active.contains(1))) == 1); // retry-callable
}
pub proof fn observation_25() {
selected_neighbors();
assert((b(retry().state.active.contains(1))) == 1); // retry-shell
}
pub proof fn observation_26() {
selected_neighbors();
assert((b(!retry().state.active.contains(1))) == 0); // retry-exception
}
pub proof fn observation_27() {
selected_neighbors();
assert((b(late().state.cfg.contains(1))) == 1); // pending-other-pkg
}
pub proof fn observation_28() {
selected_neighbors();
assert((b(late().state.cfg.contains(1))) == 1); // pending-other-rtn
}
pub proof fn observation_29() {
selected_neighbors();
assert((b(late().state.active.contains(1))) == 0); // pending-other-active
}
pub proof fn observation_30() {
selected_neighbors();
assert((b(late().state.active.contains(1))) == 0); // pending-other-callable
}
pub proof fn observation_31() {
selected_neighbors();
assert((b(late().state.active.contains(1))) == 0); // pending-other-shell
}
pub proof fn observation_32() {
selected_neighbors();
assert((b(!late().state.active.contains(1))) == 1); // pending-other-exception
}
pub proof fn observation_33() {
selected_neighbors();
assert((b(other().state.cfg.contains(4))) == 1); // valid-pkg
}
pub proof fn observation_34() {
selected_neighbors();
assert((b(other().state.cfg.contains(4))) == 1); // valid-rtn
}
pub proof fn observation_35() {
selected_neighbors();
assert((b(other().state.active.contains(4))) == 1); // valid-active
}
pub proof fn observation_36() {
selected_neighbors();
assert((b(other().state.active.contains(4))) == 1); // valid-callable
}
pub proof fn observation_37() {
selected_neighbors();
assert((b(other().state.active.contains(4))) == 1); // valid-shell
}
pub proof fn observation_38() {
selected_neighbors();
assert((b(!other().state.active.contains(4))) == 0); // valid-exception
}
pub proof fn observation_39() {
selected_neighbors();
assert((b(other().state.cfg.contains(1))) == 1); // after-other-pkg
}
pub proof fn observation_40() {
selected_neighbors();
assert((b(other().state.cfg.contains(1))) == 1); // after-other-rtn
}
pub proof fn observation_41() {
selected_neighbors();
assert((b(other().state.active.contains(1))) == 0); // after-other-active
}
pub proof fn observation_42() {
selected_neighbors();
assert((b(other().state.active.contains(1))) == 0); // after-other-callable
}
pub proof fn observation_43() {
selected_neighbors();
assert((b(other().state.active.contains(1))) == 0); // after-other-shell
}
pub proof fn observation_44() {
selected_neighbors();
assert((b(!other().state.active.contains(1))) == 1); // after-other-exception
}
pub proof fn observation_45() {
selected_neighbors();
assert((b(preview_bad().ok)) == 0); // preview
}
pub proof fn observation_46() {
selected_neighbors();
assert((b(first_bad().ok)) == 0); // first-bad
}
pub proof fn observation_47() {
selected_neighbors();
assert((b(late().ok)) == 0); // late-other
}
pub proof fn observation_48() {
selected_neighbors();
assert((b(preview_bad().state.cfg.contains(3))) == 0); // preview-final-pkg
}
pub proof fn observation_49() {
selected_neighbors();
assert((b(preview_bad().state.cfg.contains(3))) == 0); // preview-final-rtn
}
pub proof fn observation_50() {
selected_neighbors();
assert((b(preview_bad().state.active.contains(3))) == 0); // preview-final-active
}
pub proof fn observation_51() {
selected_neighbors();
assert((b(first_bad().state.cfg.contains(2))) == 0); // first-final-pkg
}
pub proof fn observation_52() {
selected_neighbors();
assert((b(first_bad().state.cfg.contains(2))) == 0); // first-final-rtn
}
pub proof fn observation_53() {
selected_neighbors();
assert((b(first_bad().state.active.contains(2))) == 0); // first-final-active
}
}
fn main() {}
