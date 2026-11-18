// Iteration75: corrected successor of consolidated/iter6/workflows.rs::global_path for a global resource's
// export and read-back path.
//
// The inherited global_path maps a declared Directory to itself, so an explicitly empty Directory drops the
// segment. The product substitutes gbl/ for an empty Directory, so this successor reads that branch.
//
// Source roles at pin 571e04a1:
//   ResourceProcessor/Default/Global.cls::Directory (16-18)          InitialExpression "gbl/" (attribute absent)
//   ResourceProcessor/Default/Global.cls::OnItemRelativePath (98-115) [preload/] SourcesRoot[/] Directory
//       ("" -> gbl/, a trailing / added) translated-name.xml
//   ResourceProcessor/Default/Global.cls::OnResolveChildren (89-94)  export: RelativePath from OnItemRelativePath
//   ResourceProcessor/Default/Global.cls::OnPhase Reload (59-72)     read back: Root/OnItemRelativePath, OBJ.Load
// Paths are segment sequences, so the trailing-slash normalisation is not represented. Preload resources
// (the preload/ prefix, not loaded at Reload) are outside this model.
#[path = "../../consolidated/iter6/workflows.rs"]
mod w;

use vstd::prelude::*;

verus! {

pub open spec fn GBL() -> int { 10 }

// Global.cls:106-111 (with the InitialExpression for an absent attribute).
pub open spec fn directory_segment(directory: Option<Seq<int>>) -> Seq<int> {
    match directory {
        None => seq![GBL()],
        Some(d) => if d.len() == 0 { seq![GBL()] } else { d },
    }
}

// Global.cls:98-115, one path function for both exporter and importer.
pub open spec fn global_path(sources: Seq<int>, directory: Option<Seq<int>>, name: int) -> Seq<int> {
    sources + directory_segment(directory) + seq![name]
}

// Export (OnResolveChildren) writes the file at global_path; Reload reads Root/global_path.
pub open spec fn export_global(sources: Seq<int>, directory: Option<Seq<int>>, name: int, value: int) -> Map<Seq<int>, int> {
    Map::empty().insert(global_path(sources, directory, name), value)
}
pub open spec fn reload_global(files: Map<Seq<int>, int>, sources: Seq<int>, directory: Option<Seq<int>>, name: int, old: int) -> int {
    let p = global_path(sources, directory, name);
    if files.contains_key(p) { files[p] } else { old }
}

// module.global.roundtrip#1: absent, declared and explicitly empty Directory; the SourcesRoot prefix.
pub proof fn path_neighbours()
    ensures
        global_path(seq![5int], None, 7) == seq![5int, 10, 7],
        global_path(seq![5int], Some(seq![3int]), 7) == seq![5int, 3, 7],
        global_path(seq![5int], Some(seq![3int, 4]), 7) == seq![5int, 3, 4, 7],
        global_path(seq![5int], Some(Seq::<int>::empty()), 7) == seq![5int, 10, 7],
        global_path(Seq::<int>::empty(), None, 7) == seq![10int, 7],
{
    assert(global_path(seq![5int], None, 7) =~= seq![5int, 10, 7]);
    assert(global_path(seq![5int], Some(seq![3int]), 7) =~= seq![5int, 3, 7]);
    assert(global_path(seq![5int], Some(seq![3int, 4]), 7) =~= seq![5int, 3, 4, 7]);
    assert(global_path(seq![5int], Some(Seq::<int>::empty()), 7) =~= seq![5int, 10, 7]);
    assert(global_path(Seq::<int>::empty(), None, 7) =~= seq![10int, 7]);
}

// The exported file is read back under the same SourcesRoot and Directory, and not under another.
pub proof fn round_trip_neighbours()
    ensures
        reload_global(export_global(seq![5int], Some(seq![3int]), 7, 42), seq![5int], Some(seq![3int]), 7, 0) == 42,
        reload_global(export_global(seq![5int], Some(seq![3int]), 7, 42), seq![5int], None, 7, 0) == 0,
        reload_global(export_global(seq![5int], Some(Seq::<int>::empty()), 7, 42), seq![5int], None, 7, 0) == 42,
        reload_global(export_global(seq![5int], None, 7, 42), Seq::<int>::empty(), None, 7, 0) == 0,
{
    path_neighbours();
    assert(seq![5int, 3, 7] != seq![5int, 10, 7]) by { assert(seq![5int, 3, 7][1] != seq![5int, 10, 7][1]); }
    assert(seq![5int, 10, 7] != seq![10int, 7]) by { assert(seq![5int, 10, 7].len() != seq![10int, 7].len()); }
}

// Where the inherited definition is faithful (absent or non-empty Directory) the successor agrees with it;
// they differ only for an explicitly empty Directory.
pub proof fn agrees_with_inherited(sources: Seq<int>, directory: Option<Seq<int>>, name: int)
    requires directory.is_none() || directory.unwrap().len() > 0
    ensures global_path(sources, directory, name) == w::global_path(sources, directory, name)
{}

pub proof fn inherited_drops_empty_directory()
    ensures w::global_path(seq![5int], Some(Seq::<int>::empty()), 7) != global_path(seq![5int], Some(Seq::<int>::empty()), 7)
{
    path_neighbours();
    assert(w::global_path(seq![5int], Some(Seq::<int>::empty()), 7) =~= seq![5int, 7]);
}

} // verus!

fn main() {}
