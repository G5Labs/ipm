#[path="../../../../Verus_verification/consolidated/iter6/workflows.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {
assert(global_path(seq![5int],None,7) =~= seq![5int,10,7]);
assert((b(application_item(Item{path:seq![1int,2,3],loaded:true,supported_ext:true},seq![1int,9]))) == 2); // template-app
assert((b(test_item(Item{path:seq![1int,9,8],loaded:true,supported_ext:true},seq![1int,9],true))) == 1); // template-test
assert((1-template_status(false)) == 1); // template-missing
assert((b(copy_ok(seq![1int,2],seq![1int,2,3],false,true))) == 0); // copy-child
assert((b(copy_ok(seq![1int,2,3],seq![1int,2],false,true))) == 0); // copy-parent
assert((b(copy_ok(seq![1int,2],seq![1int,4],false,true))) == 1); // copy-sibling
assert((b(global_path(seq![5int],None,7)==seq![5int,10,7])) == 1); // global-path
assert((reload_global(package_global(seq![5int],None,7,42),seq![5int],None,7,0)) == 42); // global-value
assert((b(selected_module(map![1int=>99int],map![2int=>99int],1)==Some(99))) == 1); // alias-internal
assert((b(selected_module(map![1int=>99int],map![2int=>99int],2)==Some(99))) == 1); // alias-external
assert((b(creates_file(Entry{name:7,size:0,regular:true}))) == 1); // zero-exists
assert((b(next_header(0,0)==512)) == 1); // zero-later
assert(((Entry{name:7,size:0,regular:true}).size as int) == 0); // zero-size
}
}
fn main() {}
