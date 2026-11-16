#[path="../../../../Verus_verification/consolidated/iter9/ownership.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(children(world().files,refs(),root(),false).contains(extra()))) == 1); // unchecked-extra
}
pub proof fn observation_1() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(children(world().files,refs(),root(),true).contains(extra()))) == 0); // checked-extra
}
pub proof fn observation_2() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(children(world().files,refs(),root(),true).contains(inner()))) == 1); // checked-inner
}
pub proof fn observation_3() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(clean(world(),refs(),1).modules.contains(1))) == 0); // pkg-row
}
pub proof fn observation_4() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(clean(world(),refs(),1).files.contains(own()))) == 0); // own-class
}
pub proof fn observation_5() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(clean(world(),refs(),1).files.contains(extra()))) == 1); // extra-class
}
pub proof fn observation_6() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(clean(world(),refs(),1).files.contains(inner()))) == 1); // inner-class
}
pub proof fn observation_7() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(seeded().steps[key(StepKind::Any)].completed)) == 1); // seed-any
}
pub proof fn observation_8() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(seeded().steps[key(StepKind::Primary)].completed)) == 1); // seed-primary
}
pub proof fn observation_9() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(seeded().steps[key(StepKind::Any)].home==1)) == 1); // seed-home-a
}
pub proof fn observation_10() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(both_moved().steps[key(StepKind::Any)].home==2)) == 1); // home-moved
}
pub proof fn observation_11() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(both_moved().steps[key(StepKind::Any)].completed)) == 1); // moved-completed
}
pub proof fn observation_12() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(delete_module(both_moved(),1,true).steps.contains_key(key(StepKind::Any)))) == 1); // any-after-a
}
pub proof fn observation_13() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(delete_module(both_moved(),1,true).steps.contains_key(key(StepKind::Primary)))) == 1); // primary-after-a
}
pub proof fn observation_14() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(delete_module(both_moved(),1,true).modules.contains(2))) == 1); // b-after-a
}
pub proof fn observation_15() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(delete_module(delete_module(both_moved(),1,true),2,true).steps.contains_key(key(StepKind::Any)))) == 0); // any-after-b
}
pub proof fn observation_16() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(delete_module(seeded(),1,true).steps.contains_key(key(StepKind::Any)))) == 0); // any-first-delete
}
pub proof fn observation_17() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(delete_module(seeded(),1,true).steps.contains_key(key(StepKind::Primary)))) == 0); // primary-first-delete
}
pub proof fn observation_18() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(delete_module(delete_module(both_moved(),1,true),2,true).steps.contains_key(key(StepKind::Primary)))) == 0); // primary-after-b
}
pub proof fn observation_19() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(children(world().files,seq![Ref{name:root(),module:1},Ref{name:own(),module:1}],root(),true).contains(own()))) == 0); // same-parent-child
}
pub proof fn observation_20() {
reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
assert((b(clean(world(),seq![Ref{name:root(),module:1},Ref{name:own(),module:1}],1).files.contains(own()))) == 0); // same-class
}
}
fn main() {}
