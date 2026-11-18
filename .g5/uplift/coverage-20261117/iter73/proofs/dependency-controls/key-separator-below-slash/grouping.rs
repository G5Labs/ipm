// Iteration73: `generate` resource grouping, UnitTest entries, the module.xml writer's one-entry-per-name collapse
// and which directory wins it, `generate -export` file placement, the module.xml backup, and how `load` resolves an
// entry's directory. Successor of iteration69's grouping.rs; this file holds definitions and general theorems only,
// so a mutation of a rule is judged by the general theorems alone. Instances and native transcriptions are in
// instances.rs.
// Transcription at 571e04a1 of Storage/ModuleTemplate.cls::{ReadResources,ReadItems,GroupClasses,SharedPackage,
// AddPackages,ProcessResources,ExportOneResource,Expfilename,SaveFile}, Storage/Module.cls::CalculatedResourcesGet
// with Utils/CodeGeneration.cls::BuildRelationshipSorter and ResourceReference.cls::GetSortKey, and
// ResourceProcessor/Default/Document.cls::{%OnNew,OnPhase}.
// Names and paths are sequences of segment identifiers. An int stands for a non-empty string; two ints are equal
// exactly when their strings are. `seg_chars` gives the character codes of the strings used by the fixtures.
use vstd::prelude::*;

verus! {

// File extensions as ReadItems sees them; PKG is the extension AddPackages writes. The lower-case extension is also
// the name of the folder Expfilename exports into and Document.%OnNew defaults Directory to (cls 1, mac 2, inc 3).
pub spec const CLS: int = 1;
pub spec const MAC: int = 2;
pub spec const INC: int = 3;
pub spec const INT: int = 4;
pub spec const PKG: int = 5;
pub spec const TXT: int = 9;
// UnitTest entry fields AddPackages writes for the tests folder.
pub spec const UNITTEST: int = 11;
pub spec const PHASE_TEST: int = 12;

// One file found by ReadFolder under the scanned folder.
pub struct File {
    pub path: Seq<int>,   // directory segments below the scanned folder
    pub name: Seq<int>,   // segments of the item name the listing returned, without extension
    pub ext: int,
    pub listed: bool,     // $system.OBJ.Load(file,"-d",,.list,1) returned OK (provider input)
}

// What ReadItems scans: `base` is the scanned folder relative to the module folder, `tests` the tests folder
// relative to the module folder (None when the answer is empty or the listing is the tests folder itself).
pub struct Excl { pub base: Seq<int>, pub tests: Option<Seq<int>> }

// One <Resource> entry: Name = name joined by "." plus "." ext, and Directory (omitted when empty).
pub struct Entry { pub name: Seq<int>, pub ext: int, pub dir: Seq<int> }

// One <UnitTest Name="/<name joined by '/'>/" Package=... Phase=.../> entry.
pub struct TestEntry { pub name: Seq<int>, pub processor: int, pub package: Seq<int>, pub phase: int }

pub struct Manifest { pub resources: Seq<Entry>, pub tests: Seq<TestEntry> }

pub open spec fn is_prefix(p: Seq<int>, a: Seq<int>) -> bool { p.len() <= a.len() && a.subrange(0, p.len() as int) == p }

pub open spec fn supported(ext: int) -> bool { ext == CLS || ext == MAC || ext == INC || ext == INT }

// ReadItems excludeDir: the file's path from the module folder starts with the tests folder.
pub open spec fn under_tests(f: File, ex: Excl) -> bool {
    ex.tests.is_some() && is_prefix(ex.tests.unwrap(), ex.base + f.path)
}

// ReadItems keeps a file when its extension is supported, it is not under the tests folder and listing succeeds.
pub open spec fn kept(f: File, ex: Excl) -> bool {
    supported(f.ext) && !under_tests(f, ex) && f.listed && f.name.len() >= 1
}

// ReadItems Directory: the leading path segments left after removing one segment per package segment.
pub open spec fn pkg_len(f: File) -> int { f.name.len() - 1 }
pub open spec fn directory(f: File) -> Seq<int> {
    if f.path.len() > pkg_len(f) { f.path.subrange(0, f.path.len() - pkg_len(f)) } else { Seq::empty() }
}

// GroupClasses moves a class with a package (at least two name segments) into packages(dir, top-level).
pub open spec fn grouped(f: File) -> bool { f.ext == CLS && f.name.len() >= 2 }
pub open spec fn package(f: File) -> Seq<int> { f.name.subrange(0, f.name.len() - 1) }
pub open spec fn in_group(f: File, ex: Excl, d: Seq<int>, t: int) -> bool {
    kept(f, ex) && grouped(f) && directory(f) == d && f.name[0] == t
}

// SharedPackage: the longest common leading run of segments.
pub open spec fn shared(a: Seq<int>, b: Seq<int>) -> Seq<int>
    decreases a.len()
{
    if a.len() == 0 || b.len() == 0 || a[0] != b[0] { Seq::empty() }
    else { seq![a[0]] + shared(a.subrange(1, a.len() as int), b.subrange(1, b.len() as int)) }
}

// packages(dir, top): seeded with the first class's package, then narrowed by SharedPackage, in file order.
pub open spec fn group(s: Seq<File>, ex: Excl, d: Seq<int>, t: int) -> Option<Seq<int>>
    decreases s.len()
{
    if s.len() == 0 { None } else {
        let prev = group(s.subrange(0, s.len() - 1), ex, d, t);
        let f = s[s.len() - 1];
        if in_group(f, ex, d, t) {
            match prev { None => Some(package(f)), Some(p) => Some(shared(p, package(f))) }
        } else { prev }
    }
}

// Items left in `items` after GroupClasses: kept files that are not grouped classes.
pub open spec fn single(f: File, ex: Excl) -> bool { kept(f, ex) && !grouped(f) }
pub open spec fn single_entry(f: File) -> Entry { Entry { name: f.name, ext: f.ext, dir: directory(f) } }

pub open spec fn singles(s: Seq<File>, ex: Excl) -> Seq<Entry>
    decreases s.len()
{
    if s.len() == 0 { Seq::empty() } else {
        let rest = singles(s.subrange(0, s.len() - 1), ex);
        let f = s[s.len() - 1];
        if single(f, ex) { rest.push(single_entry(f)) } else { rest }
    }
}

// AddPackages: one entry per key (dir, top-level) in TemplateResources $order. Without a tests folder the entry is
// <package>.PKG with Directory; with one it is a UnitTest named by the tests folder plus the directory.
pub open spec fn pkg_entry(s: Seq<File>, ex: Excl, k: (Seq<int>, int)) -> Entry {
    Entry { name: group(s, ex, k.0, k.1).unwrap(), ext: PKG, dir: k.0 }
}
pub open spec fn pkg_entries(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>) -> Seq<Entry>
    decreases keys.len()
{
    if keys.len() == 0 { Seq::empty() }
    else { pkg_entries(s, ex, keys.subrange(0, keys.len() - 1)).push(pkg_entry(s, ex, keys[keys.len() - 1])) }
}
pub open spec fn test_entry(ts: Seq<File>, ex: Excl, k: (Seq<int>, int), tf: Seq<int>) -> TestEntry {
    TestEntry { name: tf + k.0, processor: UNITTEST, package: group(ts, ex, k.0, k.1).unwrap(), phase: PHASE_TEST }
}
pub open spec fn test_entries(ts: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, tf: Seq<int>) -> Seq<TestEntry>
    decreases keys.len()
{
    if keys.len() == 0 { Seq::empty() }
    else { test_entries(ts, ex, keys.subrange(0, keys.len() - 1), tf).push(test_entry(ts, ex, keys[keys.len() - 1], tf)) }
}
pub open spec fn add_packages(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, tests_folder: Option<Seq<int>>)
    -> (Seq<Entry>, Seq<TestEntry>)
{
    if tests_folder.is_none() { (pkg_entries(s, ex, keys), Seq::empty()) }
    else { (Seq::empty(), test_entries(s, ex, keys, tests_folder.unwrap())) }
}

// CalculatedResourcesGet keys each resource by GetSortKey (name and extension, not directory) and the later one in
// relationship order wins. UnitTest names start with "/", so their sort key holds the object reference and they are
// never collapsed; the model keeps them in a separate list.
pub open spec fn same_key(a: Entry, b: Entry) -> bool { a.name == b.name && a.ext == b.ext }
pub open spec fn drop_key(es: Seq<Entry>, e: Entry) -> Seq<Entry>
    decreases es.len()
{
    if es.len() == 0 { Seq::empty() } else {
        let rest = drop_key(es.subrange(0, es.len() - 1), e);
        if same_key(es[es.len() - 1], e) { rest } else { rest.push(es[es.len() - 1]) }
    }
}
pub open spec fn collapse(es: Seq<Entry>) -> Seq<Entry>
    decreases es.len()
{
    if es.len() == 0 { Seq::empty() } else {
        let last = es[es.len() - 1];
        drop_key(collapse(es.subrange(0, es.len() - 1)), last).push(last)
    }
}

// generate: a missing source folder throws from ReadItems before anything is added; no module.xml is written.
// Otherwise singles (file keys start with "/" and sort before "pkg:"), then packages, then UnitTest entries when a
// tests folder was answered and exists. The tests listing is grouped without exclusion; its leftovers are dropped.
pub open spec fn generate(src_exists: bool, src: Seq<int>, s: Seq<File>, keys: Seq<(Seq<int>, int)>,
    tests: Option<Seq<int>>, tests_exist: bool, ts: Seq<File>, tkeys: Seq<(Seq<int>, int)>) -> Option<Manifest>
{
    if !src_exists { None } else {
        let ex = Excl { base: src, tests };
        Some(Manifest {
            resources: collapse(singles(s, ex) + add_packages(s, ex, keys, None).0),
            tests: if tests.is_some() && tests_exist {
                add_packages(ts, Excl { base: tests.unwrap(), tests: None }, tkeys, tests).1
            } else { Seq::empty() },
        })
    }
}

// Expfilename: an exported item goes to <src>/<lower-case extension>/<name>.<ext>; only a class name's package
// becomes folders.
pub open spec fn exported(name: Seq<int>, ext: int) -> File {
    File { path: if ext == CLS { seq![ext] + name.subrange(0, name.len() - 1) } else { seq![ext] }, name, ext, listed: true }
}

// SaveFile: an existing module.xml is renamed to module_<timestamp>.xml before the new one is written.
pub struct Disk { pub module_xml: Option<int>, pub backups: Seq<int> }
pub open spec fn save_file(d: Disk, new: int) -> Disk {
    if d.module_xml.is_some() { Disk { module_xml: Some(new), backups: d.backups.push(d.module_xml.unwrap()) } }
    else { Disk { module_xml: Some(new), backups: d.backups } }
}

pub open spec fn has_entry(es: Seq<Entry>, e: Entry) -> bool
    decreases es.len()
{
    if es.len() == 0 { false } else { es[es.len() - 1] == e || has_entry(es.subrange(0, es.len() - 1), e) }
}
pub open spec fn has_name(es: Seq<Entry>, name: Seq<int>, ext: int) -> bool
    decreases es.len()
{
    if es.len() == 0 { false }
    else { (es[es.len() - 1].name == name && es[es.len() - 1].ext == ext) || has_name(es.subrange(0, es.len() - 1), name, ext) }
}
pub open spec fn count_name(es: Seq<Entry>, name: Seq<int>, ext: int) -> int
    decreases es.len()
{
    if es.len() == 0 { 0 } else {
        count_name(es.subrange(0, es.len() - 1), name, ext) + if es[es.len() - 1].name == name && es[es.len() - 1].ext == ext { 1int } else { 0int }
    }
}
pub open spec fn has_test(ts: Seq<TestEntry>, t: TestEntry) -> bool
    decreases ts.len()
{
    if ts.len() == 0 { false } else { ts[ts.len() - 1] == t || has_test(ts.subrange(0, ts.len() - 1), t) }
}

// `load` (Document processor): an entry without Directory uses the lower-case extension directory when that
// directory exists under the source root, else the source root; package segments become subdirectories.
pub open spec fn ext_dir(ext: int) -> Seq<int> { seq![ext] }
pub open spec fn entry_ext(e: Entry) -> int { if e.ext == PKG { CLS } else { e.ext } }
pub open spec fn reader_dir(e: Entry, ext_dir_exists: bool) -> Seq<int> {
    if e.dir.len() > 0 { e.dir } else if ext_dir_exists { ext_dir(if e.ext == PKG { CLS } else { e.ext }) } else { Seq::empty() }
}
// The reader finds a class or routine file of entry e when the file sits at reader_dir + package segments.
pub open spec fn reader_finds(f: File, e: Entry, ext_dir_exists: bool) -> bool {
    f.path == reader_dir(e, ext_dir_exists) + package(f)
}

// A source directory exists when some scanned file lies below it.
pub open spec fn dir_exists(files: Seq<File>, seg: int) -> bool
    decreases files.len()
{
    if files.len() == 0 { false } else {
        let f = files[files.len() - 1];
        (f.path.len() > 0 && f.path[0] == seg) || dir_exists(files.subrange(0, files.len() - 1), seg)
    }
}
// Entry e loads file f: a package entry loads the classes inside its package found under its directory;
// an individual entry loads the item with its own name and extension.
pub open spec fn provides(e: Entry, f: File, files: Seq<File>) -> bool {
    supported(f.ext) && f.listed && f.name.len() >= 1
    && (if e.ext == PKG { grouped(f) && is_prefix(e.name, package(f)) } else { e.name == f.name && e.ext == f.ext })
    && reader_finds(f, e, dir_exists(files, entry_ext(e)))
}
pub open spec fn provided(out: Seq<Entry>, f: File, files: Seq<File>) -> bool
    decreases out.len()
{
    if out.len() == 0 { false } else { provides(out[out.len() - 1], f, files) || provided(out.subrange(0, out.len() - 1), f, files) }
}
pub open spec fn resolves(e: Entry, fs: Seq<File>, files: Seq<File>) -> bool
    decreases fs.len()
{
    if fs.len() == 0 { false } else { provides(e, fs[fs.len() - 1], files) || resolves(e, fs.subrange(0, fs.len() - 1), files) }
}
// `load` succeeds only when every written entry resolves to a file; otherwise it fails and creates no module.
pub open spec fn load_ok(out: Seq<Entry>, files: Seq<File>) -> bool
    decreases out.len()
{
    if out.len() == 0 { true } else { resolves(out[out.len() - 1], files, files) && load_ok(out.subrange(0, out.len() - 1), files) }
}
pub open spec fn loads_class(out: Seq<Entry>, files: Seq<File>, f: File) -> bool { load_ok(out, files) && provided(out, f, files) }

// The AddPackages key list: every packaged class's (directory, top-level) key appears once and names a group.
pub open spec fn key_in(keys: Seq<(Seq<int>, int)>, k: (Seq<int>, int)) -> bool
    decreases keys.len()
{
    if keys.len() == 0 { false } else { keys[keys.len() - 1] == k || key_in(keys.subrange(0, keys.len() - 1), k) }
}
pub open spec fn files_keyed(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>) -> bool
    decreases s.len()
{
    if s.len() == 0 { true } else {
        let f = s[s.len() - 1];
        (!(kept(f, ex) && grouped(f)) || key_in(keys, (directory(f), f.name[0]))) && files_keyed(s.subrange(0, s.len() - 1), ex, keys)
    }
}
pub open spec fn keys_used(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>) -> bool
    decreases keys.len()
{
    if keys.len() == 0 { true } else {
        group(s, ex, keys[keys.len() - 1].0, keys[keys.len() - 1].1).is_some() && keys_used(s, ex, keys.subrange(0, keys.len() - 1))
    }
}
pub open spec fn keys_distinct(keys: Seq<(Seq<int>, int)>) -> bool
    decreases keys.len()
{
    if keys.len() == 0 { true } else {
        !key_in(keys.subrange(0, keys.len() - 1), keys[keys.len() - 1]) && keys_distinct(keys.subrange(0, keys.len() - 1))
    }
}
pub open spec fn keys_ok(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>) -> bool {
    files_keyed(s, ex, keys) && keys_used(s, ex, keys) && keys_distinct(keys)
}

// Key order. ProcessResources creates the resources in $order of the TemplateResources subscripts, and IRIS orders
// non-numeric local subscripts by character code, so the package keys "pkg:/<directory>:<package>" are visited in
// ascending order of the string below (the shared prefix "pkg:/" is left out). Directory segments are joined by "/"
// (47), the directory and package by ":" (58), package segments by "." (46).
pub open spec fn seg_chars(seg: int) -> Seq<int> {
    if seg == 1 { seq![99, 108, 115] }                // cls
    else if seg == 61 { seq![120] }                   // x
    else if seg == 62 { seq![121] }                   // y
    else if seg == 66 { seq![97] }                    // a
    else if seg == 67 { seq![98] }                    // b
    else if seg == 68 { seq![97, 98] }                // ab
    else if seg == 12 { seq![66] }                    // B
    else if seg == 14 { seq![68] }                    // D
    else if seg == 21 { seq![80] }                    // P
    else if 201 <= seg <= 205 { seq![73, 97, seg - 152] }        // Ia1..Ia5
    else if 301 <= seg <= 302 { seq![73, 100, seg - 252] }       // Id1, Id2
    else if seg == 401 { seq![73, 101, 49] }                     // Ie1
    else if seg == 502 { seq![73, 98, 50, 97] }                  // Ib2a
    else if seg == 503 { seq![73, 107, 50, 98] }                 // Ik2b
    else if 601 <= seg <= 602 { seq![73, 103, seg - 552] }       // Ig1, Ig2
    else if seg == 701 { seq![73, 109, 55] }                     // Im7
    else if seg == 801 { seq![73, 109, 56] }                     // Im8
    else if 901 <= seg <= 904 { seq![73, 120, seg - 804] }       // Ixa..Ixd
    else { seq![seg] }
}
pub open spec fn join(segs: Seq<int>, sep: int) -> Seq<int>
    decreases segs.len()
{
    if segs.len() == 0 { Seq::empty() }
    else if segs.len() == 1 { seg_chars(segs[0]) }
    else { join(segs.subrange(0, segs.len() - 1), sep) + seq![sep] + seg_chars(segs[segs.len() - 1]) }
}
pub open spec fn order_key(s: Seq<File>, ex: Excl, k: (Seq<int>, int)) -> Seq<int> {
    join(k.0, 47) + seq![45] + join(group(s, ex, k.0, k.1).unwrap(), 46)
}
// Character-code order of two strings: a proper prefix sorts first.
pub open spec fn lex_lt(a: Seq<int>, b: Seq<int>) -> bool
    decreases a.len()
{
    if b.len() == 0 { false }
    else if a.len() == 0 { true }
    else if a[0] != b[0] { a[0] < b[0] }
    else { lex_lt(a.subrange(1, a.len() as int), b.subrange(1, b.len() as int)) }
}
pub open spec fn keys_sorted(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>) -> bool
    decreases keys.len()
{
    if keys.len() < 2 { true } else {
        lex_lt(order_key(s, ex, keys[keys.len() - 2]), order_key(s, ex, keys[keys.len() - 1]))
        && keys_sorted(s, ex, keys.subrange(0, keys.len() - 1))
    }
}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

// A file whose trailing folders spell its package below a non-empty containing folder.
pub open spec fn layout_ok(f: File) -> bool {
    f.path.len() > pkg_len(f) && f.path.subrange(f.path.len() - pkg_len(f), f.path.len() as int) == package(f)
}
// No two directories produce the same group name.
pub open spec fn no_clash(s: Seq<File>, ex: Excl) -> bool {
    forall|i: int, j: int| 0 <= i < s.len() && 0 <= j < s.len() && #[trigger] kept(s[i], ex) && grouped(s[i])
        && #[trigger] kept(s[j], ex) && grouped(s[j]) && directory(s[i]) != directory(s[j])
        ==> group(s, ex, directory(s[i]), s[i].name[0]) != group(s, ex, directory(s[j]), s[j].name[0])
}
pub open spec fn out_of(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>) -> Seq<Entry> {
    collapse(singles(s, ex) + pkg_entries(s, ex, keys))
}

// ===================================================================================================
// General theorems over every file sequence.

pub proof fn lemma_push_prefix<A>(r: Seq<A>, e: A)
    ensures r.push(e).subrange(0, r.len() as int) == r, r.push(e)[r.len() as int] == e, r.push(e).len() == r.len() + 1,
{
    assert(r.push(e).subrange(0, r.len() as int) =~= r);
}

pub proof fn lemma_shared_prefix(a: Seq<int>, b: Seq<int>)
    ensures is_prefix(shared(a, b), a), is_prefix(shared(a, b), b),
        forall|q: Seq<int>| is_prefix(q, a) && is_prefix(q, b) ==> is_prefix(q, #[trigger] shared(a, b))
    decreases a.len()
{
    if a.len() == 0 || b.len() == 0 || a[0] != b[0] {
        assert(shared(a, b) =~= Seq::<int>::empty());
        assert(a.subrange(0, 0) =~= Seq::<int>::empty());
        assert(b.subrange(0, 0) =~= Seq::<int>::empty());
        assert forall|q: Seq<int>| is_prefix(q, a) && is_prefix(q, b) implies is_prefix(q, #[trigger] shared(a, b)) by {
            if q.len() > 0 {
                assert(a.subrange(0, q.len() as int)[0] == a[0]);
                assert(b.subrange(0, q.len() as int)[0] == b[0]);
                assert(q[0] == a[0] && q[0] == b[0]);
            }
            assert(q =~= Seq::<int>::empty());
            assert(shared(a, b).subrange(0, 0) =~= q);
        }
    } else {
        let a1 = a.subrange(1, a.len() as int);
        let b1 = b.subrange(1, b.len() as int);
        lemma_shared_prefix(a1, b1);
        let s = shared(a, b);
        let r = shared(a1, b1);
        assert(s == seq![a[0]] + r);
        assert(a.subrange(0, s.len() as int) =~= seq![a[0]] + a1.subrange(0, r.len() as int));
        assert(b.subrange(0, s.len() as int) =~= seq![b[0]] + b1.subrange(0, r.len() as int));
        assert forall|q: Seq<int>| is_prefix(q, a) && is_prefix(q, b) implies is_prefix(q, #[trigger] shared(a, b)) by {
            if q.len() == 0 {
                assert(s.subrange(0, 0) =~= q);
            } else {
                let q1 = q.subrange(1, q.len() as int);
                assert(q[0] == a.subrange(0, q.len() as int)[0]);
                assert(a1.subrange(0, q1.len() as int) =~= a.subrange(0, q.len() as int).subrange(1, q.len() as int));
                assert(b1.subrange(0, q1.len() as int) =~= b.subrange(0, q.len() as int).subrange(1, q.len() as int));
                assert(q1 =~= q.subrange(1, q.len() as int));
                assert(is_prefix(q1, a1) && is_prefix(q1, b1));
                assert(is_prefix(q1, r));
                assert(s.subrange(0, q.len() as int) =~= seq![a[0]] + r.subrange(0, q1.len() as int));
                assert(q =~= seq![q[0]] + q1);
            }
        }
    }
}

// ---- C2: the grouping key, the tests-folder exclusion and the extension filter, for every file sequence.

// A file outside the key (another directory or another top-level package) never changes a group.
pub proof fn group_key_separates(s: Seq<File>, ex: Excl, g: File, d: Seq<int>, t: int)
    requires !(directory(g) == d && g.name.len() >= 1 && g.name[0] == t),
    ensures group(s.push(g), ex, d, t) == group(s, ex, d, t),
{
    lemma_push_prefix(s, g);
}

// A kept class with a package joins the group of its own directory and top-level package: the group seeds with
// its package, or narrows to the package shared with it.
pub proof fn group_key_joins(s: Seq<File>, ex: Excl, g: File)
    requires kept(g, ex), grouped(g),
    ensures group(s.push(g), ex, directory(g), g.name[0]) == Some(match group(s, ex, directory(g), g.name[0]) {
        None => package(g), Some(p) => shared(p, package(g)) }),
{
    lemma_push_prefix(s, g);
}

// A file under the tests folder (its path from the module folder starts with the tests folder) is in no group and
// is no leftover, whatever the other files are.
pub proof fn tests_folder_files_never_written(s: Seq<File>, ex: Excl, g: File, d: Seq<int>, t: int)
    requires ex.tests.is_some(), is_prefix(ex.tests.unwrap(), ex.base + g.path),
    ensures group(s.push(g), ex, d, t) == group(s, ex, d, t), singles(s.push(g), ex) == singles(s, ex),
{
    lemma_push_prefix(s, g);
}

// A file whose extension is not cls, mac, inc or int is in no group and is no leftover.
pub proof fn unsupported_never_written(s: Seq<File>, ex: Excl, g: File, d: Seq<int>, t: int)
    requires g.ext != CLS, g.ext != MAC, g.ext != INC, g.ext != INT,
    ensures group(s.push(g), ex, d, t) == group(s, ex, d, t), singles(s.push(g), ex) == singles(s, ex),
{
    lemma_push_prefix(s, g);
}

// A file the list-only load rejects is in no group and is no leftover.
pub proof fn unlisted_never_written(s: Seq<File>, ex: Excl, g: File, d: Seq<int>, t: int)
    requires !g.listed,
    ensures group(s.push(g), ex, d, t) == group(s, ex, d, t), singles(s.push(g), ex) == singles(s, ex),
{
    lemma_push_prefix(s, g);
}

// The group package exists exactly when the group has a member, starts with the top-level segment, is a package
// of every member, and is the deepest such package. So the emitted name does not depend on file order.
pub open spec fn member_at(s: Seq<File>, ex: Excl, d: Seq<int>, t: int, i: int) -> bool {
    0 <= i < s.len() && in_group(s[i], ex, d, t)
}

pub proof fn group_characterized(s: Seq<File>, ex: Excl, d: Seq<int>, t: int)
    ensures
        group(s, ex, d, t).is_some() <==> exists|i: int| member_at(s, ex, d, t, i),
        group(s, ex, d, t).is_some() ==> {
            let p = group(s, ex, d, t).unwrap();
            &&& p.len() >= 1 && p[0] == t
            &&& forall|i: int| member_at(s, ex, d, t, i) ==> is_prefix(p, #[trigger] package(s[i]))
            &&& forall|q: Seq<int>| (exists|i: int| member_at(s, ex, d, t, i))
                    && (forall|i: int| member_at(s, ex, d, t, i) ==> is_prefix(q, #[trigger] package(s[i])))
                    ==> is_prefix(q, p)
        },
    decreases s.len()
{
    if s.len() > 0 {
        let s0 = s.subrange(0, s.len() - 1);
        let f = s[s.len() - 1];
        let n = s.len() - 1;
        group_characterized(s0, ex, d, t);
        assert forall|i: int| 0 <= i < n implies #[trigger] s0[i] == s[i] by {}
        assert forall|i: int| member_at(s0, ex, d, t, i) <==> (member_at(s, ex, d, t, i) && i < n) by {
            if 0 <= i < n { assert(s0[i] == s[i]); }
        }
        if in_group(f, ex, d, t) {
            assert(member_at(s, ex, d, t, n));
            let pf = package(f);
            assert(pf.len() >= 1 && pf[0] == t);
            assert(is_prefix(pf, pf)) by { assert(pf.subrange(0, pf.len() as int) =~= pf); }
            match group(s0, ex, d, t) {
                None => {
                    assert forall|i: int| member_at(s, ex, d, t, i) implies is_prefix(pf, #[trigger] package(s[i])) by {
                        if i < n { assert(member_at(s0, ex, d, t, i)); }
                    }
                    assert forall|q: Seq<int>| (exists|i: int| member_at(s, ex, d, t, i))
                        && (forall|i: int| member_at(s, ex, d, t, i) ==> is_prefix(q, #[trigger] package(s[i])))
                        implies is_prefix(q, pf) by { assert(member_at(s, ex, d, t, n)); assert(package(s[n]) == pf); }
                }
                Some(p0) => {
                    lemma_shared_prefix(p0, pf);
                    let p = shared(p0, pf);
                    assert(p0.len() >= 1 && p0[0] == t);
                    assert(p.len() >= 1 && p[0] == t) by {
                        let q = seq![t];
                        assert(is_prefix(q, p0)) by { assert(p0.subrange(0, 1) =~= q); }
                        assert(is_prefix(q, pf)) by { assert(pf.subrange(0, 1) =~= q); }
                        assert(is_prefix(q, p));
                        assert(p.subrange(0, 1)[0] == p[0]);
                    }
                    assert forall|i: int| member_at(s, ex, d, t, i) implies is_prefix(p, #[trigger] package(s[i])) by {
                        if i < n {
                            assert(member_at(s0, ex, d, t, i));
                            assert(is_prefix(p0, package(s0[i])));
                            let a = package(s[i]);
                            assert(a.subrange(0, p.len() as int) =~= a.subrange(0, p0.len() as int).subrange(0, p.len() as int));
                            assert(p0.subrange(0, p.len() as int) == p);
                        } else { assert(i == n); }
                    }
                    assert forall|q: Seq<int>| (exists|i: int| member_at(s, ex, d, t, i))
                        && (forall|i: int| member_at(s, ex, d, t, i) ==> is_prefix(q, #[trigger] package(s[i])))
                        implies is_prefix(q, p) by {
                        assert(member_at(s, ex, d, t, n));
                        assert(is_prefix(q, package(s[n])));
                        if exists|i: int| member_at(s0, ex, d, t, i) {
                            assert forall|i: int| member_at(s0, ex, d, t, i) implies is_prefix(q, #[trigger] package(s0[i])) by {
                                assert(member_at(s, ex, d, t, i));
                            }
                            assert(is_prefix(q, p0));
                        }
                    }
                }
            }
        } else {
            assert(!member_at(s, ex, d, t, n));
            assert forall|i: int| member_at(s, ex, d, t, i) implies member_at(s0, ex, d, t, i) by {}
            if group(s0, ex, d, t).is_some() {
                let p = group(s0, ex, d, t).unwrap();
                assert forall|i: int| member_at(s, ex, d, t, i) implies is_prefix(p, #[trigger] package(s[i])) by {
                    assert(member_at(s0, ex, d, t, i));
                }
                assert forall|q: Seq<int>| (exists|i: int| member_at(s, ex, d, t, i))
                    && (forall|i: int| member_at(s, ex, d, t, i) ==> is_prefix(q, #[trigger] package(s[i])))
                    implies is_prefix(q, p) by {
                    let w = choose|i: int| member_at(s, ex, d, t, i);
                    assert(member_at(s0, ex, d, t, w));
                    assert forall|i: int| member_at(s0, ex, d, t, i) implies is_prefix(q, #[trigger] package(s0[i])) by {
                        assert(member_at(s, ex, d, t, i));
                    }
                }
            }
        }
    }
}

pub proof fn class_in_own_group(s: Seq<File>, ex: Excl, i: int)
    requires 0 <= i < s.len(), kept(s[i], ex), grouped(s[i]),
    ensures
        group(s, ex, directory(s[i]), s[i].name[0]).is_some(),
        is_prefix(group(s, ex, directory(s[i]), s[i].name[0]).unwrap(), package(s[i])),
{
    group_characterized(s, ex, directory(s[i]), s[i].name[0]);
    assert(member_at(s, ex, directory(s[i]), s[i].name[0], i));
}

// Leftovers: a kept file that is not a packaged class is written once as an individual resource with its own
// name and extension; a packaged class never is; skipped files never are.
pub proof fn singles_exact(s: Seq<File>, ex: Excl, f: File)
    ensures has_name(singles(s, ex), f.name, f.ext) ==> exists|i: int| 0 <= i < s.len() && single(s[i], ex) && s[i].name == f.name && s[i].ext == f.ext,
        (exists|i: int| 0 <= i < s.len() && s[i] == f) && single(f, ex) ==> has_entry(singles(s, ex), single_entry(f)),
    decreases s.len()
{
    if s.len() > 0 {
        let s0 = s.subrange(0, s.len() - 1);
        singles_exact(s0, ex, f);
        let g = s[s.len() - 1];
        let n = s.len() - 1;
        let r = singles(s0, ex);
        let e = single_entry(g);
        if single(g, ex) {
            assert(singles(s, ex) == r.push(e));
            assert(r.push(e).subrange(0, r.len() as int) =~= r);
        } else {
            assert(singles(s, ex) == r);
        }
        if has_name(singles(s, ex), f.name, f.ext) {
            if single(g, ex) && g.name == f.name && g.ext == f.ext {
                assert(0 <= n < s.len() && single(s[n], ex) && s[n].name == f.name && s[n].ext == f.ext);
            } else {
                assert(has_name(r, f.name, f.ext));
                let w = choose|i: int| 0 <= i < s0.len() && single(s0[i], ex) && s0[i].name == f.name && s0[i].ext == f.ext;
                assert(s[w] == s0[w]);
            }
        }
        if (exists|i: int| 0 <= i < s.len() && s[i] == f) && single(f, ex) {
            if g != f {
                let w = choose|i: int| 0 <= i < s.len() && s[i] == f;
                assert(w < s.len() - 1);
                assert(s0[w] == f);
                assert(has_entry(r, single_entry(f)));
            }
        }
    }
}

// A file at `d + package` gets Directory `d`, and `load` finds it there.
pub proof fn package_layout_round_trip(f: File, d: Seq<int>)
    requires f.name.len() >= 1, d.len() > 0, f.path == d + package(f),
    ensures directory(f) == d, reader_finds(f, single_entry(f), true), reader_finds(f, single_entry(f), false),
{
    assert(package(f).len() == pkg_len(f));
    assert(f.path.subrange(0, f.path.len() - pkg_len(f)) =~= d);
}

pub proof fn layout_directory(f: File)
    requires layout_ok(f), f.name.len() >= 1,
    ensures directory(f).len() > 0, directory(f) + package(f) == f.path,
{
    assert(directory(f) + package(f) =~= f.path);
}

// ---- Sequence facts used by the writer and reader theorems.

pub proof fn lemma_has_entry_index(es: Seq<Entry>, e: Entry)
    ensures has_entry(es, e) <==> (exists|m: int| 0 <= m < es.len() && es[m] == e),
    decreases es.len()
{
    if es.len() > 0 {
        let es0 = es.subrange(0, es.len() - 1);
        lemma_has_entry_index(es0, e);
        if has_entry(es, e) {
            if es[es.len() - 1] != e {
                let m = choose|m: int| 0 <= m < es0.len() && es0[m] == e;
                assert(es[m] == e);
            }
        }
        if exists|m: int| 0 <= m < es.len() && es[m] == e {
            let m = choose|m: int| 0 <= m < es.len() && es[m] == e;
            if m < es.len() - 1 { assert(es0[m] == e); }
        }
    }
}

pub proof fn lemma_count_nonneg(es: Seq<Entry>, n: Seq<int>, x: int)
    ensures count_name(es, n, x) >= 0,
    decreases es.len()
{
    if es.len() > 0 { lemma_count_nonneg(es.subrange(0, es.len() - 1), n, x); }
}

pub proof fn lemma_has_entry_count(es: Seq<Entry>, e: Entry)
    requires has_entry(es, e),
    ensures has_name(es, e.name, e.ext), count_name(es, e.name, e.ext) >= 1,
    decreases es.len()
{
    if es[es.len() - 1] != e { lemma_has_entry_count(es.subrange(0, es.len() - 1), e); }
    lemma_count_nonneg(es.subrange(0, es.len() - 1), e.name, e.ext);
}

pub proof fn lemma_count_concat(a: Seq<Entry>, c: Seq<Entry>, n: Seq<int>, x: int)
    ensures count_name(a + c, n, x) == count_name(a, n, x) + count_name(c, n, x),
    decreases c.len()
{
    if c.len() == 0 {
        assert(a + c =~= a);
    } else {
        let c0 = c.subrange(0, c.len() - 1);
        assert((a + c).subrange(0, (a + c).len() - 1) =~= a + c0);
        assert((a + c)[(a + c).len() - 1] == c[c.len() - 1]);
        lemma_count_concat(a, c0, n, x);
    }
}

pub proof fn lemma_has_entry_concat(a: Seq<Entry>, c: Seq<Entry>, e: Entry)
    requires has_entry(c, e) || has_entry(a, e),
    ensures has_entry(a + c, e),
{
    lemma_has_entry_index(a, e);
    lemma_has_entry_index(c, e);
    lemma_has_entry_index(a + c, e);
    if has_entry(a, e) {
        let m = choose|m: int| 0 <= m < a.len() && a[m] == e;
        assert((a + c)[m] == e);
    } else {
        let m = choose|m: int| 0 <= m < c.len() && c[m] == e;
        assert((a + c)[a.len() + m] == e);
    }
}

pub proof fn lemma_drop_key_count(es: Seq<Entry>, e: Entry, n: Seq<int>, x: int)
    ensures count_name(drop_key(es, e), n, x) == if e.name == n && e.ext == x { 0 } else { count_name(es, n, x) },
    decreases es.len()
{
    if es.len() > 0 {
        let es0 = es.subrange(0, es.len() - 1);
        let l = es[es.len() - 1];
        lemma_drop_key_count(es0, e, n, x);
        let r = drop_key(es0, e);
        if !same_key(l, e) { lemma_push_prefix(r, l); }
    }
}

pub proof fn lemma_drop_key_member(es: Seq<Entry>, k: Entry, e: Entry)
    requires has_entry(drop_key(es, k), e),
    ensures has_entry(es, e), !same_key(e, k),
    decreases es.len()
{
    let es0 = es.subrange(0, es.len() - 1);
    let l = es[es.len() - 1];
    let r = drop_key(es0, k);
    if same_key(l, k) {
        lemma_drop_key_member(es0, k, e);
    } else {
        lemma_push_prefix(r, l);
        if l != e { lemma_drop_key_member(es0, k, e); }
    }
}

pub proof fn lemma_drop_key_keeps(es: Seq<Entry>, k: Entry, e: Entry)
    requires has_entry(es, e), !same_key(e, k),
    ensures has_entry(drop_key(es, k), e),
    decreases es.len()
{
    let es0 = es.subrange(0, es.len() - 1);
    let l = es[es.len() - 1];
    let r = drop_key(es0, k);
    if l == e {
        lemma_push_prefix(r, l);
    } else {
        lemma_drop_key_keeps(es0, k, e);
        if !same_key(l, k) {
            lemma_push_prefix(r, l);
            lemma_has_entry_index(r, e);
            lemma_has_entry_index(r.push(l), e);
            let m = choose|m: int| 0 <= m < r.len() && r[m] == e;
            assert(r.push(l)[m] == e);
        }
    }
}

// ---- The writer: one entry per (name, extension), and the kept one is the last of its key.

pub proof fn writer_one_entry_per_name(es: Seq<Entry>, n: Seq<int>, x: int)
    ensures count_name(collapse(es), n, x) == b(has_name(es, n, x)),
    decreases es.len()
{
    if es.len() > 0 {
        let es0 = es.subrange(0, es.len() - 1);
        let l = es[es.len() - 1];
        writer_one_entry_per_name(es0, n, x);
        let y = drop_key(collapse(es0), l);
        lemma_drop_key_count(collapse(es0), l, n, x);
        lemma_push_prefix(y, l);
    }
}

pub open spec fn last_of_key(es: Seq<Entry>, r: int) -> bool {
    0 <= r < es.len() && forall|q: int| r < q < es.len() ==> !same_key(#[trigger] es[q], es[r])
}

// A written entry is the last entry of its (name, extension) in relationship order.
pub proof fn collapse_kept_is_last(es: Seq<Entry>, e: Entry)
    requires has_entry(collapse(es), e),
    ensures exists|r: int| last_of_key(es, r) && es[r] == e,
    decreases es.len()
{
    let es0 = es.subrange(0, es.len() - 1);
    let l = es[es.len() - 1];
    let y = drop_key(collapse(es0), l);
    lemma_push_prefix(y, l);
    if e == l {
        assert(last_of_key(es, es.len() - 1));
    } else {
        assert(has_entry(y, e));
        lemma_drop_key_member(collapse(es0), l, e);
        collapse_kept_is_last(es0, e);
        let r = choose|r: int| last_of_key(es0, r) && es0[r] == e;
        assert(es[r] == e);
        assert forall|q: int| r < q < es.len() implies !same_key(#[trigger] es[q], es[r]) by {
            if q < es.len() - 1 { assert(es0[q] == es[q]); assert(!same_key(es0[q], es0[r])); }
        }
        assert(last_of_key(es, r));
    }
}

pub proof fn collapse_subset(es: Seq<Entry>, e: Entry)
    requires has_entry(collapse(es), e),
    ensures has_entry(es, e),
{
    collapse_kept_is_last(es, e);
    lemma_has_entry_index(es, e);
}

// An entry whose (name, extension) occurs once is written.
pub proof fn collapse_keeps_unique(es: Seq<Entry>, e: Entry)
    requires has_entry(es, e), count_name(es, e.name, e.ext) == 1,
    ensures has_entry(collapse(es), e),
    decreases es.len()
{
    let es0 = es.subrange(0, es.len() - 1);
    let l = es[es.len() - 1];
    let y = drop_key(collapse(es0), l);
    lemma_push_prefix(y, l);
    if l != e {
        lemma_has_entry_count(es0, e);
        assert(!same_key(l, e));
        collapse_keeps_unique(es0, e);
        lemma_drop_key_keeps(collapse(es0), l, e);
        lemma_has_entry_index(y, e);
        lemma_has_entry_index(y.push(l), e);
        let m = choose|m: int| 0 <= m < y.len() && y[m] == e;
        assert(y.push(l)[m] == e);
    }
}

// ---- Facts about the written lists.

pub proof fn lemma_pkg_entries_index(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>)
    ensures pkg_entries(s, ex, keys).len() == keys.len(),
        forall|m: int| 0 <= m < keys.len() ==> #[trigger] pkg_entries(s, ex, keys)[m] == pkg_entry(s, ex, keys[m]),
    decreases keys.len()
{
    if keys.len() > 0 {
        let k0 = keys.subrange(0, keys.len() - 1);
        lemma_pkg_entries_index(s, ex, k0);
        lemma_push_prefix(pkg_entries(s, ex, k0), pkg_entry(s, ex, keys[keys.len() - 1]));
        assert forall|m: int| 0 <= m < keys.len() implies #[trigger] pkg_entries(s, ex, keys)[m] == pkg_entry(s, ex, keys[m]) by {
            if m < keys.len() - 1 { assert(k0[m] == keys[m]); }
        }
    }
}

pub proof fn lemma_singles_member(s: Seq<File>, ex: Excl, e: Entry)
    requires has_entry(singles(s, ex), e),
    ensures exists|i: int| 0 <= i < s.len() && single(s[i], ex) && e == single_entry(s[i]),
    decreases s.len()
{
    let s0 = s.subrange(0, s.len() - 1);
    let g = s[s.len() - 1];
    let r = singles(s0, ex);
    if single(g, ex) {
        lemma_push_prefix(r, single_entry(g));
        if single_entry(g) != e {
            lemma_singles_member(s0, ex, e);
            let i = choose|i: int| 0 <= i < s0.len() && single(s0[i], ex) && e == single_entry(s0[i]);
            assert(s[i] == s0[i]);
        } else {
            assert(single(s[s.len() - 1], ex));
        }
    } else {
        lemma_singles_member(s0, ex, e);
        let i = choose|i: int| 0 <= i < s0.len() && single(s0[i], ex) && e == single_entry(s0[i]);
        assert(s[i] == s0[i]);
    }
}

pub proof fn lemma_singles_no_pkg(s: Seq<File>, ex: Excl, n: Seq<int>)
    ensures count_name(singles(s, ex), n, PKG) == 0,
    decreases s.len()
{
    if s.len() > 0 {
        let s0 = s.subrange(0, s.len() - 1);
        lemma_singles_no_pkg(s0, ex, n);
        let g = s[s.len() - 1];
        if single(g, ex) { lemma_push_prefix(singles(s0, ex), single_entry(g)); }
    }
}

pub proof fn lemma_key_index(keys: Seq<(Seq<int>, int)>, k: (Seq<int>, int))
    ensures key_in(keys, k) <==> exists|m: int| 0 <= m < keys.len() && keys[m] == k,
    decreases keys.len()
{
    if keys.len() > 0 {
        let k0 = keys.subrange(0, keys.len() - 1);
        lemma_key_index(k0, k);
        if key_in(keys, k) && keys[keys.len() - 1] != k {
            let m = choose|m: int| 0 <= m < k0.len() && k0[m] == k;
            assert(keys[m] == k);
        }
        if exists|m: int| 0 <= m < keys.len() && keys[m] == k {
            let m = choose|m: int| 0 <= m < keys.len() && keys[m] == k;
            if m < keys.len() - 1 { assert(k0[m] == k); }
        }
    }
}

pub proof fn lemma_keys_used(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, k: (Seq<int>, int))
    requires keys_used(s, ex, keys), key_in(keys, k),
    ensures group(s, ex, k.0, k.1).is_some(),
    decreases keys.len()
{
    if keys[keys.len() - 1] != k { lemma_keys_used(s, ex, keys.subrange(0, keys.len() - 1), k); }
}

pub proof fn lemma_file_keyed(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, i: int)
    requires files_keyed(s, ex, keys), 0 <= i < s.len(), kept(s[i], ex), grouped(s[i]),
    ensures key_in(keys, (directory(s[i]), s[i].name[0])),
    decreases s.len()
{
    if i < s.len() - 1 {
        let s0 = s.subrange(0, s.len() - 1);
        assert(s0[i] == s[i]);
        lemma_file_keyed(s0, ex, keys, i);
    }
}

pub proof fn lemma_key_written(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, k: (Seq<int>, int))
    requires key_in(keys, k),
    ensures has_entry(pkg_entries(s, ex, keys), pkg_entry(s, ex, k)),
{
    lemma_key_index(keys, k);
    lemma_pkg_entries_index(s, ex, keys);
    let m = choose|m: int| 0 <= m < keys.len() && keys[m] == k;
    assert(pkg_entries(s, ex, keys)[m] == pkg_entry(s, ex, k));
    lemma_has_entry_index(pkg_entries(s, ex, keys), pkg_entry(s, ex, k));
}

// Before the writer collapses names, every packaged class has a written package entry in its own directory whose
// name is a package of the class.
pub proof fn class_written_before_collapse(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, i: int)
    requires keys_ok(s, ex, keys), 0 <= i < s.len(), kept(s[i], ex), grouped(s[i]),
    ensures ({
        let e = pkg_entry(s, ex, (directory(s[i]), s[i].name[0]));
        &&& has_entry(singles(s, ex) + pkg_entries(s, ex, keys), e)
        &&& e.dir == directory(s[i]) && is_prefix(e.name, package(s[i]))
    }),
{
    class_in_own_group(s, ex, i);
    lemma_file_keyed(s, ex, keys, i);
    lemma_key_written(s, ex, keys, (directory(s[i]), s[i].name[0]));
    lemma_has_entry_concat(singles(s, ex), pkg_entries(s, ex, keys), pkg_entry(s, ex, (directory(s[i]), s[i].name[0])));
}

pub proof fn lemma_two_entries(es: Seq<Entry>, e1: Entry, e2: Entry)
    requires has_entry(es, e1), has_entry(es, e2), e1 != e2, same_key(e1, e2),
    ensures count_name(es, e1.name, e1.ext) >= 2,
    decreases es.len()
{
    let es0 = es.subrange(0, es.len() - 1);
    let l = es[es.len() - 1];
    if l == e1 { lemma_has_entry_count(es0, e2); }
    else if l == e2 { lemma_has_entry_count(es0, e1); }
    else { lemma_two_entries(es0, e1, e2); }
}

// As-built divergence (C:D72), general form: when two directories' groups get the same package name, the written
// module.xml keeps that name for at most one of the two directories.
pub proof fn same_name_two_directories_keeps_one(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, i: int, j: int)
    requires keys_ok(s, ex, keys), 0 <= i < s.len(), 0 <= j < s.len(),
        kept(s[i], ex), grouped(s[i]), kept(s[j], ex), grouped(s[j]),
        directory(s[i]) != directory(s[j]),
        group(s, ex, directory(s[i]), s[i].name[0]) == group(s, ex, directory(s[j]), s[j].name[0]),
    ensures ({
        let out = out_of(s, ex, keys);
        let ei = pkg_entry(s, ex, (directory(s[i]), s[i].name[0]));
        let ej = pkg_entry(s, ex, (directory(s[j]), s[j].name[0]));
        &&& count_name(out, ei.name, PKG) == 1
        &&& !(has_entry(out, ei) && has_entry(out, ej))
    }),
{
    let es = singles(s, ex) + pkg_entries(s, ex, keys);
    let out = collapse(es);
    let ei = pkg_entry(s, ex, (directory(s[i]), s[i].name[0]));
    let ej = pkg_entry(s, ex, (directory(s[j]), s[j].name[0]));
    class_written_before_collapse(s, ex, keys, i);
    lemma_has_entry_count(es, ei);
    writer_one_entry_per_name(es, ei.name, PKG);
    if has_entry(out, ei) && has_entry(out, ej) {
        lemma_two_entries(out, ei, ej);
    }
}

// ---- C3: which directory wins the collapse.

pub proof fn lex_irrefl(a: Seq<int>)
    ensures !lex_lt(a, a),
    decreases a.len()
{
    if a.len() > 0 { lex_irrefl(a.subrange(1, a.len() as int)); }
}

pub proof fn lex_asym(a: Seq<int>, c: Seq<int>)
    requires lex_lt(a, c),
    ensures !lex_lt(c, a),
    decreases a.len()
{
    if a.len() > 0 && c.len() > 0 && a[0] == c[0] {
        lex_asym(a.subrange(1, a.len() as int), c.subrange(1, c.len() as int));
    }
}

pub proof fn lex_trans(a: Seq<int>, c: Seq<int>, e: Seq<int>)
    requires lex_lt(a, c), lex_lt(c, e),
    ensures lex_lt(a, e),
    decreases a.len()
{
    if a.len() > 0 && c.len() > 0 && e.len() > 0 && a[0] == c[0] && c[0] == e[0] {
        lex_trans(a.subrange(1, a.len() as int), c.subrange(1, c.len() as int), e.subrange(1, e.len() as int));
    }
}

pub proof fn lemma_keys_sorted(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, p: int, q: int)
    requires keys_sorted(s, ex, keys), 0 <= p < q < keys.len(),
    ensures lex_lt(order_key(s, ex, keys[p]), order_key(s, ex, keys[q])),
    decreases keys.len()
{
    let k0 = keys.subrange(0, keys.len() - 1);
    if q < keys.len() - 1 {
        lemma_keys_sorted(s, ex, k0, p, q);
        assert(k0[p] == keys[p] && k0[q] == keys[q]);
    } else if p < keys.len() - 2 {
        lemma_keys_sorted(s, ex, k0, p, keys.len() - 2);
        assert(k0[p] == keys[p] && k0[keys.len() - 2] == keys[keys.len() - 2]);
        lex_trans(order_key(s, ex, keys[p]), order_key(s, ex, keys[keys.len() - 2]), order_key(s, ex, keys[q]));
    }
}

// Under the $order premise (keys_sorted), the directory whose key string sorts earlier loses: its package entry is
// not written when another directory's group has the same name.
pub proof fn collapse_earlier_key_loses(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, i: int, j: int)
    requires keys_ok(s, ex, keys), keys_sorted(s, ex, keys), 0 <= i < s.len(), 0 <= j < s.len(),
        kept(s[i], ex), grouped(s[i]), kept(s[j], ex), grouped(s[j]),
        directory(s[i]) != directory(s[j]),
        group(s, ex, directory(s[i]), s[i].name[0]) == group(s, ex, directory(s[j]), s[j].name[0]),
        lex_lt(order_key(s, ex, (directory(s[i]), s[i].name[0])), order_key(s, ex, (directory(s[j]), s[j].name[0]))),
    ensures !has_entry(out_of(s, ex, keys), pkg_entry(s, ex, (directory(s[i]), s[i].name[0]))),
{
    let ki = (directory(s[i]), s[i].name[0]);
    let kj = (directory(s[j]), s[j].name[0]);
    let ss = singles(s, ex);
    let ps = pkg_entries(s, ex, keys);
    let es = ss + ps;
    let ei = pkg_entry(s, ex, ki);
    let ej = pkg_entry(s, ex, kj);
    if has_entry(collapse(es), ei) {
        collapse_kept_is_last(es, ei);
        let r = choose|r: int| last_of_key(es, r) && es[r] == ei;
        lemma_pkg_entries_index(s, ex, keys);
        if r < ss.len() {
            assert(es[r] == ss[r]);
            lemma_has_entry_index(ss, ei);
            lemma_singles_member(s, ex, ei);
        }
        let m = r - ss.len();
        assert(es[r] == ps[m]);
        assert(ps[m] == pkg_entry(s, ex, keys[m]));
        // keys[m] is ki: same directory, and the same group name, which starts with the key's top level.
        class_in_own_group(s, ex, i);
        group_characterized(s, ex, ki.0, ki.1);
        lemma_key_index(keys, keys[m]);
        lemma_keys_used(s, ex, keys, keys[m]);
        group_characterized(s, ex, keys[m].0, keys[m].1);
        assert(keys[m] == ki);
        lemma_file_keyed(s, ex, keys, j);
        lemma_key_index(keys, kj);
        let q = choose|q: int| 0 <= q < keys.len() && keys[q] == kj;
        if q < m {
            lemma_keys_sorted(s, ex, keys, q, m);
            lex_asym(order_key(s, ex, ki), order_key(s, ex, kj));
        }
        assert(q != m);
        assert(es[ss.len() + q] == ps[q]);
        assert(same_key(es[ss.len() + q], es[r]));
    }
}

// ---- C2 positive: with every kept file in package layout, load succeeds; with no two directories producing the
// same group name, every kept packaged class is also written and found by load.

pub proof fn lemma_resolves(e: Entry, fs: Seq<File>, files: Seq<File>, m: int)
    requires 0 <= m < fs.len(), provides(e, fs[m], files),
    ensures resolves(e, fs, files),
    decreases fs.len()
{
    if m < fs.len() - 1 {
        let f0 = fs.subrange(0, fs.len() - 1);
        assert(f0[m] == fs[m]);
        lemma_resolves(e, f0, files, m);
    }
}

pub proof fn lemma_load_ok_all(out: Seq<Entry>, files: Seq<File>)
    requires forall|m: int| 0 <= m < out.len() ==> resolves(#[trigger] out[m], files, files),
    ensures load_ok(out, files),
    decreases out.len()
{
    if out.len() > 0 {
        let o0 = out.subrange(0, out.len() - 1);
        assert forall|m: int| 0 <= m < o0.len() implies resolves(#[trigger] o0[m], files, files) by { assert(o0[m] == out[m]); }
        lemma_load_ok_all(o0, files);
    }
}

pub proof fn lemma_load_ok_member(out: Seq<Entry>, files: Seq<File>, e: Entry)
    requires load_ok(out, files), has_entry(out, e),
    ensures resolves(e, files, files),
    decreases out.len()
{
    if out[out.len() - 1] != e { lemma_load_ok_member(out.subrange(0, out.len() - 1), files, e); }
}

pub proof fn lemma_resolves_witness(e: Entry, fs: Seq<File>, files: Seq<File>)
    requires resolves(e, fs, files),
    ensures exists|m: int| 0 <= m < fs.len() && provides(e, fs[m], files),
    decreases fs.len()
{
    if !provides(e, fs[fs.len() - 1], files) {
        let f0 = fs.subrange(0, fs.len() - 1);
        lemma_resolves_witness(e, f0, files);
        let m = choose|m: int| 0 <= m < f0.len() && provides(e, f0[m], files);
        assert(fs[m] == f0[m]);
    }
}

pub proof fn lemma_provided(out: Seq<Entry>, f: File, files: Seq<File>, e: Entry)
    requires has_entry(out, e), provides(e, f, files),
    ensures provided(out, f, files),
    decreases out.len()
{
    if out[out.len() - 1] != e { lemma_provided(out.subrange(0, out.len() - 1), f, files, e); }
}

pub open spec fn all_layout(s: Seq<File>, ex: Excl) -> bool {
    forall|i: int| 0 <= i < s.len() && #[trigger] kept(s[i], ex) ==> layout_ok(s[i])
}

// Every entry the generator writes, before the collapse, is found by the reader.
pub proof fn written_entry_resolves(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, e: Entry)
    requires keys_ok(s, ex, keys), all_layout(s, ex), has_entry(singles(s, ex) + pkg_entries(s, ex, keys), e),
    ensures resolves(e, s, s),
{
    let ss = singles(s, ex);
    let ps = pkg_entries(s, ex, keys);
    lemma_has_entry_index(ss + ps, e);
    let r = choose|r: int| 0 <= r < (ss + ps).len() && (ss + ps)[r] == e;
    if r < ss.len() {
        assert(ss[r] == e);
        lemma_has_entry_index(ss, e);
        lemma_singles_member(s, ex, e);
        let i = choose|i: int| 0 <= i < s.len() && single(s[i], ex) && e == single_entry(s[i]);
        assert(kept(s[i], ex));
        layout_directory(s[i]);
        assert(provides(e, s[i], s));
        lemma_resolves(e, s, s, i);
    } else {
        let m = r - ss.len();
        lemma_pkg_entries_index(s, ex, keys);
        assert(ps[m] == e);
        let k = keys[m];
        lemma_key_index(keys, k);
        lemma_keys_used(s, ex, keys, k);
        group_characterized(s, ex, k.0, k.1);
        let j = choose|j: int| member_at(s, ex, k.0, k.1, j);
        assert(is_prefix(group(s, ex, k.0, k.1).unwrap(), package(s[j])));
        assert(kept(s[j], ex));
        layout_directory(s[j]);
        assert(provides(e, s[j], s));
        lemma_resolves(e, s, s, j);
    }
}

// With every kept file in package layout, `load` of the generated module succeeds (whatever the name clashes).
pub proof fn layout_loads(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>)
    requires keys_ok(s, ex, keys), all_layout(s, ex),
    ensures load_ok(out_of(s, ex, keys), s),
{
    let es = singles(s, ex) + pkg_entries(s, ex, keys);
    let out = collapse(es);
    assert forall|m: int| 0 <= m < out.len() implies resolves(#[trigger] out[m], s, s) by {
        lemma_has_entry_index(out, out[m]);
        collapse_subset(es, out[m]);
        written_entry_resolves(s, ex, keys, out[m]);
    }
    lemma_load_ok_all(out, s);
}

pub proof fn lemma_count_keys(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, k0: (Seq<int>, int))
    requires keys_distinct(keys),
        forall|k: (Seq<int>, int)| key_in(keys, k) && k != k0 ==> #[trigger] group(s, ex, k.0, k.1) != group(s, ex, k0.0, k0.1),
        forall|k: (Seq<int>, int)| key_in(keys, k) ==> (#[trigger] group(s, ex, k.0, k.1)).is_some(),
        group(s, ex, k0.0, k0.1).is_some(),
    ensures count_name(pkg_entries(s, ex, keys), group(s, ex, k0.0, k0.1).unwrap(), PKG) == b(key_in(keys, k0)),
    decreases keys.len()
{
    if keys.len() > 0 {
        let kk = keys.subrange(0, keys.len() - 1);
        let l = keys[keys.len() - 1];
        assert forall|k: (Seq<int>, int)| key_in(kk, k) implies key_in(keys, k) by {}
        lemma_count_keys(s, ex, kk, k0);
        lemma_push_prefix(pkg_entries(s, ex, kk), pkg_entry(s, ex, l));
        if l != k0 {
            assert(key_in(keys, l));
            assert(group(s, ex, l.0, l.1) != group(s, ex, k0.0, k0.1));
            assert(group(s, ex, l.0, l.1).unwrap() != group(s, ex, k0.0, k0.1).unwrap());
        }
    }
}

// General positive theorem: when no two directories produce the same group name and every kept file is in package
// layout, every kept class with a package (supported, listed, outside the tests folder) is written in its own
// directory's package entry, and `load` succeeds and finds it.
pub proof fn class_loads_when_no_clash(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, i: int)
    requires keys_ok(s, ex, keys), all_layout(s, ex), no_clash(s, ex), 0 <= i < s.len(), kept(s[i], ex), grouped(s[i]),
    ensures has_entry(out_of(s, ex, keys), pkg_entry(s, ex, (directory(s[i]), s[i].name[0]))),
        loads_class(out_of(s, ex, keys), s, s[i]),
{
    let ki = (directory(s[i]), s[i].name[0]);
    let ss = singles(s, ex);
    let ps = pkg_entries(s, ex, keys);
    let es = ss + ps;
    let ei = pkg_entry(s, ex, ki);
    class_written_before_collapse(s, ex, keys, i);
    class_in_own_group(s, ex, i);
    lemma_file_keyed(s, ex, keys, i);
    assert forall|k: (Seq<int>, int)| key_in(keys, k) implies (#[trigger] group(s, ex, k.0, k.1)).is_some() by {
        lemma_keys_used(s, ex, keys, k);
    }
    assert forall|k: (Seq<int>, int)| key_in(keys, k) && k != ki implies #[trigger] group(s, ex, k.0, k.1) != group(s, ex, ki.0, ki.1) by {
        lemma_keys_used(s, ex, keys, k);
        group_characterized(s, ex, k.0, k.1);
        group_characterized(s, ex, ki.0, ki.1);
        let j = choose|j: int| member_at(s, ex, k.0, k.1, j);
        if k.0 != ki.0 {
            assert(kept(s[i], ex) && kept(s[j], ex));
            assert(directory(s[i]) != directory(s[j]));
        } else {
            assert(k.1 != ki.1);
        }
    }
    lemma_count_keys(s, ex, keys, ki);
    lemma_singles_no_pkg(s, ex, ei.name);
    lemma_count_concat(ss, ps, ei.name, PKG);
    collapse_keeps_unique(es, ei);
    layout_loads(s, ex, keys);
    layout_directory(s[i]);
    assert(provides(ei, s[i], s));
    lemma_provided(out_of(s, ex, keys), s[i], s, ei);
}

// ---- C1: UnitTest entries, decided by AddPackages' tests-folder branch.

pub proof fn lemma_test_written(ts: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, tf: Seq<int>, k: (Seq<int>, int))
    requires key_in(keys, k),
    ensures has_test(test_entries(ts, ex, keys, tf), test_entry(ts, ex, k, tf)),
    decreases keys.len()
{
    let k0 = keys.subrange(0, keys.len() - 1);
    lemma_push_prefix(test_entries(ts, ex, k0, tf), test_entry(ts, ex, keys[keys.len() - 1], tf));
    if keys[keys.len() - 1] != k { lemma_test_written(ts, ex, k0, tf, k); }
}

// Every kept class with a package in the tests listing gets a UnitTest entry named by the tests folder plus its
// directory, with processor UnitTest, Phase "test" and a package that contains the class; the tests listing adds no
// <Resource> entries. The source listing adds no UnitTest entries.
pub proof fn test_class_has_unittest_entry(ts: Seq<File>, tf: Seq<int>, tkeys: Seq<(Seq<int>, int)>, i: int)
    requires keys_ok(ts, Excl { base: tf, tests: None }, tkeys), 0 <= i < ts.len(),
        kept(ts[i], Excl { base: tf, tests: None }), grouped(ts[i]),
    ensures ({
        let ex = Excl { base: tf, tests: None };
        let w = add_packages(ts, ex, tkeys, Some(tf));
        let p = group(ts, ex, directory(ts[i]), ts[i].name[0]).unwrap();
        &&& has_test(w.1, TestEntry { name: tf + directory(ts[i]), processor: UNITTEST, package: p, phase: PHASE_TEST })
        &&& is_prefix(p, package(ts[i]))
        &&& w.0.len() == 0
    }),
{
    let ex = Excl { base: tf, tests: None };
    class_in_own_group(ts, ex, i);
    lemma_file_keyed(ts, ex, tkeys, i);
    lemma_test_written(ts, ex, tkeys, tf, (directory(ts[i]), ts[i].name[0]));
}

pub proof fn source_listing_adds_no_unittest(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>)
    ensures add_packages(s, ex, keys, None).1.len() == 0, add_packages(s, ex, keys, None).0 == pkg_entries(s, ex, keys),
{
}

// Files of the tests listing never become resources: with no source files, the written resources are empty
// whatever the tests folder holds.
pub proof fn tests_listing_never_resources(src: Seq<int>, tf: Seq<int>, ts: Seq<File>, tkeys: Seq<(Seq<int>, int)>)
    ensures generate(true, src, Seq::empty(), Seq::empty(), Some(tf), true, ts, tkeys).unwrap().resources.len() == 0,
{
    let ex = Excl { base: src, tests: Some(tf) };
    assert(singles(Seq::<File>::empty(), ex) + pkg_entries(Seq::<File>::empty(), ex, Seq::empty()) =~= Seq::<Entry>::empty());
}

// A missing source folder writes no module.xml.
pub proof fn missing_source_writes_nothing(src: Seq<int>, s: Seq<File>, keys: Seq<(Seq<int>, int)>,
    tests: Option<Seq<int>>, te: bool, ts: Seq<File>, tkeys: Seq<(Seq<int>, int)>)
    ensures generate(false, src, s, keys, tests, te, ts, tkeys).is_none(),
{
}

// ---- The reader rule and C:D73.

// An entry with a Directory is looked up there; without one, in the extension folder when it exists, else the root.
pub proof fn reader_directory_rule(e: Entry)
    ensures e.dir.len() > 0 ==> reader_dir(e, true) == e.dir && reader_dir(e, false) == e.dir,
        e.dir.len() == 0 ==> reader_dir(e, true) == seq![entry_ext(e)] && reader_dir(e, false) == Seq::<int>::empty(),
{
}

// A class without a package directly in the source folder gets no Directory; `load` cannot find it when a `cls`
// folder exists, and finds it otherwise.
pub proof fn root_class_beside_cls_folder(f: File)
    requires f.path.len() == 0, f.name.len() == 1, f.ext == CLS,
    ensures !reader_finds(f, single_entry(f), true), reader_finds(f, single_entry(f), false),
{
    assert(package(f) =~= Seq::<int>::empty());
    assert(Seq::<int>::empty() + package(f) =~= f.path);
}

// ---- C5: `generate -export` file placement and the round trip through `load`.

// An exported class is always in package layout; an exported routine or include file only when its name has no dot.
pub proof fn export_layout(name: Seq<int>, ext: int)
    requires name.len() >= 1, supported(ext),
    ensures layout_ok(exported(name, ext)) <==> (ext == CLS || name.len() == 1),
        ext == CLS ==> directory(exported(name, ext)) == seq![CLS],
        ext != CLS && name.len() == 1 ==> directory(exported(name, ext)) == seq![ext],
        ext != CLS && name.len() >= 2 ==> directory(exported(name, ext)) == Seq::<int>::empty(),
{
    let f = exported(name, ext);
    if ext == CLS {
        assert(f.path.subrange(f.path.len() - pkg_len(f), f.path.len() as int) =~= package(f));
        assert(f.path.subrange(0, f.path.len() - pkg_len(f)) =~= seq![CLS]);
    } else if name.len() == 1 {
        assert(f.path.subrange(1, 1) =~= package(f));
        assert(f.path.subrange(0, 1) =~= seq![ext]);
    }
}

pub proof fn lemma_dir_exists(files: Seq<File>, seg: int, m: int)
    requires 0 <= m < files.len(), files[m].path.len() > 0, files[m].path[0] == seg,
    ensures dir_exists(files, seg),
    decreases files.len()
{
    if m < files.len() - 1 {
        let f0 = files.subrange(0, files.len() - 1);
        assert(f0[m] == files[m]);
        lemma_dir_exists(f0, seg, m);
    }
}

pub open spec fn count_files(s: Seq<File>, n: Seq<int>, x: int) -> int
    decreases s.len()
{
    if s.len() == 0 { 0 } else {
        count_files(s.subrange(0, s.len() - 1), n, x) + if s[s.len() - 1].name == n && s[s.len() - 1].ext == x { 1int } else { 0int }
    }
}

pub proof fn lemma_count_singles_le(s: Seq<File>, ex: Excl, n: Seq<int>, x: int)
    ensures count_name(singles(s, ex), n, x) <= count_files(s, n, x),
    decreases s.len()
{
    if s.len() > 0 {
        let s0 = s.subrange(0, s.len() - 1);
        lemma_count_singles_le(s0, ex, n, x);
        let g = s[s.len() - 1];
        if single(g, ex) { lemma_push_prefix(singles(s0, ex), single_entry(g)); }
    }
}

pub proof fn lemma_count_files_unique(s: Seq<File>, n: Seq<int>, x: int)
    requires forall|j: int, k: int| 0 <= j < s.len() && 0 <= k < s.len() && (#[trigger] s[j]).name == n && s[j].ext == x
        && (#[trigger] s[k]).name == n && s[k].ext == x ==> j == k,
    ensures count_files(s, n, x) <= 1,
        count_files(s, n, x) == 0 ==> forall|j: int| 0 <= j < s.len() ==> !((#[trigger] s[j]).name == n && s[j].ext == x),
    decreases s.len()
{
    if s.len() > 0 {
        let s0 = s.subrange(0, s.len() - 1);
        assert forall|j: int, k: int| 0 <= j < s0.len() && 0 <= k < s0.len() && (#[trigger] s0[j]).name == n && s0[j].ext == x
            && (#[trigger] s0[k]).name == n && s0[k].ext == x implies j == k by { assert(s0[j] == s[j] && s0[k] == s[k]); }
        lemma_count_files_unique(s0, n, x);
        let g = s[s.len() - 1];
        if g.name == n && g.ext == x {
            lemma_count_files_nonneg(s0, n, x);
            if count_files(s0, n, x) != 0 {
                // a second file with that name in s0 contradicts uniqueness
                assert(count_files(s0, n, x) >= 1);
                lemma_count_files_witness(s0, n, x);
                let j = choose|j: int| 0 <= j < s0.len() && s0[j].name == n && s0[j].ext == x;
                assert(s[j] == s0[j]);
                assert(j == s.len() - 1);
            }
        } else {
            assert forall|j: int| 0 <= j < s.len() && count_files(s, n, x) == 0 implies !((#[trigger] s[j]).name == n && s[j].ext == x) by {
                if j < s.len() - 1 { assert(s0[j] == s[j]); }
            }
        }
    }
}

pub proof fn lemma_count_files_nonneg(s: Seq<File>, n: Seq<int>, x: int)
    ensures count_files(s, n, x) >= 0,
    decreases s.len()
{
    if s.len() > 0 { lemma_count_files_nonneg(s.subrange(0, s.len() - 1), n, x); }
}

pub proof fn lemma_count_files_witness(s: Seq<File>, n: Seq<int>, x: int)
    requires count_files(s, n, x) >= 1,
    ensures exists|j: int| 0 <= j < s.len() && s[j].name == n && s[j].ext == x,
    decreases s.len()
{
    let s0 = s.subrange(0, s.len() - 1);
    let g = s[s.len() - 1];
    if !(g.name == n && g.ext == x) {
        lemma_count_files_witness(s0, n, x);
        let j = choose|j: int| 0 <= j < s0.len() && s0[j].name == n && s0[j].ext == x;
        assert(s[j] == s0[j]);
    }
}

pub proof fn lemma_pkg_count_other_ext(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, n: Seq<int>, x: int)
    requires x != PKG,
    ensures count_name(pkg_entries(s, ex, keys), n, x) == 0,
    decreases keys.len()
{
    if keys.len() > 0 {
        let k0 = keys.subrange(0, keys.len() - 1);
        lemma_pkg_count_other_ext(s, ex, k0, n, x);
        lemma_push_prefix(pkg_entries(s, ex, k0), pkg_entry(s, ex, keys[keys.len() - 1]));
    }
}

// As-built divergence (C:D73 through `generate -export`): an exported routine or include file whose name has a dot
// is written flat into its extension folder, gets no Directory, and `load` of the generated module fails, for every
// source tree in which it is the only file with that name and extension.
pub proof fn export_dotted_item_fails_load(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>, i: int)
    requires keys_ok(s, ex, keys), 0 <= i < s.len(), kept(s[i], ex),
        s[i] == exported(s[i].name, s[i].ext), s[i].ext != CLS, s[i].name.len() >= 2,
        forall|j: int, k: int| 0 <= j < s.len() && 0 <= k < s.len() && (#[trigger] s[j]).name == s[i].name && s[j].ext == s[i].ext
            && (#[trigger] s[k]).name == s[i].name && s[k].ext == s[i].ext ==> j == k,
    ensures !load_ok(out_of(s, ex, keys), s),
{
    let f = s[i];
    let e = single_entry(f);
    let ss = singles(s, ex);
    let ps = pkg_entries(s, ex, keys);
    let es = ss + ps;
    assert(single(f, ex));
    singles_exact(s, ex, f);
    assert(has_entry(ss, e));
    lemma_has_entry_count(ss, e);
    lemma_count_singles_le(s, ex, f.name, f.ext);
    lemma_count_files_unique(s, f.name, f.ext);
    lemma_pkg_count_other_ext(s, ex, keys, f.name, f.ext);
    lemma_count_concat(ss, ps, f.name, f.ext);
    lemma_has_entry_concat(ss, ps, e);
    collapse_keeps_unique(es, e);
    export_layout(f.name, f.ext);
    if load_ok(out_of(s, ex, keys), s) {
        lemma_load_ok_member(out_of(s, ex, keys), s, e);
        lemma_resolves_witness(e, s, s);
        let m = choose|m: int| 0 <= m < s.len() && provides(e, s[m], s);
        assert(m == i);
        lemma_dir_exists(s, f.ext, i);
        assert(reader_dir(e, true) == seq![f.ext]);
        assert((seq![f.ext] + package(f)).len() >= 2);
    }
}

// An exported class or dotless routine or include file is in package layout, so a tree made only of such exports
// loads; with no group-name clash every exported class is also found.
pub proof fn export_round_trip_loads(s: Seq<File>, ex: Excl, keys: Seq<(Seq<int>, int)>)
    requires keys_ok(s, ex, keys),
        forall|j: int| 0 <= j < s.len() && #[trigger] kept(s[j], ex) ==> s[j] == exported(s[j].name, s[j].ext)
            && (s[j].ext == CLS || s[j].name.len() == 1),
    ensures load_ok(out_of(s, ex, keys), s),
{
    assert forall|j: int| 0 <= j < s.len() && #[trigger] kept(s[j], ex) implies layout_ok(s[j]) by {
        export_layout(s[j].name, s[j].ext);
    }
    layout_loads(s, ex, keys);
}

// ---- C6: an existing module.xml is kept as a backup before the new one is written.
pub proof fn save_keeps_previous(d: Disk, new: int)
    ensures save_file(d, new).module_xml == Some(new),
        d.module_xml.is_some() ==> save_file(d, new).backups == d.backups.push(d.module_xml.unwrap()),
        d.module_xml.is_none() ==> save_file(d, new).backups == d.backups,
{
}

} // verus!

fn main() {}
