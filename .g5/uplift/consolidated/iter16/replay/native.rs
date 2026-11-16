#[path="../../../../Verus_verification/consolidated/iter16/provenance.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(missing().ok)) == 0); // missing
}
pub proof fn observation_1() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(missing().world.module.is_none())) == 1); // missing-no-module
}
pub proof fn observation_2() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(missing().world.code.is_none())) == 1); // missing-no-code
}
pub proof fn observation_3() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a().module.is_some())) == 1); // a-module
}
pub proof fn observation_4() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_a())==alpha())) == 1); // a-supplier
}
pub proof fn observation_5() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_a()).len()==0)) == 0); // a-supplier-empty
}
pub proof fn observation_6() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_version(after_a())==version())) == 1); // a-version
}
pub proof fn observation_7() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_root(after_a())==Some(Root::A))) == 1); // a-root
}
pub proof fn observation_8() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a().code.is_some())) == 1); // a-code
}
pub proof fn observation_9() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((payload(after_a())) == 1); // a-payload
}
pub proof fn observation_10() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_a()).is_some())) == 1); // a-show-has-repository
}
pub proof fn observation_11() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_a())==Some(alpha()))) == 1); // a-show-supplier
}
pub proof fn observation_12() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_a(),alpha()))) == 1); // a-listed-alpha
}
pub proof fn observation_13() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_a(),beta()))) == 0); // a-listed-beta
}
pub proof fn observation_14() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_delete().module.is_some())) == 1); // after-delete-module
}
pub proof fn observation_15() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_alpha_delete())==alpha())) == 1); // after-delete-supplier
}
pub proof fn observation_16() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_alpha_delete()).len()==0)) == 0); // after-delete-supplier-empty
}
pub proof fn observation_17() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_version(after_alpha_delete())==version())) == 1); // after-delete-version
}
pub proof fn observation_18() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_root(after_alpha_delete())==Some(Root::A))) == 1); // after-delete-root
}
pub proof fn observation_19() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_delete().code.is_some())) == 1); // after-delete-code
}
pub proof fn observation_20() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((payload(after_alpha_delete())) == 1); // after-delete-payload
}
pub proof fn observation_21() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_alpha_delete()).is_some())) == 1); // after-delete-show-has-repository
}
pub proof fn observation_22() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_alpha_delete())==Some(alpha()))) == 1); // after-delete-show-supplier
}
pub proof fn observation_23() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_alpha_delete(),alpha()))) == 1); // after-delete-listed-alpha
}
pub proof fn observation_24() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_alpha_delete(),beta()))) == 0); // after-delete-listed-beta
}
pub proof fn observation_25() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_recreate().module.is_some())) == 1); // after-recreate-module
}
pub proof fn observation_26() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_alpha_recreate())==alpha())) == 1); // after-recreate-supplier
}
pub proof fn observation_27() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_alpha_recreate()).len()==0)) == 0); // after-recreate-supplier-empty
}
pub proof fn observation_28() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_version(after_alpha_recreate())==version())) == 1); // after-recreate-version
}
pub proof fn observation_29() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_root(after_alpha_recreate())==Some(Root::A))) == 1); // after-recreate-root
}
pub proof fn observation_30() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_recreate().code.is_some())) == 1); // after-recreate-code
}
pub proof fn observation_31() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((payload(after_alpha_recreate())) == 1); // after-recreate-payload
}
pub proof fn observation_32() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_alpha_recreate()).is_some())) == 1); // after-recreate-show-has-repository
}
pub proof fn observation_33() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_alpha_recreate())==Some(alpha()))) == 1); // after-recreate-show-supplier
}
pub proof fn observation_34() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_alpha_recreate(),alpha()))) == 1); // after-recreate-listed-alpha
}
pub proof fn observation_35() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_alpha_recreate(),beta()))) == 0); // after-recreate-listed-beta
}
pub proof fn observation_36() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().module.is_some())) == 1); // c-module
}
pub proof fn observation_37() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_c())==alpha())) == 1); // c-supplier
}
pub proof fn observation_38() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_c()).len()==0)) == 0); // c-supplier-empty
}
pub proof fn observation_39() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_version(after_c())==version())) == 1); // c-version
}
pub proof fn observation_40() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_root(after_c())==Some(Root::C))) == 1); // c-root
}
pub proof fn observation_41() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().code.is_some())) == 1); // c-code
}
pub proof fn observation_42() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((payload(after_c())) == 3); // c-payload
}
pub proof fn observation_43() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_c()).is_some())) == 1); // c-show-has-repository
}
pub proof fn observation_44() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_c())==Some(alpha()))) == 1); // c-show-supplier
}
pub proof fn observation_45() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_c(),alpha()))) == 1); // c-listed-alpha
}
pub proof fn observation_46() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_c(),beta()))) == 0); // c-listed-beta
}
pub proof fn observation_47() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_beta().module.is_some())) == 1); // b-module
}
pub proof fn observation_48() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_beta())==beta())) == 1); // b-supplier
}
pub proof fn observation_49() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_beta()).len()==0)) == 0); // b-supplier-empty
}
pub proof fn observation_50() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_version(after_beta())==version())) == 1); // b-version
}
pub proof fn observation_51() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_root(after_beta())==Some(Root::B))) == 1); // b-root
}
pub proof fn observation_52() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_beta().code.is_some())) == 1); // b-code
}
pub proof fn observation_53() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((payload(after_beta())) == 2); // b-payload
}
pub proof fn observation_54() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_beta()).is_some())) == 1); // b-show-has-repository
}
pub proof fn observation_55() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_beta())==Some(beta()))) == 1); // b-show-supplier
}
pub proof fn observation_56() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_beta(),alpha()))) == 0); // b-listed-alpha
}
pub proof fn observation_57() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_beta(),beta()))) == 1); // b-listed-beta
}
pub proof fn observation_58() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().module.is_some())) == 1); // local-module
}
pub proof fn observation_59() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_local())==seq![])) == 1); // local-supplier
}
pub proof fn observation_60() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(supplier(after_local()).len()==0)) == 1); // local-supplier-empty
}
pub proof fn observation_61() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_version(after_local())==version())) == 1); // local-version
}
pub proof fn observation_62() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(installed_root(after_local())==Some(Root::A))) == 1); // local-root
}
pub proof fn observation_63() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().code.is_some())) == 1); // local-code
}
pub proof fn observation_64() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((payload(after_local())) == 1); // local-payload
}
pub proof fn observation_65() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(show_repository(after_local()).is_some())) == 0); // local-show-has-repository
}
pub proof fn observation_66() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_local(),alpha()))) == 0); // local-listed-alpha
}
pub proof fn observation_67() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(listed_for_repository(after_local(),beta()))) == 0); // local-listed-beta
}
pub proof fn observation_68() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a().records[0].status==1)) == 1); // a-history-success
}
pub proof fn observation_69() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a().records[0].finalized)) == 1); // a-history-finalized
}
pub proof fn observation_70() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a().records[0].ended)) == 1); // a-history-ended
}
pub proof fn observation_71() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a().records[0].committed)) == 1); // a-history-committed
}
pub proof fn observation_72() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a().records[0].version==version())) == 1); // a-history-version
}
pub proof fn observation_73() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a().records[0].source==alpha())) == 1); // a-history-name
}
pub proof fn observation_74() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a().records[0].moniker==filesystem())) == 1); // a-history-type
}
pub proof fn observation_75() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a().records[0].details==details(Root::A))) == 1); // a-history-details
}
pub proof fn observation_76() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(history_has_supplier(after_a(),0,alpha(),Root::A))) == 1); // a-history-reader
}
pub proof fn observation_77() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[1].status==1)) == 1); // c-history-success
}
pub proof fn observation_78() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[1].finalized)) == 1); // c-history-finalized
}
pub proof fn observation_79() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[1].ended)) == 1); // c-history-ended
}
pub proof fn observation_80() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[1].committed)) == 1); // c-history-committed
}
pub proof fn observation_81() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[1].version==version())) == 1); // c-history-version
}
pub proof fn observation_82() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[1].source==alpha())) == 1); // c-history-name
}
pub proof fn observation_83() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[1].moniker==filesystem())) == 1); // c-history-type
}
pub proof fn observation_84() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[1].details==details(Root::C))) == 1); // c-history-details
}
pub proof fn observation_85() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(history_has_supplier(after_c(),1,alpha(),Root::C))) == 1); // c-history-reader
}
pub proof fn observation_86() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_beta().records[2].status==1)) == 1); // b-history-success
}
pub proof fn observation_87() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_beta().records[2].finalized)) == 1); // b-history-finalized
}
pub proof fn observation_88() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_beta().records[2].ended)) == 1); // b-history-ended
}
pub proof fn observation_89() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_beta().records[2].committed)) == 1); // b-history-committed
}
pub proof fn observation_90() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_beta().records[2].version==version())) == 1); // b-history-version
}
pub proof fn observation_91() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_beta().records[2].source==beta())) == 1); // b-history-name
}
pub proof fn observation_92() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_beta().records[2].moniker==filesystem())) == 1); // b-history-type
}
pub proof fn observation_93() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_beta().records[2].details==details(Root::B))) == 1); // b-history-details
}
pub proof fn observation_94() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(history_has_supplier(after_beta(),2,beta(),Root::B))) == 1); // b-history-reader
}
pub proof fn observation_95() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[3].status==1)) == 1); // local-history-success
}
pub proof fn observation_96() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[3].finalized)) == 1); // local-history-finalized
}
pub proof fn observation_97() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[3].ended)) == 1); // local-history-ended
}
pub proof fn observation_98() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[3].committed)) == 1); // local-history-committed
}
pub proof fn observation_99() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[3].version==version())) == 1); // local-history-version
}
pub proof fn observation_100() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[3].source.len()==0)) == 1); // local-history-name-empty
}
pub proof fn observation_101() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[3].moniker.len()==0)) == 1); // local-history-type-empty
}
pub proof fn observation_102() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[3].details.len()==0)) == 1); // local-history-details-empty
}
pub proof fn observation_103() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(history_detail(after_local(),3)==Some(SourceView::NotApplicable))) == 1); // local-history-reader-na
}
pub proof fn observation_104() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_delete().records.contains_key(0))) == 1); // after-delete-row
}
pub proof fn observation_105() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_delete().records[0].source==alpha())) == 1); // after-delete-name
}
pub proof fn observation_106() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_delete().records[0].moniker==filesystem())) == 1); // after-delete-type
}
pub proof fn observation_107() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_delete().records[0].details==details(Root::A))) == 1); // after-delete-details
}
pub proof fn observation_108() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(history_has_supplier(after_alpha_delete(),0,alpha(),Root::A))) == 1); // after-delete-history-reader
}
pub proof fn observation_109() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_recreate().records.contains_key(0))) == 1); // after-recreate-row
}
pub proof fn observation_110() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_recreate().records[0].source==alpha())) == 1); // after-recreate-name
}
pub proof fn observation_111() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_recreate().records[0].moniker==filesystem())) == 1); // after-recreate-type
}
pub proof fn observation_112() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_recreate().records[0].details==details(Root::A))) == 1); // after-recreate-details
}
pub proof fn observation_113() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(history_has_supplier(after_alpha_recreate(),0,alpha(),Root::A))) == 1); // after-recreate-history-reader
}
pub proof fn observation_114() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records.contains_key(0))) == 1); // old-a-after-c-row
}
pub proof fn observation_115() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[0].source==alpha())) == 1); // old-a-after-c-name
}
pub proof fn observation_116() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[0].moniker==filesystem())) == 1); // old-a-after-c-type
}
pub proof fn observation_117() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c().records[0].details==details(Root::A))) == 1); // old-a-after-c-details
}
pub proof fn observation_118() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(history_has_supplier(after_c(),0,alpha(),Root::A))) == 1); // old-a-after-c-history-reader
}
pub proof fn observation_119() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records.contains_key(2))) == 1); // old-b-after-local-row
}
pub proof fn observation_120() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[2].source==beta())) == 1); // old-b-after-local-name
}
pub proof fn observation_121() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[2].moniker==filesystem())) == 1); // old-b-after-local-type
}
pub proof fn observation_122() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[2].details==details(Root::B))) == 1); // old-b-after-local-details
}
pub proof fn observation_123() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(history_has_supplier(after_local(),2,beta(),Root::B))) == 1); // old-b-after-local-history-reader
}
pub proof fn observation_124() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records.contains_key(0))) == 1); // old-a-final-row
}
pub proof fn observation_125() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[0].source==alpha())) == 1); // old-a-final-name
}
pub proof fn observation_126() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[0].moniker==filesystem())) == 1); // old-a-final-type
}
pub proof fn observation_127() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_local().records[0].details==details(Root::A))) == 1); // old-a-final-details
}
pub proof fn observation_128() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(history_has_supplier(after_local(),0,alpha(),Root::A))) == 1); // old-a-final-history-reader
}
pub proof fn observation_129() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a_uninstall().module.is_none())) == 1); // before-c-no-module
}
pub proof fn observation_130() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_a_uninstall().code.is_none())) == 1); // before-c-no-code
}
pub proof fn observation_131() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c_uninstall().module.is_none())) == 1); // before-b-no-module
}
pub proof fn observation_132() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_c_uninstall().code.is_none())) == 1); // before-b-no-code
}
pub proof fn observation_133() {
ordinary_neighbors();local_and_refusal_neighbors();uninstall_projection_neighbors();reader_neighbors();details_literal_neighbors();assert((b(after_alpha_delete().repos.contains_key(alpha()))) == 0); // after-delete-config
}
}
fn main() {}
