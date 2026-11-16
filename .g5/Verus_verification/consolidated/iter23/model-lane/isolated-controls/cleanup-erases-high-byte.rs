// IPM571e04a1: stored raw %Status -> inherited reader choice -> byte cleanup/format.
// Library encoding/GetErrorText/IsOK and logical-string-to-device identity are
// explicit selected provider ports, qualified by original raw native byte files.
// No generic terminal/library/UTF8-production guarantee, other columns or encoding policy.
// Raw details intent/withdrawn B2D7 remains; no new defect or policy is inferred.
// Archive paths remain UNASSESSED/UNPURSUED, with no investigation/representation.
#[path="../../../verus/history.rs"]
pub mod inherited_history;
use vstd::prelude::*;
verus! {
pub type Bytes=Seq<u8>;
#[derive(PartialEq,Eq)]
pub enum Case {Good,Controls,Bad}
#[derive(PartialEq,Eq)]
pub enum Style {Dim,Default}
// error_text/is_ok are library annotations of raw, NOT extra stored DB columns
// and NOT a supplied renderer result. The renderer still chooses which value.
pub struct Status {pub raw:Bytes,pub error_text:Bytes,pub is_ok:bool}
pub struct Record {pub projection:inherited_history::Row,pub status:Status}
pub open spec fn good_raw()->Bytes {seq![49u8]}
pub open spec fn good_decoded()->Bytes {seq![]}
pub open spec fn controls_raw()->Bytes {seq![48u8,32,0,140,2,1,4,4,137,19,27,1,102,105,114,115,116,13,115,101,99,111,110,100,10,116,104,105,114,100,9,102,111,117,114,116,104,1,1,1,1,1,1,1,0,98,2,1,31,1,70,97,105,108,43,49,94,73,116,101,114,50,51,67,111,110,116,114,111,108,115,46,72,111,111,107,115,46,49,6,1,85,83,69,82,0,57,2,1,35,1,101,94,70,97,105,108,43,49,94,73,116,101,114,50,51,67,111,110,116,114,111,108,115,46,72,111,111,107,115,46,49,94,49,47,1,101,94,100,111,73,110,118,111,107,101,43,56,94,37,73,80,77,46,83,116,111,114,97,103,101,46,73,110,118,111,107,101,82,101,102,101,114,101,110,99,101,46,49,94,49,51,1,101,94,79,110,65,102,116,101,114,80,104,97,115,101,43,53,94,37,73,80,77,46,83,116,111,114,97,103,101,46,73,110,118,111,107,101,82,101,102,101,114,101,110,99,101,46,49,94,49,45,1,101,94,69,120,101,99,117,116,101,80,104,97,115,101,115,43,49,57,48,94,37,73,80,77,46,83,116,111,114,97,103,101,46,77,111,100,117,108,101,46,49,94,49,43,1,101,94,76,111,97,100,78,101,119,77,111,100,117,108,101,43,49,54,56,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,52,1,101,94,76,111,97,100,77,111,100,117,108,101,70,114,111,109,68,105,114,101,99,116,111,114,121,43,50,48,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,48,1,101,94,76,111,97,100,77,111,100,117,108,101,82,101,102,101,114,101,110,99,101,43,55,57,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,50,1,101,94,76,111,97,100,81,117,97,108,105,102,105,101,100,82,101,102,101,114,101,110,99,101,43,52,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,28,1,101,94,73,110,115,116,97,108,108,43,54,57,94,37,73,80,77,46,77,97,105,110,46,49,94,49,34,1,100,94,83,104,101,108,108,73,110,116,101,114,110,97,108,43,54,50,94,37,73,80,77,46,77,97,105,110,46,49,94,49,25,1,100,94,83,104,101,108,108,43,53,94,37,73,80,77,46,77,97,105,110,46,49,94,49,33,1,101,94,67,109,100,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,37,1,100,94,67,97,112,116,117,114,101,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,33,1,100,94,82,117,110,43,54,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,7,1,100,94,94,94,49]}
pub open spec fn controls_decoded()->Bytes {seq![69u8,82,82,79,82,32,35,53,48,48,49,58,32,102,105,114,115,116,13,115,101,99,111,110,100,10,116,104,105,114,100,9,102,111,117,114,116,104]}
pub open spec fn bad_raw()->Bytes {seq![48u8,32,0,50,1,1,4,4,137,19,63,1,78,111,32,114,101,112,111,115,105,116,111,114,105,101,115,32,97,114,101,32,99,111,110,102,105,103,117,114,101,100,32,97,110,100,32,101,110,97,98,108,101,100,32,105,110,32,116,104,105,115,32,110,97,109,101,115,112,97,99,101,46,1,1,1,1,1,1,1,231,1,24,1,73,110,115,116,97,108,108,43,50,54,94,37,73,80,77,46,77,97,105,110,46,49,6,1,85,83,69,82,199,1,28,1,101,94,73,110,115,116,97,108,108,43,50,54,94,37,73,80,77,46,77,97,105,110,46,49,94,49,34,1,100,94,83,104,101,108,108,73,110,116,101,114,110,97,108,43,54,50,94,37,73,80,77,46,77,97,105,110,46,49,94,49,25,1,100,94,83,104,101,108,108,43,53,94,37,73,80,77,46,77,97,105,110,46,49,94,49,33,1,101,94,67,109,100,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,37,1,100,94,67,97,112,116,117,114,101,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,33,1,100,94,82,117,110,43,56,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,7,1,100,94,94,94,49]}
pub open spec fn bad_decoded()->Bytes {seq![69u8,82,82,79,82,32,35,53,48,48,49,58,32,78,111,32,114,101,112,111,115,105,116,111,114,105,101,115,32,97,114,101,32,99,111,110,102,105,103,117,114,101,100,32,97,110,100,32,101,110,97,98,108,101,100,32,105,110,32,116,104,105,115,32,110,97,109,101,115,112,97,99,101,46]}
pub open spec fn success()->Bytes {seq![83u8,117,99,99,101,115,115]}
pub open spec fn cr()->Bytes {seq![60u8,67,82,62]}
pub open spec fn lf()->Bytes {seq![60u8,76,70,62]}
pub open spec fn tab()->Bytes {seq![60u8,84,65,66,62]}
pub open spec fn observed_good_detail_on()->Bytes {seq![27u8,91,50,109,83,117,99,99,101,115,115,27,91,48,109,58,32,27,91,51,57,109,49,27,91,48,109]}
pub open spec fn observed_good_detail_none()->Bytes {seq![83u8,117,99,99,101,115,115,58,32,49]}
pub open spec fn observed_good_summary_on()->Bytes {seq![27u8,91,51,57,109,83,117,99,99,101,115,115,27,91,48,109]}
pub open spec fn observed_good_summary_none()->Bytes {seq![83u8,117,99,99,101,115,115]}
pub open spec fn observed_controls_detail_on()->Bytes {seq![27u8,91,50,109,83,117,99,99,101,115,115,27,91,48,109,58,32,27,91,51,57,109,48,32,0,140,2,1,4,4,137,19,27,1,102,105,114,115,116,27,91,50,109,60,67,82,62,27,91,48,109,115,101,99,111,110,100,27,91,50,109,60,76,70,62,27,91,48,109,116,104,105,114,100,27,91,50,109,60,84,65,66,62,27,91,48,109,102,111,117,114,116,104,1,1,1,1,1,1,1,0,98,2,1,31,1,70,97,105,108,43,49,94,73,116,101,114,50,51,67,111,110,116,114,111,108,115,46,72,111,111,107,115,46,49,6,1,85,83,69,82,0,57,2,1,35,1,101,94,70,97,105,108,43,49,94,73,116,101,114,50,51,67,111,110,116,114,111,108,115,46,72,111,111,107,115,46,49,94,49,47,1,101,94,100,111,73,110,118,111,107,101,43,56,94,37,73,80,77,46,83,116,111,114,97,103,101,46,73,110,118,111,107,101,82,101,102,101,114,101,110,99,101,46,49,94,49,51,1,101,94,79,110,65,102,116,101,114,80,104,97,115,101,43,53,94,37,73,80,77,46,83,116,111,114,97,103,101,46,73,110,118,111,107,101,82,101,102,101,114,101,110,99,101,46,49,94,49,45,1,101,94,69,120,101,99,117,116,101,80,104,97,115,101,115,43,49,57,48,94,37,73,80,77,46,83,116,111,114,97,103,101,46,77,111,100,117,108,101,46,49,94,49,43,1,101,94,76,111,97,100,78,101,119,77,111,100,117,108,101,43,49,54,56,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,52,1,101,94,76,111,97,100,77,111,100,117,108,101,70,114,111,109,68,105,114,101,99,116,111,114,121,43,50,48,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,48,1,101,94,76,111,97,100,77,111,100,117,108,101,82,101,102,101,114,101,110,99,101,43,55,57,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,50,1,101,94,76,111,97,100,81,117,97,108,105,102,105,101,100,82,101,102,101,114,101,110,99,101,43,52,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,28,1,101,94,73,110,115,116,97,108,108,43,54,57,94,37,73,80,77,46,77,97,105,110,46,49,94,49,34,1,100,94,83,104,101,108,108,73,110,116,101,114,110,97,108,43,54,50,94,37,73,80,77,46,77,97,105,110,46,49,94,49,25,1,100,94,83,104,101,108,108,43,53,94,37,73,80,77,46,77,97,105,110,46,49,94,49,33,1,101,94,67,109,100,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,37,1,100,94,67,97,112,116,117,114,101,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,33,1,100,94,82,117,110,43,54,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,7,1,100,94,94,94,49,27,91,48,109]}
pub open spec fn observed_controls_detail_none()->Bytes {seq![83u8,117,99,99,101,115,115,58,32,48,32,0,140,2,1,4,4,137,19,27,1,102,105,114,115,116,60,67,82,62,115,101,99,111,110,100,60,76,70,62,116,104,105,114,100,60,84,65,66,62,102,111,117,114,116,104,1,1,1,1,1,1,1,0,98,2,1,31,1,70,97,105,108,43,49,94,73,116,101,114,50,51,67,111,110,116,114,111,108,115,46,72,111,111,107,115,46,49,6,1,85,83,69,82,0,57,2,1,35,1,101,94,70,97,105,108,43,49,94,73,116,101,114,50,51,67,111,110,116,114,111,108,115,46,72,111,111,107,115,46,49,94,49,47,1,101,94,100,111,73,110,118,111,107,101,43,56,94,37,73,80,77,46,83,116,111,114,97,103,101,46,73,110,118,111,107,101,82,101,102,101,114,101,110,99,101,46,49,94,49,51,1,101,94,79,110,65,102,116,101,114,80,104,97,115,101,43,53,94,37,73,80,77,46,83,116,111,114,97,103,101,46,73,110,118,111,107,101,82,101,102,101,114,101,110,99,101,46,49,94,49,45,1,101,94,69,120,101,99,117,116,101,80,104,97,115,101,115,43,49,57,48,94,37,73,80,77,46,83,116,111,114,97,103,101,46,77,111,100,117,108,101,46,49,94,49,43,1,101,94,76,111,97,100,78,101,119,77,111,100,117,108,101,43,49,54,56,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,52,1,101,94,76,111,97,100,77,111,100,117,108,101,70,114,111,109,68,105,114,101,99,116,111,114,121,43,50,48,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,48,1,101,94,76,111,97,100,77,111,100,117,108,101,82,101,102,101,114,101,110,99,101,43,55,57,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,50,1,101,94,76,111,97,100,81,117,97,108,105,102,105,101,100,82,101,102,101,114,101,110,99,101,43,52,94,37,73,80,77,46,85,116,105,108,115,46,77,111,100,117,108,101,46,49,94,49,28,1,101,94,73,110,115,116,97,108,108,43,54,57,94,37,73,80,77,46,77,97,105,110,46,49,94,49,34,1,100,94,83,104,101,108,108,73,110,116,101,114,110,97,108,43,54,50,94,37,73,80,77,46,77,97,105,110,46,49,94,49,25,1,100,94,83,104,101,108,108,43,53,94,37,73,80,77,46,77,97,105,110,46,49,94,49,33,1,101,94,67,109,100,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,37,1,100,94,67,97,112,116,117,114,101,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,33,1,100,94,82,117,110,43,54,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,7,1,100,94,94,94,49]}
pub open spec fn observed_controls_summary_on()->Bytes {seq![27u8,91,51,57,109,69,82,82,79,82,32,35,53,48,48,49,58,32,102,105,114,115,116,27,91,50,109,60,67,82,62,27,91,48,109,115,101,99,111,110,100,27,91,50,109,60,76,70,62,27,91,48,109,116,104,105,114,100,27,91,50,109,60,84,65,66,62,27,91,48,109,102,111,117,114,116,104,27,91,48,109]}
pub open spec fn observed_controls_summary_none()->Bytes {seq![69u8,82,82,79,82,32,35,53,48,48,49,58,32,102,105,114,115,116,60,67,82,62,115,101,99,111,110,100,60,76,70,62,116,104,105,114,100,60,84,65,66,62,102,111,117,114,116,104]}
pub open spec fn observed_bad_detail_on()->Bytes {seq![27u8,91,50,109,83,117,99,99,101,115,115,27,91,48,109,58,32,27,91,51,57,109,48,32,0,50,1,1,4,4,137,19,63,1,78,111,32,114,101,112,111,115,105,116,111,114,105,101,115,32,97,114,101,32,99,111,110,102,105,103,117,114,101,100,32,97,110,100,32,101,110,97,98,108,101,100,32,105,110,32,116,104,105,115,32,110,97,109,101,115,112,97,99,101,46,1,1,1,1,1,1,1,231,1,24,1,73,110,115,116,97,108,108,43,50,54,94,37,73,80,77,46,77,97,105,110,46,49,6,1,85,83,69,82,199,1,28,1,101,94,73,110,115,116,97,108,108,43,50,54,94,37,73,80,77,46,77,97,105,110,46,49,94,49,34,1,100,94,83,104,101,108,108,73,110,116,101,114,110,97,108,43,54,50,94,37,73,80,77,46,77,97,105,110,46,49,94,49,25,1,100,94,83,104,101,108,108,43,53,94,37,73,80,77,46,77,97,105,110,46,49,94,49,33,1,101,94,67,109,100,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,37,1,100,94,67,97,112,116,117,114,101,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,33,1,100,94,82,117,110,43,56,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,7,1,100,94,94,94,49,27,91,48,109]}
pub open spec fn observed_bad_detail_none()->Bytes {seq![83u8,117,99,99,101,115,115,58,32,48,32,0,50,1,1,4,4,137,19,63,1,78,111,32,114,101,112,111,115,105,116,111,114,105,101,115,32,97,114,101,32,99,111,110,102,105,103,117,114,101,100,32,97,110,100,32,101,110,97,98,108,101,100,32,105,110,32,116,104,105,115,32,110,97,109,101,115,112,97,99,101,46,1,1,1,1,1,1,1,231,1,24,1,73,110,115,116,97,108,108,43,50,54,94,37,73,80,77,46,77,97,105,110,46,49,6,1,85,83,69,82,199,1,28,1,101,94,73,110,115,116,97,108,108,43,50,54,94,37,73,80,77,46,77,97,105,110,46,49,94,49,34,1,100,94,83,104,101,108,108,73,110,116,101,114,110,97,108,43,54,50,94,37,73,80,77,46,77,97,105,110,46,49,94,49,25,1,100,94,83,104,101,108,108,43,53,94,37,73,80,77,46,77,97,105,110,46,49,94,49,33,1,101,94,67,109,100,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,37,1,100,94,67,97,112,116,117,114,101,43,50,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,33,1,100,94,82,117,110,43,56,94,73,116,101,114,50,51,46,83,116,97,116,117,115,82,101,97,100,101,114,46,49,94,49,7,1,100,94,94,94,49]}
pub open spec fn observed_bad_summary_on()->Bytes {seq![27u8,91,51,57,109,69,82,82,79,82,32,35,53,48,48,49,58,32,78,111,32,114,101,112,111,115,105,116,111,114,105,101,115,32,97,114,101,32,99,111,110,102,105,103,117,114,101,100,32,97,110,100,32,101,110,97,98,108,101,100,32,105,110,32,116,104,105,115,32,110,97,109,101,115,112,97,99,101,46,27,91,48,109]}
pub open spec fn observed_bad_summary_none()->Bytes {seq![69u8,82,82,79,82,32,35,53,48,48,49,58,32,78,111,32,114,101,112,111,115,105,116,111,114,105,101,115,32,97,114,101,32,99,111,110,102,105,103,117,114,101,100,32,97,110,100,32,101,110,97,98,108,101,100,32,105,110,32,116,104,105,115,32,110,97,109,101,115,112,97,99,101,46]}
pub open spec fn status(c:Case)->Status {match c{
 Case::Good=>Status{raw:good_raw(),error_text:good_decoded(),is_ok:true},
 Case::Controls=>Status{raw:controls_raw(),error_text:controls_decoded(),is_ok:false},
 Case::Bad=>Status{raw:bad_raw(),error_text:bad_decoded(),is_ok:false}}}
// Symbolic class tag for the inherited int contract; tag2 is NOT native encoded status2.
pub open spec fn tag(s:Status)->int {if s.is_ok{1}else if s.raw==seq![48u8]{0}else{2}}
pub open spec fn initial()->Record {Record{projection:inherited_history::Row{id:0,namespace:1,
 action:inherited_history::Action::Install,package:seq![112u8],version:seq![],source:seq![],moniker:seq![],details:seq![],
 ended:false,status:0,committed:false,finalized:false},status:Status{raw:seq![48u8],error_text:seq![],is_ok:false}}}
pub open spec fn finalize(r:Record,s:Status)->Record {Record{projection:inherited_history::finalize(r.projection,tag(s)),status:s}}
pub open spec fn persist(r:Record)->Record {Record{projection:inherited_history::Row{finalized:true,..r.projection},..r}}
// Successful ordinary install, no-repo refusal and returned Invoke error choose
// these qualified SDK statuses. Neither framework rows nor raw statuses are seeded.
pub open spec fn record(c:Case)->Record {persist(finalize(initial(),status(c)))}
pub open spec fn refine(choice:inherited_history::Text,s:Status)->Bytes {match choice{
 inherited_history::Text::Success=>success(),inherited_history::Text::Error00|inherited_history::Text::Error=>s.error_text,
 inherited_history::Text::Raw(_)=>s.raw}}
pub open spec fn render(r:Record,detail:bool)->Bytes {
 refine(inherited_history::success_to_string(tag(r.status),detail),r.status)
}
pub open spec fn value(c:Case,detail:bool)->Bytes {render(record(c),detail)}
pub open spec fn prefix(style:Style)->Bytes {match style{Style::Dim=>seq![27u8,91,50,109],Style::Default=>seq![27u8,91,51,57,109]}}
pub open spec fn reset()->Bytes {seq![27u8,91,48,109]}
pub open spec fn color_scheme(class_present:bool,setting:Bytes)->bool {class_present && setting!=seq![110u8,111,110,101]}
pub open spec fn styled(b:Bytes,color:bool,style:Style)->Bytes {if color{prefix(style)+b+reset()}else{b}}
pub open spec fn piece(b:u8,color:bool)->Bytes {
 if b==13{styled(cr(),color,Style::Dim)}else if b==10{styled(lf(),color,Style::Dim)}
 else if b==9{styled(tab(),color,Style::Dim)}else{seq![b]}
}
// ShowColumns CR->LF->TAB replacement is this equivalent one-pass rule:
// inserted tags/ANSI contain no CR/LF/TAB, so subsequent replacements cannot match them.
pub open spec fn clean_from(b:Bytes,color:bool,i:nat)->Bytes
 decreases if i<b.len(){(b.len()-i) as nat}else{0nat}
{if i>=b.len(){seq![]}else{piece(b[i as int],color)+clean_from(b,color,i+1)}}
// Active cleanup follows the actual ordered $replace calls. clean_from is the
// retained one-pass prototype; no generic equivalence theorem is claimed for it.
pub open spec fn replace_from(b:Bytes,target:u8,replacement:Bytes,i:nat)->Bytes
 decreases if i<b.len(){(b.len()-i) as nat}else{0nat}
{if i>=b.len(){seq![]}else{(if b[i as int]==target{replacement}else{if b[i as int]>=128{seq![]}else{seq![b[i as int]]}})+replace_from(b,target,replacement,i+1)}}
pub open spec fn replace_all(b:Bytes,target:u8,replacement:Bytes)->Bytes {replace_from(b,target,replacement,0)}
pub open spec fn clean(b:Bytes,color:bool)->Bytes {
 replace_all(replace_all(replace_all(b,13,styled(cr(),color,Style::Dim)),10,styled(lf(),color,Style::Dim)),9,styled(tab(),color,Style::Dim))
}
pub open spec fn plain_value(c:Case,detail:bool)->Bytes {clean(value(c,detail),false)}
pub open spec fn detail_line(c:Case,color:bool)->Bytes {
 styled(success(),color,Style::Dim)+seq![58u8,32]+styled(clean(value(c,true),color),color,Style::Default)
}
pub open spec fn summary_value(c:Case,color:bool)->Bytes {styled(clean(value(c,false),color),color,Style::Default)}
pub open spec fn summary_field(c:Case,color:bool)->Bytes {summary_value(c,color)+seq![32u8]}
pub open spec fn configured_detail(c:Case,class_present:bool,setting:Bytes)->Bytes {detail_line(c,color_scheme(class_present,setting))}
pub open spec fn configured_summary(c:Case,class_present:bool,setting:Bytes)->Bytes {summary_value(c,color_scheme(class_present,setting))}
pub open spec fn observed_detail(c:Case,color:bool)->Bytes {match c{
 Case::Good=>if color{observed_good_detail_on()}else{observed_good_detail_none()},
 Case::Controls=>if color{observed_controls_detail_on()}else{observed_controls_detail_none()},
 Case::Bad=>if color{observed_bad_detail_on()}else{observed_bad_detail_none()}}}
pub open spec fn observed_summary(c:Case,color:bool)->Bytes {match c{
 Case::Good=>if color{observed_good_summary_on()}else{observed_good_summary_none()},
 Case::Controls=>if color{observed_controls_summary_on()}else{observed_controls_summary_none()},
 Case::Bad=>if color{observed_bad_summary_on()}else{observed_bad_summary_none()}}}
pub open spec fn observed_status(c:Case)->Bytes {observed_detail(c,false).subrange(9,observed_detail(c,false).len() as int)}
pub open spec fn ansi_from(b:Bytes,i:nat)->bool
 decreases if i<b.len(){(b.len()-i) as nat}else{0nat}
{i+1<b.len() && ((b[i as int]==27 && b[(i+1) as int]==91)||ansi_from(b,i+1))}
pub open spec fn has_ansi(b:Bytes)->bool {ansi_from(b,0)}
pub open spec fn has_byte_from(b:Bytes,target:u8,i:nat)->bool
 decreases if i<b.len(){(b.len()-i) as nat}else{0nat}
{i<b.len() && (b[i as int]==target || has_byte_from(b,target,i+1))}
pub open spec fn has_byte(b:Bytes,target:u8)->bool {has_byte_from(b,target,0)}
pub open spec fn ascii_from(b:Bytes,i:nat)->bool
 decreases if i<b.len(){(b.len()-i) as nat}else{0nat}
{i>=b.len() || (b[i as int]<128 && ascii_from(b,i+1))}
pub open spec fn ascii(b:Bytes)->bool {ascii_from(b,0)}
// Strict UTF8 observer predicate, not a product encoding guarantee. Pending
// continuation bounds exclude overlongs, surrogate scalars and >U+10FFFF.
pub open spec fn valid_from(b:Bytes,i:nat,pending:nat,lo:u8,hi:u8)->bool
 decreases if i<b.len(){(b.len()-i) as nat}else{0nat}
{
 if i>=b.len(){pending==0}else{let x=b[i as int];
 if pending>0{lo<=x && x<=hi && valid_from(b,i+1,(pending-1) as nat,128u8,191u8)}
 else if x<128{valid_from(b,i+1,0,128u8,191u8)}
 else if 194<=x && x<=223{valid_from(b,i+1,1,128u8,191u8)}
 else if x==224{valid_from(b,i+1,2,160u8,191u8)}
 else if (225<=x && x<=236)||(238<=x && x<=239){valid_from(b,i+1,2,128u8,191u8)}
 else if x==237{valid_from(b,i+1,2,128u8,159u8)}
 else if x==240{valid_from(b,i+1,3,144u8,191u8)}
 else if 241<=x && x<=243{valid_from(b,i+1,3,128u8,191u8)}
 else if x==244{valid_from(b,i+1,3,128u8,143u8)}else{false}}
}
pub open spec fn utf8(b:Bytes)->bool {valid_from(b,0,0,128u8,191u8)}
pub open spec fn b(v:bool)->int {if v{1}else{0}}
pub open spec fn demo()->Status {Status{raw:seq![48u8,32,137,13,10,9],error_text:seq![69u8,82,82],is_ok:false}}
pub open spec fn demo_record()->Record {persist(finalize(initial(),demo()))}



// Independent selectors call definitions only, no local/imported proof lemmas/broadcasts.


pub proof fn cleanup_neighbor()
 ensures clean(seq![65u8,13,10,9,137],false)==seq![65u8,60,67,82,62,60,76,70,62,60,84,65,66,62,137],
  clean(seq![13u8],true)==prefix(Style::Dim)+cr()+reset(),
  clean(seq![10u8],true)==prefix(Style::Dim)+lf()+reset(),
  clean(seq![9u8],true)==prefix(Style::Dim)+tab()+reset()
{
 assert(clean(seq![65u8,13,10,9,137],false)==seq![65u8,60,67,82,62,60,76,70,62,60,84,65,66,62,137]) by(compute);
 assert(clean(seq![13u8],true)==prefix(Style::Dim)+cr()+reset()) by(compute);
 assert(clean(seq![10u8],true)==prefix(Style::Dim)+lf()+reset()) by(compute);
 assert(clean(seq![9u8],true)==prefix(Style::Dim)+tab()+reset()) by(compute);
}





}
fn main(){}
