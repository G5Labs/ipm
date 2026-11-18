#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((cli_install(i1()).len()) == 1) by (compute_only);assert((cli_install(i1()).len()) == 1); // i1.rows
assert((act(rec1(cli_install(i1())))) == 2) by (compute_only);assert((act(rec1(cli_install(i1())))) == 2); // i1.r1.action
assert((b(rec1(cli_install(i1())).row.package == n_h74a())) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).row.package == n_h74a())) == 1); // i1.r1.pkg
assert((b(rec1(cli_install(i1())).row.package.len() == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i1())).row.package.len() == 0)) == 0); // i1.r1.pkgempty
assert((b(ver_is(rec1(cli_install(i1())), placeholder()))) == 0) by (compute_only);assert((b(ver_is(rec1(cli_install(i1())), placeholder()))) == 0); // i1.r1.verplaceholder
assert((b(ver_is(rec1(cli_install(i1())), v100().text))) == 1) by (compute_only);assert((b(ver_is(rec1(cli_install(i1())), v100().text))) == 1); // i1.r1.ver
assert((b(rec1(cli_install(i1())).row.status == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i1())).row.status == 0)) == 0); // i1.r1.inprogress
assert((b(ok_of(rec1(cli_install(i1()))))) == 1) by (compute_only);assert((b(ok_of(rec1(cli_install(i1()))))) == 1); // i1.r1.ok
assert((b(rec1(cli_install(i1())).row.committed)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).row.committed)) == 1); // i1.r1.committed
assert((b(rec1(cli_install(i1())).row.ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).row.ended)) == 1); // i1.r1.ended
assert((b(rec1(cli_install(i1())).row.finalized)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).row.finalized)) == 1); // i1.r1.finalized
assert((nph(rec1(cli_install(i1())))) == 5) by (compute_only);assert((nph(rec1(cli_install(i1())))) == 5); // i1.r1.nph
assert((code(rec1(cli_install(i1())).phases[0].name)) == 1) by (compute_only);assert((code(rec1(cli_install(i1())).phases[0].name)) == 1); // i1.r1.ph1
assert((b(rec1(cli_install(i1())).phases[0].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).phases[0].ok)) == 1); // i1.r1.ph1.ok
assert((b(rec1(cli_install(i1())).phases[0].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).phases[0].ended)) == 1); // i1.r1.ph1.ended
assert((code(rec1(cli_install(i1())).phases[1].name)) == 2) by (compute_only);assert((code(rec1(cli_install(i1())).phases[1].name)) == 2); // i1.r1.ph2
assert((b(rec1(cli_install(i1())).phases[1].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).phases[1].ok)) == 1); // i1.r1.ph2.ok
assert((b(rec1(cli_install(i1())).phases[1].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).phases[1].ended)) == 1); // i1.r1.ph2.ended
assert((code(rec1(cli_install(i1())).phases[2].name)) == 3) by (compute_only);assert((code(rec1(cli_install(i1())).phases[2].name)) == 3); // i1.r1.ph3
assert((b(rec1(cli_install(i1())).phases[2].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).phases[2].ok)) == 1); // i1.r1.ph3.ok
assert((b(rec1(cli_install(i1())).phases[2].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).phases[2].ended)) == 1); // i1.r1.ph3.ended
assert((code(rec1(cli_install(i1())).phases[3].name)) == 4) by (compute_only);assert((code(rec1(cli_install(i1())).phases[3].name)) == 4); // i1.r1.ph4
assert((b(rec1(cli_install(i1())).phases[3].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).phases[3].ok)) == 1); // i1.r1.ph4.ok
assert((b(rec1(cli_install(i1())).phases[3].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).phases[3].ended)) == 1); // i1.r1.ph4.ended
assert((code(rec1(cli_install(i1())).phases[4].name)) == 5) by (compute_only);assert((code(rec1(cli_install(i1())).phases[4].name)) == 5); // i1.r1.ph5
assert((b(rec1(cli_install(i1())).phases[4].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).phases[4].ok)) == 1); // i1.r1.ph5.ok
assert((b(rec1(cli_install(i1())).phases[4].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i1())).phases[4].ended)) == 1); // i1.r1.ph5.ended
assert((cli_install(i2()).len()) == 1) by (compute_only);assert((cli_install(i2()).len()) == 1); // i2.rows
assert((act(rec1(cli_install(i2())))) == 2) by (compute_only);assert((act(rec1(cli_install(i2())))) == 2); // i2.r1.action
assert((b(rec1(cli_install(i2())).row.package == n_h74a())) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).row.package == n_h74a())) == 1); // i2.r1.pkg
assert((b(rec1(cli_install(i2())).row.package.len() == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i2())).row.package.len() == 0)) == 0); // i2.r1.pkgempty
assert((b(ver_is(rec1(cli_install(i2())), placeholder()))) == 0) by (compute_only);assert((b(ver_is(rec1(cli_install(i2())), placeholder()))) == 0); // i2.r1.verplaceholder
assert((b(ver_is(rec1(cli_install(i2())), v100().text))) == 1) by (compute_only);assert((b(ver_is(rec1(cli_install(i2())), v100().text))) == 1); // i2.r1.ver
assert((b(rec1(cli_install(i2())).row.status == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i2())).row.status == 0)) == 0); // i2.r1.inprogress
assert((b(ok_of(rec1(cli_install(i2()))))) == 1) by (compute_only);assert((b(ok_of(rec1(cli_install(i2()))))) == 1); // i2.r1.ok
assert((b(rec1(cli_install(i2())).row.committed)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).row.committed)) == 1); // i2.r1.committed
assert((b(rec1(cli_install(i2())).row.ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).row.ended)) == 1); // i2.r1.ended
assert((b(rec1(cli_install(i2())).row.finalized)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).row.finalized)) == 1); // i2.r1.finalized
assert((nph(rec1(cli_install(i2())))) == 5) by (compute_only);assert((nph(rec1(cli_install(i2())))) == 5); // i2.r1.nph
assert((code(rec1(cli_install(i2())).phases[0].name)) == 1) by (compute_only);assert((code(rec1(cli_install(i2())).phases[0].name)) == 1); // i2.r1.ph1
assert((b(rec1(cli_install(i2())).phases[0].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).phases[0].ok)) == 1); // i2.r1.ph1.ok
assert((b(rec1(cli_install(i2())).phases[0].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).phases[0].ended)) == 1); // i2.r1.ph1.ended
assert((code(rec1(cli_install(i2())).phases[1].name)) == 2) by (compute_only);assert((code(rec1(cli_install(i2())).phases[1].name)) == 2); // i2.r1.ph2
assert((b(rec1(cli_install(i2())).phases[1].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).phases[1].ok)) == 1); // i2.r1.ph2.ok
assert((b(rec1(cli_install(i2())).phases[1].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).phases[1].ended)) == 1); // i2.r1.ph2.ended
assert((code(rec1(cli_install(i2())).phases[2].name)) == 3) by (compute_only);assert((code(rec1(cli_install(i2())).phases[2].name)) == 3); // i2.r1.ph3
assert((b(rec1(cli_install(i2())).phases[2].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).phases[2].ok)) == 1); // i2.r1.ph3.ok
assert((b(rec1(cli_install(i2())).phases[2].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).phases[2].ended)) == 1); // i2.r1.ph3.ended
assert((code(rec1(cli_install(i2())).phases[3].name)) == 4) by (compute_only);assert((code(rec1(cli_install(i2())).phases[3].name)) == 4); // i2.r1.ph4
assert((b(rec1(cli_install(i2())).phases[3].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).phases[3].ok)) == 1); // i2.r1.ph4.ok
assert((b(rec1(cli_install(i2())).phases[3].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).phases[3].ended)) == 1); // i2.r1.ph4.ended
assert((code(rec1(cli_install(i2())).phases[4].name)) == 5) by (compute_only);assert((code(rec1(cli_install(i2())).phases[4].name)) == 5); // i2.r1.ph5
assert((b(rec1(cli_install(i2())).phases[4].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).phases[4].ok)) == 1); // i2.r1.ph5.ok
assert((b(rec1(cli_install(i2())).phases[4].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i2())).phases[4].ended)) == 1); // i2.r1.ph5.ended
assert((cli_install(i3()).len()) == 1) by (compute_only);assert((cli_install(i3()).len()) == 1); // i3.rows
assert((act(rec1(cli_install(i3())))) == 2) by (compute_only);assert((act(rec1(cli_install(i3())))) == 2); // i3.r1.action
assert((b(rec1(cli_install(i3())).row.package == n_nosuch())) == 1) by (compute_only);assert((b(rec1(cli_install(i3())).row.package == n_nosuch())) == 1); // i3.r1.pkg
assert((b(rec1(cli_install(i3())).row.package.len() == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i3())).row.package.len() == 0)) == 0); // i3.r1.pkgempty
assert((b(ver_is(rec1(cli_install(i3())), placeholder()))) == 1) by (compute_only);assert((b(ver_is(rec1(cli_install(i3())), placeholder()))) == 1); // i3.r1.verplaceholder
assert((b(ver_is(rec1(cli_install(i3())), seq![]))) == 0) by (compute_only);assert((b(ver_is(rec1(cli_install(i3())), seq![]))) == 0); // i3.r1.ver
assert((b(rec1(cli_install(i3())).row.status == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i3())).row.status == 0)) == 0); // i3.r1.inprogress
assert((b(ok_of(rec1(cli_install(i3()))))) == 0) by (compute_only);assert((b(ok_of(rec1(cli_install(i3()))))) == 0); // i3.r1.ok
assert((b(rec1(cli_install(i3())).row.committed)) == 0) by (compute_only);assert((b(rec1(cli_install(i3())).row.committed)) == 0); // i3.r1.committed
assert((b(rec1(cli_install(i3())).row.ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i3())).row.ended)) == 1); // i3.r1.ended
assert((b(rec1(cli_install(i3())).row.finalized)) == 1) by (compute_only);assert((b(rec1(cli_install(i3())).row.finalized)) == 1); // i3.r1.finalized
assert((nph(rec1(cli_install(i3())))) == 0) by (compute_only);assert((nph(rec1(cli_install(i3())))) == 0); // i3.r1.nph
assert((cli_install(i4()).len()) == 0) by (compute_only);assert((cli_install(i4()).len()) == 0); // i4.rows
assert((api_install(i4()).len()) == 1) by (compute_only);assert((api_install(i4()).len()) == 1); // i4api.rows
assert((act(rec1(api_install(i4())))) == 2) by (compute_only);assert((act(rec1(api_install(i4())))) == 2); // i4api.r1.action
assert((b(rec1(api_install(i4())).row.package == seq![])) == 1) by (compute_only);assert((b(rec1(api_install(i4())).row.package == seq![])) == 1); // i4api.r1.pkg
assert((b(rec1(api_install(i4())).row.package.len() == 0)) == 1) by (compute_only);assert((b(rec1(api_install(i4())).row.package.len() == 0)) == 1); // i4api.r1.pkgempty
assert((b(ver_is(rec1(api_install(i4())), placeholder()))) == 1) by (compute_only);assert((b(ver_is(rec1(api_install(i4())), placeholder()))) == 1); // i4api.r1.verplaceholder
assert((b(ver_is(rec1(api_install(i4())), seq![]))) == 0) by (compute_only);assert((b(ver_is(rec1(api_install(i4())), seq![]))) == 0); // i4api.r1.ver
assert((b(rec1(api_install(i4())).row.status == 0)) == 0) by (compute_only);assert((b(rec1(api_install(i4())).row.status == 0)) == 0); // i4api.r1.inprogress
assert((b(ok_of(rec1(api_install(i4()))))) == 0) by (compute_only);assert((b(ok_of(rec1(api_install(i4()))))) == 0); // i4api.r1.ok
assert((b(rec1(api_install(i4())).row.committed)) == 0) by (compute_only);assert((b(rec1(api_install(i4())).row.committed)) == 0); // i4api.r1.committed
assert((b(rec1(api_install(i4())).row.ended)) == 1) by (compute_only);assert((b(rec1(api_install(i4())).row.ended)) == 1); // i4api.r1.ended
assert((b(rec1(api_install(i4())).row.finalized)) == 1) by (compute_only);assert((b(rec1(api_install(i4())).row.finalized)) == 1); // i4api.r1.finalized
assert((nph(rec1(api_install(i4())))) == 0) by (compute_only);assert((nph(rec1(api_install(i4())))) == 0); // i4api.r1.nph
assert((cli_install(i5()).len()) == 1) by (compute_only);assert((cli_install(i5()).len()) == 1); // i5.rows
assert((act(rec1(cli_install(i5())))) == 2) by (compute_only);assert((act(rec1(cli_install(i5())))) == 2); // i5.r1.action
assert((b(rec1(cli_install(i5())).row.package == n_h74c())) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).row.package == n_h74c())) == 1); // i5.r1.pkg
assert((b(rec1(cli_install(i5())).row.package.len() == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i5())).row.package.len() == 0)) == 0); // i5.r1.pkgempty
assert((b(ver_is(rec1(cli_install(i5())), placeholder()))) == 0) by (compute_only);assert((b(ver_is(rec1(cli_install(i5())), placeholder()))) == 0); // i5.r1.verplaceholder
assert((b(ver_is(rec1(cli_install(i5())), v100().text))) == 1) by (compute_only);assert((b(ver_is(rec1(cli_install(i5())), v100().text))) == 1); // i5.r1.ver
assert((b(rec1(cli_install(i5())).row.status == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i5())).row.status == 0)) == 0); // i5.r1.inprogress
assert((b(ok_of(rec1(cli_install(i5()))))) == 0) by (compute_only);assert((b(ok_of(rec1(cli_install(i5()))))) == 0); // i5.r1.ok
assert((b(rec1(cli_install(i5())).row.committed)) == 0) by (compute_only);assert((b(rec1(cli_install(i5())).row.committed)) == 0); // i5.r1.committed
assert((b(rec1(cli_install(i5())).row.ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).row.ended)) == 1); // i5.r1.ended
assert((b(rec1(cli_install(i5())).row.finalized)) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).row.finalized)) == 1); // i5.r1.finalized
assert((nph(rec1(cli_install(i5())))) == 4) by (compute_only);assert((nph(rec1(cli_install(i5())))) == 4); // i5.r1.nph
assert((code(rec1(cli_install(i5())).phases[0].name)) == 1) by (compute_only);assert((code(rec1(cli_install(i5())).phases[0].name)) == 1); // i5.r1.ph1
assert((b(rec1(cli_install(i5())).phases[0].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).phases[0].ok)) == 1); // i5.r1.ph1.ok
assert((b(rec1(cli_install(i5())).phases[0].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).phases[0].ended)) == 1); // i5.r1.ph1.ended
assert((code(rec1(cli_install(i5())).phases[1].name)) == 2) by (compute_only);assert((code(rec1(cli_install(i5())).phases[1].name)) == 2); // i5.r1.ph2
assert((b(rec1(cli_install(i5())).phases[1].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).phases[1].ok)) == 1); // i5.r1.ph2.ok
assert((b(rec1(cli_install(i5())).phases[1].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).phases[1].ended)) == 1); // i5.r1.ph2.ended
assert((code(rec1(cli_install(i5())).phases[2].name)) == 3) by (compute_only);assert((code(rec1(cli_install(i5())).phases[2].name)) == 3); // i5.r1.ph3
assert((b(rec1(cli_install(i5())).phases[2].ok)) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).phases[2].ok)) == 1); // i5.r1.ph3.ok
assert((b(rec1(cli_install(i5())).phases[2].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).phases[2].ended)) == 1); // i5.r1.ph3.ended
assert((code(rec1(cli_install(i5())).phases[3].name)) == 4) by (compute_only);assert((code(rec1(cli_install(i5())).phases[3].name)) == 4); // i5.r1.ph4
assert((b(rec1(cli_install(i5())).phases[3].ok)) == 0) by (compute_only);assert((b(rec1(cli_install(i5())).phases[3].ok)) == 0); // i5.r1.ph4.ok
assert((b(rec1(cli_install(i5())).phases[3].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).phases[3].ended)) == 1); // i5.r1.ph4.ended
assert((b(after_version(None, i5(), false) is Some)) == 0) by (compute_only);assert((b(after_version(None, i5(), false) is Some)) == 0); // i5.installed.h74c
assert((cli_reinstall(installed0(), missing(n_nosuch())).len()) == 0) by (compute_only);assert((cli_reinstall(installed0(), missing(n_nosuch())).len()) == 0); // r1.rows
assert((cli_update(u1()).len()) == 1) by (compute_only);assert((cli_update(u1()).len()) == 1); // u1.rows
assert((act(rec1(cli_update(u1())))) == 4) by (compute_only);assert((act(rec1(cli_update(u1())))) == 4); // u1.r1.action
assert((b(rec1(cli_update(u1())).row.package == n_h74u())) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).row.package == n_h74u())) == 1); // u1.r1.pkg
assert((b(rec1(cli_update(u1())).row.package.len() == 0)) == 0) by (compute_only);assert((b(rec1(cli_update(u1())).row.package.len() == 0)) == 0); // u1.r1.pkgempty
assert((b(ver_is(rec1(cli_update(u1())), placeholder()))) == 0) by (compute_only);assert((b(ver_is(rec1(cli_update(u1())), placeholder()))) == 0); // u1.r1.verplaceholder
assert((b(ver_is(rec1(cli_update(u1())), v110().text))) == 1) by (compute_only);assert((b(ver_is(rec1(cli_update(u1())), v110().text))) == 1); // u1.r1.ver
assert((b(rec1(cli_update(u1())).row.status == 0)) == 0) by (compute_only);assert((b(rec1(cli_update(u1())).row.status == 0)) == 0); // u1.r1.inprogress
assert((b(ok_of(rec1(cli_update(u1()))))) == 1) by (compute_only);assert((b(ok_of(rec1(cli_update(u1()))))) == 1); // u1.r1.ok
assert((b(rec1(cli_update(u1())).row.committed)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).row.committed)) == 1); // u1.r1.committed
assert((b(rec1(cli_update(u1())).row.ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).row.ended)) == 1); // u1.r1.ended
assert((b(rec1(cli_update(u1())).row.finalized)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).row.finalized)) == 1); // u1.r1.finalized
assert((nph(rec1(cli_update(u1())))) == 6) by (compute_only);assert((nph(rec1(cli_update(u1())))) == 6); // u1.r1.nph
assert((code(rec1(cli_update(u1())).phases[0].name)) == 1) by (compute_only);assert((code(rec1(cli_update(u1())).phases[0].name)) == 1); // u1.r1.ph1
assert((b(rec1(cli_update(u1())).phases[0].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[0].ok)) == 1); // u1.r1.ph1.ok
assert((b(rec1(cli_update(u1())).phases[0].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[0].ended)) == 1); // u1.r1.ph1.ended
assert((code(rec1(cli_update(u1())).phases[1].name)) == 2) by (compute_only);assert((code(rec1(cli_update(u1())).phases[1].name)) == 2); // u1.r1.ph2
assert((b(rec1(cli_update(u1())).phases[1].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[1].ok)) == 1); // u1.r1.ph2.ok
assert((b(rec1(cli_update(u1())).phases[1].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[1].ended)) == 1); // u1.r1.ph2.ended
assert((code(rec1(cli_update(u1())).phases[2].name)) == 3) by (compute_only);assert((code(rec1(cli_update(u1())).phases[2].name)) == 3); // u1.r1.ph3
assert((b(rec1(cli_update(u1())).phases[2].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[2].ok)) == 1); // u1.r1.ph3.ok
assert((b(rec1(cli_update(u1())).phases[2].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[2].ended)) == 1); // u1.r1.ph3.ended
assert((code(rec1(cli_update(u1())).phases[3].name)) == 4) by (compute_only);assert((code(rec1(cli_update(u1())).phases[3].name)) == 4); // u1.r1.ph4
assert((b(rec1(cli_update(u1())).phases[3].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[3].ok)) == 1); // u1.r1.ph4.ok
assert((b(rec1(cli_update(u1())).phases[3].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[3].ended)) == 1); // u1.r1.ph4.ended
assert((code(rec1(cli_update(u1())).phases[4].name)) == 5) by (compute_only);assert((code(rec1(cli_update(u1())).phases[4].name)) == 5); // u1.r1.ph5
assert((b(rec1(cli_update(u1())).phases[4].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[4].ok)) == 1); // u1.r1.ph5.ok
assert((b(rec1(cli_update(u1())).phases[4].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[4].ended)) == 1); // u1.r1.ph5.ended
assert((code(rec1(cli_update(u1())).phases[5].name)) == 6) by (compute_only);assert((code(rec1(cli_update(u1())).phases[5].name)) == 6); // u1.r1.ph6
assert((b(rec1(cli_update(u1())).phases[5].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[5].ok)) == 1); // u1.r1.ph6.ok
assert((b(rec1(cli_update(u1())).phases[5].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u1())).phases[5].ended)) == 1); // u1.r1.ph6.ended
assert((b(after_version(Some(v100()), u1().inst, true) == Some(v110()))) == 1) by (compute_only);assert((b(after_version(Some(v100()), u1().inst, true) == Some(v110()))) == 1); // u1.version.h74u
assert((cli_update(u2()).len()) == 1) by (compute_only);assert((cli_update(u2()).len()) == 1); // u2.rows
assert((act(rec1(cli_update(u2())))) == 4) by (compute_only);assert((act(rec1(cli_update(u2())))) == 4); // u2.r1.action
assert((b(rec1(cli_update(u2())).row.package == n_h74f())) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).row.package == n_h74f())) == 1); // u2.r1.pkg
assert((b(rec1(cli_update(u2())).row.package.len() == 0)) == 0) by (compute_only);assert((b(rec1(cli_update(u2())).row.package.len() == 0)) == 0); // u2.r1.pkgempty
assert((b(ver_is(rec1(cli_update(u2())), placeholder()))) == 0) by (compute_only);assert((b(ver_is(rec1(cli_update(u2())), placeholder()))) == 0); // u2.r1.verplaceholder
assert((b(ver_is(rec1(cli_update(u2())), v110().text))) == 1) by (compute_only);assert((b(ver_is(rec1(cli_update(u2())), v110().text))) == 1); // u2.r1.ver
assert((b(rec1(cli_update(u2())).row.status == 0)) == 0) by (compute_only);assert((b(rec1(cli_update(u2())).row.status == 0)) == 0); // u2.r1.inprogress
assert((b(ok_of(rec1(cli_update(u2()))))) == 0) by (compute_only);assert((b(ok_of(rec1(cli_update(u2()))))) == 0); // u2.r1.ok
assert((b(rec1(cli_update(u2())).row.committed)) == 0) by (compute_only);assert((b(rec1(cli_update(u2())).row.committed)) == 0); // u2.r1.committed
assert((b(rec1(cli_update(u2())).row.ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).row.ended)) == 1); // u2.r1.ended
assert((b(rec1(cli_update(u2())).row.finalized)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).row.finalized)) == 1); // u2.r1.finalized
assert((nph(rec1(cli_update(u2())))) == 6) by (compute_only);assert((nph(rec1(cli_update(u2())))) == 6); // u2.r1.nph
assert((code(rec1(cli_update(u2())).phases[0].name)) == 1) by (compute_only);assert((code(rec1(cli_update(u2())).phases[0].name)) == 1); // u2.r1.ph1
assert((b(rec1(cli_update(u2())).phases[0].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[0].ok)) == 1); // u2.r1.ph1.ok
assert((b(rec1(cli_update(u2())).phases[0].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[0].ended)) == 1); // u2.r1.ph1.ended
assert((code(rec1(cli_update(u2())).phases[1].name)) == 2) by (compute_only);assert((code(rec1(cli_update(u2())).phases[1].name)) == 2); // u2.r1.ph2
assert((b(rec1(cli_update(u2())).phases[1].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[1].ok)) == 1); // u2.r1.ph2.ok
assert((b(rec1(cli_update(u2())).phases[1].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[1].ended)) == 1); // u2.r1.ph2.ended
assert((code(rec1(cli_update(u2())).phases[2].name)) == 3) by (compute_only);assert((code(rec1(cli_update(u2())).phases[2].name)) == 3); // u2.r1.ph3
assert((b(rec1(cli_update(u2())).phases[2].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[2].ok)) == 1); // u2.r1.ph3.ok
assert((b(rec1(cli_update(u2())).phases[2].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[2].ended)) == 1); // u2.r1.ph3.ended
assert((code(rec1(cli_update(u2())).phases[3].name)) == 4) by (compute_only);assert((code(rec1(cli_update(u2())).phases[3].name)) == 4); // u2.r1.ph4
assert((b(rec1(cli_update(u2())).phases[3].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[3].ok)) == 1); // u2.r1.ph4.ok
assert((b(rec1(cli_update(u2())).phases[3].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[3].ended)) == 1); // u2.r1.ph4.ended
assert((code(rec1(cli_update(u2())).phases[4].name)) == 5) by (compute_only);assert((code(rec1(cli_update(u2())).phases[4].name)) == 5); // u2.r1.ph5
assert((b(rec1(cli_update(u2())).phases[4].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[4].ok)) == 1); // u2.r1.ph5.ok
assert((b(rec1(cli_update(u2())).phases[4].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[4].ended)) == 1); // u2.r1.ph5.ended
assert((code(rec1(cli_update(u2())).phases[5].name)) == 6) by (compute_only);assert((code(rec1(cli_update(u2())).phases[5].name)) == 6); // u2.r1.ph6
assert((b(rec1(cli_update(u2())).phases[5].ok)) == 0) by (compute_only);assert((b(rec1(cli_update(u2())).phases[5].ok)) == 0); // u2.r1.ph6.ok
assert((b(rec1(cli_update(u2())).phases[5].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u2())).phases[5].ended)) == 1); // u2.r1.ph6.ended
assert((b(after_version(Some(v100()), u2().inst, true) == Some(v100()))) == 1) by (compute_only);assert((b(after_version(Some(v100()), u2().inst, true) == Some(v100()))) == 1); // u2.version.h74f
assert((cli_update(u3()).len()) == 1) by (compute_only);assert((cli_update(u3()).len()) == 1); // u3.rows
assert((act(rec1(cli_update(u3())))) == 4) by (compute_only);assert((act(rec1(cli_update(u3())))) == 4); // u3.r1.action
assert((b(rec1(cli_update(u3())).row.package == n_h74f())) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).row.package == n_h74f())) == 1); // u3.r1.pkg
assert((b(rec1(cli_update(u3())).row.package.len() == 0)) == 0) by (compute_only);assert((b(rec1(cli_update(u3())).row.package.len() == 0)) == 0); // u3.r1.pkgempty
assert((b(ver_is(rec1(cli_update(u3())), placeholder()))) == 0) by (compute_only);assert((b(ver_is(rec1(cli_update(u3())), placeholder()))) == 0); // u3.r1.verplaceholder
assert((b(ver_is(rec1(cli_update(u3())), v110().text))) == 1) by (compute_only);assert((b(ver_is(rec1(cli_update(u3())), v110().text))) == 1); // u3.r1.ver
assert((b(rec1(cli_update(u3())).row.status == 0)) == 0) by (compute_only);assert((b(rec1(cli_update(u3())).row.status == 0)) == 0); // u3.r1.inprogress
assert((b(ok_of(rec1(cli_update(u3()))))) == 0) by (compute_only);assert((b(ok_of(rec1(cli_update(u3()))))) == 0); // u3.r1.ok
assert((b(rec1(cli_update(u3())).row.committed)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).row.committed)) == 1); // u3.r1.committed
assert((b(rec1(cli_update(u3())).row.ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).row.ended)) == 1); // u3.r1.ended
assert((b(rec1(cli_update(u3())).row.finalized)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).row.finalized)) == 1); // u3.r1.finalized
assert((nph(rec1(cli_update(u3())))) == 6) by (compute_only);assert((nph(rec1(cli_update(u3())))) == 6); // u3.r1.nph
assert((code(rec1(cli_update(u3())).phases[0].name)) == 1) by (compute_only);assert((code(rec1(cli_update(u3())).phases[0].name)) == 1); // u3.r1.ph1
assert((b(rec1(cli_update(u3())).phases[0].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[0].ok)) == 1); // u3.r1.ph1.ok
assert((b(rec1(cli_update(u3())).phases[0].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[0].ended)) == 1); // u3.r1.ph1.ended
assert((code(rec1(cli_update(u3())).phases[1].name)) == 2) by (compute_only);assert((code(rec1(cli_update(u3())).phases[1].name)) == 2); // u3.r1.ph2
assert((b(rec1(cli_update(u3())).phases[1].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[1].ok)) == 1); // u3.r1.ph2.ok
assert((b(rec1(cli_update(u3())).phases[1].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[1].ended)) == 1); // u3.r1.ph2.ended
assert((code(rec1(cli_update(u3())).phases[2].name)) == 3) by (compute_only);assert((code(rec1(cli_update(u3())).phases[2].name)) == 3); // u3.r1.ph3
assert((b(rec1(cli_update(u3())).phases[2].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[2].ok)) == 1); // u3.r1.ph3.ok
assert((b(rec1(cli_update(u3())).phases[2].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[2].ended)) == 1); // u3.r1.ph3.ended
assert((code(rec1(cli_update(u3())).phases[3].name)) == 4) by (compute_only);assert((code(rec1(cli_update(u3())).phases[3].name)) == 4); // u3.r1.ph4
assert((b(rec1(cli_update(u3())).phases[3].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[3].ok)) == 1); // u3.r1.ph4.ok
assert((b(rec1(cli_update(u3())).phases[3].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[3].ended)) == 1); // u3.r1.ph4.ended
assert((code(rec1(cli_update(u3())).phases[4].name)) == 5) by (compute_only);assert((code(rec1(cli_update(u3())).phases[4].name)) == 5); // u3.r1.ph5
assert((b(rec1(cli_update(u3())).phases[4].ok)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[4].ok)) == 1); // u3.r1.ph5.ok
assert((b(rec1(cli_update(u3())).phases[4].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[4].ended)) == 1); // u3.r1.ph5.ended
assert((code(rec1(cli_update(u3())).phases[5].name)) == 6) by (compute_only);assert((code(rec1(cli_update(u3())).phases[5].name)) == 6); // u3.r1.ph6
assert((b(rec1(cli_update(u3())).phases[5].ok)) == 0) by (compute_only);assert((b(rec1(cli_update(u3())).phases[5].ok)) == 0); // u3.r1.ph6.ok
assert((b(rec1(cli_update(u3())).phases[5].ended)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).phases[5].ended)) == 1); // u3.r1.ph6.ended
assert((b(after_version(Some(v100()), u3().inst, true) == Some(v110()))) == 1) by (compute_only);assert((b(after_version(Some(v100()), u3().inst, true) == Some(v110()))) == 1); // u3.version.h74f
assert((cli_update(u4()).len()) == 0) by (compute_only);assert((cli_update(u4()).len()) == 0); // u4.rows
assert((cli_update(u5()).len()) == 0) by (compute_only);assert((cli_update(u5()).len()) == 0); // u5.rows
assert((cli_install(i6()).len()) == 1) by (compute_only);assert((cli_install(i6()).len()) == 1); // i6.rows
assert((act(rec1(cli_install(i6())))) == 2) by (compute_only);assert((act(rec1(cli_install(i6())))) == 2); // i6.r1.action
assert((b(rec1(cli_install(i6())).row.package == n_h74u())) == 1) by (compute_only);assert((b(rec1(cli_install(i6())).row.package == n_h74u())) == 1); // i6.r1.pkg
assert((b(rec1(cli_install(i6())).row.package.len() == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i6())).row.package.len() == 0)) == 0); // i6.r1.pkgempty
assert((b(ver_is(rec1(cli_install(i6())), placeholder()))) == 0) by (compute_only);assert((b(ver_is(rec1(cli_install(i6())), placeholder()))) == 0); // i6.r1.verplaceholder
assert((b(ver_is(rec1(cli_install(i6())), v100().text))) == 1) by (compute_only);assert((b(ver_is(rec1(cli_install(i6())), v100().text))) == 1); // i6.r1.ver
assert((b(rec1(cli_install(i6())).row.status == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i6())).row.status == 0)) == 0); // i6.r1.inprogress
assert((b(ok_of(rec1(cli_install(i6()))))) == 0) by (compute_only);assert((b(ok_of(rec1(cli_install(i6()))))) == 0); // i6.r1.ok
assert((b(rec1(cli_install(i6())).row.committed)) == 0) by (compute_only);assert((b(rec1(cli_install(i6())).row.committed)) == 0); // i6.r1.committed
assert((b(rec1(cli_install(i6())).row.ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i6())).row.ended)) == 1); // i6.r1.ended
assert((b(rec1(cli_install(i6())).row.finalized)) == 1) by (compute_only);assert((b(rec1(cli_install(i6())).row.finalized)) == 1); // i6.r1.finalized
assert((nph(rec1(cli_install(i6())))) == 0) by (compute_only);assert((nph(rec1(cli_install(i6())))) == 0); // i6.r1.nph
assert((b(after_version(Some(v110()), i6(), false) == Some(v110()))) == 1) by (compute_only);assert((b(after_version(Some(v110()), i6(), false) == Some(v110()))) == 1); // i6.version.h74u
assert((cli_install(i7()).len()) == 1) by (compute_only);assert((cli_install(i7()).len()) == 1); // i7.rows
assert((act(rec1(cli_install(i7())))) == 2) by (compute_only);assert((act(rec1(cli_install(i7())))) == 2); // i7.r1.action
assert((b(rec1(cli_install(i7())).row.package == n_h74u())) == 1) by (compute_only);assert((b(rec1(cli_install(i7())).row.package == n_h74u())) == 1); // i7.r1.pkg
assert((b(rec1(cli_install(i7())).row.package.len() == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i7())).row.package.len() == 0)) == 0); // i7.r1.pkgempty
assert((b(ver_is(rec1(cli_install(i7())), placeholder()))) == 0) by (compute_only);assert((b(ver_is(rec1(cli_install(i7())), placeholder()))) == 0); // i7.r1.verplaceholder
assert((b(ver_is(rec1(cli_install(i7())), v100().text))) == 1) by (compute_only);assert((b(ver_is(rec1(cli_install(i7())), v100().text))) == 1); // i7.r1.ver
assert((b(rec1(cli_install(i7())).row.status == 0)) == 0) by (compute_only);assert((b(rec1(cli_install(i7())).row.status == 0)) == 0); // i7.r1.inprogress
assert((b(ok_of(rec1(cli_install(i7()))))) == 0) by (compute_only);assert((b(ok_of(rec1(cli_install(i7()))))) == 0); // i7.r1.ok
assert((b(rec1(cli_install(i7())).row.committed)) == 0) by (compute_only);assert((b(rec1(cli_install(i7())).row.committed)) == 0); // i7.r1.committed
assert((b(rec1(cli_install(i7())).row.ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i7())).row.ended)) == 1); // i7.r1.ended
assert((b(rec1(cli_install(i7())).row.finalized)) == 1) by (compute_only);assert((b(rec1(cli_install(i7())).row.finalized)) == 1); // i7.r1.finalized
assert((nph(rec1(cli_install(i7())))) == 0) by (compute_only);assert((nph(rec1(cli_install(i7())))) == 0); // i7.r1.nph
assert((b(after_version(Some(v110()), i7(), false) == Some(v110()))) == 1) by (compute_only);assert((b(after_version(Some(v110()), i7(), false) == Some(v110()))) == 1); // i7.version.h74u
assert((b(running_ok(twin_while_running(h::Action::Install, n_h74a()), h::Action::Install, n_h74a()))) == 1) by (compute_only);assert((b(running_ok(twin_while_running(h::Action::Install, n_h74a()), h::Action::Install, n_h74a()))) == 1); // mid.install
assert((nph(twin_while_running(h::Action::Install, n_h74a()))) == 0) by (compute_only);assert((nph(twin_while_running(h::Action::Install, n_h74a()))) == 0); // mid.install.permph
assert((started_through(Ph::Activate, Ph::Activate)) == 5) by (compute_only);assert((started_through(Ph::Activate, Ph::Activate)) == 5); // mid.install.tempph
assert((b(running_ok(twin_while_running(h::Action::Update, n_h74u()), h::Action::Update, n_h74u()))) == 1) by (compute_only);assert((b(running_ok(twin_while_running(h::Action::Update, n_h74u()), h::Action::Update, n_h74u()))) == 1); // mid.update
assert((nph(twin_while_running(h::Action::Update, n_h74u()))) == 0) by (compute_only);assert((nph(twin_while_running(h::Action::Update, n_h74u()))) == 0); // mid.update.permph
assert((started_through(Ph::ApplyUpdateSteps, Ph::ApplyUpdateSteps)) == 6) by (compute_only);assert((started_through(Ph::ApplyUpdateSteps, Ph::ApplyUpdateSteps)) == 6); // mid.update.tempph
assert((b(running_ok(twin_while_running(h::Action::Uninstall, pkg_of(5)), h::Action::Uninstall, pkg_of(5)))) == 1) by (compute_only);assert((b(running_ok(twin_while_running(h::Action::Uninstall, pkg_of(5)), h::Action::Uninstall, pkg_of(5)))) == 1); // mid.uninstall
assert((nph(twin_while_running(h::Action::Uninstall, pkg_of(5)))) == 0) by (compute_only);assert((nph(twin_while_running(h::Action::Uninstall, pkg_of(5)))) == 0); // mid.uninstall.permph
assert((started_through(Ph::Clean, Ph::Clean)) == 1) by (compute_only);assert((started_through(Ph::Clean, Ph::Clean)) == 1); // mid.uninstall.tempph
uninstall_fields();assert((n1_rs().len()) == 1); // n1.rows
uninstall_fields();assert((act(n1_rs()[0])) == 3); // n1.r1.action
uninstall_fields();assert((b(n1_rs()[0].row.package == pkg_of(2))) == 1); // n1.r1.pkg
uninstall_fields();assert((b(n1_rs()[0].row.package.len() == 0)) == 0); // n1.r1.pkgempty
uninstall_fields();assert((b(ver_is(n1_rs()[0], placeholder()))) == 0); // n1.r1.verplaceholder
uninstall_fields();assert((b(ver_is(n1_rs()[0], fix_ver()))) == 1); // n1.r1.ver
uninstall_fields();assert((b(n1_rs()[0].row.status == 0)) == 0); // n1.r1.inprogress
uninstall_fields();assert((b(ok_of(n1_rs()[0]))) == 1); // n1.r1.ok
uninstall_fields();assert((b(n1_rs()[0].row.committed)) == 1); // n1.r1.committed
uninstall_fields();assert((b(n1_rs()[0].row.ended)) == 1); // n1.r1.ended
uninstall_fields();assert((b(n1_rs()[0].row.finalized)) == 1); // n1.r1.finalized
uninstall_fields();assert((nph(n1_rs()[0])) == 1); // n1.r1.nph
uninstall_fields();assert((code(n1_rs()[0].phases[0].name)) == 7); // n1.r1.ph1
uninstall_fields();assert((b(n1_rs()[0].phases[0].ok)) == 1); // n1.r1.ph1.ok
uninstall_fields();assert((b(n1_rs()[0].phases[0].ended)) == 1); // n1.r1.ph1.ended
uninstall_fields();assert((b(unin_out(w_single(), 2, false, false, false, 3).w.mods.contains_key(2))) == 0); // n1.installed.h74s
uninstall_fields();assert((n2_rs().len()) == 1); // n2.rows
uninstall_fields();assert((act(n2_rs()[0])) == 3); // n2.r1.action
uninstall_fields();assert((b(n2_rs()[0].row.package == pkg_of(9))) == 1); // n2.r1.pkg
uninstall_fields();assert((b(n2_rs()[0].row.package.len() == 0)) == 0); // n2.r1.pkgempty
uninstall_fields();assert((b(ver_is(n2_rs()[0], placeholder()))) == 1); // n2.r1.verplaceholder
uninstall_fields();assert((b(ver_is(n2_rs()[0], fix_ver()))) == 0); // n2.r1.ver
uninstall_fields();assert((b(n2_rs()[0].row.status == 0)) == 0); // n2.r1.inprogress
uninstall_fields();assert((b(ok_of(n2_rs()[0]))) == 0); // n2.r1.ok
uninstall_fields();assert((b(n2_rs()[0].row.committed)) == 0); // n2.r1.committed
uninstall_fields();assert((b(n2_rs()[0].row.ended)) == 1); // n2.r1.ended
uninstall_fields();assert((b(n2_rs()[0].row.finalized)) == 1); // n2.r1.finalized
uninstall_fields();assert((nph(n2_rs()[0])) == 0); // n2.r1.nph
uninstall_fields();assert((n3_rs().len()) == 0); // n3.rows
assert((n4_rs().len()) == 1) by (compute_only);assert((n4_rs().len()) == 1); // n4.rows
assert((act(n4_rs()[0])) == 3) by (compute_only);assert((act(n4_rs()[0])) == 3); // n4.r1.action
assert((b(n4_rs()[0].row.package == pkg_of(5))) == 1) by (compute_only);assert((b(n4_rs()[0].row.package == pkg_of(5))) == 1); // n4.r1.pkg
assert((b(n4_rs()[0].row.package.len() == 0)) == 0) by (compute_only);assert((b(n4_rs()[0].row.package.len() == 0)) == 0); // n4.r1.pkgempty
assert((b(ver_is(n4_rs()[0], placeholder()))) == 0) by (compute_only);assert((b(ver_is(n4_rs()[0], placeholder()))) == 0); // n4.r1.verplaceholder
assert((b(ver_is(n4_rs()[0], fix_ver()))) == 1) by (compute_only);assert((b(ver_is(n4_rs()[0], fix_ver()))) == 1); // n4.r1.ver
assert((b(n4_rs()[0].row.status == 0)) == 0) by (compute_only);assert((b(n4_rs()[0].row.status == 0)) == 0); // n4.r1.inprogress
assert((b(ok_of(n4_rs()[0]))) == 0) by (compute_only);assert((b(ok_of(n4_rs()[0]))) == 0); // n4.r1.ok
assert((b(n4_rs()[0].row.committed)) == 0) by (compute_only);assert((b(n4_rs()[0].row.committed)) == 0); // n4.r1.committed
assert((b(n4_rs()[0].row.ended)) == 1) by (compute_only);assert((b(n4_rs()[0].row.ended)) == 1); // n4.r1.ended
assert((b(n4_rs()[0].row.finalized)) == 1) by (compute_only);assert((b(n4_rs()[0].row.finalized)) == 1); // n4.r1.finalized
assert((nph(n4_rs()[0])) == 1) by (compute_only);assert((nph(n4_rs()[0])) == 1); // n4.r1.nph
assert((code(n4_rs()[0].phases[0].name)) == 7) by (compute_only);assert((code(n4_rs()[0].phases[0].name)) == 7); // n4.r1.ph1
assert((b(n4_rs()[0].phases[0].ok)) == 0) by (compute_only);assert((b(n4_rs()[0].phases[0].ok)) == 0); // n4.r1.ph1.ok
assert((b(n4_rs()[0].phases[0].ended)) == 1) by (compute_only);assert((b(n4_rs()[0].phases[0].ended)) == 1); // n4.r1.ph1.ended
uninstall_fields();assert((n5_rs().len()) == 2); // n5.rows
uninstall_fields();assert((act(n5_rs()[0])) == 3); // n5.r1.action
uninstall_fields();assert((b(n5_rs()[0].row.package == pkg_of(3))) == 1); // n5.r1.pkg
uninstall_fields();assert((b(n5_rs()[0].row.package.len() == 0)) == 0); // n5.r1.pkgempty
uninstall_fields();assert((b(ver_is(n5_rs()[0], placeholder()))) == 0); // n5.r1.verplaceholder
uninstall_fields();assert((b(ver_is(n5_rs()[0], fix_ver()))) == 1); // n5.r1.ver
uninstall_fields();assert((b(n5_rs()[0].row.status == 0)) == 0); // n5.r1.inprogress
uninstall_fields();assert((b(ok_of(n5_rs()[0]))) == 0); // n5.r1.ok
uninstall_fields();assert((b(n5_rs()[0].row.committed)) == 0); // n5.r1.committed
uninstall_fields();assert((b(n5_rs()[0].row.ended)) == 1); // n5.r1.ended
uninstall_fields();assert((b(n5_rs()[0].row.finalized)) == 1); // n5.r1.finalized
uninstall_fields();assert((nph(n5_rs()[0])) == 1); // n5.r1.nph
uninstall_fields();assert((code(n5_rs()[0].phases[0].name)) == 7); // n5.r1.ph1
uninstall_fields();assert((b(n5_rs()[0].phases[0].ok)) == 0); // n5.r1.ph1.ok
uninstall_fields();assert((b(n5_rs()[0].phases[0].ended)) == 1); // n5.r1.ph1.ended
uninstall_fields();assert((act(n5_rs()[1])) == 3); // n5.r2.action
uninstall_fields();assert((b(n5_rs()[1].row.package == pkg_of(3))) == 0); // n5.r2.pkg
uninstall_fields();assert((b(n5_rs()[1].row.package.len() == 0)) == 0); // n5.r2.pkgempty
uninstall_fields();assert((b(ver_is(n5_rs()[1], placeholder()))) == 0); // n5.r2.verplaceholder
uninstall_fields();assert((b(ver_is(n5_rs()[1], fix_ver()))) == 1); // n5.r2.ver
uninstall_fields();assert((b(n5_rs()[1].row.status == 0)) == 0); // n5.r2.inprogress
uninstall_fields();assert((b(ok_of(n5_rs()[1]))) == 0); // n5.r2.ok
uninstall_fields();assert((b(n5_rs()[1].row.committed)) == 0); // n5.r2.committed
uninstall_fields();assert((b(n5_rs()[1].row.ended)) == 1); // n5.r2.ended
uninstall_fields();assert((b(n5_rs()[1].row.finalized)) == 1); // n5.r2.finalized
uninstall_fields();assert((nph(n5_rs()[1])) == 1); // n5.r2.nph
uninstall_fields();assert((code(n5_rs()[1].phases[0].name)) == 7); // n5.r2.ph1
uninstall_fields();assert((b(n5_rs()[1].phases[0].ok)) == 0); // n5.r2.ph1.ok
uninstall_fields();assert((b(n5_rs()[1].phases[0].ended)) == 1); // n5.r2.ph1.ended
uninstall_fields();assert((b(unin_out(w_hookfail(), 3, false, true, false, 3).w.mods.contains_key(3))) == 1); // n5.installed.h74y
uninstall_fields();assert((b(unin_out(w_hookfail(), 3, false, true, false, 3).w.mods.contains_key(2))) == 1); // n5.installed.h74z
uninstall_fields();assert((b(unin_out(w_hookfail(), 3, false, true, false, 3).w.present.contains(12))) == 0); // n5.class.H74y.A
uninstall_fields();assert((b(unin_out(w_hookfail(), 3, false, true, false, 3).w.present.contains(10))) == 1); // n5.class.H74z.A
uninstall_fields();assert((c1_rs().len()) == 1); // c1.rows
uninstall_fields();assert((b(unin::clean_cmd(unin::w_recremove(), 3, false, false, 3).w.mods.contains_key(2))) == 0); // c1.installed.h74d
uninstall_fields();assert((b(unin::clean_cmd(unin::w_recremove(), 3, false, false, 3).w.mods.contains_key(3))) == 1); // c1.installed.h74m
uninstall_fields();assert((c2_rows()) == 0); // c2.rows
uninstall_fields();assert((act(c1_rs()[0])) == 3); // c1.r1.action
uninstall_fields();assert((b(c1_rs()[0].row.package == pkg_of(2))) == 1); // c1.r1.pkg
uninstall_fields();assert((b(c1_rs()[0].row.package.len() == 0)) == 0); // c1.r1.pkgempty
uninstall_fields();assert((b(ver_is(c1_rs()[0], placeholder()))) == 0); // c1.r1.verplaceholder
uninstall_fields();assert((b(ver_is(c1_rs()[0], fix_ver()))) == 1); // c1.r1.ver
uninstall_fields();assert((b(c1_rs()[0].row.status == 0)) == 0); // c1.r1.inprogress
uninstall_fields();assert((b(ok_of(c1_rs()[0]))) == 1); // c1.r1.ok
uninstall_fields();assert((b(c1_rs()[0].row.committed)) == 1); // c1.r1.committed
uninstall_fields();assert((b(c1_rs()[0].row.ended)) == 1); // c1.r1.ended
uninstall_fields();assert((b(c1_rs()[0].row.finalized)) == 1); // c1.r1.finalized
uninstall_fields();assert((nph(c1_rs()[0])) == 1); // c1.r1.nph
uninstall_fields();assert((code(c1_rs()[0].phases[0].name)) == 7); // c1.r1.ph1
uninstall_fields();assert((b(c1_rs()[0].phases[0].ok)) == 1); // c1.r1.ph1.ok
uninstall_fields();assert((b(c1_rs()[0].phases[0].ended)) == 1); // c1.r1.ph1.ended
t1_facts();assert((cmd_rows(Cmd::Phase(Ph::Test), w_single(), 2, 3, t1_scoped()).len()) == 1); // t1.rows
assert((act(t1_row())) == 1) by (compute_only);assert((act(t1_row())) == 1); // t1.r1.action
assert((b(t1_row().row.package == n_h74b())) == 1) by (compute_only);assert((b(t1_row().row.package == n_h74b())) == 1); // t1.r1.pkg
assert((b(t1_row().row.package.len() == 0)) == 0) by (compute_only);assert((b(t1_row().row.package.len() == 0)) == 0); // t1.r1.pkgempty
assert((b(ver_is(t1_row(), placeholder()))) == 0) by (compute_only);assert((b(ver_is(t1_row(), placeholder()))) == 0); // t1.r1.verplaceholder
assert((b(ver_is(t1_row(), v100().text))) == 1) by (compute_only);assert((b(ver_is(t1_row(), v100().text))) == 1); // t1.r1.ver
assert((b(t1_row().row.status == 0)) == 0) by (compute_only);assert((b(t1_row().row.status == 0)) == 0); // t1.r1.inprogress
assert((b(ok_of(t1_row()))) == 1) by (compute_only);assert((b(ok_of(t1_row()))) == 1); // t1.r1.ok
assert((b(t1_row().row.committed)) == 1) by (compute_only);assert((b(t1_row().row.committed)) == 1); // t1.r1.committed
assert((b(t1_row().row.ended)) == 1) by (compute_only);assert((b(t1_row().row.ended)) == 1); // t1.r1.ended
assert((b(t1_row().row.finalized)) == 1) by (compute_only);assert((b(t1_row().row.finalized)) == 1); // t1.r1.finalized
assert((nph(t1_row())) == 5) by (compute_only);assert((nph(t1_row())) == 5); // t1.r1.nph
assert((code(t1_row().phases[0].name)) == 1) by (compute_only);assert((code(t1_row().phases[0].name)) == 1); // t1.r1.ph1
assert((b(t1_row().phases[0].ok)) == 1) by (compute_only);assert((b(t1_row().phases[0].ok)) == 1); // t1.r1.ph1.ok
assert((b(t1_row().phases[0].ended)) == 1) by (compute_only);assert((b(t1_row().phases[0].ended)) == 1); // t1.r1.ph1.ended
assert((code(t1_row().phases[1].name)) == 2) by (compute_only);assert((code(t1_row().phases[1].name)) == 2); // t1.r1.ph2
assert((b(t1_row().phases[1].ok)) == 1) by (compute_only);assert((b(t1_row().phases[1].ok)) == 1); // t1.r1.ph2.ok
assert((b(t1_row().phases[1].ended)) == 1) by (compute_only);assert((b(t1_row().phases[1].ended)) == 1); // t1.r1.ph2.ended
assert((code(t1_row().phases[2].name)) == 3) by (compute_only);assert((code(t1_row().phases[2].name)) == 3); // t1.r1.ph3
assert((b(t1_row().phases[2].ok)) == 1) by (compute_only);assert((b(t1_row().phases[2].ok)) == 1); // t1.r1.ph3.ok
assert((b(t1_row().phases[2].ended)) == 1) by (compute_only);assert((b(t1_row().phases[2].ended)) == 1); // t1.r1.ph3.ended
assert((code(t1_row().phases[3].name)) == 4) by (compute_only);assert((code(t1_row().phases[3].name)) == 4); // t1.r1.ph4
assert((b(t1_row().phases[3].ok)) == 1) by (compute_only);assert((b(t1_row().phases[3].ok)) == 1); // t1.r1.ph4.ok
assert((b(t1_row().phases[3].ended)) == 1) by (compute_only);assert((b(t1_row().phases[3].ended)) == 1); // t1.r1.ph4.ended
assert((code(t1_row().phases[4].name)) == 5) by (compute_only);assert((code(t1_row().phases[4].name)) == 5); // t1.r1.ph5
assert((b(t1_row().phases[4].ok)) == 1) by (compute_only);assert((b(t1_row().phases[4].ok)) == 1); // t1.r1.ph5.ok
assert((b(t1_row().phases[4].ended)) == 1) by (compute_only);assert((b(t1_row().phases[4].ended)) == 1); // t1.r1.ph5.ended
assert((b(lnm(set_version(init(h::Action::Load, n_h74b()), v100().text), t1_dep(), false).kept)) == 1) by (compute_only);assert((b(lnm(set_version(init(h::Action::Load, n_h74b()), v100().text), t1_dep(), false).kept)) == 1); // t1.installed.h74b
}
}
fn main() {}
