// Iteration75: -env file loading with file presence and parse success derived from a filesystem state.
//
// The inherited loader (consolidated/verus/env_config.rs::load_file) takes `read_parse_ok` as an input. Here
// that flag is computed from a filesystem map, so a missing file, an unparsable file and a readable file are
// three states of the same input rather than three values of the decision. The merge, evaluation and
// clear-on-failure logic is the inherited one.
//
// Source roles at pin 571e04a1:
//   Lifecycle/Base.cls::LoadEnvironmentConfig (2188-2200)  Clear, then Load each ";"-separated path in order
//   General/EnvironmentConfig.cls::Load (9-32)              %File.Exists or throw FileDoesNotExist (12-14);
//       LinkToFile and %FromJSON or throw (15-22); a JSON array root throws (23-25); MergeDynamicObjects and
//       %Evaluate (26-27); any throw clears the configuration and rethrows (28-31)
// Premise: a present file the process cannot read or link fails like an unparsable one (both throw inside
// the same try block), so the model folds them into `fs[path] == None`.
#[path = "../../consolidated/verus/env_config.rs"]
mod env;

use vstd::prelude::*;
use env::{Json, FileInput, LoadResult, empty, at, load_file, load_files, command};

verus! {

// Filesystem: a path absent from the map does not exist; Some(None) exists but does not parse as JSON;
// Some(Some(j)) parses to j.
pub type Fs = Map<int, Option<Json>>;

pub open spec fn present(fs: Fs, path: int) -> bool { fs.contains_key(path) }
pub open spec fn parses(fs: Fs, path: int) -> bool { present(fs, path) && fs[path].is_some() }

// EnvironmentConfig.cls:12-22: existence, then link and parse.
pub open spec fn file_input(fs: Fs, path: int, primitive: spec_fn(int) -> Option<int>) -> FileInput {
    FileInput {
        read_parse_ok: parses(fs, path),
        parsed: if parses(fs, path) { fs[path].unwrap() } else { empty(false) },
        primitive,
    }
}

pub open spec fn inputs(fs: Fs, paths: Seq<int>, primitive: spec_fn(int) -> Option<int>) -> Seq<FileInput> {
    paths.map(|_i: int, p: int| file_input(fs, p, primitive))
}

// Base.cls:2188-2200: one -env command (the configuration is cleared first).
pub open spec fn env_command(fs: Fs, paths: Seq<int>, primitive: spec_fn(int) -> Option<int>, fuel: nat) -> LoadResult {
    command(inputs(fs, paths, primitive), fuel)
}

pub open spec fn ident() -> spec_fn(int) -> Option<int> { |x: int| Some(x) }
pub open spec fn x_obj() -> Json { Json::Container { array: false, entries: map![1int => Json::Atom(1)] } }
pub open spec fn y_obj() -> Json { Json::Container { array: false, entries: map![2int => Json::Atom(2)] } }
pub open spec fn y_array() -> Json { Json::Container { array: true, entries: map![0int => y_obj()] } }

// Paths: 1 holds {x:1}, 2 holds {y:2}, 4 holds [{y:2}], 5 exists but is not JSON, 3 does not exist.
pub open spec fn fs0() -> Fs {
    map![1int => Some(x_obj()), 2int => Some(y_obj()), 4int => Some(y_array()), 5int => None::<Json>]
}

proof fn merge_x_into_empty()
    ensures env::merge_value(Some(empty(false)), x_obj(), 3) == Some(x_obj())
{
    reveal_with_fuel(env::merge_value, 3);
    let m = env::merge_value(Some(empty(false)), x_obj(), 3);
    assert(env::merge_value(None, Json::Atom(1), 2) == Some(Json::Atom(1)));
    assert(m.is_some());
    match m.unwrap() {
        Json::Container { array, entries } => {
            assert(entries =~= map![1int => Json::Atom(1)]);
        }
        Json::Atom(_) => {}
    }
}

proof fn eval_x()
    ensures env::evaluate_json(x_obj(), 3, ident()) == Some(x_obj())
{
    reveal_with_fuel(env::evaluate_json, 3);
    assert(env::evaluate_json(Json::Atom(1), 2, ident()) == Some(Json::Atom(1)));
    let e = env::evaluate_json(x_obj(), 3, ident());
    assert(e.is_some());
    match e.unwrap() {
        Json::Container { array, entries } => { assert(entries =~= map![1int => Json::Atom(1)]); }
        Json::Atom(_) => {}
    }
}

proof fn merge_y_into_x()
    ensures env::merge_value(Some(x_obj()), y_obj(), 3).is_some(),
        at(env::merge_value(Some(x_obj()), y_obj(), 3).unwrap(), 1) == Some(Json::Atom(1)),
        at(env::merge_value(Some(x_obj()), y_obj(), 3).unwrap(), 2) == Some(Json::Atom(2)),
{
    reveal_with_fuel(env::merge_value, 3);
    assert(env::merge_value(None, Json::Atom(2), 2) == Some(Json::Atom(2)));
}

proof fn eval_xy(c: Json)
    requires c == env::merge_value(Some(x_obj()), y_obj(), 3).unwrap(),
        env::merge_value(Some(x_obj()), y_obj(), 3).is_some(),
        at(c, 1) == Some(Json::Atom(1)), at(c, 2) == Some(Json::Atom(2)),
    ensures env::evaluate_json(c, 3, ident()).is_some(),
        at(env::evaluate_json(c, 3, ident()).unwrap(), 1) == Some(Json::Atom(1)),
        at(env::evaluate_json(c, 3, ident()).unwrap(), 2) == Some(Json::Atom(2)),
{
    reveal_with_fuel(env::merge_value, 3);
    reveal_with_fuel(env::evaluate_json, 3);
    assert(env::evaluate_json(Json::Atom(1), 2, ident()) == Some(Json::Atom(1)));
    assert(env::evaluate_json(Json::Atom(2), 2, ident()) == Some(Json::Atom(2)));
    match c {
        Json::Container { array, entries } => {
            assert(entries.dom() =~= set![1int, 2int]);
            assert(forall|k: int| entries.dom().contains(k) ==> #[trigger] env::evaluate_json(entries[k], 2, ident()).is_some());
        }
        Json::Atom(_) => {}
    }
}

// module.env#4: a missing -env file fails the command; the same path holding a readable JSON object loads.
pub proof fn missing_versus_readable()
    ensures
        !env_command(fs0(), seq![3int], ident(), 3).ok,
        env_command(fs0(), seq![3int], ident(), 3).config == empty(false),
        env_command(fs0(), seq![1int], ident(), 3).ok,
        at(env_command(fs0(), seq![1int], ident(), 3).config, 1) == Some(Json::Atom(1)),
        // The same path once the file exists: presence is the deciding state.
        env_command(fs0().insert(3, Some(x_obj())), seq![3int], ident(), 3).ok,
{
    reveal_with_fuel(load_files, 2);
    merge_x_into_empty();
    eval_x();
    assert(inputs(fs0(), seq![3int], ident())[0] == file_input(fs0(), 3, ident()));
    assert(inputs(fs0(), seq![1int], ident())[0] == file_input(fs0(), 1, ident()));
    assert(inputs(fs0(), seq![3int], ident()).subrange(1, 1) =~= Seq::<FileInput>::empty());
    assert(inputs(fs0(), seq![1int], ident()).subrange(1, 1) =~= Seq::<FileInput>::empty());
    let fs3 = fs0().insert(3, Some(x_obj()));
    assert(inputs(fs3, seq![3int], ident())[0] == file_input(fs3, 3, ident()));
    assert(inputs(fs3, seq![3int], ident()).subrange(1, 1) =~= Seq::<FileInput>::empty());
}

// module.env#10: two readable files merge; a later missing, unparsable or array-root file clears everything
// and fails. The array-root failure happens with a present, parsable file: it is decided by the parsed
// content (EnvironmentConfig.cls:23-25), not by the read flag.
pub proof fn later_files()
    ensures
        env_command(fs0(), seq![1int, 2int], ident(), 3).ok,
        at(env_command(fs0(), seq![1int, 2int], ident(), 3).config, 1) == Some(Json::Atom(1)),
        at(env_command(fs0(), seq![1int, 2int], ident(), 3).config, 2) == Some(Json::Atom(2)),
        !env_command(fs0(), seq![1int, 3int], ident(), 3).ok,
        env_command(fs0(), seq![1int, 3int], ident(), 3).config == empty(false),
        !env_command(fs0(), seq![1int, 5int], ident(), 3).ok,
        env_command(fs0(), seq![1int, 5int], ident(), 3).config == empty(false),
        parses(fs0(), 4),
        !env_command(fs0(), seq![1int, 4int], ident(), 3).ok,
        env_command(fs0(), seq![1int, 4int], ident(), 3).config == empty(false),
{
    reveal_with_fuel(load_files, 3);
    merge_x_into_empty();
    eval_x();
    merge_y_into_x();
    let c = env::merge_value(Some(x_obj()), y_obj(), 3).unwrap();
    eval_xy(c);
    let i12 = inputs(fs0(), seq![1int, 2int], ident());
    assert(i12[0] == file_input(fs0(), 1, ident()));
    assert(i12.subrange(1, 2) =~= seq![file_input(fs0(), 2, ident())]);
    assert(i12.subrange(1, 2).subrange(1, 1) =~= Seq::<FileInput>::empty());
    let i13 = inputs(fs0(), seq![1int, 3int], ident());
    assert(i13[0] == file_input(fs0(), 1, ident()));
    assert(i13.subrange(1, 2) =~= seq![file_input(fs0(), 3, ident())]);
    let i15 = inputs(fs0(), seq![1int, 5int], ident());
    assert(i15[0] == file_input(fs0(), 1, ident()));
    assert(i15.subrange(1, 2) =~= seq![file_input(fs0(), 5, ident())]);
    let i14 = inputs(fs0(), seq![1int, 4int], ident());
    assert(i14[0] == file_input(fs0(), 1, ident()));
    assert(i14.subrange(1, 2) =~= seq![file_input(fs0(), 4, ident())]);
}

} // verus!

fn main() {}
