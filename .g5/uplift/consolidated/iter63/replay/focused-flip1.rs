#[path="../../../../Verus_verification/consolidated/iter63/cleanup.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(remains(Kind::SourcesEntry, false))) == 1);assert((b(remains(Kind::SourcesEntry, false))) == 0); // x2.dev.sourcesmodule
}
}
fn main() {}
