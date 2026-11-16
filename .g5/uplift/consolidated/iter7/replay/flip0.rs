#[path="../../../../Verus_verification/consolidated/iter7/inputs.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
reveal_with_fuel(split,6);
assert((split(seq![97int,44,98,124,99],delimiter(None)).len() as int) == 3); // comma-count
}
pub proof fn observation_1() {
reveal_with_fuel(split,6);
assert((split(seq![97int,44,98,124,99],delimiter(None))[0].len() as int) == 1); // comma-first-length
}
pub proof fn observation_2() {
reveal_with_fuel(split,6);
assert((split(seq![97int,44,98,124,99],delimiter(Some(124))).len() as int) == 2); // pipe-count
}
pub proof fn observation_3() {
reveal_with_fuel(split,6);
assert((split(seq![97int,44,98,124,99],delimiter(Some(124)))[0].len() as int) == 3); // pipe-first-length
}
pub proof fn observation_4() {
assert(csp_path(None,false,seq![1int,2],seq![3int]).unwrap() =~= seq![1int,2,3]);
assert((b(csp_path(None,false,seq![1int,2],seq![3int]).unwrap()==seq![1int,2,3])) == 1); // csp-default
}
pub proof fn observation_5() {
assert((b(csp_directory(None,Some(1),10)==-1)) == 1); // csp-sentinel
}
pub proof fn observation_6() {
assert((b(csp_after_before(csp_directory(None,Some(1),10))==None)) == 1); // csp-final-empty
}
pub proof fn observation_7() {
assert((b(detail(Some(1),Some(2),Some(3),Some(4))==Detail::Dispatch(1))) == 1); // detail-dispatch
}
pub proof fn observation_8() {
assert((b(detail(None,Some(2),Some(3),Some(4))==Detail::Wsgi(2))) == 1); // detail-wsgi
}
pub proof fn observation_9() {
assert((b(detail(None,None,Some(3),Some(4))==Detail::Files(3))) == 1); // detail-path
}
pub proof fn observation_10() {
assert((b(detail(None,None,None,Some(4))==Detail::Files(4))) == 1); // detail-directory
}
pub proof fn observation_11() {
assert((b(app_status(map![1int=>10int],map![1int=>10int],false))) == 1); // app-equal-ok
}
pub proof fn observation_12() {
change_witness(map![1int=>10int],map![1int=>20int],1);
assert((b(!app_status(map![1int=>10int],map![1int=>20int],false))) == 1); // app-changed-fails
}
pub proof fn observation_13() {
reveal_with_fuel(split,6);reveal_with_fuel(cors_calls,6);reveal_with_fuel(cors_ok,6);
assert((b(cors_ok(split(seq![1int,44,2],44),|o:Seq<int>| true))) == 1); // cors-valid
}
pub proof fn observation_14() {
reveal_with_fuel(split,6);reveal_with_fuel(cors_calls,6);reveal_with_fuel(cors_ok,6);
assert((b(cors_ok(split(seq![1int,44,2,44,3],44),|o:Seq<int>| o.len()!=1 || o[0]!=2))) == 0); // cors-invalid
}
pub proof fn observation_15() {
assert((b(!stamp(42,seq![],true,true).ok)) == 1); // hash-empty-fails
}
pub proof fn observation_16() {
assert((b(stamp(42,seq![],true,true).value==42)) == 1); // hash-old-kept
}
pub proof fn observation_17() {
assert((b(stamp(42,seq![99int],true,true).value==99)) == 1); // hash-new
}
pub proof fn observation_18() {
assert((b(reload(0,false,10,true).env_called)) == 1); // reload-nondev-env
}
pub proof fn observation_19() {
assert((b(reload_status(0,false,10,true,false))) == 1); // reload-nondev-ok
}
pub proof fn observation_20() {
assert((b(!reload_status(0,true,10,true,false))) == 1); // reload-dev-fails
}
pub proof fn observation_21() {
assert((b(config_get_called(set![1int],1))) == 1); // config-known-reads
}
pub proof fn observation_22() {
assert((b(config_get_called(set![1int],2))) == 0); // config-unknown-extra-reads
}
pub proof fn observation_23() {
reveal_with_fuel(replacement,8);reveal_with_fuel(replace,8);reveal_with_fuel(sweep,8);
assert((b(replacement(seq![123int,36,66,125],seq![123int,36,65,125])==None)) == 1); // param-forward-fails
}
pub proof fn observation_24() {
reveal_with_fuel(replacement,8);reveal_with_fuel(replace,8);reveal_with_fuel(sweep,8);
assert((b(replacement(seq![123int,36,65,125],seq![123int,36,66,125])==None)) == 1); // param-backward-fails
}
pub proof fn observation_25() {
reveal_with_fuel(replacement,8);reveal_with_fuel(replace,8);reveal_with_fuel(sweep,8);
assert((b(sweep(seq![Token::Parameter(1),Token::Literal(47),Token::Parameter(2)],seq![1int,2],map![1int=>seq![Token::Literal(120)],2int=>seq![Token::Literal(121)]]).len()==3)) == 1); // param-ordinary
}
pub proof fn observation_26() {
reveal_with_fuel(split,6);
let parts=split(seq![67int,83,80,46,84],46);
assert(parts.len()==2);
assert(parts[0] =~= seq![67int,83,80]);
assert(parts[1] =~= seq![84int]);
assert((system_value(9,seq![67int,83,80,46,84],0,123)) == 123); // system-value
}
pub proof fn observation_27() {
reveal_with_fuel(split,6);
let parts=split(seq![67int,83,80,46,84],46);
assert(parts.len()==2);
assert(parts[0] =~= seq![67int,83,80]);
assert(parts[1] =~= seq![84int]);
assert((system_value(system_value(9,seq![67int,83,80,46,84],0,123),seq![67int,83,80,46,84],1,0)) == 123); // system-value-after
}
}
fn main() {}
