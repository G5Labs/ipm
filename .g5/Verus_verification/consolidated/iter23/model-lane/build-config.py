from pathlib import Path
import json
root=Path('.g5/Verus_verification/consolidated/iter23');source=root/'status_reader.rs';s=source.read_text()
models={};functions={}
def ctl(name,old,new,fn):
 assert s.count(old)==1,(name,s.count(old));models[name]=[old,new];functions[name]=fn
ctl('finalize-stores-decoded','projection:inherited_history::finalize(r.projection,tag(s)),status:s','projection:inherited_history::finalize(r.projection,tag(s)),status:Status{raw:s.error_text,..s}','producer_neighbor')
ctl('finalize-does-not-end','projection:inherited_history::finalize(r.projection,tag(s)),status:s','projection:inherited_history::Row{ended:false,..inherited_history::finalize(r.projection,tag(s))},status:s','producer_neighbor')
ctl('finalize-commits-error','projection:inherited_history::finalize(r.projection,tag(s)),status:s','projection:inherited_history::Row{committed:true,..inherited_history::finalize(r.projection,tag(s))},status:s','producer_neighbor')
ctl('persist-drops-raw','projection:inherited_history::Row{finalized:true,..r.projection},..r','projection:inherited_history::Row{finalized:true,..r.projection},status:Status{raw:seq![],..r.status},..r','producer_neighbor')
ctl('persist-not-finalized','Row{finalized:true,..r.projection}','Row{finalized:false,..r.projection}','producer_neighbor')
ctl('details-decodes','inherited_history::Text::Raw(_)=>s.raw','inherited_history::Text::Raw(_)=>s.error_text','branch_neighbor')
ctl('error-summary-uses-raw','inherited_history::Text::Error00|inherited_history::Text::Error=>s.error_text','inherited_history::Text::Error00|inherited_history::Text::Error=>s.raw','branch_neighbor')
ctl('success-summary-uses-raw','inherited_history::Text::Success=>success()','inherited_history::Text::Success=>s.raw','branch_neighbor')
ctl('reader-ignores-detail','success_to_string(tag(r.status),detail)','success_to_string(tag(r.status),false)','branch_neighbor')
ctl('summary-forces-success','success_to_string(tag(r.status),detail)','success_to_string(if detail{tag(r.status)}else{1},detail)','branch_neighbor')
ctl('cleanup-cr-label-swapped','replace_all(b,13,styled(cr(),color,Style::Dim))','replace_all(b,13,styled(lf(),color,Style::Dim))','cleanup_neighbor')
ctl('cleanup-skips-lf',')),10,styled(lf(),color,Style::Dim))',')),12,styled(lf(),color,Style::Dim))','cleanup_neighbor')
ctl('cleanup-skips-tab',')),9,styled(tab(),color,Style::Dim))',')),8,styled(tab(),color,Style::Dim))','cleanup_neighbor')
ctl('cleanup-erases-high-byte','else{seq![b[i as int]]})+replace_from','else{if b[i as int]>=128{seq![]}else{seq![b[i as int]]}})+replace_from','cleanup_neighbor')
ctl('cleanup-token-style-wrong','replace_all(b,13,styled(cr(),color,Style::Dim))','replace_all(b,13,styled(cr(),color,Style::Default))','cleanup_neighbor')
ctl('format-forces-color','if color{prefix(style)+b+reset()}else{b}','if true{prefix(style)+b+reset()}else{b}','format_neighbor')
ctl('format-omits-reset','prefix(style)+b+reset()','prefix(style)+b','format_neighbor')
ctl('format-default-color-wrong','Style::Default=>seq![27u8,91,51,57,109]','Style::Default=>seq![27u8,91,51,49,109]','format_neighbor')
ctl('format-label-style-wrong','styled(success(),color,Style::Dim)+seq![58u8,32]','styled(success(),color,Style::Default)+seq![58u8,32]','format_neighbor')
ctl('color-ignores-class-absence','class_present && setting!=seq![110u8,111,110,101]','true && setting!=seq![110u8,111,110,101]','format_neighbor')
ctl('color-disables-empty-setting','class_present && setting!=seq![110u8,111,110,101]','class_present && setting.len()>0 && setting!=seq![110u8,111,110,101]','format_neighbor')
ctl('color-ignores-none-setting','class_present && setting!=seq![110u8,111,110,101]','class_present','format_neighbor')
ctl('utf8-accepts-isolated-continuation','else if x<128{valid_from','else if x<192{valid_from','utf8_neighbor')
ctl('utf8-accepts-overlong','else if 194<=x && x<=223','else if 192<=x && x<=223','utf8_neighbor')
ctl('utf8-accepts-surrogate','x==237{valid_from(b,i+1,2,128u8,159u8)}','x==237{valid_from(b,i+1,2,128u8,191u8)}','utf8_neighbor')
ctl('utf8-accepts-out-of-range','x==244{valid_from(b,i+1,3,128u8,143u8)}','x==244{valid_from(b,i+1,3,128u8,191u8)}','utf8_neighbor')
ctl('utf8-accepts-truncation','if i>=b.len(){pending==0}','if i>=b.len(){true}','utf8_neighbor')
obs_path=Path('.g5/uplift/consolidated/iter23/native/20261115T231537227723Z/observations.json')
obs=json.loads(obs_path.read_text())['observations'];replay={}
def add(key,expr):
 assert key in obs;replay[key]=expr
# First three are opposite-capable semantic Booleans, not counts.
for label in ('good','controls','bad'):
 c='Case::'+label.title();add(label+'-detail-status-utf8',f'b(utf8(plain_value({c},true)))')
for label in ('good','controls','bad'):
 c='Case::'+label.title()
 add(label+'-record-is-ok',f'b(record({c}).status.is_ok)')
 add(label+'-record-ended',f'b(record({c}).projection.ended)')
 add(label+'-record-finalized',f'b(record({c}).projection.finalized)')
 add(label+'-raw-length',f'status({c}).raw.len() as int')
 add(label+'-color-on','b(color_scheme(true,seq![]))')
 add(label+'-stored-raw-utf8',f'b(utf8(status({c}).raw))')
 add(label+'-decoded-utf8',f'b(utf8(status({c}).error_text))')
 for ch,n in [('cr',13),('lf',10),('tab',9)]:add(label+'-raw-has-'+ch,f'b(has_byte(status({c}).raw,{n}u8))')
 for suf,col,setting in [('', 'true','seq![]'),('-none','false','seq![110u8,111,110,101]')]:
  k=label+suf
  if suf:add(k+'-detail-status-utf8',f'b(utf8(plain_value({c},true)))')
  add(k+'-detail-equals-cleaned-raw',f'b(plain_value({c},true)==observed_status({c}))')
  add(k+'-detail-exact-format',f'b(configured_detail({c},true,{setting})==observed_detail({c},{col}))')
  add(k+'-summary-has-status',f'b(configured_summary({c},true,{setting})==observed_summary({c},{col}))')
  add(k+'-detail-status-length',f'plain_value({c},true).len() as int')
  add(k+'-emitted-status-line-length',f'configured_detail({c},true,{setting}).len() as int')
  add(k+'-has-ansi',f'b(has_ansi(configured_detail({c},true,{setting})))')
add('color-none-is-off','b(!color_scheme(true,seq![110u8,111,110,101]))')
pre='producer_neighbor(); branch_neighbor(); format_neighbor(); native_good_bytes(); native_controls_bytes(); native_bad_bytes();'
cfg={'models':{'status_reader.rs':models},'control_functions':{'status_reader.rs':functions},'control_options':['--multiple-errors','0'],'replay_grouped':True,'replay':replay,'replay_prelude':pre,'native_expected':{k:v for k,v in obs.items() if k not in replay},'native_receipt':str(obs_path),'domain':{'status':'literal SDK raw/error_text/IsOK ports, not generic encoding','record':'successful Finalize/PersistToTwin ports; selected Success field only','output':'original Success field slices; current device byte identity; other columns/whole commands qualification only','colors':'compiled class present and returned setting empty or none; GetValue storage provider','archive':'unassessed/unpursued'}}
(root/'status-reader-cases.json').write_text(json.dumps(cfg,indent=2)+'\n')
print(len(models),'controls',len(set(functions.values())),'selectors',len(replay),'aliases',len(cfg['native_expected']),'gates')
