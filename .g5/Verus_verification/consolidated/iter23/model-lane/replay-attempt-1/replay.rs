use vstd::prelude::*;
#[path="../status_reader.rs"] pub mod model;
use model::*;
verus! {pub proof fn observed_group() {
producer_neighbor(); branch_neighbor(); format_neighbor(); native_good_bytes(); native_controls_bytes(); native_bad_bytes();
// good-detail-status-utf8
assert((b(utf8(plain_value(Case::Good,true))))==1);
// controls-detail-status-utf8
assert((b(utf8(plain_value(Case::Controls,true))))==0);
// bad-detail-status-utf8
assert((b(utf8(plain_value(Case::Bad,true))))==0);
// good-record-is-ok
assert((b(record(Case::Good).status.is_ok))==1);
// good-record-ended
assert((b(record(Case::Good).projection.ended))==1);
// good-record-finalized
assert((b(record(Case::Good).projection.finalized))==1);
// good-raw-length
assert((status(Case::Good).raw.len() as int)==1);
// good-color-on
assert((b(color_scheme(true,seq![])))==1);
// good-stored-raw-utf8
assert((b(utf8(status(Case::Good).raw)))==1);
// good-decoded-utf8
assert((b(utf8(status(Case::Good).error_text)))==1);
// good-raw-has-cr
assert((b(has_byte(status(Case::Good).raw,13u8)))==0);
// good-raw-has-lf
assert((b(has_byte(status(Case::Good).raw,10u8)))==0);
// good-raw-has-tab
assert((b(has_byte(status(Case::Good).raw,9u8)))==0);
// good-detail-equals-cleaned-raw
assert((b(plain_value(Case::Good,true)==observed_status(Case::Good)))==1);
// good-detail-exact-format
assert((b(configured_detail(Case::Good,true,seq![])==observed_detail(Case::Good,true)))==1);
// good-summary-has-status
assert((b(configured_summary(Case::Good,true,seq![])==observed_summary(Case::Good,true)))==1);
// good-detail-status-length
assert((plain_value(Case::Good,true).len() as int)==1);
// good-emitted-status-line-length
assert((configured_detail(Case::Good,true,seq![]).len() as int)==27);
// good-has-ansi
assert((b(has_ansi(configured_detail(Case::Good,true,seq![]))))==1);
// good-none-detail-status-utf8
assert((b(utf8(plain_value(Case::Good,true))))==1);
// good-none-detail-equals-cleaned-raw
assert((b(plain_value(Case::Good,true)==observed_status(Case::Good)))==1);
// good-none-detail-exact-format
assert((b(configured_detail(Case::Good,true,seq![110u8,111,110,101])==observed_detail(Case::Good,false)))==1);
// good-none-summary-has-status
assert((b(configured_summary(Case::Good,true,seq![110u8,111,110,101])==observed_summary(Case::Good,false)))==1);
// good-none-detail-status-length
assert((plain_value(Case::Good,true).len() as int)==1);
// good-none-emitted-status-line-length
assert((configured_detail(Case::Good,true,seq![110u8,111,110,101]).len() as int)==10);
// good-none-has-ansi
assert((b(has_ansi(configured_detail(Case::Good,true,seq![110u8,111,110,101]))))==0);
// controls-record-is-ok
assert((b(record(Case::Controls).status.is_ok))==0);
// controls-record-ended
assert((b(record(Case::Controls).projection.ended))==1);
// controls-record-finalized
assert((b(record(Case::Controls).projection.finalized))==1);
// controls-raw-length
assert((status(Case::Controls).raw.len() as int)==657);
// controls-color-on
assert((b(color_scheme(true,seq![])))==1);
// controls-stored-raw-utf8
assert((b(utf8(status(Case::Controls).raw)))==0);
// controls-decoded-utf8
assert((b(utf8(status(Case::Controls).error_text)))==1);
// controls-raw-has-cr
assert((b(has_byte(status(Case::Controls).raw,13u8)))==1);
// controls-raw-has-lf
assert((b(has_byte(status(Case::Controls).raw,10u8)))==1);
// controls-raw-has-tab
assert((b(has_byte(status(Case::Controls).raw,9u8)))==1);
// controls-detail-equals-cleaned-raw
assert((b(plain_value(Case::Controls,true)==observed_status(Case::Controls)))==1);
// controls-detail-exact-format
assert((b(configured_detail(Case::Controls,true,seq![])==observed_detail(Case::Controls,true)))==1);
// controls-summary-has-status
assert((b(configured_summary(Case::Controls,true,seq![])==observed_summary(Case::Controls,true)))==1);
// controls-detail-status-length
assert((plain_value(Case::Controls,true).len() as int)==667);
// controls-emitted-status-line-length
assert((configured_detail(Case::Controls,true,seq![]).len() as int)==717);
// controls-has-ansi
assert((b(has_ansi(configured_detail(Case::Controls,true,seq![]))))==1);
// controls-none-detail-status-utf8
assert((b(utf8(plain_value(Case::Controls,true))))==0);
// controls-none-detail-equals-cleaned-raw
assert((b(plain_value(Case::Controls,true)==observed_status(Case::Controls)))==1);
// controls-none-detail-exact-format
assert((b(configured_detail(Case::Controls,true,seq![110u8,111,110,101])==observed_detail(Case::Controls,false)))==1);
// controls-none-summary-has-status
assert((b(configured_summary(Case::Controls,true,seq![110u8,111,110,101])==observed_summary(Case::Controls,false)))==1);
// controls-none-detail-status-length
assert((plain_value(Case::Controls,true).len() as int)==667);
// controls-none-emitted-status-line-length
assert((configured_detail(Case::Controls,true,seq![110u8,111,110,101]).len() as int)==676);
// controls-none-has-ansi
assert((b(has_ansi(configured_detail(Case::Controls,true,seq![110u8,111,110,101]))))==0);
// bad-record-is-ok
assert((b(record(Case::Bad).status.is_ok))==0);
// bad-record-ended
assert((b(record(Case::Bad).projection.ended))==1);
// bad-record-finalized
assert((b(record(Case::Bad).projection.finalized))==1);
// bad-raw-length
assert((status(Case::Bad).raw.len() as int)==311);
// bad-color-on
assert((b(color_scheme(true,seq![])))==1);
// bad-stored-raw-utf8
assert((b(utf8(status(Case::Bad).raw)))==0);
// bad-decoded-utf8
assert((b(utf8(status(Case::Bad).error_text)))==1);
// bad-raw-has-cr
assert((b(has_byte(status(Case::Bad).raw,13u8)))==0);
// bad-raw-has-lf
assert((b(has_byte(status(Case::Bad).raw,10u8)))==0);
// bad-raw-has-tab
assert((b(has_byte(status(Case::Bad).raw,9u8)))==0);
// bad-detail-equals-cleaned-raw
assert((b(plain_value(Case::Bad,true)==observed_status(Case::Bad)))==1);
// bad-detail-exact-format
assert((b(configured_detail(Case::Bad,true,seq![])==observed_detail(Case::Bad,true)))==1);
// bad-summary-has-status
assert((b(configured_summary(Case::Bad,true,seq![])==observed_summary(Case::Bad,true)))==1);
// bad-detail-status-length
assert((plain_value(Case::Bad,true).len() as int)==311);
// bad-emitted-status-line-length
assert((configured_detail(Case::Bad,true,seq![]).len() as int)==337);
// bad-has-ansi
assert((b(has_ansi(configured_detail(Case::Bad,true,seq![]))))==1);
// bad-none-detail-status-utf8
assert((b(utf8(plain_value(Case::Bad,true))))==0);
// bad-none-detail-equals-cleaned-raw
assert((b(plain_value(Case::Bad,true)==observed_status(Case::Bad)))==1);
// bad-none-detail-exact-format
assert((b(configured_detail(Case::Bad,true,seq![110u8,111,110,101])==observed_detail(Case::Bad,false)))==1);
// bad-none-summary-has-status
assert((b(configured_summary(Case::Bad,true,seq![110u8,111,110,101])==observed_summary(Case::Bad,false)))==1);
// bad-none-detail-status-length
assert((plain_value(Case::Bad,true).len() as int)==311);
// bad-none-emitted-status-line-length
assert((configured_detail(Case::Bad,true,seq![110u8,111,110,101]).len() as int)==320);
// bad-none-has-ansi
assert((b(has_ansi(configured_detail(Case::Bad,true,seq![110u8,111,110,101]))))==0);
// color-none-is-off
assert((b(!color_scheme(true,seq![110u8,111,110,101])))==1);
}}
fn main() {}
