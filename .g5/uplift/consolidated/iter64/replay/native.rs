#[path="../../../../Verus_verification/consolidated/iter64/failkeep.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(kept(Kind::Cpf, 2, false))) == 1);assert((b(kept(Kind::Cpf, 2, false))) == 1); // ec.cpf
assert((b(kept(Kind::File, 3, false))) == 1);assert((b(kept(Kind::File, 3, false))) == 1); // la.filea
assert((b(kept(Kind::Mapping, 2, true))) == 1);assert((b(kept(Kind::Mapping, 2, true))) == 1); // dv.mapg
assert((b(kept(Kind::Record, 0, false))) == 1);assert((b(kept(Kind::Record, 0, false))) == 1); // ok.record
assert((b(kept(Kind::Class, 0, false))) == 1);assert((b(kept(Kind::Class, 0, false))) == 1); // ok.good
assert((b(kept(Kind::Class, 0, false))) == 1);assert((b(kept(Kind::Class, 0, false))) == 1); // ok.gooddef
assert((b(kept(Kind::File, 0, false))) == 1);assert((b(kept(Kind::File, 0, false))) == 1); // ok.filea
assert((b(kept(Kind::File, 0, false))) == 1);assert((b(kept(Kind::File, 0, false))) == 1); // ok.fileb
assert((b(kept(Kind::WebApp, 0, false))) == 1);assert((b(kept(Kind::WebApp, 0, false))) == 1); // ok.webapp
assert((b(kept(Kind::Mapping, 0, false))) == 1);assert((b(kept(Kind::Mapping, 0, false))) == 1); // ok.mapg
assert((b(kept(Kind::Mapping, 0, false))) == 1);assert((b(kept(Kind::Mapping, 0, false))) == 1); // ok.mapp
assert((b(kept(Kind::Cpf, 0, false))) == 1);assert((b(kept(Kind::Cpf, 0, false))) == 1); // ok.cpf
assert((b(kept(Kind::Msg, 0, false))) == 1);assert((b(kept(Kind::Msg, 0, false))) == 1); // ok.msg
assert((b(kept(Kind::LocDoc, 0, false))) == 1);assert((b(kept(Kind::LocDoc, 0, false))) == 1); // ok.locdoc
assert((b(kept(Kind::LocExport, 0, false))) == 1);assert((b(kept(Kind::LocExport, 0, false))) == 1); // ok.locexport
assert((b(kept(Kind::Sources, 0, false))) == 0);assert((b(kept(Kind::Sources, 0, false))) == 0); // ok.sources
assert((b(hist_committed(0, false))) == 1);assert((b(hist_committed(0, false))) == 1); // ok.hist.committed
assert((b(0 == 0)) == 1);assert((b(0 == 0)) == 1); // ok.hist.success
assert((b(0 == 0)) == 1);assert((b(0 == 0)) == 1); // ok.ok
assert((b(0 == 2)) == 0);assert((b(0 == 2)) == 0); // ok.failcompile
assert((b(0 == 3)) == 0);assert((b(0 == 3)) == 0); // ok.failactivate
assert((b(kept(Kind::Record, 2, false))) == 0);assert((b(kept(Kind::Record, 2, false))) == 0); // ec.record
assert((b(kept(Kind::Class, 2, false))) == 0);assert((b(kept(Kind::Class, 2, false))) == 0); // ec.good
assert((b(kept(Kind::Class, 2, false))) == 0);assert((b(kept(Kind::Class, 2, false))) == 0); // ec.gooddef
assert((b(kept(Kind::File, 2, false))) == 0);assert((b(kept(Kind::File, 2, false))) == 0); // ec.filea
assert((b(kept(Kind::File, 2, false))) == 0);assert((b(kept(Kind::File, 2, false))) == 0); // ec.fileb
assert((b(kept(Kind::WebApp, 2, false))) == 0);assert((b(kept(Kind::WebApp, 2, false))) == 0); // ec.webapp
assert((b(kept(Kind::Mapping, 2, false))) == 0);assert((b(kept(Kind::Mapping, 2, false))) == 0); // ec.mapg
assert((b(kept(Kind::Mapping, 2, false))) == 0);assert((b(kept(Kind::Mapping, 2, false))) == 0); // ec.mapp
assert((b(kept(Kind::Msg, 2, false))) == 0);assert((b(kept(Kind::Msg, 2, false))) == 0); // ec.msg
assert((b(kept(Kind::LocDoc, 2, false))) == 0);assert((b(kept(Kind::LocDoc, 2, false))) == 0); // ec.locdoc
assert((b(kept(Kind::LocExport, 2, false))) == 0);assert((b(kept(Kind::LocExport, 2, false))) == 0); // ec.locexport
assert((b(kept(Kind::Sources, 2, false))) == 0);assert((b(kept(Kind::Sources, 2, false))) == 0); // ec.sources
assert((b(hist_committed(2, false))) == 0);assert((b(hist_committed(2, false))) == 0); // ec.hist.committed
assert((b(2 == 0)) == 0);assert((b(2 == 0)) == 0); // ec.hist.success
assert((b(2 == 0)) == 0);assert((b(2 == 0)) == 0); // ec.ok
assert((b(2 == 2)) == 1);assert((b(2 == 2)) == 1); // ec.failcompile
assert((b(2 == 3)) == 0);assert((b(2 == 3)) == 0); // ec.failactivate
assert((b(kept(Kind::Record, 3, false))) == 0);assert((b(kept(Kind::Record, 3, false))) == 0); // la.record
assert((b(kept(Kind::Class, 3, false))) == 0);assert((b(kept(Kind::Class, 3, false))) == 0); // la.good
assert((b(kept(Kind::Class, 3, false))) == 0);assert((b(kept(Kind::Class, 3, false))) == 0); // la.gooddef
assert((b(kept(Kind::File, 3, false))) == 1);assert((b(kept(Kind::File, 3, false))) == 1); // la.fileb
assert((b(kept(Kind::WebApp, 3, false))) == 0);assert((b(kept(Kind::WebApp, 3, false))) == 0); // la.webapp
assert((b(kept(Kind::Mapping, 3, false))) == 0);assert((b(kept(Kind::Mapping, 3, false))) == 0); // la.mapg
assert((b(kept(Kind::Mapping, 3, false))) == 0);assert((b(kept(Kind::Mapping, 3, false))) == 0); // la.mapp
assert((b(kept(Kind::Cpf, 3, false))) == 1);assert((b(kept(Kind::Cpf, 3, false))) == 1); // la.cpf
assert((b(kept(Kind::Msg, 3, false))) == 0);assert((b(kept(Kind::Msg, 3, false))) == 0); // la.msg
assert((b(kept(Kind::LocDoc, 3, false))) == 0);assert((b(kept(Kind::LocDoc, 3, false))) == 0); // la.locdoc
assert((b(kept(Kind::LocExport, 3, false))) == 1);assert((b(kept(Kind::LocExport, 3, false))) == 1); // la.locexport
assert((b(kept(Kind::Sources, 3, false))) == 0);assert((b(kept(Kind::Sources, 3, false))) == 0); // la.sources
assert((b(hist_committed(3, false))) == 0);assert((b(hist_committed(3, false))) == 0); // la.hist.committed
assert((b(3 == 0)) == 0);assert((b(3 == 0)) == 0); // la.hist.success
assert((b(3 == 0)) == 0);assert((b(3 == 0)) == 0); // la.ok
assert((b(3 == 2)) == 0);assert((b(3 == 2)) == 0); // la.failcompile
assert((b(3 == 3)) == 1);assert((b(3 == 3)) == 1); // la.failactivate
assert((b(kept(Kind::Record, 2, true))) == 1);assert((b(kept(Kind::Record, 2, true))) == 1); // dv.record
assert((b(kept(Kind::Class, 2, true))) == 1);assert((b(kept(Kind::Class, 2, true))) == 1); // dv.good
assert((b(kept(Kind::Class, 2, true))) == 1);assert((b(kept(Kind::Class, 2, true))) == 1); // dv.gooddef
assert((b(kept(Kind::File, 2, true))) == 0);assert((b(kept(Kind::File, 2, true))) == 0); // dv.filea
assert((b(kept(Kind::File, 2, true))) == 0);assert((b(kept(Kind::File, 2, true))) == 0); // dv.fileb
assert((b(kept(Kind::WebApp, 2, true))) == 0);assert((b(kept(Kind::WebApp, 2, true))) == 0); // dv.webapp
assert((b(kept(Kind::Mapping, 2, true))) == 1);assert((b(kept(Kind::Mapping, 2, true))) == 1); // dv.mapp
assert((b(kept(Kind::Cpf, 2, true))) == 1);assert((b(kept(Kind::Cpf, 2, true))) == 1); // dv.cpf
assert((b(kept(Kind::Msg, 2, true))) == 1);assert((b(kept(Kind::Msg, 2, true))) == 1); // dv.msg
assert((b(kept(Kind::LocDoc, 2, true))) == 1);assert((b(kept(Kind::LocDoc, 2, true))) == 1); // dv.locdoc
assert((b(kept(Kind::LocExport, 2, true))) == 0);assert((b(kept(Kind::LocExport, 2, true))) == 0); // dv.locexport
assert((b(kept(Kind::Sources, 2, true))) == 1);assert((b(kept(Kind::Sources, 2, true))) == 1); // dv.sources
assert((b(hist_committed(2, true))) == 1);assert((b(hist_committed(2, true))) == 1); // dv.hist.committed
assert((b(2 == 0)) == 0);assert((b(2 == 0)) == 0); // dv.hist.success
assert((b(2 == 0)) == 0);assert((b(2 == 0)) == 0); // dv.ok
assert((b(2 == 2)) == 1);assert((b(2 == 2)) == 1); // dv.failcompile
assert((b(2 == 3)) == 0);assert((b(2 == 3)) == 0); // dv.failactivate
}
}
fn main() {}
