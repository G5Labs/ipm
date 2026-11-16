use vstd::prelude::*;

verus! {
// String property names and array indexes are injected into integer keys by a decoder.
// A Container's `array` tag is retained; arrays are never treated as replacement lists.
pub enum Json { Atom(int), Container { array: bool, entries: Map<int, Json> } }
pub open spec fn empty(array: bool) -> Json { Json::Container { array, entries: Map::empty() } }
pub open spec fn compatible(old: Option<Json>, array: bool) -> bool {
    match old { None => true, Some(Json::Atom(_)) => false,
        Some(Json::Container { array: a, entries: _ }) => a == array }
}
pub open spec fn entries(old: Option<Json>) -> Map<int, Json> {
    match old { Some(Json::Container { array: _, entries: m }) => m, _ => Map::empty() }
}
pub open spec fn get(m: Map<int, Json>, k: int) -> Option<Json> { if m.dom().contains(k) { Some(m[k]) } else { None } }
// Explicit recursion budget: successful correspondence requires fuel above source depth.
// Exhaustion is an open modeling bound, not an IPM error prediction.
pub open spec fn merge_value(old: Option<Json>, new: Json, fuel: nat) -> Option<Json>
    decreases fuel
{
    match new {
        Json::Atom(v) => Some(Json::Atom(v)),
        Json::Container { array, entries: src } => {
            let dst = entries(old);
            if fuel == 0 || !compatible(old, array) { None }
            else if forall|k: int| src.dom().contains(k) ==> #[trigger] merge_value(
                get(dst, k), src[k], (fuel - 1) as nat).is_some() {
                Some(Json::Container { array, entries: Map::new(
                    src.dom().union(dst.dom()),
                    |k: int| if src.dom().contains(k) { merge_value(
                        get(dst, k), src[k], (fuel - 1) as nat).unwrap() }
                    else { dst[k] }) })
            } else { None }
        }
    }
}
pub open spec fn at(value: Json, k: int) -> Option<Json> {
    let m = entries(Some(value));
    if m.dom().contains(k) { Some(m[k]) } else { None }
}
pub proof fn scalar_overwrites(old: Option<Json>, x: int, fuel: nat)
    ensures merge_value(old, Json::Atom(x), fuel) == Some(Json::Atom(x)) {}
pub proof fn recursive_frame(old: Json, new: Json, fuel: nat, k: int)
    requires fuel > 0, merge_value(Some(old), new, fuel).is_some(),
        !entries(Some(new)).dom().contains(k), entries(Some(old)).dom().contains(k),
        matches!(new, Json::Container { .. })
    ensures at(merge_value(Some(old), new, fuel).unwrap(), k) == at(old, k)
{}
pub proof fn scalar_container_is_error(x: int, a: bool, m: Map<int, Json>, fuel: nat)
    ensures merge_value(Some(Json::Atom(x)), Json::Container { array: a, entries: m }, fuel).is_none() {}
pub proof fn mismatched_container_is_error(a: bool, d: Map<int, Json>, s: Map<int, Json>, fuel: nat)
    ensures merge_value(Some(Json::Container { array: a, entries: d }), Json::Container { array: !a, entries: s }, fuel).is_none() {}

pub open spec fn array_tag(value: Json) -> Option<bool> {
    match value { Json::Atom(_) => None, Json::Container { array, entries: _ } => Some(array) }
}
pub open spec fn json_eq(a: Json, b: Json, fuel: nat) -> bool
    decreases fuel
{
    match (a, b) {
        (Json::Atom(x), Json::Atom(y)) => x == y,
        (Json::Container { array: aa, entries: am }, Json::Container { array: ba, entries: bm }) => {
            fuel > 0 && aa == ba && am.dom() == bm.dom() &&
            forall|k: int| am.dom().contains(k) ==> #[trigger] json_eq(am[k], bm[k], (fuel-1) as nat)
        },
        _ => false,
    }
}
pub open spec fn lookup(value: Json, path: Seq<int>) -> Option<Json>
    decreases path.len()
{
    if path.len() == 0 { Some(value) }
    else { match at(value, path[0]) {
        None => None, Some(child) => lookup(child, path.subrange(1, path.len() as int))
    } }
}
pub proof fn scalar_on_lookup_path_is_empty(x: int, path: Seq<int>)
    requires path.len() > 0
    ensures lookup(Json::Atom(x), path).is_none() {}
// Primitive callback is the environment decoder/replacement contract, not a predicted outcome.
pub open spec fn evaluate_json(value: Json, fuel: nat, primitive: spec_fn(int) -> Option<int>) -> Option<Json>
    decreases fuel
{
    match value {
        Json::Atom(x) => if primitive(x).is_some() { Some(Json::Atom(primitive(x).unwrap())) } else { None },
        Json::Container { array, entries: m } => {
            if fuel == 0 { None }
            else if forall|k: int| m.dom().contains(k) ==> #[trigger] evaluate_json(m[k], (fuel-1) as nat, primitive).is_some() {
                Some(Json::Container { array, entries: Map::new(m.dom(), |k: int| evaluate_json(m[k], (fuel-1) as nat, primitive).unwrap()) })
            } else { None }
        }
    }
}
pub proof fn evaluate_keeps_container_shape(a: bool, m: Map<int, Json>, fuel: nat, primitive: spec_fn(int) -> Option<int>)
    requires evaluate_json(Json::Container { array: a, entries: m }, fuel, primitive).is_some()
    ensures entries(evaluate_json(Json::Container { array: a, entries: m }, fuel, primitive)).dom() == m.dom() {}
// A live reference to the process singleton retains identity; another process has no such reference.
pub open spec fn singleton_get(held: Option<int>, fresh: int) -> int { held.unwrap_or(fresh) }
pub proof fn singleton_same_while_referenced(id: int, candidate: int)
    ensures singleton_get(Some(id), candidate) == id, singleton_get(None, candidate) == candidate {}

pub struct FileInput { pub read_parse_ok: bool, pub parsed: Json, pub primitive: spec_fn(int) -> Option<int> }
pub struct LoadResult { pub config: Json, pub ok: bool }
pub open spec fn load_file(old: Json, file: FileInput, fuel: nat) -> LoadResult {
    let merged = merge_value(Some(old), file.parsed, fuel);
    let evaluated = if merged.is_some() { evaluate_json(merged.unwrap(), fuel, file.primitive) } else { None };
    if file.read_parse_ok && array_tag(file.parsed) == Some(false) && merged.is_some() && evaluated.is_some() {
        LoadResult { config: evaluated.unwrap(), ok: true }
    } else { LoadResult { config: empty(false), ok: false } }
}
pub open spec fn load_files(old: Json, files: Seq<FileInput>, fuel: nat) -> LoadResult
    decreases files.len()
{
    if files.len() == 0 { LoadResult { config: old, ok: true } }
    else {
        let first = load_file(old, files[0], fuel);
        if !first.ok { first } else { load_files(first.config, files.subrange(1, files.len() as int), fuel) }
    }
}
pub open spec fn command(files: Seq<FileInput>, fuel: nat) -> LoadResult { load_files(empty(false), files, fuel) }
pub proof fn any_file_error_clears(old: Json, file: FileInput, fuel: nat)
    requires !file.read_parse_ok || array_tag(file.parsed) != Some(false)
    ensures !load_file(old, file, fuel).ok, load_file(old, file, fuel).config == empty(false) {}
pub proof fn command_without_files_resets(fuel: nat)
    ensures command(Seq::empty(), fuel).config == empty(false), command(Seq::empty(), fuel).ok {}
pub proof fn later_failure_erases_earlier(first: FileInput, bad: FileInput, fuel: nat)
    requires !bad.read_parse_ok
    ensures !command(seq![first, bad], fuel).ok, command(seq![first, bad], fuel).config == empty(false)
{
    reveal_with_fuel(load_files, 3);
}
pub proof fn every_failed_file_sequence_is_empty(old: Json, files: Seq<FileInput>, fuel: nat)
    requires !load_files(old, files, fuel).ok
    ensures load_files(old, files, fuel).config == empty(false)
    decreases files.len()
{
    if files.len() > 0 {
        let first = load_file(old, files[0], fuel);
        if first.ok { every_failed_file_sequence_is_empty(first.config, files.subrange(1, files.len() as int), fuel); }
    }
}
pub open spec fn clear(_old: Json) -> Json { empty(false) }
pub open spec fn is_empty(config: Json) -> bool { entries(Some(config)).dom().is_empty() }
pub proof fn empty_object_is_empty(old: Json, k: int, v: Json)
    ensures is_empty(clear(old)), !is_empty(Json::Container { array: false, entries: map![k => v] }) {}
// Environment contract: IRIS compiler workers do not share this process's singleton.
pub open spec fn compile_single(config_empty: bool) -> bool { !config_empty }
pub open spec fn generator_visible(same_process: bool, config_value: Option<int>) -> Option<int> {
    if same_process { config_value } else { None }
}
pub proof fn loaded_configuration_is_visible(v: int)
    ensures generator_visible(compile_single(false), Some(v)) == Some(v),
        generator_visible(false, Some(v)).is_none() {}

// ICU replacement-text transducer: dollars refer to match groups; backslash quotes one char.
// Group values are inputs from the regex matcher; this does not assume a fixture result.
pub open spec fn replacement(text: Seq<char>, whole: Seq<char>, group: Seq<char>) -> Option<Seq<char>>
    decreases text.len()
{
    if text.len() == 0 { Some(Seq::empty()) }
    else if text[0] == '\\' && text.len() > 1 {
        let rest = replacement(text.subrange(2, text.len() as int), whole, group);
        if rest.is_some() { Some(seq![text[1]] + rest.unwrap()) } else { None }
    } else if text[0] == '$' {
        if text.len() > 1 && (text[1] == '0' || text[1] == '1') {
            let rest = replacement(text.subrange(2, text.len() as int), whole, group);
            if rest.is_some() { Some((if text[1] == '0' { whole } else { group }) + rest.unwrap()) } else { None }
        } else { None }
    } else {
        let rest = replacement(text.subrange(1, text.len() as int), whole, group);
        if rest.is_some() { Some(seq![text[0]] + rest.unwrap()) } else { None }
    }
}
pub proof fn dollar_is_not_literal(whole: Seq<char>, group: Seq<char>)
    requires group != seq!['$', '1']
    ensures replacement(seq!['$', '1'], whole, group) == Some(group),
        replacement(seq!['$', '9'], whole, group).is_none(),
        replacement(seq!['a','\\','b'], whole, group) == Some(seq!['a','b'])
{ reveal_with_fuel(replacement, 4); }

pub open spec fn x_obj() -> Json { Json::Container { array: false, entries: map![1int => Json::Atom(1)] } }
pub open spec fn y_obj() -> Json { Json::Container { array: false, entries: map![2int => Json::Atom(2)] } }
pub open spec fn object_array(x: Json) -> Json { Json::Container { array: true, entries: map![0int => x] } }
pub proof fn object_in_array_keeps_both_keys()
    ensures at(at(merge_value(Some(object_array(x_obj())), object_array(y_obj()), 4).unwrap(), 0).unwrap(), 1) == Some(Json::Atom(1)),
        at(at(merge_value(Some(object_array(x_obj())), object_array(y_obj()), 4).unwrap(), 0).unwrap(), 2) == Some(Json::Atom(2))
{ reveal_with_fuel(merge_value, 5); }
pub proof fn primitive_array_keeps_tail()
    ensures at(merge_value(Some(Json::Container { array: true, entries: map![0int => Json::Atom(1), 1int => Json::Atom(2)] }),
        Json::Container { array: true, entries: map![0int => Json::Atom(3)] }, 3).unwrap(), 0) == Some(Json::Atom(3)),
        at(merge_value(Some(Json::Container { array: true, entries: map![0int => Json::Atom(1), 1int => Json::Atom(2)] }),
        Json::Container { array: true, entries: map![0int => Json::Atom(3)] }, 3).unwrap(), 1) == Some(Json::Atom(2))
{ reveal_with_fuel(merge_value, 4); }
}
fn main() {}
