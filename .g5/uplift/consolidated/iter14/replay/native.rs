#[path="../../../../Verus_verification/consolidated/iter14/protection.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(seq![1int,2],roots(),before(false,false)))) == 1); // classify-install
}
pub proof fn observation_1() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(seq![1int],roots(),before(false,false)))) == 1); // classify-ancestor
}
pub proof fn observation_2() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(seq![1int,2,5],roots(),before(false,false)))) == 0); // classify-child
}
pub proof fn observation_3() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,false).dir)) == 1); // ordinary-before-dir
}
pub proof fn observation_4() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,false).payload)) == 0); // ordinary-before-payload
}
pub proof fn observation_5() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,false).sentinel)) == 1); // ordinary-before-sentinel
}
pub proof fn observation_6() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,false).exact_marker)) == 0); // ordinary-before-marker
}
pub proof fn observation_7() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,false).nested_marker)) == 0); // ordinary-before-nested
}
pub proof fn observation_8() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),before(false,false)))) == 0); // ordinary-before-protected
}
pub proof fn observation_9() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_loaded().dir)) == 1); // ordinary-loaded-dir
}
pub proof fn observation_10() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_loaded().payload)) == 1); // ordinary-loaded-payload
}
pub proof fn observation_11() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_loaded().sentinel)) == 1); // ordinary-loaded-sentinel
}
pub proof fn observation_12() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_loaded().exact_marker)) == 0); // ordinary-loaded-marker
}
pub proof fn observation_13() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_loaded().nested_marker)) == 0); // ordinary-loaded-nested
}
pub proof fn observation_14() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),ordinary_loaded()))) == 0); // ordinary-loaded-protected
}
pub proof fn observation_15() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_after().dir)) == 0); // ordinary-cleaned-dir
}
pub proof fn observation_16() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_after().payload)) == 0); // ordinary-cleaned-payload
}
pub proof fn observation_17() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_after().sentinel)) == 0); // ordinary-cleaned-sentinel
}
pub proof fn observation_18() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_after().exact_marker)) == 0); // ordinary-cleaned-marker
}
pub proof fn observation_19() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_after().nested_marker)) == 0); // ordinary-cleaned-nested
}
pub proof fn observation_20() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),ordinary_after()))) == 0); // ordinary-cleaned-protected
}
pub proof fn observation_21() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).dir)) == 1); // marked-before-dir
}
pub proof fn observation_22() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).payload)) == 0); // marked-before-payload
}
pub proof fn observation_23() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).sentinel)) == 1); // marked-before-sentinel
}
pub proof fn observation_24() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).exact_marker)) == 1); // marked-before-marker
}
pub proof fn observation_25() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).nested_marker)) == 0); // marked-before-nested
}
pub proof fn observation_26() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),before(true,false)))) == 1); // marked-before-protected
}
pub proof fn observation_27() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_loaded().dir)) == 1); // marked-loaded-dir
}
pub proof fn observation_28() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_loaded().payload)) == 1); // marked-loaded-payload
}
pub proof fn observation_29() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_loaded().sentinel)) == 1); // marked-loaded-sentinel
}
pub proof fn observation_30() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_loaded().exact_marker)) == 1); // marked-loaded-marker
}
pub proof fn observation_31() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_loaded().nested_marker)) == 0); // marked-loaded-nested
}
pub proof fn observation_32() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),marked_loaded()))) == 1); // marked-loaded-protected
}
pub proof fn observation_33() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_after().dir)) == 1); // marked-cleaned-dir
}
pub proof fn observation_34() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_after().payload)) == 1); // marked-cleaned-payload
}
pub proof fn observation_35() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_after().sentinel)) == 1); // marked-cleaned-sentinel
}
pub proof fn observation_36() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_after().exact_marker)) == 1); // marked-cleaned-marker
}
pub proof fn observation_37() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_after().nested_marker)) == 0); // marked-cleaned-nested
}
pub proof fn observation_38() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),marked_after()))) == 1); // marked-cleaned-protected
}
pub proof fn observation_39() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,false).dir)) == 1); // added-before-dir
}
pub proof fn observation_40() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,false).payload)) == 0); // added-before-payload
}
pub proof fn observation_41() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,false).sentinel)) == 1); // added-before-sentinel
}
pub proof fn observation_42() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,false).exact_marker)) == 0); // added-before-marker
}
pub proof fn observation_43() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,false).nested_marker)) == 0); // added-before-nested
}
pub proof fn observation_44() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),before(false,false)))) == 0); // added-before-protected
}
pub proof fn observation_45() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_loaded().dir)) == 1); // added-loaded-dir
}
pub proof fn observation_46() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_loaded().payload)) == 1); // added-loaded-payload
}
pub proof fn observation_47() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_loaded().sentinel)) == 1); // added-loaded-sentinel
}
pub proof fn observation_48() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_loaded().exact_marker)) == 0); // added-loaded-marker
}
pub proof fn observation_49() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(ordinary_loaded().nested_marker)) == 0); // added-loaded-nested
}
pub proof fn observation_50() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),ordinary_loaded()))) == 0); // added-loaded-protected
}
pub proof fn observation_51() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(added_after().dir)) == 1); // added-cleaned-dir
}
pub proof fn observation_52() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(added_after().payload)) == 1); // added-cleaned-payload
}
pub proof fn observation_53() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(added_after().sentinel)) == 1); // added-cleaned-sentinel
}
pub proof fn observation_54() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(added_after().exact_marker)) == 1); // added-cleaned-marker
}
pub proof fn observation_55() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(added_after().nested_marker)) == 0); // added-cleaned-nested
}
pub proof fn observation_56() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),added_after()))) == 1); // added-cleaned-protected
}
pub proof fn observation_57() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(added_ready().dir)) == 1); // added-changed-dir
}
pub proof fn observation_58() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(added_ready().payload)) == 1); // added-changed-payload
}
pub proof fn observation_59() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(added_ready().sentinel)) == 1); // added-changed-sentinel
}
pub proof fn observation_60() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(added_ready().exact_marker)) == 1); // added-changed-marker
}
pub proof fn observation_61() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(added_ready().nested_marker)) == 0); // added-changed-nested
}
pub proof fn observation_62() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),added_ready()))) == 1); // added-changed-protected
}
pub proof fn observation_63() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).dir)) == 1); // removed-before-dir
}
pub proof fn observation_64() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).payload)) == 0); // removed-before-payload
}
pub proof fn observation_65() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).sentinel)) == 1); // removed-before-sentinel
}
pub proof fn observation_66() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).exact_marker)) == 1); // removed-before-marker
}
pub proof fn observation_67() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).nested_marker)) == 0); // removed-before-nested
}
pub proof fn observation_68() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),before(true,false)))) == 1); // removed-before-protected
}
pub proof fn observation_69() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_loaded().dir)) == 1); // removed-loaded-dir
}
pub proof fn observation_70() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_loaded().payload)) == 1); // removed-loaded-payload
}
pub proof fn observation_71() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_loaded().sentinel)) == 1); // removed-loaded-sentinel
}
pub proof fn observation_72() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_loaded().exact_marker)) == 1); // removed-loaded-marker
}
pub proof fn observation_73() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(marked_loaded().nested_marker)) == 0); // removed-loaded-nested
}
pub proof fn observation_74() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),marked_loaded()))) == 1); // removed-loaded-protected
}
pub proof fn observation_75() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(removed_after().dir)) == 0); // removed-cleaned-dir
}
pub proof fn observation_76() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(removed_after().payload)) == 0); // removed-cleaned-payload
}
pub proof fn observation_77() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(removed_after().sentinel)) == 0); // removed-cleaned-sentinel
}
pub proof fn observation_78() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(removed_after().exact_marker)) == 0); // removed-cleaned-marker
}
pub proof fn observation_79() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(removed_after().nested_marker)) == 0); // removed-cleaned-nested
}
pub proof fn observation_80() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),removed_after()))) == 0); // removed-cleaned-protected
}
pub proof fn observation_81() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(removed_ready().dir)) == 1); // removed-changed-dir
}
pub proof fn observation_82() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(removed_ready().payload)) == 1); // removed-changed-payload
}
pub proof fn observation_83() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(removed_ready().sentinel)) == 1); // removed-changed-sentinel
}
pub proof fn observation_84() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(removed_ready().exact_marker)) == 0); // removed-changed-marker
}
pub proof fn observation_85() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(removed_ready().nested_marker)) == 0); // removed-changed-nested
}
pub proof fn observation_86() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),removed_ready()))) == 0); // removed-changed-protected
}
pub proof fn observation_87() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).dir)) == 1); // file-before-dir
}
pub proof fn observation_88() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).payload)) == 0); // file-before-payload
}
pub proof fn observation_89() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).sentinel)) == 1); // file-before-sentinel
}
pub proof fn observation_90() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).exact_marker)) == 1); // file-before-marker
}
pub proof fn observation_91() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(true,false).nested_marker)) == 0); // file-before-nested
}
pub proof fn observation_92() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),before(true,false)))) == 1); // file-before-protected
}
pub proof fn observation_93() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(file_loaded().dir)) == 1); // file-loaded-dir
}
pub proof fn observation_94() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(file_loaded().payload)) == 1); // file-loaded-payload
}
pub proof fn observation_95() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(file_loaded().sentinel)) == 1); // file-loaded-sentinel
}
pub proof fn observation_96() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(file_loaded().exact_marker)) == 1); // file-loaded-marker
}
pub proof fn observation_97() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(file_loaded().nested_marker)) == 0); // file-loaded-nested
}
pub proof fn observation_98() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),file_loaded()))) == 1); // file-loaded-protected
}
pub proof fn observation_99() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(file_after().dir)) == 1); // file-cleaned-dir
}
pub proof fn observation_100() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(file_after().payload)) == 0); // file-cleaned-payload
}
pub proof fn observation_101() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(file_after().sentinel)) == 1); // file-cleaned-sentinel
}
pub proof fn observation_102() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(file_after().exact_marker)) == 1); // file-cleaned-marker
}
pub proof fn observation_103() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(file_after().nested_marker)) == 0); // file-cleaned-nested
}
pub proof fn observation_104() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),file_after()))) == 1); // file-cleaned-protected
}
pub proof fn observation_105() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,true).dir)) == 1); // nested-before-dir
}
pub proof fn observation_106() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,true).payload)) == 0); // nested-before-payload
}
pub proof fn observation_107() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,true).sentinel)) == 1); // nested-before-sentinel
}
pub proof fn observation_108() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,true).exact_marker)) == 0); // nested-before-marker
}
pub proof fn observation_109() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(before(false,true).nested_marker)) == 1); // nested-before-nested
}
pub proof fn observation_110() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),before(false,true)))) == 0); // nested-before-protected
}
pub proof fn observation_111() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(nested_loaded().dir)) == 1); // nested-loaded-dir
}
pub proof fn observation_112() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(nested_loaded().payload)) == 1); // nested-loaded-payload
}
pub proof fn observation_113() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(nested_loaded().sentinel)) == 1); // nested-loaded-sentinel
}
pub proof fn observation_114() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(nested_loaded().exact_marker)) == 0); // nested-loaded-marker
}
pub proof fn observation_115() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(nested_loaded().nested_marker)) == 1); // nested-loaded-nested
}
pub proof fn observation_116() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),nested_loaded()))) == 0); // nested-loaded-protected
}
pub proof fn observation_117() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(nested_after().dir)) == 0); // nested-cleaned-dir
}
pub proof fn observation_118() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(nested_after().payload)) == 0); // nested-cleaned-payload
}
pub proof fn observation_119() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(nested_after().sentinel)) == 0); // nested-cleaned-sentinel
}
pub proof fn observation_120() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(nested_after().exact_marker)) == 0); // nested-cleaned-marker
}
pub proof fn observation_121() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(nested_after().nested_marker)) == 0); // nested-cleaned-nested
}
pub proof fn observation_122() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(protected(scratch(),roots(),nested_after()))) == 0); // nested-cleaned-protected
}
pub proof fn observation_123() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(unix_copy(before(false,false),unix_args(false)).sentinel)) == 1); // copy-0-sentinel
}
pub proof fn observation_124() {
lifecycle_neighbors(); whole_segment_neighbors(); reveal_with_fuel(ancestor_of_root,4);
assert((b(unix_copy(before(false,false),unix_args(true)).sentinel)) == 1); // copy-1-sentinel
}
}
fn main() {}
