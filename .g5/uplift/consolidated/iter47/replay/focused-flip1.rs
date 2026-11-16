#[path="../../../../Verus_verification/consolidated/iter47/formats.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(toon_text(crlf()) =~= crlf())) == 0) by (compute);assert((b(toon_text(crlf()) =~= crlf())) == 1); // toonf.exact.TestDCrLf
}
}
fn main() {}
