use vstd::prelude::*;
verus! {
pub enum Action { Load, Install, Uninstall, Update }
pub struct Row {
    pub id:int, pub namespace:int, pub action:Action, pub package:Seq<u8>, pub version:Seq<u8>,
    pub source:Seq<u8>,pub moniker:Seq<u8>,pub details:Seq<u8>,pub ended:bool,
    pub status:int,pub committed:bool,pub finalized:bool,
}
pub open spec fn set_source(r:Row,source:Seq<u8>,moniker:Seq<u8>,details:Seq<u8>) -> Row { Row {source,moniker,details,..r} }
pub open spec fn finalize(r:Row,status:int) -> Row { Row {status,ended:true,committed:status == 1,..r} }
pub enum Text { Success, Error00, Error, Raw(int) }
pub open spec fn success_to_string(status:int,detail:bool) -> Text {
    if detail { Text::Raw(status) } else if status == 1 { Text::Success }
    else if status == 0 { Text::Error00 } else { Text::Error }
}
pub open spec fn source_shown(r:Row) -> bool { r.action == Action::Install }
pub proof fn unfinished_row_displays_error00()
    ensures success_to_string(0,false) == Text::Error00, success_to_string(0,true) == Text::Raw(0),
        success_to_string(1,false) == Text::Success
{}
pub proof fn source_and_identity_are_preserved(r:Row,s:Seq<u8>,m:Seq<u8>,d:Seq<u8>)
    ensures set_source(r,s,m,d).id == r.id,set_source(r,s,m,d).namespace == r.namespace,
        set_source(r,s,m,d).source == s,set_source(r,s,m,d).moniker == m,set_source(r,s,m,d).details == d
{}
pub proof fn finalize_preserves_committed_on_late_error(r:Row)
    requires r.committed
    ensures finalize(r,2).committed,finalize(r,2).status == 2,finalize(r,2).ended
{}
pub open spec fn letter(c:u8) -> bool { (65 <= c <= 90) || (97 <= c <= 122) }
pub open spec fn alnum(c:u8) -> bool { letter(c) || (48 <= c <= 57) }
pub open spec fn valid_column(s:Seq<u8>) -> bool {
    s.len() > 0 && letter(s[0]) && forall|i:int| 1 <= i < s.len() ==> alnum(s[i])
}
pub struct Filter { pub key:Seq<u8>, pub value_present:bool, pub matches:Set<int>, pub column_known:bool }
// `matches` is the SQL operator contract for this column/value. Empty values differ from absent ones.
// Native ICU/SQL collation, LIKE, date conversion and access rights remain boundary premises.
pub open spec fn where_prefix(fs:Seq<Filter>) -> Seq<Filter>
    decreases fs.len()
{
    if fs.len() == 0 || !fs[0].value_present || !valid_column(fs[0].key) { seq![] }
    else { seq![fs[0]] + where_prefix(fs.drop_first()) }
}
pub open spec fn selection(ids:Set<int>,fs:Seq<Filter>) -> Set<int>
    decreases fs.len()
{
    if fs.len() == 0 { ids } else { selection(ids.intersect(fs[0].matches),fs.drop_first()) }
}
pub open spec fn delete(ids:Set<int>,fs:Seq<Filter>) -> Set<int> { ids.difference(selection(ids,where_prefix(fs))) }
pub proof fn invalid_first_key_widens_delete(ids:Set<int>,f:Filter,rest:Seq<Filter>)
    requires !valid_column(f.key)
    ensures delete(ids,seq![f]+rest) =~= Set::<int>::empty()
{}
pub proof fn valid_single_key_deletes_only_matches(ids:Set<int>,f:Filter)
    requires valid_column(f.key),f.value_present
    ensures delete(ids,seq![f]) =~= ids.difference(f.matches)
{ reveal_with_fuel(where_prefix,4); reveal_with_fuel(selection,4); assert(seq![f].drop_first() =~= Seq::<Filter>::empty()); }
pub proof fn earlier_filter_remains_effective(ids:Set<int>,a:Filter,b:Filter)
    requires valid_column(a.key),a.value_present,!valid_column(b.key)
    ensures delete(ids,seq![a,b]) =~= ids.difference(a.matches)
{ reveal_with_fuel(where_prefix,4); reveal_with_fuel(selection,4); assert(seq![a,b].drop_first() =~= seq![b]); assert(seq![a].drop_first() =~= Seq::<Filter>::empty()); }
pub proof fn underscore_column_is_invalid()
    ensures !valid_column(seq![86u8,101u8,114u8,115u8,105u8,111u8,110u8,95u8,77u8,97u8,106u8,111u8,114u8])
{}
pub open spec fn details(rows:Map<(int,int),Row>,ns:int,id:int) -> Option<Row> {
    if rows.contains_key((ns,id)) { Some(rows[(ns,id)]) } else { None }
}
pub open spec fn cli_global(rows:Map<(int,int),Row>,scopes:Set<int>) -> Set<(int,int)> {
    rows.dom().filter(|key:(int,int)| scopes.contains(key.0))
}
// GlobalRecordsFetch invokes RecordsFetch in caller NS. It repeats caller rows once per result set;
// row output has neither namespace nor ID, unlike CLI find -globally.
pub open spec fn programmatic_global(db:Map<int,Seq<Row>>,caller:int,handles:Seq<int>) -> Seq<Row>
    recommends db.contains_key(caller)
    decreases handles.len()
{
    if handles.len() == 0 { seq![] } else { db[caller] + programmatic_global(db,caller,handles.drop_first()) }
}
pub proof fn local_id_collision_is_scoped(rows:Map<(int,int),Row>,a:int,b:int,id:int)
    requires a != b,rows.contains_key((a,id)),!rows.contains_key((b,id))
    ensures details(rows,a,id).is_some(),details(rows,b,id).is_none()
{}
pub proof fn global_programmatic_repeats_caller(db:Map<int,Seq<Row>>,caller:int,a:int,b:int)
    requires db.contains_key(caller)
    ensures programmatic_global(db,caller,seq![a,b]) == db[caller]+db[caller]
{ reveal_with_fuel(programmatic_global,4); assert(seq![a,b].drop_first() =~= seq![b]); assert(seq![b].drop_first() =~= Seq::<int>::empty()); }
pub struct Purge {pub rows:Set<int>,pub marker_today:bool}
pub open spec fn purge(rows:Set<int>,ran_today:bool,retain_positive:bool,quoted_date_matches:Set<int>) -> Purge {
    if ran_today || !retain_positive {Purge {rows,marker_today:ran_today}}
    else {Purge {rows:rows.difference(quoted_date_matches),marker_today:true}}
}
pub proof fn purge_can_advance_marker_without_removal(rows:Set<int>)
    ensures purge(rows,false,true,Set::empty()).marker_today,
        purge(rows,false,true,Set::empty()).rows =~= rows
{}
// Hypothesis H-DELETE-FINALIZE: save of an in-memory persisted twin can recreate a deleted row.
// The save-success input is not a proved IRIS behavior. Independent native check decides correspondence.
pub open spec fn persist_after_delete(rows:Map<int,Row>,cached:Row,save_recreates:bool) -> Map<int,Row> {
    if save_recreates { rows.insert(cached.id,Row{finalized:true,..cached}) } else {rows}
}
pub proof fn delete_wins_depends_on_save_semantics(rows:Map<int,Row>,r:Row)
    ensures persist_after_delete(rows.remove(r.id),r,true).contains_key(r.id),
        !persist_after_delete(rows.remove(r.id),r,false).contains_key(r.id)
{}
}
fn main() {}
