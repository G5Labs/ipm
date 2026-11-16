#[path="../../../../Verus_verification/consolidated/iter10/all.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::resolve(set![71int],true).ok)) == 1); // resolve-ok
}
pub proof fn observation_1() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::resolve(set![71int],true).children.contains(71))) == 2); // resolve-child
}
pub proof fn observation_2() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::resolve(set![71int],false).ok)) == 0); // resolve-fail
}
pub proof fn observation_3() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::resolve(set![71int],false).children.is_empty())) == 1); // resolve-empty
}
pub proof fn observation_4() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::package(error::before(),set![71int],false).ok)) == 0); // error-package
}
pub proof fn observation_5() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::uninstall(error::before(),set![71int],false).ok)) == 1); // error-uninstall
}
pub proof fn observation_6() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::uninstall(error::before(),set![71int],false).state.module)) == 0); // error-module
}
pub proof fn observation_7() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::uninstall(error::before(),set![71int],false).state.manifest)) == 0); // error-manifest
}
pub proof fn observation_8() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::uninstall(error::before(),set![71int],false).state.code.contains(71))) == 1); // error-code
}
pub proof fn observation_9() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::package(error::before(),set![71int],true).ok)) == 1); // valid-package
}
pub proof fn observation_10() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::uninstall(error::before(),set![71int],true).ok)) == 1); // valid-uninstall
}
pub proof fn observation_11() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::uninstall(error::before(),set![71int],true).state.module)) == 0); // valid-module
}
pub proof fn observation_12() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((error::b(error::uninstall(error::before(),set![71int],true).state.code.contains(71))) == 0); // valid-code
}
pub proof fn observation_13() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(unit_children(fixture(Phase::Test)).paths.contains(fixture(Phase::Test).name))) == 0); // ut-direct-path
}
pub proof fn observation_14() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(unit_children(fixture(Phase::Test)).metadata)) == 1); // ut-direct-scope
}
pub proof fn observation_15() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(references(split_scope(),seq![Phase::Test]).paths.contains(split_scope().name))) == 0); // ut-filter-resource
}
pub proof fn observation_16() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(references(split_scope(),seq![Phase::Verify]).paths.contains(split_scope().name))) == 0); // ut-filter-returned
}
pub proof fn observation_17() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(references(split_scope(),seq![Phase::Test,Phase::Verify]).paths.contains(split_scope().name))) == 1); // ut-filter-both
}
pub proof fn observation_18() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(references(split_scope(),seq![Phase::Test,Phase::Verify]).metadata)) == 0); // ut-meta-leak
}
pub proof fn observation_19() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(path_has_processor())) == 0); // ut-path-processor
}
pub proof fn observation_20() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(test_path_export_handled())) == 0); // ut-path-handled
}
pub proof fn observation_21() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(0).stored)) == 0); // pkg-default-stored
}
pub proof fn observation_22() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(0).contents.contains_key(filename()))) == 0); // pkg-default-test
}
pub proof fn observation_23() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(0).contents.contains_key(filename()) && matrix(0).contents[filename()]==source()[filename()])) == 0); // pkg-default-bytes
}
pub proof fn observation_24() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(0).contents.contains_key(manifest_path()))) == 1); // pkg-default-manifest
}
pub proof fn observation_25() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(1).stored)) == 0); // pkg-flag-stored
}
pub proof fn observation_26() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(1).contents.contains_key(filename()))) == 1); // pkg-flag-test
}
pub proof fn observation_27() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(1).contents.contains_key(filename()) && matrix(1).contents[filename()]==source()[filename()])) == 1); // pkg-flag-bytes
}
pub proof fn observation_28() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(1).contents.contains_key(manifest_path()))) == 1); // pkg-flag-manifest
}
pub proof fn observation_29() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(2).stored)) == 1); // pkg-manifest-stored
}
pub proof fn observation_30() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(2).contents.contains_key(filename()))) == 1); // pkg-manifest-test
}
pub proof fn observation_31() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(2).contents.contains_key(filename()) && matrix(2).contents[filename()]==source()[filename()])) == 1); // pkg-manifest-bytes
}
pub proof fn observation_32() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(2).contents.contains_key(manifest_path()))) == 1); // pkg-manifest-manifest
}
pub proof fn observation_33() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(3).stored)) == 1); // pkg-zero-stored
}
pub proof fn observation_34() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(3).contents.contains_key(filename()))) == 0); // pkg-zero-test
}
pub proof fn observation_35() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(3).contents.contains_key(filename()) && matrix(3).contents[filename()]==source()[filename()])) == 0); // pkg-zero-bytes
}
pub proof fn observation_36() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(3).contents.contains_key(manifest_path()))) == 1); // pkg-zero-manifest
}
pub proof fn observation_37() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(4).stored)) == 1); // pkg-sticky-stored
}
pub proof fn observation_38() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(4).contents.contains_key(filename()))) == 1); // pkg-sticky-test
}
pub proof fn observation_39() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(4).contents.contains_key(filename()) && matrix(4).contents[filename()]==source()[filename()])) == 1); // pkg-sticky-bytes
}
pub proof fn observation_40() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(4).contents.contains_key(manifest_path()))) == 1); // pkg-sticky-manifest
}
pub proof fn observation_41() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(5).stored)) == 0); // pkg-verify-flag-stored
}
pub proof fn observation_42() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(5).contents.contains_key(filename()))) == 1); // pkg-verify-flag-test
}
pub proof fn observation_43() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(5).contents.contains_key(filename()) && matrix(5).contents[filename()]==source()[filename()])) == 1); // pkg-verify-flag-bytes
}
pub proof fn observation_44() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(5).contents.contains_key(manifest_path()))) == 1); // pkg-verify-flag-manifest
}
pub proof fn observation_45() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(6).stored)) == 0); // pkg-verify-default-stored
}
pub proof fn observation_46() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(6).contents.contains_key(filename()))) == 0); // pkg-verify-default-test
}
pub proof fn observation_47() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(6).contents.contains_key(filename()) && matrix(6).contents[filename()]==source()[filename()])) == 0); // pkg-verify-default-bytes
}
pub proof fn observation_48() {
assert(filename()!=manifest_path()) by (compute);
assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
assert(!has_scope(seq![Phase::Package],seq![Phase::Verify]));
assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(references(fixture(Phase::Verify),seq![Phase::Package]).paths==Set::<Text>::empty());
assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
assert(copied(fixture(Phase::Verify),seq![Phase::Package],source())==Map::<Text,Text>::empty());
reveal_with_fuel(matrix_prior,10);
let p=export_phases(seq![Phase::Package],Some(true),false);
assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
let q=export_phases(seq![Phase::Package],None,true);
assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
assert((b(matrix(6).contents.contains_key(manifest_path()))) == 1); // pkg-verify-default-manifest
}
}
fn main() {}
