#[path="../../../../Verus_verification/consolidated/iter53/generator.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(fixture_author().license == 2)) == 1);assert((b(fixture_author().license == 2)) == 1); // t2.licenseIsYear
assert((b(exported(skip_rout()))) == 1);assert((b(exported(skip_rout()))) == 1); // e1.skiprout
assert((b(fixture_author().copyright == 1)) == 1);assert((b(fixture_author().copyright == 1)) == 1); // t2.copyrightIsLicense
assert((b(fixture_author().license == 1)) == 0);assert((b(fixture_author().license == 1)) == 0); // t2.licenseIsLicense
assert((b(fixture_author().copyright == 2)) == 0);assert((b(fixture_author().copyright == 2)) == 0); // t2.copyrightIsYear
assert((b(exported(keep()))) == 1);assert((b(exported(keep()))) == 1); // e1.keep
assert((b(exported(skip_cls()))) == 0);assert((b(exported(skip_cls()))) == 0); // e1.skipcls
assert((b(exported(rout()))) == 1);assert((b(exported(rout()))) == 1); // e1.rout
assert((b(exported(zz()))) == 1);assert((b(exported(zz()))) == 1); // e1.zz
assert((b(shared_resource(1, 1))) == 1);assert((b(shared_resource(1, 1))) == 1); // t2.pkgG53
assert((b(!shared_resource(1, 1))) == 0);assert((b(!shared_resource(1, 1))) == 0); // t2.pkgG53A
}
}
fn main() {}
