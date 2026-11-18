// Iteration69: `generate` resource grouping, emitted resource names, leftovers, the module.xml writer's
// one-entry-per-name collapse, and how `load` resolves an entry's directory.
// Transcription of Storage/ModuleTemplate.cls::{ReadResources,ReadItems,GroupClasses,SharedPackage,AddPackages,
// ProcessResources}, Storage/Module.cls::CalculatedResourcesGet with ResourceReference.cls::GetSortKey, and
// ResourceProcessor/Default/Document.cls::{OnBeforePhase directory default, OnPhase directory search} at 571e04a1.
// Names and directories are sequences of segment identifiers (ints stand for distinct non-empty strings).
use vstd::prelude::*;

verus! {

// File extensions as ReadItems sees them; PKG is the extension AddPackages writes.
pub spec const CLS: int = 1;
pub spec const MAC: int = 2;
pub spec const INC: int = 3;
pub spec const INT: int = 4;
pub spec const PKG: int = 5;
pub spec const TXT: int = 9;

// One file found by ReadFolder under the scanned folder.
pub struct File {
    pub path: Seq<int>,   // directory segments below the scanned folder
    pub name: Seq<int>,   // segments of the item name the listing returned, without extension
    pub ext: int,
    pub listed: bool,     // $system.OBJ.Load(file,"-d",,.list,1) returned OK (provider input)
    pub in_tests: bool,   // the absolute path starts with the tests directory (ReadItems excludeDir)
}

// One <Resource> entry: Name = name joined by "." plus "." ext, and Directory (omitted when empty).
pub struct Entry { pub name: Seq<int>, pub ext: int, pub dir: Seq<int> }

// One <UnitTest Name="/<tests>/<dir>/" Package=... Phase="test"/> entry.
pub struct TestEntry { pub dir: Seq<int>, pub package: Seq<int> }

pub struct Manifest { pub resources: Seq<Entry>, pub tests: Seq<TestEntry> }

pub open spec fn supported(ext: int) -> bool { ext == CLS || ext == MAC || ext == INC || ext == INT }

// ReadItems keeps a file when its extension is supported, it is not under the tests directory and listing succeeds.
pub open spec fn kept(f: File, exclude_tests: bool) -> bool {
    supported(f.ext) && !(exclude_tests && f.in_tests) && f.listed && f.name.len() >= 1
}

// ReadItems Directory: the leading path segments left after removing one segment per package segment.
pub open spec fn pkg_len(f: File) -> int { f.name.len() - 1 }
pub open spec fn directory(f: File) -> Seq<int> {
    if f.path.len() > pkg_len(f) { f.path.subrange(0, f.path.len() - pkg_len(f)) } else { Seq::empty() }
}

// GroupClasses moves a class with a package (at least two name segments) into packages(dir, top-level).
pub open spec fn grouped(f: File) -> bool { f.ext == CLS && f.name.len() >= 2 }
pub open spec fn package(f: File) -> Seq<int> { f.name.subrange(0, f.name.len() - 1) }
pub open spec fn in_group(f: File, ex: bool, d: Seq<int>, t: int) -> bool {
    kept(f, ex) && grouped(f) && f.name[0] == t
}

// SharedPackage: the longest common leading run of segments.
pub open spec fn shared(a: Seq<int>, b: Seq<int>) -> Seq<int>
    decreases a.len()
{
    if a.len() == 0 || b.len() == 0 || a[0] != b[0] { Seq::empty() }
    else { seq![a[0]] + shared(a.subrange(1, a.len() as int), b.subrange(1, b.len() as int)) }
}

// packages(dir, top): seeded with the first class's package, then narrowed by SharedPackage, in file order.
pub open spec fn group(s: Seq<File>, ex: bool, d: Seq<int>, t: int) -> Option<Seq<int>>
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
pub open spec fn single(f: File, ex: bool) -> bool { kept(f, ex) && !grouped(f) }

pub open spec fn singles(s: Seq<File>) -> Seq<Entry>
    decreases s.len()
{
    if s.len() == 0 { Seq::empty() } else {
        let rest = singles(s.subrange(0, s.len() - 1));
        let f = s[s.len() - 1];
        if single(f, true) { rest.push(Entry { name: f.name, ext: f.ext, dir: directory(f) }) } else { rest }
    }
}

// AddPackages: one entry per key (dir, top-level), named by the group package. `keys` is the key list in
// TemplateResources $order ("pkg:/<dir>:<package>").
pub open spec fn pkg_entry(s: Seq<File>, k: (Seq<int>, int)) -> Entry {
    Entry { name: group(s, true, k.0, k.1).unwrap(), ext: PKG, dir: k.0 }
}
pub open spec fn pkg_entries(s: Seq<File>, keys: Seq<(Seq<int>, int)>) -> Seq<Entry>
    decreases keys.len()
{
    if keys.len() == 0 { Seq::empty() }
    else { pkg_entries(s, keys.subrange(0, keys.len() - 1)).push(pkg_entry(s, keys[keys.len() - 1])) }
}

// AddPackages for the tests folder: the tests listing is grouped the same way, without exclusion.
// Its leftovers are never merged into the template.
pub open spec fn test_entries(ts: Seq<File>, tkeys: Seq<(Seq<int>, int)>) -> Seq<TestEntry>
    decreases tkeys.len()
{
    if tkeys.len() == 0 { Seq::empty() } else {
        let k = tkeys[tkeys.len() - 1];
        test_entries(ts, tkeys.subrange(0, tkeys.len() - 1)).push(TestEntry { dir: k.0, package: group(ts, false, k.0, k.1).unwrap() })
    }
}

// CalculatedResourcesGet keys each resource by GetSortKey (name and extension, not directory) and the later one
// in relationship order wins.
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
// Otherwise singles (file keys sort before "pkg:"), then packages, then tests (when the tests folder exists).
pub open spec fn generate(src_exists: bool, s: Seq<File>, keys: Seq<(Seq<int>, int)>,
    tests_exist: bool, ts: Seq<File>, tkeys: Seq<(Seq<int>, int)>) -> Option<Manifest>
{
    if !src_exists { None } else {
        Some(Manifest {
            resources: collapse(singles(s) + pkg_entries(s, keys)),
            tests: if tests_exist { test_entries(ts, tkeys) } else { Seq::empty() },
        })
    }
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
pub open spec fn entry_ext(e: Entry) -> int { if e.ext == PKG { CLS } else { e.ext } }
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
pub open spec fn files_keyed(s: Seq<File>, ex: bool, keys: Seq<(Seq<int>, int)>) -> bool
    decreases s.len()
{
    if s.len() == 0 { true } else {
        let f = s[s.len() - 1];
        (!(kept(f, ex) && grouped(f)) || key_in(keys, (directory(f), f.name[0]))) && files_keyed(s.subrange(0, s.len() - 1), ex, keys)
    }
}
pub open spec fn keys_used(s: Seq<File>, ex: bool, keys: Seq<(Seq<int>, int)>) -> bool
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
pub open spec fn keys_ok(s: Seq<File>, ex: bool, keys: Seq<(Seq<int>, int)>) -> bool {
    files_keyed(s, ex, keys) && keys_used(s, ex, keys) && keys_distinct(keys)
}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

// ---------------------------------------------------------------------------------------------------
// General properties over every file sequence.

pub open spec fn is_prefix(p: Seq<int>, a: Seq<int>) -> bool { p.len() <= a.len() && a.subrange(0, p.len() as int) == p }

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

// The group package exists exactly when the group has a member, starts with the top-level segment, is a package
// of every member, and is the deepest such package. So the emitted name does not depend on file order.
pub open spec fn member_at(s: Seq<File>, ex: bool, d: Seq<int>, t: int, i: int) -> bool {
    0 <= i < s.len() && in_group(s[i], ex, d, t)
}

pub proof fn group_characterized(s: Seq<File>, ex: bool, d: Seq<int>, t: int)
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

// Every class in a package lands in exactly one written group: the one keyed by its own directory and top-level
// package, whose name is a package of the class. Classes from a different directory or top-level package never
// narrow it.
pub proof fn class_in_own_group(s: Seq<File>, i: int)
    requires 0 <= i < s.len(), kept(s[i], true), grouped(s[i]),
    ensures
        group(s, true, directory(s[i]), s[i].name[0]).is_some(),
        is_prefix(group(s, true, directory(s[i]), s[i].name[0]).unwrap(), package(s[i])),
{
    group_characterized(s, true, directory(s[i]), s[i].name[0]);
    assert(member_at(s, true, directory(s[i]), s[i].name[0], i));
}

// Leftovers: a kept file that is not a packaged class is written once as an individual resource with its own
// name and extension; a packaged class never is; skipped files (unsupported, unlisted, under tests) never are.
pub proof fn singles_exact(s: Seq<File>, f: File)
    ensures has_name(singles(s), f.name, f.ext) ==> exists|i: int| 0 <= i < s.len() && single(s[i], true) && s[i].name == f.name && s[i].ext == f.ext,
        (exists|i: int| 0 <= i < s.len() && s[i] == f) && single(f, true) ==> has_entry(singles(s), Entry { name: f.name, ext: f.ext, dir: directory(f) }),
    decreases s.len()
{
    if s.len() > 0 {
        let s0 = s.subrange(0, s.len() - 1);
        singles_exact(s0, f);
        let g = s[s.len() - 1];
        let n = s.len() - 1;
        let r = singles(s0);
        let e = Entry { name: g.name, ext: g.ext, dir: directory(g) };
        if single(g, true) {
            assert(singles(s) == r.push(e));
            assert(r.push(e).subrange(0, r.len() as int) =~= r);
        } else {
            assert(singles(s) == r);
        }
        if has_name(singles(s), f.name, f.ext) {
            if single(g, true) && g.name == f.name && g.ext == f.ext {
                assert(0 <= n < s.len() && single(s[n], true) && s[n].name == f.name && s[n].ext == f.ext);
            } else {
                assert(has_name(r, f.name, f.ext));
                let w = choose|i: int| 0 <= i < s0.len() && single(s0[i], true) && s0[i].name == f.name && s0[i].ext == f.ext;
                assert(s[w] == s0[w]);
            }
        }
        if (exists|i: int| 0 <= i < s.len() && s[i] == f) && single(f, true) {
            if g != f {
                let w = choose|i: int| 0 <= i < s.len() && s[i] == f;
                assert(w < s.len() - 1);
                assert(s0[w] == f);
                assert(has_entry(r, Entry { name: f.name, ext: f.ext, dir: directory(f) }));
            }
        }
    }
}

// A file in package layout (path = d + package) gets Directory d, and `load` finds it there.
pub proof fn package_layout_round_trip(f: File, d: Seq<int>)
    requires f.name.len() >= 1, d.len() > 0, f.path == d + package(f),
    ensures directory(f) == d, reader_finds(f, Entry { name: f.name, ext: f.ext, dir: directory(f) }, true),
        reader_finds(f, Entry { name: f.name, ext: f.ext, dir: directory(f) }, false),
{
    assert(package(f).len() == pkg_len(f));
    assert(f.path.subrange(0, f.path.len() - pkg_len(f)) =~= d);
}

pub proof fn lemma_push_prefix<A>(r: Seq<A>, e: A)
    ensures r.push(e).subrange(0, r.len() as int) == r, r.push(e)[r.len() as int] == e, r.push(e).len() == r.len() + 1,
{
    assert(r.push(e).subrange(0, r.len() as int) =~= r);
}

pub proof fn lemma_has_entry_concat(a: Seq<Entry>, c: Seq<Entry>, e: Entry)
    requires has_entry(c, e) || has_entry(a, e),
    ensures has_entry(a + c, e),
    decreases c.len()
{
    if c.len() == 0 {
        assert(a + c =~= a);
    } else {
        let c0 = c.subrange(0, c.len() - 1);
        assert((a + c).subrange(0, (a + c).len() - 1) =~= a + c0);
        assert((a + c)[(a + c).len() - 1] == c[c.len() - 1]);
        if c[c.len() - 1] != e { lemma_has_entry_concat(a, c0, e); }
    }
}

pub proof fn lemma_has_entry_count(es: Seq<Entry>, e: Entry)
    requires has_entry(es, e),
    ensures has_name(es, e.name, e.ext), count_name(es, e.name, e.ext) >= 1,
    decreases es.len()
{
    if es[es.len() - 1] != e { lemma_has_entry_count(es.subrange(0, es.len() - 1), e); }
    lemma_count_nonneg(es.subrange(0, es.len() - 1), e.name, e.ext);
}

pub proof fn lemma_count_nonneg(es: Seq<Entry>, n: Seq<int>, x: int)
    ensures count_name(es, n, x) >= 0,
    decreases es.len()
{
    if es.len() > 0 { lemma_count_nonneg(es.subrange(0, es.len() - 1), n, x); }
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

// The module.xml writer keeps exactly one entry for each written (name, extension).
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

pub proof fn lemma_key_written(s: Seq<File>, keys: Seq<(Seq<int>, int)>, k: (Seq<int>, int))
    requires key_in(keys, k),
    ensures has_entry(pkg_entries(s, keys), pkg_entry(s, k)),
    decreases keys.len()
{
    let k0 = keys.subrange(0, keys.len() - 1);
    if keys[keys.len() - 1] != k {
        lemma_key_written(s, k0, k);
        lemma_push_prefix(pkg_entries(s, k0), pkg_entry(s, keys[keys.len() - 1]));
    }
}

pub proof fn lemma_file_keyed(s: Seq<File>, ex: bool, keys: Seq<(Seq<int>, int)>, i: int)
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

// Before the writer collapses names, every packaged class has a written package entry in its own directory whose
// name is a package of the class.
pub proof fn class_written_before_collapse(s: Seq<File>, keys: Seq<(Seq<int>, int)>, i: int)
    requires keys_ok(s, true, keys), 0 <= i < s.len(), kept(s[i], true), grouped(s[i]),
    ensures ({
        let e = pkg_entry(s, (directory(s[i]), s[i].name[0]));
        &&& has_entry(singles(s) + pkg_entries(s, keys), e)
        &&& e.dir == directory(s[i]) && is_prefix(e.name, package(s[i]))
    }),
{
    class_in_own_group(s, i);
    lemma_file_keyed(s, true, keys, i);
    lemma_key_written(s, keys, (directory(s[i]), s[i].name[0]));
    lemma_has_entry_concat(singles(s), pkg_entries(s, keys), pkg_entry(s, (directory(s[i]), s[i].name[0])));
}

// As-built divergence, general form: when two directories' groups get the same package name, the written
// module.xml keeps that name for at most one of the two directories.
pub proof fn same_name_two_directories_keeps_one(s: Seq<File>, keys: Seq<(Seq<int>, int)>, i: int, j: int)
    requires keys_ok(s, true, keys), 0 <= i < s.len(), 0 <= j < s.len(),
        kept(s[i], true), grouped(s[i]), kept(s[j], true), grouped(s[j]),
        directory(s[i]) != directory(s[j]),
        group(s, true, directory(s[i]), s[i].name[0]) == group(s, true, directory(s[j]), s[j].name[0]),
    ensures ({
        let out = collapse(singles(s) + pkg_entries(s, keys));
        let ei = pkg_entry(s, (directory(s[i]), s[i].name[0]));
        let ej = pkg_entry(s, (directory(s[j]), s[j].name[0]));
        &&& count_name(out, ei.name, PKG) == 1
        &&& !(has_entry(out, ei) && has_entry(out, ej))
    }),
{
    let es = singles(s) + pkg_entries(s, keys);
    let out = collapse(es);
    let ei = pkg_entry(s, (directory(s[i]), s[i].name[0]));
    let ej = pkg_entry(s, (directory(s[j]), s[j].name[0]));
    class_written_before_collapse(s, keys, i);
    lemma_has_entry_count(es, ei);
    writer_one_entry_per_name(es, ei.name, PKG);
    if has_entry(out, ei) && has_entry(out, ej) {
        lemma_two_entries(out, ei, ej);
    }
}

// ---------------------------------------------------------------------------------------------------
// Neighbouring source trees. Segment identifiers: top-level packages 101..; second-level 2..8;
// directories: cls 1 (= CLS), mac 2 (= MAC), inc 3 (= INC), x 61, y 62, flat 63.
pub open spec fn cls(path: Seq<int>, name: Seq<int>) -> File { File { path, name, ext: CLS, listed: true, in_tests: false } }

// N1: A.B.C + A.B.D in one directory -> A.B.PKG, not A.PKG.
// N2: A.X + E.Y -> A.PKG and E.PKG (separate top-level packages, same directory).
// N3: A.B.C alone -> A.B.PKG (a group of one is still a package).
// N4: A.B.C + A.D.E in one directory -> A.PKG.
pub open spec fn n1() -> Seq<File> { seq![cls(seq![1, 101, 2], seq![101, 2, 3]), cls(seq![1, 101, 2], seq![101, 2, 4])] }
pub open spec fn n2() -> Seq<File> { seq![cls(seq![1, 104], seq![104, 7]), cls(seq![1, 105], seq![105, 8])] }
pub open spec fn n3() -> Seq<File> { seq![cls(seq![1, 103, 2], seq![103, 2, 3])] }
pub open spec fn n4() -> Seq<File> { seq![cls(seq![1, 102, 2], seq![102, 2, 3]), cls(seq![1, 102, 4], seq![102, 4, 5])] }
// N5: the same package in directories x and y: two package entries with one name; the writer keeps y's.
pub open spec fn n5() -> Seq<File> { seq![cls(seq![61, 111, 2], seq![111, 2, 3]), cls(seq![62, 111, 2], seq![111, 2, 5])] }
pub open spec fn n5_keys() -> Seq<(Seq<int>, int)> { seq![(seq![61], 111), (seq![62], 111)] }
// N5b: one top-level package with different second levels in x and y: two entries, nothing lost.
pub open spec fn n5b() -> Seq<File> { seq![cls(seq![61, 112, 2], seq![112, 2, 3]), cls(seq![62, 112, 4], seq![112, 4, 5])] }
pub open spec fn n5b_keys() -> Seq<(Seq<int>, int)> { seq![(seq![61], 112), (seq![62], 112)] }
// N6: a class without a package directly in the source root, beside a cls directory.
pub open spec fn root_cls() -> File { cls(Seq::empty(), seq![121]) }
// N6b: a flat-named class file A.B.C.cls in directory flat.
pub open spec fn flat_cls() -> File { cls(seq![63], seq![122, 2, 3]) }
// N8: routine and include files, an unsupported file, an unlisted file and a class under the tests folder.
pub open spec fn rtn() -> File { File { path: seq![2], name: seq![131], ext: MAC, listed: true, in_tests: false } }
pub open spec fn inc() -> File { File { path: seq![3], name: seq![132], ext: INC, listed: true, in_tests: false } }
pub open spec fn txt() -> File { File { path: Seq::empty(), name: seq![133], ext: TXT, listed: true, in_tests: false } }
// An unlisted file keeps the name its path suggests, so only `listed` keeps it out.
pub open spec fn unlisted() -> File { File { path: seq![1, 134], name: seq![134, 6], ext: CLS, listed: false, in_tests: false } }
pub open spec fn under_tests() -> File { File { path: seq![71, 135], name: seq![135, 2], ext: CLS, listed: true, in_tests: true } }

pub open spec fn pkg(name: Seq<int>, dir: Seq<int>) -> Entry { Entry { name, ext: PKG, dir } }

pub proof fn grouping_names()
    ensures
        group(n1(), true, seq![1], 101) == Some(seq![101int, 2]),
        group(n2(), true, seq![1], 104) == Some(seq![104int]) && group(n2(), true, seq![1], 105) == Some(seq![105int]),
        group(n3(), true, seq![1], 103) == Some(seq![103int, 2]),
        group(n4(), true, seq![1], 102) == Some(seq![102int]),
        group(n5b(), true, seq![61], 112) == Some(seq![112int, 2]) && group(n5b(), true, seq![62], 112) == Some(seq![112int, 4]),
{
    assert(group(n1(), true, seq![1], 101) == Some(seq![101int, 2])) by (compute_only);
    assert(group(n2(), true, seq![1], 104) == Some(seq![104int])) by (compute_only);
    assert(group(n2(), true, seq![1], 105) == Some(seq![105int])) by (compute_only);
    assert(group(n3(), true, seq![1], 103) == Some(seq![103int, 2])) by (compute_only);
    assert(group(n4(), true, seq![1], 102) == Some(seq![102int])) by (compute_only);
    assert(group(n5b(), true, seq![61], 112) == Some(seq![112int, 2])) by (compute_only);
    assert(group(n5b(), true, seq![62], 112) == Some(seq![112int, 4])) by (compute_only);
}

// As-built divergence: with the same package in two directories the written module.xml keeps one entry, so the
// class in x is in no written resource and `load` never sees it.
pub proof fn same_package_two_directories_loses_class()
    ensures
        pkg_entries(n5(), n5_keys()) == seq![pkg(seq![111, 2], seq![61]), pkg(seq![111, 2], seq![62])],
        collapse(singles(n5()) + pkg_entries(n5(), n5_keys())) == seq![pkg(seq![111, 2], seq![62])],
        !has_entry(collapse(singles(n5()) + pkg_entries(n5(), n5_keys())), pkg(seq![111, 2], directory(n5()[0]))),
        count_name(collapse(singles(n5b()) + pkg_entries(n5b(), n5b_keys())), seq![112, 2], PKG) == 1,
        count_name(collapse(singles(n5b()) + pkg_entries(n5b(), n5b_keys())), seq![112, 4], PKG) == 1,
{
    assert(pkg_entries(n5(), n5_keys()) == seq![pkg(seq![111, 2], seq![61]), pkg(seq![111, 2], seq![62])]) by (compute_only);
    assert(collapse(singles(n5()) + pkg_entries(n5(), n5_keys())) == seq![pkg(seq![111, 2], seq![62])]) by (compute_only);
    assert(!has_entry(collapse(singles(n5()) + pkg_entries(n5(), n5_keys())), pkg(seq![111, 2], seq![61]))) by (compute_only);
    assert(directory(n5()[0]) == seq![61int]) by (compute_only);
    assert(count_name(collapse(singles(n5b()) + pkg_entries(n5b(), n5b_keys())), seq![112, 2], PKG) == 1) by (compute_only);
    assert(count_name(collapse(singles(n5b()) + pkg_entries(n5b(), n5b_keys())), seq![112, 4], PKG) == 1) by (compute_only);
}

pub proof fn leftovers_and_skips()
    ensures
        singles(seq![root_cls(), rtn(), inc(), txt(), unlisted(), under_tests(), flat_cls()])
            == seq![Entry { name: seq![121], ext: CLS, dir: Seq::empty() }, Entry { name: seq![131], ext: MAC, dir: seq![2] },
                    Entry { name: seq![132], ext: INC, dir: seq![3] }],
        group(seq![under_tests()], true, seq![71], 135).is_none(),
        group(seq![unlisted()], true, seq![1], 134).is_none(),
        directory(flat_cls()) == Seq::<int>::empty(),
        group(seq![flat_cls()], true, Seq::empty(), 122) == Some(seq![122int, 2]),
{
    assert(singles(seq![root_cls(), rtn(), inc(), txt(), unlisted(), under_tests(), flat_cls()])
            == seq![Entry { name: seq![121], ext: CLS, dir: Seq::empty() }, Entry { name: seq![131], ext: MAC, dir: seq![2] },
                    Entry { name: seq![132], ext: INC, dir: seq![3] }]) by (compute_only);
    assert(group(seq![under_tests()], true, seq![71], 135).is_none()) by (compute_only);
    assert(group(seq![unlisted()], true, seq![1], 134).is_none()) by (compute_only);
    assert(directory(flat_cls()) == Seq::<int>::empty()) by (compute_only);
    assert(group(seq![flat_cls()], true, Seq::empty(), 122) == Some(seq![122int, 2])) by (compute_only);
}

// As-built divergence: `load` cannot find a root class without a package when the source root also has a cls
// directory, nor a flat-named class file; a package-layout class is found.
pub proof fn load_resolution()
    ensures
        !reader_finds(root_cls(), Entry { name: seq![121], ext: CLS, dir: directory(root_cls()) }, true),
        reader_finds(root_cls(), Entry { name: seq![121], ext: CLS, dir: directory(root_cls()) }, false),
        !reader_finds(flat_cls(), pkg(seq![122, 2], directory(flat_cls())), true),
        !reader_finds(flat_cls(), pkg(seq![122, 2], directory(flat_cls())), false),
        reader_finds(n1()[0], pkg(seq![101, 2], directory(n1()[0])), true),
{
    assert(!reader_finds(root_cls(), Entry { name: seq![121], ext: CLS, dir: directory(root_cls()) }, true)) by (compute_only);
    assert(reader_finds(root_cls(), Entry { name: seq![121], ext: CLS, dir: directory(root_cls()) }, false)) by (compute_only);
    assert(!reader_finds(flat_cls(), pkg(seq![122, 2], directory(flat_cls())), true)) by (compute_only);
    assert(!reader_finds(flat_cls(), pkg(seq![122, 2], directory(flat_cls())), false)) by (compute_only);
    assert(reader_finds(n1()[0], pkg(seq![101, 2], directory(n1()[0])), true)) by (compute_only);
}

// Tests: grouped like application classes and written as UnitTest entries; tests-folder leftovers are written
// nowhere. A missing source folder writes nothing; a missing tests folder writes no UnitTest.
pub open spec fn tfiles() -> Seq<File> {
    seq![File { path: Seq::empty(), name: seq![141], ext: MAC, listed: true, in_tests: false },
         File { path: seq![142], name: seq![142, 2], ext: CLS, listed: true, in_tests: false },
         File { path: seq![142, 3], name: seq![142, 3, 4], ext: CLS, listed: true, in_tests: false },
         File { path: Seq::empty(), name: seq![143], ext: CLS, listed: true, in_tests: false }]
}
pub open spec fn tkeys() -> Seq<(Seq<int>, int)> { seq![(Seq::<int>::empty(), 142)] }

pub proof fn tests_and_status()
    ensures
        test_entries(tfiles(), tkeys()) == seq![TestEntry { dir: Seq::empty(), package: seq![142] }],
        generate(true, Seq::empty(), Seq::empty(), true, tfiles(), tkeys()).unwrap().resources.len() == 0,
        generate(true, Seq::empty(), Seq::empty(), true, tfiles(), tkeys()).unwrap().tests.len() == 1,
        generate(true, Seq::empty(), Seq::empty(), false, tfiles(), tkeys()).unwrap().tests.len() == 0,
        generate(false, n1(), seq![(seq![1], 101)], true, tfiles(), tkeys()).is_none(),
        generate(true, n1(), seq![(seq![1], 101)], true, tfiles(), tkeys()).unwrap().resources == seq![pkg(seq![101, 2], seq![1])],
{
    assert(test_entries(tfiles(), tkeys()) == seq![TestEntry { dir: Seq::empty(), package: seq![142] }]) by (compute_only);
    assert(generate(true, n1(), seq![(seq![1], 101)], true, tfiles(), tkeys()).unwrap().resources == seq![pkg(seq![101, 2], seq![1])]) by (compute_only);
    assert(generate(true, Seq::empty(), Seq::empty(), true, tfiles(), tkeys()).unwrap().resources.len() == 0) by (compute_only);
}

// ---------------------------------------------------------------------------------------------------
// Literal transcriptions of the native fixture trees (native/), in ReadFolder $order of the full path.
// Directory segments: cls 1, mac 2, inc 3, x 61, y 62, flat 63, tests 64 (inside src). Name segments:
// B 12, C 13, D 14, E 15, X 16, Y 17, U 18, V 19, W 20, P 21, Q 22, A 23, Good 24, Bad 25, Junk 26.
pub open spec fn f(path: Seq<int>, name: Seq<int>, ext: int, listed: bool, in_tests: bool) -> File { File { path, name, ext, listed, in_tests } }

// m1: Ia1..Ia5 = 201..205, Ir69 206, Ii69 207, notes 208, Ij1 209; tests: It1 211, Ih69 212, Inopkg69 213.
pub open spec fn m1() -> Seq<File> {
    seq![f(seq![1, 201, 12], seq![201, 12, 13], CLS, true, false), f(seq![1, 201, 12], seq![201, 12, 14], CLS, true, false),
         f(seq![1, 202, 12], seq![202, 12, 13], CLS, true, false), f(seq![1, 202, 14], seq![202, 14, 15], CLS, true, false),
         f(seq![1, 203, 12], seq![203, 12, 13], CLS, true, false), f(seq![1, 204], seq![204, 16], CLS, true, false),
         f(seq![1, 205], seq![205, 17], CLS, true, false), f(seq![1, 209], seq![209, 26], CLS, false, false),
         f(seq![3], seq![207], INC, true, false), f(seq![2], seq![206], MAC, true, false),
         f(Seq::empty(), seq![208], TXT, true, false)]
}
pub open spec fn m1_keys() -> Seq<(Seq<int>, int)> { seq![(seq![1], 201), (seq![1], 202), (seq![1], 203), (seq![1], 204), (seq![1], 205)] }
pub open spec fn m1_tests() -> Seq<File> {
    seq![f(Seq::empty(), seq![212], MAC, true, false), f(Seq::empty(), seq![213], CLS, true, false),
         f(seq![211], seq![211, 18], CLS, true, false), f(seq![211, 19], seq![211, 19, 20], CLS, true, false)]
}
pub open spec fn m1_tkeys() -> Seq<(Seq<int>, int)> { seq![(Seq::<int>::empty(), 211)] }
pub open spec fn m1_out() -> Manifest { generate(true, m1(), m1_keys(), true, m1_tests(), m1_tkeys()).unwrap() }

// m3: Id1 301, Id2 302.
pub open spec fn m3() -> Seq<File> {
    seq![f(seq![61, 301, 12], seq![301, 12, 13], CLS, true, false), f(seq![61, 302, 12], seq![302, 12, 13], CLS, true, false),
         f(seq![62, 301, 12], seq![301, 12, 15], CLS, true, false), f(seq![62, 302, 14], seq![302, 14, 15], CLS, true, false)]
}
pub open spec fn m3_keys() -> Seq<(Seq<int>, int)> { seq![(seq![61], 301), (seq![61], 302), (seq![62], 301), (seq![62], 302)] }
pub open spec fn m3_out() -> Manifest { generate(true, m3(), m3_keys(), false, Seq::empty(), Seq::empty()).unwrap() }

// m4: Ie1 401, Ie1T 402, Ihelp69 403, Inopkg69b 404; tests folder src/tests.
pub open spec fn m4() -> Seq<File> {
    seq![f(seq![1, 401], seq![401, 23], CLS, true, false), f(seq![64, 402], seq![402, 18], CLS, true, true),
         f(seq![64], seq![403], MAC, true, true), f(seq![64], seq![404], CLS, true, true)]
}
pub open spec fn m4_keys() -> Seq<(Seq<int>, int)> { seq![(seq![1], 401)] }
pub open spec fn m4_tests() -> Seq<File> {
    seq![f(seq![402], seq![402, 18], CLS, true, false), f(Seq::empty(), seq![403], MAC, true, false), f(Seq::empty(), seq![404], CLS, true, false)]
}
pub open spec fn m4_tkeys() -> Seq<(Seq<int>, int)> { seq![(Seq::<int>::empty(), 402)] }
pub open spec fn m4_out() -> Manifest { generate(true, m4(), m4_keys(), true, m4_tests(), m4_tkeys()).unwrap() }

// m2a: Ir2a 501, Ib2a 502; m2e: Ir2e 504; m2b: Ik2b 503.
pub open spec fn m2a() -> Seq<File> { seq![f(Seq::empty(), seq![501], CLS, true, false), f(seq![1, 502, 21], seq![502, 21, 22], CLS, true, false)] }
pub open spec fn m2a_keys() -> Seq<(Seq<int>, int)> { seq![(seq![1], 502)] }
pub open spec fn m2a_out() -> Manifest { generate(true, m2a(), m2a_keys(), false, Seq::empty(), Seq::empty()).unwrap() }
pub open spec fn m2e() -> Seq<File> { seq![f(Seq::empty(), seq![504], CLS, true, false)] }
pub open spec fn m2e_out() -> Manifest { generate(true, m2e(), Seq::empty(), false, Seq::empty(), Seq::empty()).unwrap() }
pub open spec fn m2b() -> Seq<File> { seq![f(seq![63], seq![503, 21, 22], CLS, true, false)] }
pub open spec fn m2b_keys() -> Seq<(Seq<int>, int)> { seq![(Seq::<int>::empty(), 503)] }
pub open spec fn m2b_out() -> Manifest { generate(true, m2b(), m2b_keys(), false, Seq::empty(), Seq::empty()).unwrap() }

// m5: Ig1 601, Ig2 602 (class header, broken body: listed), Ig3 603 (no header: not listed).
pub open spec fn m5() -> Seq<File> {
    seq![f(seq![1, 601], seq![601, 24], CLS, true, false), f(seq![1, 602], seq![602, 25], CLS, true, false),
         f(seq![1, 603], seq![603, 26], CLS, false, false)]
}
pub open spec fn m5_keys() -> Seq<(Seq<int>, int)> { seq![(seq![1], 601), (seq![1], 602)] }
pub open spec fn m5_out() -> Manifest { generate(true, m5(), m5_keys(), false, Seq::empty(), Seq::empty()).unwrap() }

// The key lists above are the product's: complete, used and distinct.
pub proof fn native_keys_ok()
    ensures keys_ok(m1(), true, m1_keys()), keys_ok(m1_tests(), false, m1_tkeys()), keys_ok(m3(), true, m3_keys()),
        keys_ok(m4(), true, m4_keys()), keys_ok(m4_tests(), false, m4_tkeys()), keys_ok(m2a(), true, m2a_keys()),
        keys_ok(m2e(), true, Seq::empty()), keys_ok(m2b(), true, m2b_keys()), keys_ok(m5(), true, m5_keys()),
{
    assert(keys_ok(m1(), true, m1_keys())) by (compute_only);
    assert(keys_ok(m1_tests(), false, m1_tkeys())) by (compute_only);
    assert(keys_ok(m3(), true, m3_keys())) by (compute_only);
    assert(keys_ok(m4(), true, m4_keys())) by (compute_only);
    assert(keys_ok(m4_tests(), false, m4_tkeys())) by (compute_only);
    assert(keys_ok(m2a(), true, m2a_keys())) by (compute_only);
    assert(keys_ok(m2e(), true, Seq::empty())) by (compute_only);
    assert(keys_ok(m2b(), true, m2b_keys())) by (compute_only);
    assert(keys_ok(m5(), true, m5_keys())) by (compute_only);
}

// The general divergence applies to m3: Id1.B.C (x) and Id1.B.E (y) share Id1.B.
pub proof fn m3_keeps_one_directory()
    ensures !(has_entry(m3_out().resources, pkg(seq![301, 12], seq![61])) && has_entry(m3_out().resources, pkg(seq![301, 12], seq![62]))),
{
    native_keys_ok();
    assert(directory(m3()[0]) == seq![61int] && directory(m3()[2]) == seq![62int]) by (compute_only);
    assert(group(m3(), true, seq![61], 301) == Some(seq![301int, 12]) && group(m3(), true, seq![62], 301) == Some(seq![301int, 12])) by (compute_only);
    assert(seq![61int][0] != seq![62int][0]);
    same_name_two_directories_keeps_one(m3(), m3_keys(), 0, 2);
}

} // verus!

fn main() {}
