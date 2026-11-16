#[path="../../../../Verus_verification/consolidated/iter23/status_reader.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(utf8(plain_value(Case::Good,true))))==1) by(compute);assert((b(utf8(plain_value(Case::Good,true)))) == 1); // good-detail-status-utf8
assert((b(utf8(plain_value(Case::Controls,true))))==0) by(compute);assert((b(utf8(plain_value(Case::Controls,true)))) == 0); // controls-detail-status-utf8
assert((b(utf8(plain_value(Case::Bad,true))))==0) by(compute);assert((b(utf8(plain_value(Case::Bad,true)))) == 0); // bad-detail-status-utf8
assert((b(record(Case::Good).status.is_ok))==1) by(compute);assert((b(record(Case::Good).status.is_ok)) == 1); // good-record-is-ok
assert((b(record(Case::Good).projection.ended))==1) by(compute);assert((b(record(Case::Good).projection.ended)) == 1); // good-record-ended
assert((b(record(Case::Good).projection.finalized))==1) by(compute);assert((b(record(Case::Good).projection.finalized)) == 1); // good-record-finalized
assert((status(Case::Good).raw.len() as int)==1) by(compute);assert((status(Case::Good).raw.len() as int) == 1); // good-raw-length
assert((b(color_scheme(true,seq![])))==1) by(compute);assert((b(color_scheme(true,seq![]))) == 1); // good-color-on
assert((b(utf8(status(Case::Good).raw)))==1) by(compute);assert((b(utf8(status(Case::Good).raw))) == 1); // good-stored-raw-utf8
assert((b(utf8(status(Case::Good).error_text)))==1) by(compute);assert((b(utf8(status(Case::Good).error_text))) == 1); // good-decoded-utf8
assert((b(has_byte(status(Case::Good).raw,13u8)))==0) by(compute);assert((b(has_byte(status(Case::Good).raw,13u8))) == 0); // good-raw-has-cr
assert((b(has_byte(status(Case::Good).raw,10u8)))==0) by(compute);assert((b(has_byte(status(Case::Good).raw,10u8))) == 0); // good-raw-has-lf
assert((b(has_byte(status(Case::Good).raw,9u8)))==0) by(compute);assert((b(has_byte(status(Case::Good).raw,9u8))) == 0); // good-raw-has-tab
assert((b(plain_value(Case::Good,true)==observed_status(Case::Good)))==1) by(compute);assert((b(plain_value(Case::Good,true)==observed_status(Case::Good))) == 1); // good-detail-equals-cleaned-raw
assert((b(configured_detail(Case::Good,true,seq![])==observed_detail(Case::Good,true)))==1) by(compute);assert((b(configured_detail(Case::Good,true,seq![])==observed_detail(Case::Good,true))) == 1); // good-detail-exact-format
assert((b(configured_summary(Case::Good,true,seq![])==observed_summary(Case::Good,true)))==1) by(compute);assert((b(configured_summary(Case::Good,true,seq![])==observed_summary(Case::Good,true))) == 1); // good-summary-has-status
assert((plain_value(Case::Good,true).len() as int)==1) by(compute);assert((plain_value(Case::Good,true).len() as int) == 1); // good-detail-status-length
assert((configured_detail(Case::Good,true,seq![]).len() as int)==27) by(compute);assert((configured_detail(Case::Good,true,seq![]).len() as int) == 27); // good-emitted-status-line-length
assert((b(has_ansi(configured_detail(Case::Good,true,seq![]))))==1) by(compute);assert((b(has_ansi(configured_detail(Case::Good,true,seq![])))) == 1); // good-has-ansi
assert((b(utf8(plain_value(Case::Good,true))))==1) by(compute);assert((b(utf8(plain_value(Case::Good,true)))) == 1); // good-none-detail-status-utf8
assert((b(plain_value(Case::Good,true)==observed_status(Case::Good)))==1) by(compute);assert((b(plain_value(Case::Good,true)==observed_status(Case::Good))) == 1); // good-none-detail-equals-cleaned-raw
assert((b(configured_detail(Case::Good,true,seq![110u8,111,110,101])==observed_detail(Case::Good,false)))==1) by(compute);assert((b(configured_detail(Case::Good,true,seq![110u8,111,110,101])==observed_detail(Case::Good,false))) == 1); // good-none-detail-exact-format
assert((b(configured_summary(Case::Good,true,seq![110u8,111,110,101])==observed_summary(Case::Good,false)))==1) by(compute);assert((b(configured_summary(Case::Good,true,seq![110u8,111,110,101])==observed_summary(Case::Good,false))) == 1); // good-none-summary-has-status
assert((plain_value(Case::Good,true).len() as int)==1) by(compute);assert((plain_value(Case::Good,true).len() as int) == 1); // good-none-detail-status-length
assert((configured_detail(Case::Good,true,seq![110u8,111,110,101]).len() as int)==10) by(compute);assert((configured_detail(Case::Good,true,seq![110u8,111,110,101]).len() as int) == 10); // good-none-emitted-status-line-length
assert((b(has_ansi(configured_detail(Case::Good,true,seq![110u8,111,110,101]))))==0) by(compute);assert((b(has_ansi(configured_detail(Case::Good,true,seq![110u8,111,110,101])))) == 0); // good-none-has-ansi
assert((b(record(Case::Controls).status.is_ok))==0) by(compute);assert((b(record(Case::Controls).status.is_ok)) == 0); // controls-record-is-ok
assert((b(record(Case::Controls).projection.ended))==1) by(compute);assert((b(record(Case::Controls).projection.ended)) == 1); // controls-record-ended
assert((b(record(Case::Controls).projection.finalized))==1) by(compute);assert((b(record(Case::Controls).projection.finalized)) == 1); // controls-record-finalized
assert((status(Case::Controls).raw.len() as int)==657) by(compute);assert((status(Case::Controls).raw.len() as int) == 657); // controls-raw-length
assert((b(color_scheme(true,seq![])))==1) by(compute);assert((b(color_scheme(true,seq![]))) == 1); // controls-color-on
assert((b(utf8(status(Case::Controls).raw)))==0) by(compute);assert((b(utf8(status(Case::Controls).raw))) == 0); // controls-stored-raw-utf8
assert((b(utf8(status(Case::Controls).error_text)))==1) by(compute);assert((b(utf8(status(Case::Controls).error_text))) == 1); // controls-decoded-utf8
assert((b(has_byte(status(Case::Controls).raw,13u8)))==1) by(compute);assert((b(has_byte(status(Case::Controls).raw,13u8))) == 1); // controls-raw-has-cr
assert((b(has_byte(status(Case::Controls).raw,10u8)))==1) by(compute);assert((b(has_byte(status(Case::Controls).raw,10u8))) == 1); // controls-raw-has-lf
assert((b(has_byte(status(Case::Controls).raw,9u8)))==1) by(compute);assert((b(has_byte(status(Case::Controls).raw,9u8))) == 1); // controls-raw-has-tab
assert((b(plain_value(Case::Controls,true)==observed_status(Case::Controls)))==1) by(compute);assert((b(plain_value(Case::Controls,true)==observed_status(Case::Controls))) == 1); // controls-detail-equals-cleaned-raw
assert((b(configured_detail(Case::Controls,true,seq![])==observed_detail(Case::Controls,true)))==1) by(compute);assert((b(configured_detail(Case::Controls,true,seq![])==observed_detail(Case::Controls,true))) == 1); // controls-detail-exact-format
assert((b(configured_summary(Case::Controls,true,seq![])==observed_summary(Case::Controls,true)))==1) by(compute);assert((b(configured_summary(Case::Controls,true,seq![])==observed_summary(Case::Controls,true))) == 1); // controls-summary-has-status
assert((plain_value(Case::Controls,true).len() as int)==667) by(compute);assert((plain_value(Case::Controls,true).len() as int) == 667); // controls-detail-status-length
assert((configured_detail(Case::Controls,true,seq![]).len() as int)==717) by(compute);assert((configured_detail(Case::Controls,true,seq![]).len() as int) == 717); // controls-emitted-status-line-length
assert((b(has_ansi(configured_detail(Case::Controls,true,seq![]))))==1) by(compute);assert((b(has_ansi(configured_detail(Case::Controls,true,seq![])))) == 1); // controls-has-ansi
assert((b(utf8(plain_value(Case::Controls,true))))==0) by(compute);assert((b(utf8(plain_value(Case::Controls,true)))) == 0); // controls-none-detail-status-utf8
assert((b(plain_value(Case::Controls,true)==observed_status(Case::Controls)))==1) by(compute);assert((b(plain_value(Case::Controls,true)==observed_status(Case::Controls))) == 1); // controls-none-detail-equals-cleaned-raw
assert((b(configured_detail(Case::Controls,true,seq![110u8,111,110,101])==observed_detail(Case::Controls,false)))==1) by(compute);assert((b(configured_detail(Case::Controls,true,seq![110u8,111,110,101])==observed_detail(Case::Controls,false))) == 1); // controls-none-detail-exact-format
assert((b(configured_summary(Case::Controls,true,seq![110u8,111,110,101])==observed_summary(Case::Controls,false)))==1) by(compute);assert((b(configured_summary(Case::Controls,true,seq![110u8,111,110,101])==observed_summary(Case::Controls,false))) == 1); // controls-none-summary-has-status
assert((plain_value(Case::Controls,true).len() as int)==667) by(compute);assert((plain_value(Case::Controls,true).len() as int) == 667); // controls-none-detail-status-length
assert((configured_detail(Case::Controls,true,seq![110u8,111,110,101]).len() as int)==676) by(compute);assert((configured_detail(Case::Controls,true,seq![110u8,111,110,101]).len() as int) == 676); // controls-none-emitted-status-line-length
assert((b(has_ansi(configured_detail(Case::Controls,true,seq![110u8,111,110,101]))))==0) by(compute);assert((b(has_ansi(configured_detail(Case::Controls,true,seq![110u8,111,110,101])))) == 0); // controls-none-has-ansi
assert((b(record(Case::Bad).status.is_ok))==0) by(compute);assert((b(record(Case::Bad).status.is_ok)) == 0); // bad-record-is-ok
assert((b(record(Case::Bad).projection.ended))==1) by(compute);assert((b(record(Case::Bad).projection.ended)) == 1); // bad-record-ended
assert((b(record(Case::Bad).projection.finalized))==1) by(compute);assert((b(record(Case::Bad).projection.finalized)) == 1); // bad-record-finalized
assert((status(Case::Bad).raw.len() as int)==311) by(compute);assert((status(Case::Bad).raw.len() as int) == 311); // bad-raw-length
assert((b(color_scheme(true,seq![])))==1) by(compute);assert((b(color_scheme(true,seq![]))) == 1); // bad-color-on
assert((b(utf8(status(Case::Bad).raw)))==0) by(compute);assert((b(utf8(status(Case::Bad).raw))) == 0); // bad-stored-raw-utf8
assert((b(utf8(status(Case::Bad).error_text)))==1) by(compute);assert((b(utf8(status(Case::Bad).error_text))) == 1); // bad-decoded-utf8
assert((b(has_byte(status(Case::Bad).raw,13u8)))==0) by(compute);assert((b(has_byte(status(Case::Bad).raw,13u8))) == 0); // bad-raw-has-cr
assert((b(has_byte(status(Case::Bad).raw,10u8)))==0) by(compute);assert((b(has_byte(status(Case::Bad).raw,10u8))) == 0); // bad-raw-has-lf
assert((b(has_byte(status(Case::Bad).raw,9u8)))==0) by(compute);assert((b(has_byte(status(Case::Bad).raw,9u8))) == 0); // bad-raw-has-tab
assert((b(plain_value(Case::Bad,true)==observed_status(Case::Bad)))==1) by(compute);assert((b(plain_value(Case::Bad,true)==observed_status(Case::Bad))) == 1); // bad-detail-equals-cleaned-raw
assert((b(configured_detail(Case::Bad,true,seq![])==observed_detail(Case::Bad,true)))==1) by(compute);assert((b(configured_detail(Case::Bad,true,seq![])==observed_detail(Case::Bad,true))) == 1); // bad-detail-exact-format
assert((b(configured_summary(Case::Bad,true,seq![])==observed_summary(Case::Bad,true)))==1) by(compute);assert((b(configured_summary(Case::Bad,true,seq![])==observed_summary(Case::Bad,true))) == 1); // bad-summary-has-status
assert((plain_value(Case::Bad,true).len() as int)==311) by(compute);assert((plain_value(Case::Bad,true).len() as int) == 311); // bad-detail-status-length
assert((configured_detail(Case::Bad,true,seq![]).len() as int)==337) by(compute);assert((configured_detail(Case::Bad,true,seq![]).len() as int) == 337); // bad-emitted-status-line-length
assert((b(has_ansi(configured_detail(Case::Bad,true,seq![]))))==1) by(compute);assert((b(has_ansi(configured_detail(Case::Bad,true,seq![])))) == 1); // bad-has-ansi
assert((b(utf8(plain_value(Case::Bad,true))))==0) by(compute);assert((b(utf8(plain_value(Case::Bad,true)))) == 0); // bad-none-detail-status-utf8
assert((b(plain_value(Case::Bad,true)==observed_status(Case::Bad)))==1) by(compute);assert((b(plain_value(Case::Bad,true)==observed_status(Case::Bad))) == 1); // bad-none-detail-equals-cleaned-raw
assert((b(configured_detail(Case::Bad,true,seq![110u8,111,110,101])==observed_detail(Case::Bad,false)))==1) by(compute);assert((b(configured_detail(Case::Bad,true,seq![110u8,111,110,101])==observed_detail(Case::Bad,false))) == 1); // bad-none-detail-exact-format
assert((b(configured_summary(Case::Bad,true,seq![110u8,111,110,101])==observed_summary(Case::Bad,false)))==1) by(compute);assert((b(configured_summary(Case::Bad,true,seq![110u8,111,110,101])==observed_summary(Case::Bad,false))) == 1); // bad-none-summary-has-status
assert((plain_value(Case::Bad,true).len() as int)==311) by(compute);assert((plain_value(Case::Bad,true).len() as int) == 311); // bad-none-detail-status-length
assert((configured_detail(Case::Bad,true,seq![110u8,111,110,101]).len() as int)==320) by(compute);assert((configured_detail(Case::Bad,true,seq![110u8,111,110,101]).len() as int) == 320); // bad-none-emitted-status-line-length
assert((b(has_ansi(configured_detail(Case::Bad,true,seq![110u8,111,110,101]))))==0) by(compute);assert((b(has_ansi(configured_detail(Case::Bad,true,seq![110u8,111,110,101])))) == 0); // bad-none-has-ansi
assert((b(!color_scheme(true,seq![110u8,111,110,101])))==1) by(compute);assert((b(!color_scheme(true,seq![110u8,111,110,101]))) == 1); // color-none-is-off
}
}
fn main() {}
