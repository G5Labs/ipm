use vstd::prelude::*;
verus! {
// Bounded raw-tag domain: literal release 1.0.0/2.0.0, nonempty current/bad
// platform texts or no suffix, one repository and successful metadata/payload
// providers. General SemVer/XML/JSON, arbitrary invalid tags and OCI tie order
// are open. Equal-version tags retain the provider input order, not a new order.
pub type Text=Seq<char>;
pub open spec fn replace(s:Text,from:char,to:char)->Text
 decreases s.len()
{if s.len()==0{seq![]}else{seq![if s[0]==from{to}else{s[0]}]+replace(s.skip(1),from,to)}}
pub open spec fn encode(v:Text)->Text {replace(v,'+','_')}
pub open spec fn decode(v:Text)->Text {replace(v,'_','+')}
pub open spec fn has_char(s:Text,c:char)->bool
 decreases s.len()
{s.len()>0 && (s[0]==c || has_char(s.skip(1),c))}
pub open spec fn sep(t:Text,i:nat)->nat
 decreases if i<t.len(){t.len()-i}else{0}
{if i+1>=t.len(){t.len()}else if t[i as int]=='_' && t[(i+1) as int]=='_'{i}else{sep(t,i+1)}}
pub struct Parsed {pub encoded:Text,pub platform:Text}
pub open spec fn parse(t:Text)->Parsed {
 let p=sep(t,0);let tail=if p+1<t.len(){t.skip((p+2) as int)}else{seq![]};
 Parsed{encoded:t.take(p as int),platform:tail.take(sep(tail,0) as int)}
}
pub open spec fn joined(encoded:Text,platform:Text)->Text {if platform.len()==0{encoded}else{encoded+seq!['_','_']+platform}}
pub open spec fn tag_for(version:Text,deployed:bool,platform:Text)->Option<Text> {
 if deployed && platform.len()==0{None}else{Some(if deployed{joined(encode(version),platform)}else{encode(version)})}
}
pub open spec fn v1()->Text {seq!['1','.','0','.','0']}
pub open spec fn v2()->Text {seq!['2','.','0','.','0']}
pub open spec fn current()->Text {seq!['2','0','2','6','.','1']}
pub open spec fn bad()->Text {seq!['9','9','9','9','.','1']}
pub open spec fn t1()->Text {joined(v1(),current())}
pub open spec fn t2()->Text {joined(v2(),bad())}
pub open spec fn t1bad()->Text {joined(v1(),bad())}
pub open spec fn build_version()->Text {seq!['1','.','2','.','3','-','a','l','p','h','a','+','b','u','i','l','d','.','7']}
pub open spec fn rank(v:Text)->int {if v==v2(){2}else if v==v1(){1}else{0}}
pub open spec fn tag_rank(t:Text)->int {rank(decode(parse(t).encoded))}
pub open spec fn has_text(xs:Seq<Text>,v:Text)->bool
 decreases xs.len()
{xs.len()>0 && (xs[0]==v || has_text(xs.skip(1),v))}
pub open spec fn unique(xs:Seq<Text>,seen:Seq<Text>)->Seq<Text>
 decreases xs.len()
{if xs.len()==0{seq![]}else if has_text(seen,xs[0]){unique(xs.skip(1),seen)}else{seq![xs[0]]+unique(xs.skip(1),seen.push(xs[0]))}}
pub open spec fn insert_tag(t:Text,sorted:Seq<Text>)->Seq<Text>
 decreases sorted.len()
{if sorted.len()==0{seq![t]}else if tag_rank(t)>=tag_rank(sorted[0]){seq![t]+sorted}else{seq![sorted[0]]+insert_tag(t,sorted.skip(1))}}
pub open spec fn sort_tags(xs:Seq<Text>)->Seq<Text>
 decreases xs.len()
{if xs.len()==0{seq![]}else{insert_tag(xs[0],sort_tags(xs.skip(1)))}}
pub open spec fn get_all(raw:Seq<Text>)->Seq<Text> {sort_tags(unique(raw,seq![]))}
pub struct Group {pub encoded:Text,pub platforms:Seq<Text>}
pub open spec fn add_group(gs:Seq<Group>,t:Text)->Seq<Group>
 decreases gs.len()
{let p=parse(t);if gs.len()==0{seq![Group{encoded:p.encoded,platforms:seq![p.platform]}]}else if gs[0].encoded==p.encoded{
 seq![Group{platforms:gs[0].platforms.push(p.platform),..gs[0]}]+gs.skip(1)
}else{seq![gs[0]]+add_group(gs.skip(1),t)}}
pub open spec fn gather(tags:Seq<Text>,gs:Seq<Group>)->Seq<Group>
 decreases tags.len()
{if tags.len()==0{gs}else{gather(tags.skip(1),add_group(gs,tags[0]))}}
// Aggregate's SortVersions compares encoded keys directly. This rank is only
// defined as a correspondence for literal releases; do not decode build labels
// here and silently repair the actual encoded-key parsing boundary.
pub open spec fn insert_group(g:Group,gs:Seq<Group>)->Seq<Group>
 decreases gs.len()
{if gs.len()==0{seq![g]}else if rank(g.encoded)>=rank(gs[0].encoded){seq![g]+gs}else{seq![gs[0]]+insert_group(g,gs.skip(1))}}
pub open spec fn sort_groups(gs:Seq<Group>)->Seq<Group>
 decreases gs.len()
{if gs.len()==0{seq![]}else{insert_group(gs[0],sort_groups(gs.skip(1)))}}
pub open spec fn aggregate(tags:Seq<Text>)->Seq<Group> {sort_groups(gather(tags,seq![]))}
pub open spec fn group_compatible(g:Group,c:Text)->bool {has_text(g.platforms,seq![]) || has_text(g.platforms,c)}
pub open spec fn partition(gs:Seq<Group>,c:Text,want:bool)->Seq<Group>
 decreases gs.len()
{if gs.len()==0{seq![]}else{(if group_compatible(gs[0],c)==want{seq![gs[0]]}else{seq![]})+partition(gs.skip(1),c,want)}}
pub open spec fn prefer(gs:Seq<Group>,c:Text)->Seq<Group> {partition(gs,c,true)+partition(gs,c,false)}
pub enum Mode {Normal,AllTags,Dependencies}
pub enum Range {Any,AtLeastOne,MajorTwo}
pub open spec fn matches(v:Text,r:Range)->bool {match r{Range::Any=>rank(v)>0,Range::AtLeastOne=>rank(v)>=1,Range::MajorTwo=>rank(v)==2}}
pub struct Meta {pub deployed:bool,pub description:int}
pub struct Entry {pub tag:Text,pub meta:Meta,pub payload:int}
pub open spec fn find(entries:Seq<Entry>,tag:Text)->Option<Entry>
 decreases entries.len()
{if entries.len()==0{None}else if entries[0].tag==tag{Some(entries[0])}else{find(entries.skip(1),tag)}}
pub open spec fn metadata_tag(g:Group)->Text {joined(g.encoded,if g.platforms.len()>0{g.platforms[0]}else{seq![]})}
pub struct Row {pub version:Text,pub deployed:bool,pub platforms:Seq<Text>,pub metadata:Text,pub description:int,pub selected:Text,pub all_tags:Seq<Text>}
pub struct Rows {pub ok:bool,pub rows:Seq<Row>}
pub open spec fn produce(gs:Seq<Group>,mode:Mode,r:Range,entries:Seq<Entry>,all:Seq<Text>)->Rows
 decreases gs.len()
{if gs.len()==0{Rows{ok:true,rows:seq![]}}else if !matches(decode(gs[0].encoded),r){produce(gs.skip(1),mode,r,entries,all)}else{
 match find(entries,metadata_tag(gs[0])){None=>Rows{ok:false,rows:seq![]},Some(e)=>{
 let row=Row{version:decode(gs[0].encoded),deployed:e.meta.deployed,platforms:if e.meta.deployed && mode!=Mode::AllTags{gs[0].platforms}else{seq![]},metadata:e.tag,description:e.meta.description,selected:seq![],all_tags:if mode==Mode::AllTags{all}else{seq![]}};
 if mode!=Mode::Dependencies{Rows{ok:true,rows:seq![row]}}else{
 let rest=produce(gs.skip(1),mode,r,entries,all);Rows{ok:rest.ok,rows:seq![row]+rest.rows}}
 }}
}}
pub open spec fn service(raw:Seq<Text>,mode:Mode,r:Range,c:Text,entries:Seq<Entry>)->Rows {
 let all=get_all(raw);let groups=aggregate(all);
 produce(if mode==Mode::Dependencies{groups}else{prefer(groups,c)},mode,r,entries,all)
}
pub open spec fn compatible(row:Row,c:Text)->bool {!row.deployed || has_text(row.platforms,c)}
pub open spec fn manager(rows:Seq<Row>,c:Text)->Seq<Row>
 decreases rows.len()
{if rows.len()==0{seq![]}else{(if compatible(rows[0],c){seq![Row{selected:if rows[0].deployed && has_text(rows[0].platforms,c){c}else{seq![]},..rows[0]}]}else{seq![]})+manager(rows.skip(1),c)}}
pub open spec fn display(rows:Seq<Row>,show_all:bool,c:Text)->Seq<Row>
 decreases rows.len()
{if rows.len()==0{seq![]}else{(if show_all || compatible(rows[0],c){seq![rows[0]]}else{seq![]})+display(rows.skip(1),show_all,c)}}
pub struct Install {pub ok:bool,pub dependency:bool,pub consumer:bool,pub payload:Option<int>,pub tag:Option<Text>}
// Composition with the inherited normal new-load contract: exactly one declared
// dependency, no preexisting rows, no other failures, successful pull/compile.
// Payload integer is a provider projection checked by actual callable code.
pub open spec fn normal_consumer(raw:Seq<Text>,r:Range,c:Text,entries:Seq<Entry>)->Install {
 let listed=service(raw,Mode::Dependencies,r,c,entries);let selected=manager(listed.rows,c);
 if !listed.ok || selected.len()==0{Install{ok:false,dependency:false,consumer:false,payload:None,tag:None}}else{
 let row=selected[0];let tag=tag_for(row.version,row.deployed,row.selected);
 match tag{None=>Install{ok:false,dependency:false,consumer:false,payload:None,tag:None},Some(t)=>match find(entries,t){None=>Install{ok:false,dependency:false,consumer:false,payload:None,tag:Some(t)},Some(e)=>Install{ok:true,dependency:true,consumer:true,payload:Some(e.payload),tag:Some(t)}}}
 }
}
pub open spec fn entries()->Seq<Entry> {seq![Entry{tag:t1(),meta:Meta{deployed:true,description:26},payload:1},Entry{tag:t2(),meta:Meta{deployed:true,description:99},payload:2},Entry{tag:t1bad(),meta:Meta{deployed:true,description:91},payload:1}]}
pub open spec fn mixed()->Seq<Text> {seq![t1(),t2()]}
pub open spec fn same()->Seq<Text> {seq![t1bad(),t1()]}
pub open spec fn same_current_first()->Seq<Text> {seq![t1(),t1bad()]}
pub open spec fn normal()->Rows {service(mixed(),Mode::Normal,Range::Any,current(),entries())}
pub open spec fn all()->Rows {service(mixed(),Mode::AllTags,Range::Any,current(),entries())}
pub open spec fn dependencies()->Rows {service(mixed(),Mode::Dependencies,Range::Any,current(),entries())}
pub open spec fn ranged()->Install {normal_consumer(mixed(),Range::AtLeastOne,current(),entries())}
pub open spec fn pinned()->Install {normal_consumer(mixed(),Range::MajorTwo,current(),entries())}
pub open spec fn triple()->Seq<Text> {seq![t1(),t2(),t1bad()]}
pub open spec fn multi()->Rows {service(triple(),Mode::Normal,Range::Any,current(),entries())}
pub open spec fn standalone()->Install {Install{consumer:false,..normal_consumer(mixed(),Range::Any,current(),entries())}}
pub open spec fn payload_value(x:Install)->int {match x.payload{Some(n)=>n,None=>0}}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
// Additional exact two-literal boundary, independently native-calibrated.
// FromString receives encoded aggregate keys directly. In these hyphen-free
// literals, its fallback Build is dot piece four, then an explicit '+' overrides
// Build. Core property marshalling is not proved; identical core tokens suffice
// for the equality comparison witnessed here, regardless of deterministic casts.
pub open spec fn char_at(s:Text,c:char,i:nat)->nat
 decreases if i<s.len(){s.len()-i}else{0}
{if i>=s.len(){s.len()}else if s[i as int]==c{i}else{char_at(s,c,i+1)}}
pub open spec fn head(s:Text,c:char)->Text {s.take(char_at(s,c,0) as int)}
pub open spec fn tail(s:Text,c:char)->Text {let p=char_at(s,c,0);if p<s.len(){s.skip((p+1) as int)}else{seq![]}}
pub open spec fn dot_piece(s:Text,n:nat)->Text
 decreases n
{if n==0{head(s,'.')}else{dot_piece(tail(s,'.'),(n-1) as nat)}}
pub struct BuildFields {pub major:Text,pub minor:Text,pub patch:Text,pub build:Text}
pub open spec fn deep_fields(v:Text)->BuildFields {
 BuildFields{major:head(dot_piece(v,0),'+'),minor:head(dot_piece(v,1),'+'),patch:head(dot_piece(v,2),'+'),
 build:if has_char(v,'+'){tail(v,'+')}else{dot_piece(v,3)}}
}
pub open spec fn same_core(a:BuildFields,b:BuildFields)->bool {a.major==b.major && a.minor==b.minor && a.patch==b.patch}
pub open spec fn lex_less(a:Text,b:Text)->bool
 decreases a.len()+b.len()
{if a.len()==0{b.len()>0}else if b.len()==0{false}else if a[0]!=b[0]{(a[0] as int)<(b[0] as int)}else{lex_less(a.skip(1),b.skip(1))}}
pub open spec fn deep_a()->Text {seq!['1','.','0','.','0','_','a','.','1','.','a']}
pub open spec fn deep_z()->Text {seq!['1','.','0','.','0','_','a','.','1','.','z']}
pub struct PairOrder {pub known:bool,pub keys:Seq<Text>}
// Only same-core field pairs are covered. Unknown is explicit, not an asserted
// error or order for unrelated versions or provider-dependent numeric casts.
pub open spec fn pair_order(left:Text,right:Text,decode_first:bool)->PairOrder {
 let a=deep_fields(if decode_first{decode(parse(left).encoded)}else{left});
 let b=deep_fields(if decode_first{decode(parse(right).encoded)}else{right});
 if !same_core(a,b){PairOrder{known:false,keys:seq![]}}else{
 PairOrder{known:true,keys:if !lex_less(a.build,b.build){seq![left,right]}else{seq![right,left]}}
 }
}
pub open spec fn deep_natural()->Seq<Text> {if lex_less(deep_a(),deep_z()){seq![deep_a(),deep_z()]}else{seq![deep_z(),deep_a()]}}
pub open spec fn deep_aggregate()->PairOrder {pair_order(deep_natural()[0],deep_natural()[1],false)}
pub open spec fn deep_oci()->PairOrder {pair_order(joined(deep_a(),current()),joined(deep_z(),current()),true)}
// Both literal groups have current-platform metadata and satisfy <2. Service
// normal/display stops at the first; dependency enumeration emits both. The
// manager's actual map key omits Build, so its later same-base row overwrites.
pub open spec fn deep_decoded_a()->Text {seq!['1','.','0','.','0','+','a','.','1','.','a']}
pub open spec fn deep_decoded_z()->Text {seq!['1','.','0','.','0','+','a','.','1','.','z']}
pub open spec fn deep_service_first_version()->Text {decode(deep_aggregate().keys[0])}
pub open spec fn deep_dependency_versions()->Seq<Text> {seq![decode(deep_aggregate().keys[0]),decode(deep_aggregate().keys[1])]}
pub open spec fn same_base_insert(rows:Seq<Text>,selected:Option<Text>)->Option<Text>
 decreases rows.len()
{if rows.len()==0{selected}else{same_base_insert(rows.skip(1),Some(rows[0]))}}
pub open spec fn deep_manager_version()->Option<Text> {same_base_insert(deep_dependency_versions(),None)}
pub open spec fn deep_pull_tag()->Option<Text> {match deep_manager_version(){Some(v)=>tag_for(v,true,current()),None=>None}}
pub proof fn deep_build_neighbors()
 ensures deep_fields(deep_a()).major==deep_fields(deep_z()).major,
 deep_fields(deep_a()).minor==deep_fields(deep_z()).minor,
 deep_fields(deep_a()).patch==deep_fields(deep_z()).patch,
 deep_fields(deep_a()).build==deep_fields(deep_z()).build,
 deep_aggregate().known,deep_aggregate().keys==seq![deep_a(),deep_z()],
 deep_oci().known,deep_oci().keys==seq![joined(deep_z(),current()),joined(deep_a(),current())],
 deep_service_first_version()==deep_decoded_a(),deep_dependency_versions()==seq![deep_decoded_a(),deep_decoded_z()],
 deep_manager_version()==Some(deep_decoded_z()),deep_pull_tag()==Some(joined(deep_z(),current()))
{
 assert(deep_fields(deep_a()).major==seq!['1']) by (compute);
 assert(deep_fields(deep_z()).major==seq!['1']) by (compute);
 assert(deep_fields(deep_a()).minor==seq!['0']) by (compute);
 assert(deep_fields(deep_z()).minor==seq!['0']) by (compute);
 assert(deep_fields(deep_a()).patch==seq!['0','_','a']) by (compute);
 assert(deep_fields(deep_z()).patch==seq!['0','_','a']) by (compute);
 assert(deep_fields(deep_a()).build==seq!['1']) by (compute);
 assert(deep_fields(deep_z()).build==seq!['1']) by (compute);
 assert(deep_natural()==seq![deep_a(),deep_z()]) by (compute);
 assert(decode(deep_a())==deep_decoded_a()) by (compute);
 assert(decode(deep_z())==deep_decoded_z()) by (compute);
 assert(encode(deep_decoded_z())==deep_z()) by (compute);
 assert(parse(joined(deep_a(),current())).encoded==deep_a()) by (compute);
 assert(parse(joined(deep_z(),current())).encoded==deep_z()) by (compute);
 assert(deep_fields(deep_decoded_a()).major==seq!['1']) by (compute);
 assert(deep_fields(deep_decoded_z()).major==seq!['1']) by (compute);
 assert(deep_fields(deep_decoded_a()).minor==seq!['0']) by (compute);
 assert(deep_fields(deep_decoded_z()).minor==seq!['0']) by (compute);
 assert(deep_fields(deep_decoded_a()).patch==seq!['0']) by (compute);
 assert(deep_fields(deep_decoded_z()).patch==seq!['0']) by (compute);
 assert(deep_fields(deep_decoded_a()).build==seq!['a','.','1','.','a']) by (compute);
 assert(deep_fields(deep_decoded_z()).build==seq!['a','.','1','.','z']) by (compute);
 assert(lex_less(seq!['a','.','1','.','a'],seq!['a','.','1','.','z'])) by (compute);
 reveal_with_fuel(lex_less,6);reveal_with_fuel(same_base_insert,3);
 assert(deep_aggregate().keys==seq![deep_a(),deep_z()]);
 assert(deep_oci().keys==seq![joined(deep_z(),current()),joined(deep_a(),current())]);
 assert(deep_service_first_version()==deep_decoded_a());
 assert(deep_manager_version()==Some(deep_decoded_z()));
 assert(deep_pull_tag()==Some(joined(deep_z(),current())));
}
pub proof fn replace_cons(c:char,s:Text,from:char,to:char)
 ensures replace(seq![c]+s,from,to)==seq![if c==from{to}else{c}]+replace(s,from,to)
{
 assert((seq![c]+s).skip(1)==s);
}
pub proof fn codec_roundtrip(s:Text)
 requires !has_char(s,'_')
 ensures decode(encode(s))==s
 decreases s.len()
{
 reveal_with_fuel(replace,3);
 if s.len()>0 {
  assert(s[0]!='_');
  codec_roundtrip(s.skip(1));
  assert(encode(s)==seq![if s[0]=='+'{'_'}else{s[0]}]+encode(s.skip(1)));
  replace_cons(if s[0]=='+'{'_'}else{s[0]},encode(s.skip(1)),'_','+');
  assert(decode(encode(s))==seq![s[0]]+decode(encode(s.skip(1))));
  assert(s==seq![s[0]]+s.skip(1));
 }
}
pub proof fn raw_neighbors()
 ensures parse(t1()).encoded==v1(),parse(t1()).platform==current(),get_all(mixed())==seq![t2(),t1()],
 aggregate(get_all(same()))[0].platforms==seq![bad(),current()],
 aggregate(get_all(same_current_first()))[0].platforms==seq![current(),bad()],
 metadata_tag(aggregate(get_all(same()))[0])==t1bad(),
 metadata_tag(aggregate(get_all(same_current_first()))[0])==t1(),
 decode(parse(joined(encode(build_version()),current())).encoded)==build_version(),
 encode(build_version())==seq!['1','.','2','.','3','-','a','l','p','h','a','_','b','u','i','l','d','.','7'],
 parse(joined(encode(build_version()),current())).platform==current(),
 get_all(seq![t1(),t1(),t2()])==seq![t2(),t1()]
{
 assert(parse(t1()).encoded==v1()) by (compute);assert(parse(t1()).platform==current()) by (compute);
 assert(get_all(mixed())==seq![t2(),t1()]) by (compute);
 assert(aggregate(get_all(same()))[0].platforms==seq![bad(),current()]) by (compute);
 assert(aggregate(get_all(same_current_first()))[0].platforms==seq![current(),bad()]) by (compute);
 assert(metadata_tag(aggregate(get_all(same()))[0])==t1bad()) by (compute);
 assert(metadata_tag(aggregate(get_all(same_current_first()))[0])==t1()) by (compute);
 assert(decode(parse(joined(encode(build_version()),current())).encoded)==build_version()) by (compute);
 assert(tag_for(v1(),true,seq![])==None) by (compute);
 assert(encode(build_version())==seq!['1','.','2','.','3','-','a','l','p','h','a','_','b','u','i','l','d','.','7']) by (compute);
 assert(parse(joined(encode(build_version()),current())).platform==current()) by (compute);
 assert(get_all(seq![t1(),t1(),t2()])==seq![t2(),t1()]) by (compute);
}
pub proof fn selected_neighbors()
 ensures normal().ok,normal().rows.len()==1,normal().rows[0].version==v1(),normal().rows[0].platforms==seq![current()],
 all().rows.len()==1,all().rows[0].version==v1(),all().rows[0].platforms.len()==0,all().rows[0].all_tags==seq![t2(),t1()],
 dependencies().rows.len()==2,dependencies().rows[0].version==v2(),dependencies().rows[1].version==v1(),
 manager(dependencies().rows,current()).len()==1,manager(dependencies().rows,current())[0].selected==current(),
 display(all().rows,true,current()).len()==1,manager(all().rows,current()).len()==0,
 ranged().ok,ranged().dependency,ranged().consumer,ranged().payload==Some(1int),ranged().tag==Some(t1()),
 !pinned().ok,!pinned().dependency,!pinned().consumer,pinned().payload==None,
 multi().rows.len()==1,multi().rows[0].platforms==seq![current(),bad()],manager(multi().rows,current())[0].selected==current()
{
 assert(normal().ok) by (compute);assert(normal().rows.len()==1) by (compute);assert(normal().rows[0].version==v1()) by (compute);assert(normal().rows[0].platforms==seq![current()]) by (compute);
 assert(all().rows.len()==1) by (compute);assert(all().rows[0].version==v1()) by (compute);assert(all().rows[0].platforms.len()==0) by (compute);assert(all().rows[0].all_tags==seq![t2(),t1()]) by (compute);
 assert(dependencies().rows.len()==2) by (compute);assert(dependencies().rows[0].version==v2()) by (compute);assert(dependencies().rows[1].version==v1()) by (compute);
 assert(manager(dependencies().rows,current()).len()==1) by (compute);assert(manager(dependencies().rows,current())[0].selected==current()) by (compute);
 assert(display(all().rows,true,current()).len()==1) by (compute);assert(manager(all().rows,current()).len()==0) by (compute);
 assert(ranged().ok) by (compute);assert(ranged().dependency) by (compute);assert(ranged().consumer) by (compute);assert(ranged().payload==Some(1int)) by (compute);assert(ranged().tag==Some(t1())) by (compute);
 assert(!pinned().ok) by (compute);assert(!pinned().dependency) by (compute);assert(!pinned().consumer) by (compute);assert(pinned().payload==None) by (compute);
 assert(multi().rows.len()==1) by (compute);assert(multi().rows[0].platforms==seq![current(),bad()]) by (compute);assert(manager(multi().rows,current())[0].selected==current()) by (compute);
}
}
fn main(){}
