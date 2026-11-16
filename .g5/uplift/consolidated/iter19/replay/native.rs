#[path="../../../../Verus_verification/consolidated/iter19/duplicates.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(build(single_a()).ok)) == 1); // repo-single
}
pub proof fn observation_1() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(build(ab_a_first()).ok)) == 1); // repo-ab
}
pub proof fn observation_2() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(build(ba_b_first()).ok)) == 1); // repo-ba
}
pub proof fn observation_3() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((single_a().order.len()) == 1); // single-scan-count
}
pub proof fn observation_4() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(single_a().directories[single_a().order[0]].manifest.marker)) == 1); // single-scan-1-marker
}
pub proof fn observation_5() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(single_a().directories[single_a().order[0]].manifest.marker)) == 1); // single-first-marker
}
pub proof fn observation_6() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(retained_dir(single_a())==single_a().order[0])) == 1); // single-cache-first-directory
}
pub proof fn observation_7() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(cached_marker(single_a()))) == 1); // single-cache-marker
}
pub proof fn observation_8() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((rows(single_a())) == 1); // single-cache-count
}
pub proof fn observation_9() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(match service_manifest(build(single_a()),target(single_a())){Some(m)=>m.marker,None=>seq![]})) == 1); // single-service-marker
}
pub proof fn observation_10() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(service_directory(build(single_a()),target(single_a()))==Some(single_a().root+retained_dir(single_a())+slash()))) == 1); // single-service-directory-cache
}
pub proof fn observation_11() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(single_a().directories[retained_dir(single_a())].manifest.marker)) == 1); // single-service-directory-marker
}
pub proof fn observation_12() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(cached_marker(single_a())==(match service_manifest(build(single_a()),target(single_a())){Some(m)=>m.marker,None=>seq![]}))) == 1); // single-cache-service-agree
}
pub proof fn observation_13() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(cached_marker(single_a())==single_a().directories[single_a().order[0]].manifest.marker)) == 1); // single-cache-first-marker
}
pub proof fn observation_14() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(loaded(single_a()).module.is_some())) == 1); // single-installed
}
pub proof fn observation_15() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(installed_root(loaded(single_a()))==Some(single_a().root+single_a().order[0]+slash()))) == 1); // single-installed-root
}
pub proof fn observation_16() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(supplier(loaded(single_a()))==single_a().repository)) == 1); // single-installed-supplier
}
pub proof fn observation_17() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(installed_version(loaded(single_a()))==version())) == 1); // single-installed-version
}
pub proof fn observation_18() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(description(loaded(single_a())))) == 1); // single-installed-marker
}
pub proof fn observation_19() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(description(loaded(single_a()))==source_a())) == 1); // single-installed-description-exact
}
pub proof fn observation_20() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((payload(loaded(single_a()))) == 101); // single-payload
}
pub proof fn observation_21() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(warnings(single_a())>0)) == 0); // single-duplicate-warning
}
pub proof fn observation_22() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(shown_supplier(loaded(single_a()))==Some(single_a().repository))) == 1); // single-show-supplier
}
pub proof fn observation_23() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(listed_for_repository(loaded(single_a()),single_a().repository))) == 1); // single-listed-module
}
pub proof fn observation_24() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(loaded(single_a()).ok)) == 1); // install-single
}
pub proof fn observation_25() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((ab_a_first().order.len()) == 2); // ab-scan-count
}
pub proof fn observation_26() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ab_a_first().directories[ab_a_first().order[0]].manifest.marker)) == 1); // ab-scan-1-marker
}
pub proof fn observation_27() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ab_a_first().directories[ab_a_first().order[0]].manifest.marker)) == 1); // ab-first-marker
}
pub proof fn observation_28() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(retained_dir(ab_a_first())==ab_a_first().order[0])) == 1); // ab-cache-first-directory
}
pub proof fn observation_29() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(cached_marker(ab_a_first()))) == 1); // ab-cache-marker
}
pub proof fn observation_30() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((rows(ab_a_first())) == 1); // ab-cache-count
}
pub proof fn observation_31() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(match service_manifest(build(ab_a_first()),target(ab_a_first())){Some(m)=>m.marker,None=>seq![]})) == 1); // ab-service-marker
}
pub proof fn observation_32() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(service_directory(build(ab_a_first()),target(ab_a_first()))==Some(ab_a_first().root+retained_dir(ab_a_first())+slash()))) == 1); // ab-service-directory-cache
}
pub proof fn observation_33() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ab_a_first().directories[retained_dir(ab_a_first())].manifest.marker)) == 1); // ab-service-directory-marker
}
pub proof fn observation_34() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(cached_marker(ab_a_first())==(match service_manifest(build(ab_a_first()),target(ab_a_first())){Some(m)=>m.marker,None=>seq![]}))) == 1); // ab-cache-service-agree
}
pub proof fn observation_35() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(cached_marker(ab_a_first())==ab_a_first().directories[ab_a_first().order[0]].manifest.marker)) == 1); // ab-cache-first-marker
}
pub proof fn observation_36() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(loaded(ab_a_first()).module.is_some())) == 1); // ab-installed
}
pub proof fn observation_37() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(installed_root(loaded(ab_a_first()))==Some(ab_a_first().root+ab_a_first().order[0]+slash()))) == 1); // ab-installed-root
}
pub proof fn observation_38() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(supplier(loaded(ab_a_first()))==ab_a_first().repository)) == 1); // ab-installed-supplier
}
pub proof fn observation_39() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(installed_version(loaded(ab_a_first()))==version())) == 1); // ab-installed-version
}
pub proof fn observation_40() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(description(loaded(ab_a_first())))) == 1); // ab-installed-marker
}
pub proof fn observation_41() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(description(loaded(ab_a_first()))==source_a())) == 1); // ab-installed-description-exact
}
pub proof fn observation_42() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((payload(loaded(ab_a_first()))) == 101); // ab-payload
}
pub proof fn observation_43() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(warnings(ab_a_first())>0)) == 1); // ab-duplicate-warning
}
pub proof fn observation_44() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(shown_supplier(loaded(ab_a_first()))==Some(ab_a_first().repository))) == 1); // ab-show-supplier
}
pub proof fn observation_45() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(listed_for_repository(loaded(ab_a_first()),ab_a_first().repository))) == 1); // ab-listed-module
}
pub proof fn observation_46() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ab_a_first().directories[ab_a_first().order[1]].manifest.marker)) == 2); // ab-scan-2-marker
}
pub proof fn observation_47() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(loaded(ab_a_first()).ok)) == 1); // install-ab
}
pub proof fn observation_48() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((ba_b_first().order.len()) == 2); // ba-scan-count
}
pub proof fn observation_49() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ba_b_first().directories[ba_b_first().order[0]].manifest.marker)) == 2); // ba-scan-1-marker
}
pub proof fn observation_50() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ba_b_first().directories[ba_b_first().order[0]].manifest.marker)) == 2); // ba-first-marker
}
pub proof fn observation_51() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(retained_dir(ba_b_first())==ba_b_first().order[0])) == 1); // ba-cache-first-directory
}
pub proof fn observation_52() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(cached_marker(ba_b_first()))) == 2); // ba-cache-marker
}
pub proof fn observation_53() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((rows(ba_b_first())) == 1); // ba-cache-count
}
pub proof fn observation_54() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(match service_manifest(build(ba_b_first()),target(ba_b_first())){Some(m)=>m.marker,None=>seq![]})) == 2); // ba-service-marker
}
pub proof fn observation_55() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(service_directory(build(ba_b_first()),target(ba_b_first()))==Some(ba_b_first().root+retained_dir(ba_b_first())+slash()))) == 1); // ba-service-directory-cache
}
pub proof fn observation_56() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ba_b_first().directories[retained_dir(ba_b_first())].manifest.marker)) == 2); // ba-service-directory-marker
}
pub proof fn observation_57() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(cached_marker(ba_b_first())==(match service_manifest(build(ba_b_first()),target(ba_b_first())){Some(m)=>m.marker,None=>seq![]}))) == 1); // ba-cache-service-agree
}
pub proof fn observation_58() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(cached_marker(ba_b_first())==ba_b_first().directories[ba_b_first().order[0]].manifest.marker)) == 1); // ba-cache-first-marker
}
pub proof fn observation_59() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(loaded(ba_b_first()).module.is_some())) == 1); // ba-installed
}
pub proof fn observation_60() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(installed_root(loaded(ba_b_first()))==Some(ba_b_first().root+ba_b_first().order[0]+slash()))) == 1); // ba-installed-root
}
pub proof fn observation_61() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(supplier(loaded(ba_b_first()))==ba_b_first().repository)) == 1); // ba-installed-supplier
}
pub proof fn observation_62() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(installed_version(loaded(ba_b_first()))==version())) == 1); // ba-installed-version
}
pub proof fn observation_63() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(description(loaded(ba_b_first())))) == 2); // ba-installed-marker
}
pub proof fn observation_64() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(description(loaded(ba_b_first()))==source_b())) == 1); // ba-installed-description-exact
}
pub proof fn observation_65() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((payload(loaded(ba_b_first()))) == 202); // ba-payload
}
pub proof fn observation_66() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(warnings(ba_b_first())>0)) == 1); // ba-duplicate-warning
}
pub proof fn observation_67() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(shown_supplier(loaded(ba_b_first()))==Some(ba_b_first().repository))) == 1); // ba-show-supplier
}
pub proof fn observation_68() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(listed_for_repository(loaded(ba_b_first()),ba_b_first().repository))) == 1); // ba-listed-module
}
pub proof fn observation_69() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ba_b_first().directories[ba_b_first().order[1]].manifest.marker)) == 1); // ba-scan-2-marker
}
pub proof fn observation_70() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(loaded(ba_b_first()).ok)) == 1); // install-ba
}
pub proof fn observation_71() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((single_a().order.len()) == 1); // single-constructor-count
}
pub proof fn observation_72() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(single_a().directories[single_a().order[0]].manifest.marker)) == 1); // single-constructor-1-marker
}
pub proof fn observation_73() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((ab_a_first().order.len()) == 2); // ab-constructor-count
}
pub proof fn observation_74() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ab_a_first().directories[ab_a_first().order[0]].manifest.marker)) == 1); // ab-constructor-1-marker
}
pub proof fn observation_75() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ab_a_first().directories[ab_a_first().order[1]].manifest.marker)) == 2); // ab-constructor-2-marker
}
pub proof fn observation_76() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((ba_b_first().order.len()) == 2); // ba-constructor-count
}
pub proof fn observation_77() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ba_b_first().directories[ba_b_first().order[0]].manifest.marker)) == 2); // ba-constructor-1-marker
}
pub proof fn observation_78() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((marker_code(ba_b_first().directories[ba_b_first().order[1]].manifest.marker)) == 1); // ba-constructor-2-marker
}
pub proof fn observation_79() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(description(loaded(single_a()))==source_a())) == 1); // single-show-description
}
pub proof fn observation_80() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(installed_root(loaded(single_a()))==Some(root_single()+low_a()+slash()))) == 1); // single-show-root
}
pub proof fn observation_81() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(description(loaded(ab_a_first()))==source_a())) == 1); // ab-show-description
}
pub proof fn observation_82() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(installed_root(loaded(ab_a_first()))==Some(root_ab()+low_a()+slash()))) == 1); // ab-show-root
}
pub proof fn observation_83() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(description(loaded(ba_b_first()))==source_b())) == 1); // ba-show-description
}
pub proof fn observation_84() {
admission_neighbor();warning_neighbor();service_neighbor();installation_neighbor();reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);assert((b(installed_root(loaded(ba_b_first()))==Some(root_ba()+low_b()+slash()))) == 1); // ba-show-root
}
}
fn main() {}
