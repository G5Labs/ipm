#[path="../../../../Verus_verification/consolidated/iter18/reloading.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(reload(initial(),request(alpha())).ok)) == 1); // a
}
pub proof fn observation_1() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(reload(after_a(),request(beta())).ok)) == 1); // b
}
pub proof fn observation_2() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(missing_after_b().ok)) == 0); // missing-after-b
}
pub proof fn observation_3() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(reload(after_missing(),request(alpha())).ok)) == 1); // a-again
}
pub proof fn observation_4() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((payload(after_a())) == 1); // a-payload
}
pub proof fn observation_5() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(supplier(after_a())==alpha())) == 1); // a-supplier
}
pub proof fn observation_6() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(installed_root(after_a())==Some(Root::A))) == 1); // a-root
}
pub proof fn observation_7() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(installed_version(after_a())==version())) == 1); // a-version
}
pub proof fn observation_8() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(after_a().module.is_some())) == 1); // a-module
}
pub proof fn observation_9() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(after_a().code.is_some())) == 1); // a-code
}
pub proof fn observation_10() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(supplier(after_a()).len()==0)) == 0); // a-supplier-empty
}
pub proof fn observation_11() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(show_repository(after_a())==Some(alpha()))) == 1); // a-show-supplier
}
pub proof fn observation_12() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(listed_for_repository(after_a(),alpha()))) == 1); // a-listed-alpha
}
pub proof fn observation_13() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(listed_for_repository(after_a(),beta()))) == 0); // a-listed-beta
}
pub proof fn observation_14() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((payload(after_b())) == 2); // b-payload
}
pub proof fn observation_15() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(supplier(after_b())==beta())) == 1); // b-supplier
}
pub proof fn observation_16() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(installed_root(after_b())==Some(Root::B))) == 1); // b-root
}
pub proof fn observation_17() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(installed_version(after_b())==version())) == 1); // b-version
}
pub proof fn observation_18() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(after_b().module.is_some())) == 1); // b-module
}
pub proof fn observation_19() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(after_b().code.is_some())) == 1); // b-code
}
pub proof fn observation_20() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(supplier(after_b()).len()==0)) == 0); // b-supplier-empty
}
pub proof fn observation_21() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(show_repository(after_b())==Some(beta()))) == 1); // b-show-supplier
}
pub proof fn observation_22() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(listed_for_repository(after_b(),alpha()))) == 0); // b-listed-alpha
}
pub proof fn observation_23() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(listed_for_repository(after_b(),beta()))) == 1); // b-listed-beta
}
pub proof fn observation_24() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((payload(after_missing())) == 2); // after-missing-payload
}
pub proof fn observation_25() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(supplier(after_missing())==beta())) == 1); // after-missing-supplier
}
pub proof fn observation_26() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(installed_root(after_missing())==Some(Root::B))) == 1); // after-missing-root
}
pub proof fn observation_27() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(installed_version(after_missing())==version())) == 1); // after-missing-version
}
pub proof fn observation_28() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(after_missing().module.is_some())) == 1); // after-missing-module
}
pub proof fn observation_29() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(after_missing().code.is_some())) == 1); // after-missing-code
}
pub proof fn observation_30() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(supplier(after_missing()).len()==0)) == 0); // after-missing-supplier-empty
}
pub proof fn observation_31() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(show_repository(after_missing())==Some(beta()))) == 1); // after-missing-show-supplier
}
pub proof fn observation_32() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(listed_for_repository(after_missing(),alpha()))) == 0); // after-missing-listed-alpha
}
pub proof fn observation_33() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(listed_for_repository(after_missing(),beta()))) == 1); // after-missing-listed-beta
}
pub proof fn observation_34() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((payload(after_a_again())) == 1); // a-again-payload
}
pub proof fn observation_35() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(supplier(after_a_again())==alpha())) == 1); // a-again-supplier
}
pub proof fn observation_36() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(installed_root(after_a_again())==Some(Root::A))) == 1); // a-again-root
}
pub proof fn observation_37() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(installed_version(after_a_again())==version())) == 1); // a-again-version
}
pub proof fn observation_38() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(after_a_again().module.is_some())) == 1); // a-again-module
}
pub proof fn observation_39() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(after_a_again().code.is_some())) == 1); // a-again-code
}
pub proof fn observation_40() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(supplier(after_a_again()).len()==0)) == 0); // a-again-supplier-empty
}
pub proof fn observation_41() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(show_repository(after_a_again())==Some(alpha()))) == 1); // a-again-show-supplier
}
pub proof fn observation_42() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(listed_for_repository(after_a_again(),alpha()))) == 1); // a-again-listed-alpha
}
pub proof fn observation_43() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(listed_for_repository(after_a_again(),beta()))) == 0); // a-again-listed-beta
}
pub proof fn observation_44() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(receipt_ok(after_a(),0))) == 1); // a-history-success
}
pub proof fn observation_45() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(history_has_supplier(after_a(),0,alpha(),Root::A))) == 1); // a-rendered-source-details
}
pub proof fn observation_46() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(receipt_ok(after_b(),1))) == 1); // b-history-success
}
pub proof fn observation_47() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(history_has_supplier(after_b(),1,beta(),Root::B))) == 1); // b-rendered-source-details
}
pub proof fn observation_48() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(receipt_ok(after_a_again(),2))) == 1); // a-again-history-success
}
pub proof fn observation_49() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(history_has_supplier(after_a_again(),2,alpha(),Root::A))) == 1); // a-again-rendered-source-details
}
pub proof fn observation_50() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(history_has_supplier(after_b(),0,alpha(),Root::A))) == 1); // old-a-after-b-rendered-source-details
}
pub proof fn observation_51() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(history_has_supplier(after_missing(),0,alpha(),Root::A))) == 1); // old-a-after-missing-rendered-source-details
}
pub proof fn observation_52() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(history_has_supplier(after_missing(),1,beta(),Root::B))) == 1); // old-b-after-missing-rendered-source-details
}
pub proof fn observation_53() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(history_has_supplier(after_a_again(),0,alpha(),Root::A))) == 1); // old-a-final-rendered-source-details
}
pub proof fn observation_54() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((b(history_has_supplier(after_a_again(),1,beta(),Root::B))) == 1); // old-b-final-rendered-source-details
}
pub proof fn observation_55() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((match follows_plain_equal(installed_version(after_a()),version()){Some(v)=>b(v),None=>-1}) == 0); // equal-forward-follows
}
pub proof fn observation_56() {
admission_neighbor();module_neighbor();receipt_neighbor();refusal_neighbor();bounded_version_neighbor();assert((match follows_plain_equal(version(),installed_version(after_a())){Some(v)=>b(v),None=>-1}) == 0); // equal-reverse-follows
}
}
fn main() {}
