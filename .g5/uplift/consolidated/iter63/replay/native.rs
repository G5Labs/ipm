#[path="../../../../Verus_verification/consolidated/iter63/cleanup.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(remains(Kind::DeclaredMapping, false))) == 1);assert((b(remains(Kind::DeclaredMapping, false))) == 1); // x1.map.pkg
assert((b(remains(Kind::SourcesEntry, false))) == 1);assert((b(remains(Kind::SourcesEntry, false))) == 1); // x2.dev.sourcesmodule
assert((b(remains(Kind::GblAttr, false))) == 0);assert((b(remains(Kind::GblAttr, false))) == 0); // x1.gbl.attr
assert((b(remains(Kind::Cls, false))) == 0);assert((b(remains(Kind::Cls, false))) == 0); // x1.cls.one
assert((b(remains(Kind::Pkg, false))) == 0);assert((b(remains(Kind::Pkg, false))) == 0); // x1.pkg.a
assert((b(remains(Kind::Pkg, false))) == 0);assert((b(remains(Kind::Pkg, false))) == 0); // x1.pkg.b
assert((b(remains(Kind::Inc, false))) == 0);assert((b(remains(Kind::Inc, false))) == 0); // x1.inc
assert((b(remains(Kind::Mac, false))) == 0);assert((b(remains(Kind::Mac, false))) == 0); // x1.mac
assert((b(remains(Kind::Mac, false))) == 0);assert((b(remains(Kind::Mac, false))) == 0); // x1.int
assert((b(remains(Kind::Mac, false))) == 0);assert((b(remains(Kind::Mac, false))) == 0); // x1.obj
assert((b(remains(Kind::GblName, false))) == 1);assert((b(remains(Kind::GblName, false))) == 1); // x1.gbl.nameonly
assert((b(remains(Kind::DeclaredMapping, false))) == 1);assert((b(remains(Kind::DeclaredMapping, false))) == 1); // x1.map.gbl
assert((b(remains(Kind::FileCopy { keep: false }, false))) == 0);assert((b(remains(Kind::FileCopy { keep: false }, false))) == 0); // x1.file.a
assert((b(remains(Kind::FileCopy { keep: false }, false))) == 0);assert((b(remains(Kind::FileCopy { keep: false }, false))) == 0); // x1.file.dir
assert((b(remains(Kind::FileCopy { keep: true }, false))) == 1);assert((b(remains(Kind::FileCopy { keep: true }, false))) == 1); // x1.file.keepdir
assert((b(remains(Kind::FileCopy { keep: true }, false))) == 1);assert((b(remains(Kind::FileCopy { keep: true }, false))) == 1); // x1.file.keepfile
assert((b(remains(Kind::Record, false))) == 0);assert((b(remains(Kind::Record, false))) == 0); // x1.record
assert((b(remains(Kind::Stream, false))) == 0);assert((b(remains(Kind::Stream, false))) == 0); // x1.stream
assert((b(remains(Kind::Cls, true))) == 0);assert((b(remains(Kind::Cls, true))) == 0); // x3.cls.one
assert((b(remains(Kind::Pkg, true))) == 0);assert((b(remains(Kind::Pkg, true))) == 0); // x3.pkg.a
assert((b(remains(Kind::Pkg, true))) == 0);assert((b(remains(Kind::Pkg, true))) == 0); // x3.pkg.b
assert((b(remains(Kind::Inc, true))) == 0);assert((b(remains(Kind::Inc, true))) == 0); // x3.inc
assert((b(remains(Kind::Mac, true))) == 0);assert((b(remains(Kind::Mac, true))) == 0); // x3.mac
assert((b(remains(Kind::Mac, true))) == 0);assert((b(remains(Kind::Mac, true))) == 0); // x3.int
assert((b(remains(Kind::Mac, true))) == 0);assert((b(remains(Kind::Mac, true))) == 0); // x3.obj
assert((b(remains(Kind::GblName, true))) == 1);assert((b(remains(Kind::GblName, true))) == 1); // x3.gbl.nameonly
assert((b(remains(Kind::GblAttr, true))) == 0);assert((b(remains(Kind::GblAttr, true))) == 0); // x3.gbl.attr
assert((b(remains(Kind::DeclaredMapping, true))) == 1);assert((b(remains(Kind::DeclaredMapping, true))) == 1); // x3.map.gbl
assert((b(remains(Kind::DeclaredMapping, true))) == 1);assert((b(remains(Kind::DeclaredMapping, true))) == 1); // x3.map.pkg
assert((b(remains(Kind::FileCopy { keep: false }, true))) == 0);assert((b(remains(Kind::FileCopy { keep: false }, true))) == 0); // x3.file.a
assert((b(remains(Kind::FileCopy { keep: false }, true))) == 0);assert((b(remains(Kind::FileCopy { keep: false }, true))) == 0); // x3.file.dir
assert((b(remains(Kind::FileCopy { keep: true }, true))) == 1);assert((b(remains(Kind::FileCopy { keep: true }, true))) == 1); // x3.file.keepdir
assert((b(remains(Kind::FileCopy { keep: true }, true))) == 1);assert((b(remains(Kind::FileCopy { keep: true }, true))) == 1); // x3.file.keepfile
assert((b(remains(Kind::Record, true))) == 0);assert((b(remains(Kind::Record, true))) == 0); // x3.record
assert((b(remains(Kind::Stream, true))) == 0);assert((b(remains(Kind::Stream, true))) == 0); // x3.stream
assert((b(remains(Kind::Cls, false))) == 0);assert((b(remains(Kind::Cls, false))) == 0); // x2.dev.cls
assert((b(remains(Kind::Record, false))) == 0);assert((b(remains(Kind::Record, false))) == 0); // x2.dev.record
}
}
fn main() {}
