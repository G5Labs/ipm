#[path="../../../../Verus_verification/consolidated/iter8/all.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(generated_path(7,Some(seq![1int,2,3]),seq![1int,2])==None)) == 1); // gen-csp-only
}
pub proof fn observation_1() {
assert((b(matches!(manifest_path(seq![1int,2,3],seq![1int,2]),PathExpr::Csp(_)))) == 1); // gen-path-expression
}
pub proof fn observation_2() {
assert((b(filecopy_target(generated_path(0,Some(seq![1int,2,3]),seq![1int,2])).is_some())) == 1); // gen-copy-expression
}
pub proof fn observation_3() {
assert((generated_props(map![Key::Authentication=>1int,Key::Dispatch=>7int],map![Key::Authentication=>1int],jwt_prompt(Jwt{enable:1,access:120,refresh:1800}))[Key::JwtEnabled]) == 1); // gen-jwt-enabled
}
pub proof fn observation_4() {
assert((generated_props(map![Key::Authentication=>1int,Key::Dispatch=>7int],map![Key::Authentication=>1int],jwt_prompt(Jwt{enable:1,access:120,refresh:1800}))[Key::AccessTimeout]) == 120); // gen-jwt-access
}
pub proof fn observation_5() {
assert((generated_props(map![Key::Authentication=>1int,Key::Dispatch=>7int],map![Key::Authentication=>1int],jwt_prompt(Jwt{enable:1,access:120,refresh:1800}))[Key::RefreshTimeout]) == 1800); // gen-jwt-refresh
}
pub proof fn observation_6() {
assert(evaluate_path(manifest_path(seq![1int,2,3],seq![1int,2]),seq![1int,2]) =~= seq![1int,2,3]);
assert((b(evaluate_path(manifest_path(seq![1int,2,3],seq![1int,2]),seq![1int,2])==seq![1int,2,3])) == 1); // gen-path
}
pub proof fn observation_7() {
assert((b(filecopy_target(generated_path(0,Some(seq![1int,2,3]),seq![1int,2])).is_some())) == 1); // gen-copied-file
}
pub proof fn observation_8() {
assert((b(package_webapp(PathExpr::Csp(seq![3int]),false).is_some())) == 1); // web-publish
}
pub proof fn observation_9() {
assert((b(!uninstall(map![7int=>seq![1int,2,3]],7).contains_key(7))) == 1); // web-absent
}
pub proof fn observation_10() {
assert(package_roundtrip(map![7int=>seq![1int,2,3]],7,PathExpr::Csp(seq![3int]),seq![1int,2])[7] =~= seq![1int,2,3]);
assert((b(package_roundtrip(map![7int=>seq![1int,2,3]],7,PathExpr::Csp(seq![3int]),seq![1int,2])[7]==seq![1int,2,3])) == 1); // web-path-same
}
pub proof fn observation_11() {
sort_three();sort_two();filter_three();
assert((grouped(seq![1int,3,2],None,true).rows as int) == 1); // catalog-count
}
pub proof fn observation_12() {
sort_three();sort_two();filter_three();
assert((grouped(seq![1int,3,2],None,true).selected.unwrap()) == 3); // catalog-selected
}
pub proof fn observation_13() {
sort_three();sort_two();filter_three();
assert((b(grouped(seq![1int,3,2],None,true).versions==seq![3int,2,1])) == 1); // catalog-all
}
pub proof fn observation_14() {
sort_three();sort_two();filter_three();
assert((grouped(seq![1int,3,2],Some(3),true).rows as int) == 1); // catalog-limited-count
}
pub proof fn observation_15() {
sort_three();sort_two();filter_three();
assert((grouped(seq![1int,3,2],Some(3),true).selected.unwrap()) == 2); // catalog-limited-selected
}
pub proof fn observation_16() {
sort_three();sort_two();filter_three();
assert((b(grouped(seq![1int,3,2],Some(3),true).versions==seq![2int,1])) == 1); // catalog-limited-all
}
pub proof fn observation_17() {
assert((b(catalog_visible(Artifact{version:1,deployed:true,platforms:set![26int]},26,false))) == 1); // platform-visible
}
pub proof fn observation_18() {
assert((b(reported_platform(Artifact{version:1,deployed:true,platforms:set![26int]},26)==Some(26))) == 1); // platform-reported
}
pub proof fn observation_19() {
assert((b(install_candidate(Artifact{version:1,deployed:true,platforms:set![26int]},26))) == 1); // platform-install-compatible
}
pub proof fn observation_20() {
assert((b(catalog_visible(Artifact{version:1,deployed:true,platforms:set![9999int]},26,false))) == 0); // platform-incompatible-visible
}
pub proof fn observation_21() {
assert((b(install_candidate(Artifact{version:1,deployed:true,platforms:set![9999int]},26))) == 0); // platform-incompatible-install
}
pub proof fn observation_22() {
assert((b(wheel(2,Phase::Initialize,true,true,true,false).ok)) == 1); // wheel-load
}
pub proof fn observation_23() {
assert((b(wheel(2,Phase::Initialize,true,true,true,false).installed==Some(true))) == 1); // wheel-version
}
pub proof fn observation_24() {
assert((b(wheel(2,Phase::Clean,false,true,true,true).installed==Some(true))) == 1); // wheel-kept
}
pub proof fn observation_25() {
assert((b(wheel(2,Phase::Initialize,false,false,false,false).ok)) == 0); // wheel-missing-other
}
pub proof fn observation_26() {
assert((b(wheel(2,Phase::Initialize,false,false,false,false).handled)) == 0); // wheel-missing-other-handled
}
pub proof fn observation_27() {
assert((b(wheel(1,Phase::Initialize,false,false,false,false).ok)) == 1); // wheel-missing-ipm
}
pub proof fn observation_28() {
assert((b(wheel(1,Phase::Initialize,false,false,false,false).handled)) == 0); // wheel-missing-ipm-handled
}
pub proof fn observation_29() {
assert((b(archive_manifest_name(1,2,true)==1)) == 1); // external-manifest-internal
}
pub proof fn observation_30() {
assert((b(archive_manifest_name(1,2,true)==2)) == 0); // external-manifest-external
}
pub proof fn observation_31() {
assert(evaluate_path(PathExpr::Csp(seq![3int]),seq![1int,2])+seq![5int] =~= seq![1int,2,3,5]);
assert((b(copy_one(Map::empty(),map![5int=>77int],5,Some(PathExpr::Csp(seq![3int])),seq![1int,2])[seq![1int,2,3,5]]==77)) == 1); // gen-copied-bytes
}
}
fn main() {}
