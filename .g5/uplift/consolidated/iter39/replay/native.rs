#[path="../../../../Verus_verification/consolidated/iter39/document.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((doc_empty(e2().state)) == 1) by (compute);assert((doc_empty(e2().state)) == 1); // e2.doctext.empty
assert((version(e3().state)) == 5) by (compute);assert((version(e3().state)) == 5); // e3.version
assert((doc_names_other(e3().state)) == 1) by (compute);assert((doc_names_other(e3().state)) == 1); // e3.doctext.other
assert((has_record(s0())) == 1) by (compute);assert((has_record(s0())) == 1); // setup.record
assert((version(s0())) == 1) by (compute);assert((version(s0())) == 1); // setup.version
assert((resources(s0())) == 1) by (compute);assert((resources(s0())) == 1); // setup.resources
assert((has_doc(s0())) == 1) by (compute);assert((has_doc(s0())) == 1); // setup.doc
assert((b((s0()).classes)) == 1) by (compute);assert((b((s0()).classes)) == 1); // setup.class
assert((doc_names_other(s0())) == 0) by (compute);assert((doc_names_other(s0())) == 0); // setup.doctext.other
assert((doc_version_is(s0(), 2)) == 0) by (compute);assert((doc_version_is(s0(), 2)) == 0); // setup.doctext.v110
assert((doc_version_is(s0(), 5)) == 0) by (compute);assert((doc_version_is(s0(), 5)) == 0); // setup.doctext.v120
assert((doc_version_is(s0(), 3)) == 0) by (compute);assert((doc_version_is(s0(), 3)) == 0); // setup.doctext.v050
assert((doc_empty(s0())) == 0) by (compute);assert((doc_empty(s0())) == 0); // setup.doctext.empty
assert((has_record(e1().state)) == 1) by (compute);assert((has_record(e1().state)) == 1); // e1.record
assert((version(e1().state)) == 2) by (compute);assert((version(e1().state)) == 2); // e1.version
assert((resources(e1().state)) == 1) by (compute);assert((resources(e1().state)) == 1); // e1.resources
assert((has_doc(e1().state)) == 1) by (compute);assert((has_doc(e1().state)) == 1); // e1.doc
assert((b((e1().state).classes)) == 1) by (compute);assert((b((e1().state).classes)) == 1); // e1.class
assert((doc_names_other(e1().state)) == 0) by (compute);assert((doc_names_other(e1().state)) == 0); // e1.doctext.other
assert((doc_version_is(e1().state, 2)) == 1) by (compute);assert((doc_version_is(e1().state, 2)) == 1); // e1.doctext.v110
assert((doc_version_is(e1().state, 5)) == 0) by (compute);assert((doc_version_is(e1().state, 5)) == 0); // e1.doctext.v120
assert((doc_version_is(e1().state, 3)) == 0) by (compute);assert((doc_version_is(e1().state, 3)) == 0); // e1.doctext.v050
assert((doc_empty(e1().state)) == 0) by (compute);assert((doc_empty(e1().state)) == 0); // e1.doctext.empty
assert((has_record(e2().state)) == 1) by (compute);assert((has_record(e2().state)) == 1); // e2.record
assert((version(e2().state)) == 2) by (compute);assert((version(e2().state)) == 2); // e2.version
assert((resources(e2().state)) == 1) by (compute);assert((resources(e2().state)) == 1); // e2.resources
assert((has_doc(e2().state)) == 1) by (compute);assert((has_doc(e2().state)) == 1); // e2.doc
assert((b((e2().state).classes)) == 1) by (compute);assert((b((e2().state).classes)) == 1); // e2.class
assert((doc_names_other(e2().state)) == 0) by (compute);assert((doc_names_other(e2().state)) == 0); // e2.doctext.other
assert((doc_version_is(e2().state, 2)) == 0) by (compute);assert((doc_version_is(e2().state, 2)) == 0); // e2.doctext.v110
assert((doc_version_is(e2().state, 5)) == 0) by (compute);assert((doc_version_is(e2().state, 5)) == 0); // e2.doctext.v120
assert((doc_version_is(e2().state, 3)) == 0) by (compute);assert((doc_version_is(e2().state, 3)) == 0); // e2.doctext.v050
assert((has_record(e3().state)) == 1) by (compute);assert((has_record(e3().state)) == 1); // e3.record
assert((resources(e3().state)) == 1) by (compute);assert((resources(e3().state)) == 1); // e3.resources
assert((has_doc(e3().state)) == 1) by (compute);assert((has_doc(e3().state)) == 1); // e3.doc
assert((b((e3().state).classes)) == 1) by (compute);assert((b((e3().state).classes)) == 1); // e3.class
assert((doc_version_is(e3().state, 2)) == 0) by (compute);assert((doc_version_is(e3().state, 2)) == 0); // e3.doctext.v110
assert((doc_version_is(e3().state, 5)) == 1) by (compute);assert((doc_version_is(e3().state, 5)) == 1); // e3.doctext.v120
assert((doc_version_is(e3().state, 3)) == 0) by (compute);assert((doc_version_is(e3().state, 3)) == 0); // e3.doctext.v050
assert((doc_empty(e3().state)) == 0) by (compute);assert((doc_empty(e3().state)) == 0); // e3.doctext.empty
assert((has_record(e4().state)) == 1) by (compute);assert((has_record(e4().state)) == 1); // e4.record
assert((version(e4().state)) == 3) by (compute);assert((version(e4().state)) == 3); // e4.version
assert((resources(e4().state)) == 1) by (compute);assert((resources(e4().state)) == 1); // e4.resources
assert((has_doc(e4().state)) == 1) by (compute);assert((has_doc(e4().state)) == 1); // e4.doc
assert((b((e4().state).classes)) == 1) by (compute);assert((b((e4().state).classes)) == 1); // e4.class
assert((doc_names_other(e4().state)) == 0) by (compute);assert((doc_names_other(e4().state)) == 0); // e4.doctext.other
assert((doc_version_is(e4().state, 2)) == 0) by (compute);assert((doc_version_is(e4().state, 2)) == 0); // e4.doctext.v110
assert((doc_version_is(e4().state, 5)) == 0) by (compute);assert((doc_version_is(e4().state, 5)) == 0); // e4.doctext.v120
assert((doc_version_is(e4().state, 3)) == 1) by (compute);assert((doc_version_is(e4().state, 3)) == 1); // e4.doctext.v050
assert((doc_empty(e4().state)) == 0) by (compute);assert((doc_empty(e4().state)) == 0); // e4.doctext.empty
assert((has_record(e5().state)) == 1) by (compute);assert((has_record(e5().state)) == 1); // e5.record
assert((version(e5().state)) == 4) by (compute);assert((version(e5().state)) == 4); // e5.version
assert((resources(e5().state)) == 1) by (compute);assert((resources(e5().state)) == 1); // e5.resources
assert((has_doc(e5().state)) == 1) by (compute);assert((has_doc(e5().state)) == 1); // e5.doc
assert((b((e5().state).classes)) == 1) by (compute);assert((b((e5().state).classes)) == 1); // e5.class
assert((doc_names_other(e5().state)) == 0) by (compute);assert((doc_names_other(e5().state)) == 0); // e5.doctext.other
assert((doc_version_is(e5().state, 2)) == 0) by (compute);assert((doc_version_is(e5().state, 2)) == 0); // e5.doctext.v110
assert((doc_version_is(e5().state, 5)) == 0) by (compute);assert((doc_version_is(e5().state, 5)) == 0); // e5.doctext.v120
assert((doc_version_is(e5().state, 3)) == 0) by (compute);assert((doc_version_is(e5().state, 3)) == 0); // e5.doctext.v050
assert((doc_empty(e5().state)) == 1) by (compute);assert((doc_empty(e5().state)) == 1); // e5.doctext.empty
assert((has_record(e6())) == 0) by (compute);assert((has_record(e6())) == 0); // e6.record
assert((version(e6())) == 0) by (compute);assert((version(e6())) == 0); // e6.version
assert((resources(e6())) == -1) by (compute);assert((resources(e6())) == -1); // e6.resources
assert((has_doc(e6())) == 0) by (compute);assert((has_doc(e6())) == 0); // e6.doc
assert((b((e6()).classes)) == 1) by (compute);assert((b((e6()).classes)) == 1); // e6.class
assert((doc_names_other(e6())) == 0) by (compute);assert((doc_names_other(e6())) == 0); // e6.doctext.other
assert((doc_version_is(e6(), 2)) == 0) by (compute);assert((doc_version_is(e6(), 2)) == 0); // e6.doctext.v110
assert((doc_version_is(e6(), 5)) == 0) by (compute);assert((doc_version_is(e6(), 5)) == 0); // e6.doctext.v120
assert((doc_version_is(e6(), 3)) == 0) by (compute);assert((doc_version_is(e6(), 3)) == 0); // e6.doctext.v050
assert((doc_empty(e6())) == 1) by (compute);assert((doc_empty(e6())) == 1); // e6.doctext.empty
assert((b(e1().ok)) == 1) by (compute);assert((b(e1().ok)) == 1); // e1.ok
assert((b(e2().ok)) == 0) by (compute);assert((b(e2().ok)) == 0); // e2.ok
assert((b(e3().ok)) == 1) by (compute);assert((b(e3().ok)) == 1); // e3.ok
assert((b(e4().ok)) == 1) by (compute);assert((b(e4().ok)) == 1); // e4.ok
assert((b(e5().ok)) == 1) by (compute);assert((b(e5().ok)) == 1); // e5.ok
}
}
fn main() {}
