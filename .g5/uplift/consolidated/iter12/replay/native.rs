#[path="../../../../Verus_verification/consolidated/iter12/all.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),false,true,false).ok)) == 0); // repos-without-map
}
pub proof fn observation_1() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),true,true,true).ok)) == 1); // map-preview
}
pub proof fn observation_2() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),true,true,false).ok)) == 1); // map-first
}
pub proof fn observation_3() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::first(),true,true,false).ok)) == 1); // map-second
}
pub proof fn observation_4() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::unmap_request(ns::second(),true).ok)) == 1); // repos-unmap
}
pub proof fn observation_5() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::unmap_request(ns::third(),false).ok)) == 1); // full-unmap
}
pub proof fn observation_6() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::requirement(ns::first(),1,1))) == 1); // local-requirement
}
pub proof fn observation_7() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::first(),1,1).ok)) == 1); // local-load
}
pub proof fn observation_8() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::first(),1,1).row)) == 1); // local-row
}
pub proof fn observation_9() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::has_ipm(ns::first(),2))) == 1); // mapped-shell
}
pub proof fn observation_10() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::requirement(ns::first(),2,1))) == 0); // mapped-requirement
}
pub proof fn observation_11() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::first(),2,1).ok)) == 0); // mapped-load
}
pub proof fn observation_12() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::first(),2,1).row)) == 0); // mapped-row
}
pub proof fn observation_13() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::has_ipm(ns::unmapped(),2))) == 1); // after-unmap-shell
}
pub proof fn observation_14() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::requirement(ns::unmapped(),2,1))) == 0); // after-unmap-requirement
}
pub proof fn observation_15() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::unmapped(),2,1).ok)) == 0); // after-unmap-load
}
pub proof fn observation_16() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::unmapped(),2,1).row)) == 0); // after-unmap-row
}
pub proof fn observation_17() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::w4().ns[2].cfg.pkg.is_some())) == 0); // empty-pkg
}
pub proof fn observation_18() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::w4().ns[2].cfg.rtn.is_some())) == 0); // empty-rtn
}
pub proof fn observation_19() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::w4().ns[2].cfg.repo.is_some())) == 0); // empty-repo
}
pub proof fn observation_20() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::pkg_dest(ns::w4().ns[2].act)==1)) == 0); // empty-active-source
}
pub proof fn observation_21() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),false,true,false).state.ns[2].cfg.pkg.is_some())) == 0); // invalid-pkg
}
pub proof fn observation_22() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),false,true,false).state.ns[2].cfg.rtn.is_some())) == 0); // invalid-rtn
}
pub proof fn observation_23() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),false,true,false).state.ns[2].cfg.repo.is_some())) == 0); // invalid-repo
}
pub proof fn observation_24() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::pkg_dest(ns::request(ns::w4(),false,true,false).state.ns[2].act)==1)) == 0); // invalid-active-source
}
pub proof fn observation_25() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),true,true,true).state.ns[2].cfg.pkg.is_some())) == 0); // preview-pkg
}
pub proof fn observation_26() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),true,true,true).state.ns[2].cfg.rtn.is_some())) == 0); // preview-rtn
}
pub proof fn observation_27() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),true,true,true).state.ns[2].cfg.repo.is_some())) == 0); // preview-repo
}
pub proof fn observation_28() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::pkg_dest(ns::request(ns::w4(),true,true,true).state.ns[2].act)==1)) == 0); // preview-active-source
}
pub proof fn observation_29() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::first().ns[2].cfg.pkg.is_some())) == 1); // first-pkg
}
pub proof fn observation_30() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::first().ns[2].cfg.rtn.is_some())) == 1); // first-rtn
}
pub proof fn observation_31() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::first().ns[2].cfg.repo.is_some())) == 0); // first-repo
}
pub proof fn observation_32() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::pkg_dest(ns::first().ns[2].act)==1)) == 1); // first-active-source
}
pub proof fn observation_33() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::second().ns[2].cfg.pkg.is_some())) == 1); // second-pkg
}
pub proof fn observation_34() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::second().ns[2].cfg.rtn.is_some())) == 1); // second-rtn
}
pub proof fn observation_35() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::second().ns[2].cfg.repo.is_some())) == 1); // second-repo
}
pub proof fn observation_36() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::pkg_dest(ns::second().ns[2].act)==1)) == 1); // second-active-source
}
pub proof fn observation_37() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repos_only().ns[2].cfg.pkg.is_some())) == 1); // repos-only-pkg
}
pub proof fn observation_38() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repos_only().ns[2].cfg.rtn.is_some())) == 1); // repos-only-rtn
}
pub proof fn observation_39() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repos_only().ns[2].cfg.repo.is_some())) == 0); // repos-only-repo
}
pub proof fn observation_40() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::pkg_dest(ns::repos_only().ns[2].act)==1)) == 1); // repos-only-active-source
}
pub proof fn observation_41() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::unmapped().ns[2].cfg.pkg.is_some())) == 0); // unmapped-pkg
}
pub proof fn observation_42() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::unmapped().ns[2].cfg.rtn.is_some())) == 1); // unmapped-rtn
}
pub proof fn observation_43() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::unmapped().ns[2].cfg.repo.is_some())) == 0); // unmapped-repo
}
pub proof fn observation_44() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::pkg_dest(ns::unmapped().ns[2].act)==1)) == 1); // unmapped-active-source
}
pub proof fn observation_45() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::w4().ns[2].cfg.repo.is_some())) == 0); // empty-repo-s
}
pub proof fn observation_46() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::w4().ns[2].cfg.repo.is_some())) == 0); // empty-repo-i
}
pub proof fn observation_47() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),false,true,false).state.ns[2].cfg.repo.is_some())) == 0); // invalid-repo-s
}
pub proof fn observation_48() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),false,true,false).state.ns[2].cfg.repo.is_some())) == 0); // invalid-repo-i
}
pub proof fn observation_49() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),true,true,true).state.ns[2].cfg.repo.is_some())) == 0); // preview-repo-s
}
pub proof fn observation_50() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::request(ns::w4(),true,true,true).state.ns[2].cfg.repo.is_some())) == 0); // preview-repo-i
}
pub proof fn observation_51() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::first().ns[2].cfg.repo.is_some())) == 0); // first-repo-s
}
pub proof fn observation_52() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::first().ns[2].cfg.repo.is_some())) == 0); // first-repo-i
}
pub proof fn observation_53() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::second().ns[2].cfg.repo.is_some())) == 1); // second-repo-s
}
pub proof fn observation_54() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::second().ns[2].cfg.repo.is_some())) == 1); // second-repo-i
}
pub proof fn observation_55() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repos_only().ns[2].cfg.repo.is_some())) == 0); // repos-only-repo-s
}
pub proof fn observation_56() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repos_only().ns[2].cfg.repo.is_some())) == 0); // repos-only-repo-i
}
pub proof fn observation_57() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::unmapped().ns[2].cfg.repo.is_some())) == 0); // unmapped-repo-s
}
pub proof fn observation_58() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::unmapped().ns[2].cfg.repo.is_some())) == 0); // unmapped-repo-i
}
pub proof fn observation_59() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repo_visible(ns::first(),2))) == 0); // mapped-repo-visible
}
pub proof fn observation_60() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repo_visible(ns::second(),2))) == 1); // shared-repo-visible
}
pub proof fn observation_61() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::requirement(ns::second(),2,1))) == 0); // shared-requirement
}
pub proof fn observation_62() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::second(),2,1).ok)) == 0); // shared-load
}
pub proof fn observation_63() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::second(),2,1).row)) == 0); // shared-row
}
pub proof fn observation_64() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::has_ipm(ns::second(),2))) == 1); // shared-shell
}
pub proof fn observation_65() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repo_visible(ns::repos_only(),2))) == 0); // repos-removed-repo-visible
}
pub proof fn observation_66() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::requirement(ns::repos_only(),2,1))) == 0); // repos-removed-requirement
}
pub proof fn observation_67() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::repos_only(),2,1).ok)) == 0); // repos-removed-load
}
pub proof fn observation_68() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::repos_only(),2,1).row)) == 0); // repos-removed-row
}
pub proof fn observation_69() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::has_ipm(ns::repos_only(),2))) == 1); // repos-removed-shell
}
pub proof fn observation_70() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repo_visible(ns::unmapped(),2))) == 0); // after-unmap-repo-visible
}
pub proof fn observation_71() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repo_visible(ns::own_target(),2))) == 0); // own-created-repo-visible
}
pub proof fn observation_72() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_visible(ns::own_target(),2))) == 1); // own-created-own-visible
}
pub proof fn observation_73() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::requirement(ns::own_target(),2,1))) == 0); // own-created-requirement
}
pub proof fn observation_74() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::own_target(),2,1).ok)) == 0); // own-created-load
}
pub proof fn observation_75() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::own_target(),2,1).row)) == 0); // own-created-row
}
pub proof fn observation_76() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::has_ipm(ns::own_target(),2))) == 1); // own-created-shell
}
pub proof fn observation_77() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repo_visible(ns::own_skip(),2))) == 0); // own-skip-repo-visible
}
pub proof fn observation_78() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_visible(ns::own_skip(),2))) == 1); // own-skip-own-visible
}
pub proof fn observation_79() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::requirement(ns::own_skip(),2,1))) == 0); // own-skip-requirement
}
pub proof fn observation_80() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::own_skip(),2,1).ok)) == 0); // own-skip-load
}
pub proof fn observation_81() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::own_skip(),2,1).row)) == 0); // own-skip-row
}
pub proof fn observation_82() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::has_ipm(ns::own_skip(),2))) == 1); // own-skip-shell
}
pub proof fn observation_83() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::repo_visible(ns::own_cleared(),2))) == 0); // own-cleared-repo-visible
}
pub proof fn observation_84() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_visible(ns::own_cleared(),2))) == 0); // own-cleared-own-visible
}
pub proof fn observation_85() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::requirement(ns::own_cleared(),2,1))) == 0); // own-cleared-requirement
}
pub proof fn observation_86() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::own_cleared(),2,1).ok)) == 0); // own-cleared-load
}
pub proof fn observation_87() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::new_normal_load(ns::own_cleared(),2,1).row)) == 0); // own-cleared-row
}
pub proof fn observation_88() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::has_ipm(ns::own_cleared(),2))) == 1); // own-cleared-shell
}
pub proof fn observation_89() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_visible(ns::first(),2))) == 0); // mapped-own-visible
}
pub proof fn observation_90() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_visible(ns::second(),2))) == 0); // shared-own-visible
}
pub proof fn observation_91() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_visible(ns::repos_only(),2))) == 0); // repos-removed-own-visible
}
pub proof fn observation_92() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_visible(ns::unmapped(),2))) == 0); // after-unmap-own-visible
}
pub proof fn observation_93() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_skip().ns[2].cfg.pkg.is_some())) == 1); // own-skip-pkg
}
pub proof fn observation_94() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_skip().ns[2].cfg.rtn.is_some())) == 1); // own-skip-rtn
}
pub proof fn observation_95() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_skip().ns[2].cfg.repo.is_some())) == 0); // own-skip-repo
}
pub proof fn observation_96() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_skip().ns[2].cfg.repo.is_some())) == 0); // own-skip-repo-s
}
pub proof fn observation_97() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::own_skip().ns[2].cfg.repo.is_some())) == 0); // own-skip-repo-i
}
pub proof fn observation_98() {
ns::nsp_witness_repos_second_run();
ns::selected_namespace_neighbors();
assert((ns::b(ns::pkg_dest(ns::own_skip().ns[2].act)==1)) == 1); // own-skip-active-source
}
pub proof fn observation_99() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((k_doc().dependencies.dom().len() as int) == 4); // lf-k-count
}
pub proof fn observation_100() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(json_keys(seq![1int,2,3,5],k_doc())==seq![1int,2,3,5])) == 1); // lf-k-order
}
pub proof fn observation_101() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(k_doc().dependencies[1].version==installed()[1].version && k_doc().dependencies[3].version==installed()[3].version)) == 1); // lf-k-versions
}
pub proof fn observation_102() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((k_doc().repositories.dom().len() as int) == 2); // lf-k-repos
}
pub proof fn observation_103() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(k_doc().dependencies[3].requirements[1]==11)) == 1); // lf-k-requirement
}
pub proof fn observation_104() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(k_doc().repositories[10][Field::Root]==100)) == 1); // lf-k-base-root
}
pub proof fn observation_105() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(flatten(k_rows(),32).names==seq![1int,2,3,5])) == 1); // lf-k-flat
}
pub proof fn observation_106() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((flatten(k_rows(),32).names.len() as int) == 4); // lf-k-flat-count
}
pub proof fn observation_107() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((duplicate_doc().dependencies.dom().len() as int) == 4); // lf-duplicate-count
}
pub proof fn observation_108() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(json_keys(seq![1int,2,3,5],duplicate_doc())==seq![1int,2,3,5])) == 1); // lf-duplicate-order
}
pub proof fn observation_109() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(duplicate_doc().dependencies[1].version==installed()[1].version && duplicate_doc().dependencies[3].version==installed()[3].version)) == 1); // lf-duplicate-versions
}
pub proof fn observation_110() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((duplicate_doc().repositories.dom().len() as int) == 2); // lf-duplicate-repos
}
pub proof fn observation_111() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(duplicate_doc().dependencies[1].version==1)) == 1); // lf-duplicate-installed
}
pub proof fn observation_112() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((l_doc().dependencies.dom().len() as int) == 6); // lf-l-count
}
pub proof fn observation_113() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(json_keys(seq![1int,2,3,4,5,6],l_doc())==seq![1int,2,3,4,5,6])) == 1); // lf-l-order
}
pub proof fn observation_114() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(l_doc().dependencies[1].version==installed()[1].version && l_doc().dependencies[6].version==installed()[6].version)) == 1); // lf-l-versions
}
pub proof fn observation_115() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((l_doc().repositories.dom().len() as int) == 2); // lf-l-repos
}
pub proof fn observation_116() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6])) == 1); // lf-l-flat
}
pub proof fn observation_117() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((flatten(l_rows(),64).names.len() as int) == 6); // lf-l-flat-count
}
pub proof fn observation_118() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((reverse_doc().dependencies.dom().len() as int) == 2); // lf-reverse-count
}
pub proof fn observation_119() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(json_keys(seq![1int,2],reverse_doc())==seq![1int,2])) == 1); // lf-reverse-order
}
pub proof fn observation_120() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(reverse_doc().dependencies[1].version==reverse_store()[1].version && reverse_doc().dependencies[2].version==reverse_store()[2].version)) == 1); // lf-reverse-versions
}
pub proof fn observation_121() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((reverse_doc().repositories.dom().len() as int) == 1); // lf-reverse-repos
}
pub proof fn observation_122() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(json_keys(seq![1int,2],reverse_doc())==flatten(reverse_rows(),8).names)) == 0); // lf-reverse-topological
}
pub proof fn observation_123() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(flatten(reverse_rows(),8).names==seq![2int,1])) == 1); // lf-reverse-flat
}
pub proof fn observation_124() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((flatten(reverse_rows(),8).names.len() as int) == 2); // lf-reverse-flat-count
}
pub proof fn observation_125() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(flatten(cursor_rows(),24).names==seq![2int,3,4,1])) == 1); // lf-cursor
}
pub proof fn observation_126() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(flatten(cursor_rows(),24).remaining.len()==0)) == 1); // lf-cursor-consumed
}
pub proof fn observation_127() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((credential_doc().repositories.dom().len() as int) == 1); // lf-credential-count
}
pub proof fn observation_128() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(credential_doc().repositories[40][Field::Url]==80)) == 1); // lf-credential-url
}
pub proof fn observation_129() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(credential_doc().repositories[40].contains_key(Field::Username))) == 0); // lf-credential-username
}
pub proof fn observation_130() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(credential_doc().repositories[40].contains_key(Field::Password))) == 0); // lf-credential-password
}
pub proof fn observation_131() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(credential_doc().repositories[40].contains_key(Field::Token))) == 0); // lf-credential-token
}
pub proof fn observation_132() {
reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
writer_neighbors();
assert(flatten(cursor_rows(),24).remaining.len()==0) by (compute);
assert((b(!add_repo(empty(),99,repos()).ok)) == 1); // lf-missing-repo
}
}
fn main() {}
