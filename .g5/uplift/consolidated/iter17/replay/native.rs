#[path="../../../../Verus_verification/consolidated/iter17/report_identity.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
raw_neighbors();fixture_neighbors();assert((b(pass().ok)) == 1); // pass
}
pub proof fn observation_1() {
raw_neighbors();fixture_neighbors();assert((b(fail().ok)) == 0); // fail
}
pub proof fn observation_2() {
raw_neighbors();fixture_neighbors();assert((b(nested().ok)) == 0); // nested
}
pub proof fn observation_3() {
raw_neighbors();fixture_neighbors();assert((pass_state().db.len()) == 1); // pass-new-runs
}
pub proof fn observation_4() {
raw_neighbors();fixture_neighbors();assert((file_id(pass())-0) == 1); // pass-file-offset
}
pub proof fn observation_5() {
raw_neighbors();fixture_neighbors();assert((device_id(pass())-0) == 1); // pass-device-offset
}
pub proof fn observation_6() {
raw_neighbors();fixture_neighbors();assert((b(latest(pass_state().db)==Some(file_id(pass())))) == 1); // pass-file-is-latest
}
pub proof fn observation_7() {
raw_neighbors();fixture_neighbors();assert((b(file_id(pass())==device_id(pass()))) == 1); // pass-file-device-same
}
pub proof fn observation_8() {
raw_neighbors();fixture_neighbors();assert((emitted(pass()).len()) == 1); // pass-device-count
}
pub proof fn observation_9() {
raw_neighbors();fixture_neighbors();assert((match pass().file{Some(s)=>summary(s.tree).methods,None=>0}) == 1); // pass-file-methods
}
pub proof fn observation_10() {
raw_neighbors();fixture_neighbors();assert((file_failed_methods(pass())) == 0); // pass-file-failed-methods
}
pub proof fn observation_11() {
raw_neighbors();fixture_neighbors();assert((file_assertions(pass())) == 3); // pass-file-assertions
}
pub proof fn observation_12() {
raw_neighbors();fixture_neighbors();assert((file_failed_assertions(pass())) == 0); // pass-file-failed-assertions
}
pub proof fn observation_13() {
raw_neighbors();fixture_neighbors();assert((b(file_case(pass())==child())) == 0); // pass-file-is-child-case
}
pub proof fn observation_14() {
raw_neighbors();fixture_neighbors();assert((b(file_case(pass())==outer())) == 1); // pass-file-is-outer-case
}
pub proof fn observation_15() {
raw_neighbors();fixture_neighbors();assert((match pass().device{Some(s)=>summary(s.tree).methods,None=>0}) == 1); // pass-device-methods-total
}
pub proof fn observation_16() {
raw_neighbors();fixture_neighbors();assert(((match pass().device{Some(s)=>summary(s.tree).methods,None=>0})-device_failed_methods(pass())) == 1); // pass-device-methods-passed
}
pub proof fn observation_17() {
raw_neighbors();fixture_neighbors();assert((device_failed_methods(pass())) == 0); // pass-device-methods-failed
}
pub proof fn observation_18() {
raw_neighbors();fixture_neighbors();assert((device_assertions(pass())) == 3); // pass-device-assertions-total
}
pub proof fn observation_19() {
raw_neighbors();fixture_neighbors();assert((device_assertions(pass())-device_failed_assertions(pass())) == 3); // pass-device-assertions-passed
}
pub proof fn observation_20() {
raw_neighbors();fixture_neighbors();assert((device_failed_assertions(pass())) == 0); // pass-device-assertions-failed
}
pub proof fn observation_21() {
raw_neighbors();fixture_neighbors();assert((device_failures(decode(pass_run(false))).len()) == 0); // pass-device-failure-rows
}
pub proof fn observation_22() {
raw_neighbors();fixture_neighbors();assert((b(pass().file==snapshot(pass_state(),Some(file_id(pass()))))) == 1); // pass-file-matches-stored-tree
}
pub proof fn observation_23() {
raw_neighbors();fixture_neighbors();assert((b(!decode(pass_run(false)).method_passed)) == 0); // pass-stored-outer-method-failed
}
pub proof fn observation_24() {
raw_neighbors();fixture_neighbors();assert((b(decode(pass_run(false)).method_error.len()==0)) == 1); // pass-stored-outer-error-empty
}
pub proof fn observation_25() {
raw_neighbors();fixture_neighbors();assert((b(passed(pass_run(false).method_header.status))) == 1); // pass-raw-method-pass
}
pub proof fn observation_26() {
raw_neighbors();fixture_neighbors();assert((b(pass_run(false).method_header.category.len()==0)) == 1); // pass-raw-error-category-empty
}
pub proof fn observation_27() {
raw_neighbors();fixture_neighbors();assert((pass_run(false).entries.len()) == 3); // pass-raw-row-count
}
pub proof fn observation_28() {
raw_neighbors();fixture_neighbors();assert((failed_entries(decode(pass_run(false)).entries)) == 0); // pass-raw-failed-count
}
pub proof fn observation_29() {
raw_neighbors();fixture_neighbors();assert((duration_rows(pass_run(false).entries)) == 1); // pass-raw-duration-rows
}
pub proof fn observation_30() {
raw_neighbors();fixture_neighbors();assert((b(passed(duration().status))) == 1); // pass-raw-duration-passed
}
pub proof fn observation_31() {
raw_neighbors();fixture_neighbors();assert((fail_state().db.len()-pass().after.db.len()) == 1); // fail-new-runs
}
pub proof fn observation_32() {
raw_neighbors();fixture_neighbors();assert((file_id(fail())-1) == 1); // fail-file-offset
}
pub proof fn observation_33() {
raw_neighbors();fixture_neighbors();assert((device_id(fail())-1) == 1); // fail-device-offset
}
pub proof fn observation_34() {
raw_neighbors();fixture_neighbors();assert((b(latest(fail_state().db)==Some(file_id(fail())))) == 1); // fail-file-is-latest
}
pub proof fn observation_35() {
raw_neighbors();fixture_neighbors();assert((b(file_id(fail())==device_id(fail()))) == 1); // fail-file-device-same
}
pub proof fn observation_36() {
raw_neighbors();fixture_neighbors();assert((emitted(fail()).len()) == 1); // fail-device-count
}
pub proof fn observation_37() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((match fail().file{Some(s)=>summary(s.tree).methods,None=>0}) == 1); // fail-file-methods
}
pub proof fn observation_38() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((file_failed_methods(fail())) == 1); // fail-file-failed-methods
}
pub proof fn observation_39() {
raw_neighbors();fixture_neighbors();assert((file_assertions(fail())) == 4); // fail-file-assertions
}
pub proof fn observation_40() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((file_failed_assertions(fail())) == 2); // fail-file-failed-assertions
}
pub proof fn observation_41() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((b(file_case(fail())==child())) == 0); // fail-file-is-child-case
}
pub proof fn observation_42() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((b(file_case(fail())==outer())) == 1); // fail-file-is-outer-case
}
pub proof fn observation_43() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((match fail().device{Some(s)=>summary(s.tree).methods,None=>0}) == 1); // fail-device-methods-total
}
pub proof fn observation_44() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert(((match fail().device{Some(s)=>summary(s.tree).methods,None=>0})-device_failed_methods(fail())) == 0); // fail-device-methods-passed
}
pub proof fn observation_45() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((device_failed_methods(fail())) == 1); // fail-device-methods-failed
}
pub proof fn observation_46() {
raw_neighbors();fixture_neighbors();assert((device_assertions(fail())) == 4); // fail-device-assertions-total
}
pub proof fn observation_47() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((device_assertions(fail())-device_failed_assertions(fail())) == 2); // fail-device-assertions-passed
}
pub proof fn observation_48() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((device_failed_assertions(fail())) == 2); // fail-device-assertions-failed
}
pub proof fn observation_49() {
raw_neighbors();fixture_neighbors();assert((device_failures(decode(fail_run(false))).len()) == 2); // fail-device-failure-rows
}
pub proof fn observation_50() {
raw_neighbors();fixture_neighbors();assert((b(fail().file==snapshot(fail_state(),Some(file_id(fail()))))) == 1); // fail-file-matches-stored-tree
}
pub proof fn observation_51() {
raw_neighbors();fixture_neighbors();assert((b(!decode(fail_run(false)).method_passed)) == 1); // fail-stored-outer-method-failed
}
pub proof fn observation_52() {
raw_neighbors();fixture_neighbors();assert((b(decode(fail_run(false)).method_error.len()==0)) == 1); // fail-stored-outer-error-empty
}
pub proof fn observation_53() {
raw_neighbors();fixture_neighbors();assert((b(passed(fail_run(false).method_header.status))) == 0); // fail-raw-method-pass
}
pub proof fn observation_54() {
raw_neighbors();fixture_neighbors();assert((b(fail_run(false).method_header.category.len()==0)) == 1); // fail-raw-error-category-empty
}
pub proof fn observation_55() {
raw_neighbors();fixture_neighbors();assert((fail_run(false).entries.len()) == 4); // fail-raw-row-count
}
pub proof fn observation_56() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((failed_entries(decode(fail_run(false)).entries)) == 2); // fail-raw-failed-count
}
pub proof fn observation_57() {
raw_neighbors();fixture_neighbors();assert((duration_rows(fail_run(false).entries)) == 1); // fail-raw-duration-rows
}
pub proof fn observation_58() {
raw_neighbors();fixture_neighbors();assert((b(passed(duration().status))) == 1); // fail-raw-duration-passed
}
pub proof fn observation_59() {
raw_neighbors();fixture_neighbors();assert((nested_full_state().db.len()-fail().after.db.len()) == 2); // nested-new-runs
}
pub proof fn observation_60() {
raw_neighbors();fixture_neighbors();assert((file_id(nested())-2) == 2); // nested-file-offset
}
pub proof fn observation_61() {
raw_neighbors();fixture_neighbors();assert((device_id(nested())-2) == 1); // nested-device-offset
}
pub proof fn observation_62() {
raw_neighbors();fixture_neighbors();assert((b(latest(nested_full_state().db)==Some(file_id(nested())))) == 1); // nested-file-is-latest
}
pub proof fn observation_63() {
raw_neighbors();fixture_neighbors();assert((b(file_id(nested())==device_id(nested()))) == 0); // nested-file-device-same
}
pub proof fn observation_64() {
raw_neighbors();fixture_neighbors();assert((nested_devices().len()) == 2); // nested-device-count
}
pub proof fn observation_65() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((match nested().file{Some(s)=>summary(s.tree).methods,None=>0}) == 1); // nested-file-methods
}
pub proof fn observation_66() {
raw_neighbors();fixture_neighbors();assert((file_failed_methods(nested())) == 0); // nested-file-failed-methods
}
pub proof fn observation_67() {
raw_neighbors();fixture_neighbors();assert((file_assertions(nested())) == 3); // nested-file-assertions
}
pub proof fn observation_68() {
raw_neighbors();fixture_neighbors();assert((file_failed_assertions(nested())) == 0); // nested-file-failed-assertions
}
pub proof fn observation_69() {
raw_neighbors();fixture_neighbors();assert((b(file_case(nested())==child())) == 1); // nested-file-is-child-case
}
pub proof fn observation_70() {
raw_neighbors();fixture_neighbors();assert((b(file_case(nested())==outer())) == 0); // nested-file-is-outer-case
}
pub proof fn observation_71() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((match nested().device{Some(s)=>summary(s.tree).methods,None=>0}) == 1); // nested-device-methods-total
}
pub proof fn observation_72() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert(((match nested().device{Some(s)=>summary(s.tree).methods,None=>0})-device_failed_methods(nested())) == 0); // nested-device-methods-passed
}
pub proof fn observation_73() {
raw_neighbors();fixture_neighbors();assert((device_failed_methods(nested())) == 1); // nested-device-methods-failed
}
pub proof fn observation_74() {
raw_neighbors();fixture_neighbors();assert((device_assertions(nested())) == 4); // nested-device-assertions-total
}
pub proof fn observation_75() {
raw_neighbors();fixture_neighbors();assert((device_assertions(nested())-device_failed_assertions(nested())) == 2); // nested-device-assertions-passed
}
pub proof fn observation_76() {
raw_neighbors();fixture_neighbors();assert((device_failed_assertions(nested())) == 2); // nested-device-assertions-failed
}
pub proof fn observation_77() {
raw_neighbors();fixture_neighbors();assert((device_failures(decode(fail_run(true))).len()) == 2); // nested-device-failure-rows
}
pub proof fn observation_78() {
raw_neighbors();fixture_neighbors();assert((b(nested().file==snapshot(nested_full_state(),Some(file_id(nested()))))) == 1); // nested-file-matches-stored-tree
}
pub proof fn observation_79() {
raw_neighbors();fixture_neighbors();assert((b(!decode(fail_run(true)).method_passed)) == 1); // nested-stored-outer-method-failed
}
pub proof fn observation_80() {
raw_neighbors();fixture_neighbors();assert((b(decode(fail_run(true)).method_error.len()==0)) == 1); // nested-stored-outer-error-empty
}
pub proof fn observation_81() {
raw_neighbors();fixture_neighbors();assert((b(passed(fail_run(true).method_header.status))) == 0); // nested-raw-method-pass
}
pub proof fn observation_82() {
raw_neighbors();fixture_neighbors();assert((b(fail_run(true).method_header.category.len()==0)) == 1); // nested-raw-error-category-empty
}
pub proof fn observation_83() {
raw_neighbors();fixture_neighbors();assert((fail_run(true).entries.len()) == 4); // nested-raw-row-count
}
pub proof fn observation_84() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((failed_entries(decode(fail_run(true)).entries)) == 2); // nested-raw-failed-count
}
pub proof fn observation_85() {
raw_neighbors();fixture_neighbors();assert((duration_rows(fail_run(true).entries)) == 1); // nested-raw-duration-rows
}
pub proof fn observation_86() {
raw_neighbors();fixture_neighbors();assert((b(passed(duration().status))) == 1); // nested-raw-duration-passed
}
pub proof fn observation_87() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((match child_report().file{Some(s)=>summary(s.tree).methods,None=>0}) == 1); // child-file-methods
}
pub proof fn observation_88() {
raw_neighbors();fixture_neighbors();assert((file_failed_methods(child_report())) == 0); // child-file-failed-methods
}
pub proof fn observation_89() {
raw_neighbors();fixture_neighbors();assert((file_assertions(child_report())) == 3); // child-file-assertions
}
pub proof fn observation_90() {
raw_neighbors();fixture_neighbors();assert((file_failed_assertions(child_report())) == 0); // child-file-failed-assertions
}
pub proof fn observation_91() {
raw_neighbors();fixture_neighbors();assert((b(file_case(child_report())==child())) == 1); // child-file-is-child-case
}
pub proof fn observation_92() {
raw_neighbors();fixture_neighbors();assert((b(file_case(child_report())==outer())) == 0); // child-file-is-outer-case
}
pub proof fn observation_93() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);assert((match forwarded().file{Some(s)=>summary(s.tree).methods,None=>0}) == 1); // explicit-outer-file-methods
}
pub proof fn observation_94() {
raw_neighbors();fixture_neighbors();assert((file_failed_methods(forwarded())) == 1); // explicit-outer-file-failed-methods
}
pub proof fn observation_95() {
raw_neighbors();fixture_neighbors();assert((file_assertions(forwarded())) == 4); // explicit-outer-file-assertions
}
pub proof fn observation_96() {
raw_neighbors();fixture_neighbors();assert((file_failed_assertions(forwarded())) == 2); // explicit-outer-file-failed-assertions
}
pub proof fn observation_97() {
raw_neighbors();fixture_neighbors();assert((b(file_case(forwarded())==child())) == 0); // explicit-outer-file-is-child-case
}
pub proof fn observation_98() {
raw_neighbors();fixture_neighbors();assert((b(file_case(forwarded())==outer())) == 1); // explicit-outer-file-is-outer-case
}
pub proof fn observation_99() {
raw_neighbors();fixture_neighbors();assert((b(file_id(child_report())>device_id(nested()))) == 1); // nested-child-later-than-outer
}
pub proof fn observation_100() {
raw_neighbors();fixture_neighbors();assert((b(file_id(nested())==file_id(child_report()))) == 1); // nested-file-is-child
}
pub proof fn observation_101() {
raw_neighbors();fixture_neighbors();assert((b(nested().file==child_report().file)) == 1); // nested-file-equals-child-file
}
pub proof fn observation_102() {
raw_neighbors();fixture_neighbors();assert((b(file_id(forwarded())==device_id(nested()))) == 1); // explicit-file-is-outer
}
pub proof fn observation_103() {
raw_neighbors();fixture_neighbors();assert((b(forwarded().file==snapshot(nested_full_state(),Some(3)))) == 1); // explicit-file-matches-outer-tree
}
pub proof fn observation_104() {
raw_neighbors();fixture_neighbors();assert((b(device_case(nested())==outer() && device_method(nested())==test_nested())) == 1); // nested-device-failures-have-outer-labels
}
pub proof fn observation_105() {
raw_neighbors();fixture_neighbors();reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(inherited_readers::count_methods,3);reveal_with_fuel(slots,3);reveal_with_fuel(entry_log,5);reveal_with_fuel(inherited_outcome::failures,8);reveal_with_fuel(inherited_outcome::suffix_failures,3);assert((b(child_report().ok)) == 1); // child-shell-ok
}
}
fn main() {}
