#[path="../../../../Verus_verification/consolidated/iter15/oras.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(encode(build_version())==seq!['1','.','2','.','3','-','a','l','p','h','a','_','b','u','i','l','d','.','7'])) == 1); // codec-encode
}
pub proof fn observation_1() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(decode(parse(joined(encode(build_version()),current())).encoded)==build_version())) == 1); // codec-roundtrip
}
pub proof fn observation_2() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(parse(joined(encode(build_version()),current())).platform==current())) == 1); // codec-platform
}
pub proof fn observation_3() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((normal().rows.len() as int) == 1); // normal-service-count
}
pub proof fn observation_4() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(normal().rows[0].version)) == 1); // normal-first
}
pub proof fn observation_5() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(normal().rows[0].platforms,current()))) == 1); // normal-current
}
pub proof fn observation_6() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(normal().rows[0].platforms,bad()))) == 0) by (compute);
assert((b(has_text(normal().rows[0].platforms,bad()))) == 0); // normal-bad
}
pub proof fn observation_7() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(normal().rows[0].all_tags,t2()))) == 0) by (compute);
assert((b(has_text(normal().rows[0].all_tags,t2()))) == 0); // normal-all-has-v2
}
pub proof fn observation_8() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((manager(normal().rows,current()).len() as int) == 1) by (compute);
assert((manager(normal().rows,current()).len() as int) == 1); // normal-manager-count
}
pub proof fn observation_9() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(manager(normal().rows,current())[0].version)) == 1); // normal-manager-first
}
pub proof fn observation_10() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((all().rows.len() as int) == 1); // all-service-count
}
pub proof fn observation_11() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(all().rows[0].version)) == 1); // all-first
}
pub proof fn observation_12() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(all().rows[0].platforms,current()))) == 0); // all-current
}
pub proof fn observation_13() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(all().rows[0].platforms,bad()))) == 0); // all-bad
}
pub proof fn observation_14() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(all().rows[0].all_tags,t2()))) == 1); // all-all-has-v2
}
pub proof fn observation_15() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((manager(all().rows,current()).len() as int) == 0); // all-manager-count
}
pub proof fn observation_16() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((dependencies().rows.len() as int) == 2); // deps-service-count
}
pub proof fn observation_17() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(dependencies().rows[0].version)) == 2); // deps-first
}
pub proof fn observation_18() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(dependencies().rows[0].platforms,current()))) == 0) by (compute);
assert((b(has_text(dependencies().rows[0].platforms,current()))) == 0); // deps-current
}
pub proof fn observation_19() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(dependencies().rows[0].platforms,bad()))) == 1) by (compute);
assert((b(has_text(dependencies().rows[0].platforms,bad()))) == 1); // deps-bad
}
pub proof fn observation_20() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(dependencies().rows[0].all_tags,t2()))) == 0) by (compute);
assert((b(has_text(dependencies().rows[0].all_tags,t2()))) == 0); // deps-all-has-v2
}
pub proof fn observation_21() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((manager(dependencies().rows,current()).len() as int) == 1); // deps-manager-count
}
pub proof fn observation_22() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(manager(dependencies().rows,current())[0].version)) == 1) by (compute);
assert((rank(manager(dependencies().rows,current())[0].version)) == 1); // deps-manager-first
}
pub proof fn observation_23() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((multi().rows.len() as int) == 1); // same-version-service-count
}
pub proof fn observation_24() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(multi().rows[0].version)) == 1) by (compute);
assert((rank(multi().rows[0].version)) == 1); // same-version-first
}
pub proof fn observation_25() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(multi().rows[0].platforms,current()))) == 1); // same-version-current
}
pub proof fn observation_26() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(multi().rows[0].platforms,bad()))) == 1) by (compute);
assert((b(has_text(multi().rows[0].platforms,bad()))) == 1); // same-version-bad
}
pub proof fn observation_27() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(multi().rows[0].all_tags,t2()))) == 0) by (compute);
assert((b(has_text(multi().rows[0].all_tags,t2()))) == 0); // same-version-all-has-v2
}
pub proof fn observation_28() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((manager(multi().rows,current()).len() as int) == 1) by (compute);
assert((manager(multi().rows,current()).len() as int) == 1); // same-version-manager-count
}
pub proof fn observation_29() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(manager(multi().rows,current())[0].version)) == 1) by (compute);
assert((rank(manager(multi().rows,current())[0].version)) == 1); // same-version-manager-first
}
pub proof fn observation_30() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(display(normal().rows,false,current()).len()>0)) == 1); // display-normal-row
}
pub proof fn observation_31() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(display(normal().rows,false,current())[0].version)) == 1); // display-normal-version
}
pub proof fn observation_32() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(display(normal().rows,false,current())[0].platforms==seq![current()])) == 1); // display-normal-current
}
pub proof fn observation_33() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(display(normal().rows,false,current())[0].all_tags,t2()))) == 0) by (compute);
assert((b(has_text(display(normal().rows,false,current())[0].all_tags,t2()))) == 0); // display-normal-all-v2
}
pub proof fn observation_34() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(display(all().rows,true,current()).len()>0)) == 1); // display-all-row
}
pub proof fn observation_35() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(display(all().rows,true,current())[0].version)) == 1); // display-all-version
}
pub proof fn observation_36() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(display(all().rows,true,current())[0].platforms==seq![current()])) == 0); // display-all-current
}
pub proof fn observation_37() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(display(all().rows,true,current())[0].all_tags,t2()))) == 1); // display-all-all-v2
}
pub proof fn observation_38() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(get_all(mixed()),t1()))) == 1) by (compute);
assert((b(has_text(get_all(mixed()),t1()))) == 1); // mixed-v1-current
}
pub proof fn observation_39() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(get_all(mixed()),t2()))) == 1); // mixed-v2-bad
}
pub proof fn observation_40() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(aggregate(get_all(mixed()))[0].encoded==v2())) == 1) by (compute);
assert((b(aggregate(get_all(mixed()))[0].encoded==v2())) == 1); // mixed-sorted-v2-first
}
pub proof fn observation_41() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(aggregate(get_all(mixed()))[1].platforms,current()))) == 1) by (compute);
assert((b(has_text(aggregate(get_all(mixed()))[1].platforms,current()))) == 1); // mixed-v1-has-current
}
pub proof fn observation_42() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(aggregate(get_all(mixed()))[1].platforms,bad()))) == 0) by (compute);
assert((b(has_text(aggregate(get_all(mixed()))[1].platforms,bad()))) == 0); // mixed-v1-has-bad
}
pub proof fn observation_43() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(get_all(triple()),t1()))) == 1) by (compute);
assert((b(has_text(get_all(triple()),t1()))) == 1); // same-version-v1-current
}
pub proof fn observation_44() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(get_all(triple()),t2()))) == 1) by (compute);
assert((b(has_text(get_all(triple()),t2()))) == 1); // same-version-v2-bad
}
pub proof fn observation_45() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(aggregate(get_all(triple()))[0].encoded==v2())) == 1) by (compute);
assert((b(aggregate(get_all(triple()))[0].encoded==v2())) == 1); // same-version-sorted-v2-first
}
pub proof fn observation_46() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(aggregate(get_all(triple()))[1].platforms,current()))) == 1) by (compute);
assert((b(has_text(aggregate(get_all(triple()))[1].platforms,current()))) == 1); // same-version-v1-has-current
}
pub proof fn observation_47() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(has_text(aggregate(get_all(triple()))[1].platforms,bad()))) == 1) by (compute);
assert((b(has_text(aggregate(get_all(triple()))[1].platforms,bad()))) == 1); // same-version-v1-has-bad
}
pub proof fn observation_48() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(manager(dependencies().rows,current())[0].version)) == 1) by (compute);
assert((rank(manager(dependencies().rows,current())[0].version)) == 1); // installed-version
}
pub proof fn observation_49() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((payload_value(standalone())) == 1) by (compute);
assert((payload_value(standalone())) == 1); // installed-payload
}
pub proof fn observation_50() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(manager(dependencies().rows,current())[0].version)) == 1) by (compute);
assert((rank(manager(dependencies().rows,current())[0].version)) == 1); // range-dependency
}
pub proof fn observation_51() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((payload_value(ranged())) == 1); // range-payload
}
pub proof fn observation_52() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(ranged().consumer)) == 1); // range-row
}
pub proof fn observation_53() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(ranged().ok)) == 1); // range-load
}
pub proof fn observation_54() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(pinned().ok)) == 0); // pinned-load
}
pub proof fn observation_55() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(pinned().dependency)) == 0); // pinned-dependency
}
pub proof fn observation_56() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((payload_value(pinned())) == 0); // pinned-payload
}
pub proof fn observation_57() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(pinned().consumer)) == 0); // pinned-row
}
pub proof fn observation_58() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((rank(manager(multi().rows,current())[0].version)) == 1) by (compute);
assert((rank(manager(multi().rows,current())[0].version)) == 1); // same-version-record
}
pub proof fn observation_59() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((payload_value(normal_consumer(triple(),Range::Any,current(),entries()))) == 1) by (compute);
assert((payload_value(normal_consumer(triple(),Range::Any,current(),entries()))) == 1); // same-version-payload
}
pub proof fn observation_60() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_fields(deep_a()).major==deep_fields(deep_z()).major && deep_fields(deep_a()).minor==deep_fields(deep_z()).minor && deep_fields(deep_a()).patch==deep_fields(deep_z()).patch && deep_fields(deep_a()).build==deep_fields(deep_z()).build)) == 1); // deep-build-encoded-fields-equal
}
pub proof fn observation_61() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_oci().keys[0]==joined(deep_z(),current()))) == 1); // deep-build-tags-z-first
}
pub proof fn observation_62() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_aggregate().keys[0]==deep_z())) == 0); // deep-build-aggregate-z-first
}
pub proof fn observation_63() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_aggregate().keys[0]==deep_a())) == 1); // deep-build-aggregate-a-first
}
pub proof fn observation_64() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_service_first_version()==deep_decoded_a())) == 1); // build-service-normal-a
}
pub proof fn observation_65() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((deep_dependency_versions().len() as int) == 2); // build-service-deps-count
}
pub proof fn observation_66() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_dependency_versions()[0]==deep_decoded_a())) == 1); // build-service-deps-a-first
}
pub proof fn observation_67() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_dependency_versions()[deep_dependency_versions().len()-1]==deep_decoded_z())) == 1); // build-service-deps-z-last
}
pub proof fn observation_68() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((if deep_manager_version().is_some(){1int}else{0int}) == 1); // build-manager-count
}
pub proof fn observation_69() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_manager_version()==Some(deep_decoded_z()))) == 1); // build-manager-z
}
pub proof fn observation_70() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_service_first_version()==deep_decoded_a())) == 1); // build-display-a
}
pub proof fn observation_71() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_service_first_version()==deep_decoded_z())) == 0); // build-display-z
}
pub proof fn observation_72() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_manager_version()==Some(deep_decoded_z()))) == 1); // build-installed-z
}
pub proof fn observation_73() {
raw_neighbors();selected_neighbors();deep_build_neighbors();
assert((b(deep_manager_version()==Some(deep_decoded_a()))) == 0); // build-installed-a
}
}
fn main() {}
