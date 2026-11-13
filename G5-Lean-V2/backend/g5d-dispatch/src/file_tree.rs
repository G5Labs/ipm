//! The attached owner's live Files tree, listed in one call.
//!
//! The live tree enumerates the running checkout. Unlike the committed cut,
//! that walk sees everything on disk, including dependency and build output
//! such as `node_modules/` or `target/` that Git ignores. Those trees dwarf
//! the repository (a Rust `target/` or a `node_modules/` alone is tens of
//! thousands of entries) and would trip the tree's entry cap. The walk
//! therefore applies the checkout's own `.gitignore` files the way
//! `git status` does, so the live tree shows the committed cut's paths plus
//! the owner's untracked work and nothing else (`g5.documents.read.live-tree`).
//!
//! The walk is written over [`TreeSource`] so the guest's confined file
//! service and the API tests' in-memory dispatcher share one definition of
//! the live tree. Only `.gitignore` files inside the checkout are honored;
//! `.git/info/exclude` and global excludes are not reachable here.

use std::collections::{BTreeSet, VecDeque};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;

/// The rule file Git reads in every directory.
pub const GITIGNORE: &str = ".gitignore";

/// One direct child of a listed directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// One entry of the walked tree, repository relative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

/// A walk's result: the entries listed, or that the subtree is too large.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeWalk {
    pub entries: Vec<TreeEntry>,
    /// The subtree exceeds a [`WalkLimits`] bound; `entries` holds what was
    /// walked before the bound was crossed.
    pub truncated: bool,
}

/// How large one walk may grow. Both bounds exist because one reply carries
/// the whole listing: the entry cap bounds what a panel shows, the byte cap
/// keeps the serialized reply under the transport's reply limit even when
/// paths are long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalkLimits {
    /// The walk stops once more than this many entries are collected.
    pub max_entries: usize,
    /// The walk stops once the entries' estimated serialized size exceeds
    /// this many bytes (see [`entry_bytes`]).
    pub max_bytes: usize,
    /// The walk stops once more than this many directories are collected:
    /// a watch registers directories only, so files do not count against it.
    pub max_directories: usize,
}

impl WalkLimits {
    /// Bounds for a listing reply: entries and bytes, directories unbounded.
    pub const fn reply(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            max_entries,
            max_bytes,
            max_directories: usize::MAX,
        }
    }

    /// Bounds for a watch registration: directories only.
    pub const fn directories(max_directories: usize) -> Self {
        Self {
            max_entries: usize::MAX,
            max_bytes: usize::MAX,
            max_directories,
        }
    }
}

/// The serialized size one entry is budgeted at: its path as the JSON string
/// it becomes (quotes and control characters escape to more bytes than they
/// occupy) plus the envelope around it
/// (`{"path":…,"type":"directory","size":N},`, 57 bytes with a 20-digit
/// size), with headroom.
pub fn entry_bytes(path: &str) -> usize {
    let quoted = serde_json::to_string(path).map_or(path.len().saturating_mul(6), |s| s.len());
    quoted + 64
}

/// What a [`TreeSource`] found at a directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Listing {
    Entries(Vec<SourceEntry>),
    /// The directory does not exist (any more); the walk skips it.
    Missing,
    /// The directory holds more entries than the source lists; the walk
    /// stops and reports `truncated`, as past the entry cap.
    TooWide,
}

/// A checkout the walk can read: direct children of a directory and the
/// `.gitignore` file inside it.
pub trait TreeSource {
    type Error;

    /// Direct children of `directory` (repository relative; `""` is the
    /// root), or that it is missing or too wide to list.
    fn list(&self, directory: &str) -> Result<Listing, Self::Error>;

    /// The text of the `.gitignore` in `directory`; `None` when there is
    /// none or it cannot be read as text. The tree still lists, only less
    /// filtered.
    fn read_gitignore(&self, directory: &str) -> Option<String>;
}

/// Walk `source` from `path`, honoring the checkout's `.gitignore` files: an
/// entry the rules ignore is neither listed nor entered unless it is
/// `tracked`, as Git keeps tracked paths. Hidden product paths are skipped
/// as the Files tree hides them everywhere. `depth` counts directory levels
/// below `path`; negative means unlimited. The walk stops once it crosses
/// either bound of `limits` and reports `truncated`.
pub fn walk_tree<S: TreeSource>(
    source: &S,
    path: &str,
    depth: i32,
    tracked: &BTreeSet<String>,
    limits: WalkLimits,
) -> Result<TreeWalk, S::Error> {
    walk_tree_with(source, path, depth, tracked, limits, Hidden::Skip)
}

/// What the walk does with paths the Files tree hides (dot-directories
/// other than the durable `.g5` content).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hidden {
    /// Leave them out, as the Files tree does.
    Skip,
    /// List them: a watch covers writes the tree does not show.
    Keep,
}

/// [`walk_tree`] with a choice about hidden paths.
pub fn walk_tree_with<S: TreeSource>(
    source: &S,
    path: &str,
    depth: i32,
    tracked: &BTreeSet<String>,
    limits: WalkLimits,
    hidden: Hidden,
) -> Result<TreeWalk, S::Error> {
    let path = path.trim_matches('/');
    let mut rules = IgnoreRules::default();
    // The rules above the listed subtree govern it as well: the root's and
    // every ancestor's `.gitignore`, shallowest first. The subtree's own
    // file is read when its listing shows it.
    let ancestors =
        std::iter::once("").chain(path.match_indices('/').map(|(index, _)| &path[..index]));
    for ancestor in ancestors {
        if ancestor != path {
            if let Some(content) = source.read_gitignore(ancestor) {
                rules.add_gitignore(ancestor, &content);
            }
        }
    }
    let mut queue = VecDeque::from([(path.to_string(), 0_u32)]);
    let mut entries = Vec::new();
    let mut bytes = 0_usize;
    let mut directories = 0_usize;
    while let Some((directory, level)) = queue.pop_front() {
        let mut listed = match source.list(&directory)? {
            Listing::Entries(listed) => listed,
            Listing::Missing => continue,
            Listing::TooWide => {
                return Ok(TreeWalk {
                    entries,
                    truncated: true,
                })
            }
        };
        // Directory order is the filesystem's; the tree's is by name.
        listed.sort_by(|left, right| left.name.cmp(&right.name));
        // A directory's own `.gitignore` governs its entries.
        if listed
            .iter()
            .any(|entry| entry.name == GITIGNORE && !entry.is_dir)
        {
            if let Some(content) = source.read_gitignore(&directory) {
                rules.add_gitignore(&directory, &content);
            }
        }
        for entry in listed {
            let entry_path = join(&directory, &entry.name);
            if hidden == Hidden::Skip
                && !g5_contracts::g5_paths::is_files_tree_visible_path(&entry_path)
            {
                continue;
            }
            if rules.is_ignored(&entry_path, entry.is_dir) && !tracked.contains(&entry_path) {
                continue;
            }
            let is_dir = entry.is_dir;
            bytes = bytes.saturating_add(entry_bytes(&entry_path));
            entries.push(TreeEntry {
                path: entry_path.clone(),
                is_dir,
                size: entry.size,
            });
            if is_dir {
                directories += 1;
            }
            if entries.len() > limits.max_entries
                || bytes > limits.max_bytes
                || directories > limits.max_directories
            {
                return Ok(TreeWalk {
                    entries,
                    truncated: true,
                });
            }
            if is_dir && (depth < 0 || level.saturating_add(1) < depth as u32) {
                queue.push_back((entry_path, level.saturating_add(1)));
            }
        }
    }
    Ok(TreeWalk {
        entries,
        truncated: false,
    })
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

/// The `.gitignore` layers read so far, shallowest first.
#[derive(Default)]
pub struct IgnoreRules {
    layers: Vec<(String, Gitignore)>,
}

impl IgnoreRules {
    /// Add the rules of the `.gitignore` file in `directory` (repository
    /// relative; `""` is the root). Lines that are not valid patterns are
    /// skipped and the rest of the file still applies, as in Git.
    pub fn add_gitignore(&mut self, directory: &str, content: &str) {
        let directory = directory.trim_matches('/');
        let root = if directory.is_empty() { "." } else { directory };
        let mut builder = GitignoreBuilder::new(root);
        // Git reads past a UTF-8 byte order mark; the first rule must too.
        let content = content.strip_prefix('\u{feff}').unwrap_or(content);
        for line in content.lines() {
            let _ = builder.add_line(None, line);
        }
        match builder.build() {
            Ok(gitignore) => self.layers.push((directory.to_string(), gitignore)),
            Err(error) => {
                tracing::debug!(directory, %error, "live file tree: .gitignore is unusable")
            }
        }
    }

    /// Whether Git would ignore `path` (repository relative) by rule. A
    /// directory excluded by its own verdict excludes everything below it;
    /// nothing re-includes under it. Otherwise the path's own verdict
    /// decides, across every `.gitignore` above it, deepest first, so a
    /// nested file can re-include what a parent excluded. Git never ignores
    /// a tracked path; the caller keeps those.
    pub fn is_ignored(&self, path: &str, is_dir: bool) -> bool {
        let path = path.trim_matches('/');
        if path
            .match_indices('/')
            .any(|(index, _)| self.matched(&path[..index], true) == Some(true))
        {
            return true;
        }
        self.matched(path, is_dir) == Some(true)
    }

    /// The rules' verdict on `path` alone: `Some(true)` excluded,
    /// `Some(false)` re-included, `None` unnamed.
    fn matched(&self, path: &str, is_dir: bool) -> Option<bool> {
        for (directory, gitignore) in self.layers.iter().rev() {
            if !directory.is_empty() && !path.starts_with(&format!("{directory}/")) {
                continue;
            }
            match gitignore.matched(path, is_dir) {
                Match::None => continue,
                Match::Ignore(_) => return Some(true),
                Match::Whitelist(_) => return Some(false),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::convert::Infallible;

    #[test]
    fn root_rules_match_at_any_depth_unless_anchored() {
        let mut rules = IgnoreRules::default();
        rules.add_gitignore("", "target/\nnode_modules/\n*.log\n!keep.log\n/dist\n");

        assert!(rules.is_ignored("target", true));
        assert!(rules.is_ignored("frontend/node_modules", true));
        assert!(rules.is_ignored("logs/debug.log", false));
        assert!(rules.is_ignored("dist", true));

        assert!(!rules.is_ignored("keep.log", false));
        assert!(
            !rules.is_ignored("frontend/dist", true),
            "/dist is anchored to the root"
        );
        assert!(
            !rules.is_ignored("target.rs", false),
            "target/ is a directory rule"
        );
        assert!(!rules.is_ignored("src/main.rs", false));
    }

    #[test]
    fn nested_rules_apply_below_their_directory_only() {
        let mut rules = IgnoreRules::default();
        rules.add_gitignore("", "*.generated\n");
        rules.add_gitignore("frontend", "dist/\n!api.generated\n");

        assert!(rules.is_ignored("frontend/dist", true));
        assert!(
            !rules.is_ignored("backend/dist", true),
            "a sibling tree keeps its own rules"
        );
        assert!(
            !rules.is_ignored("frontend/api.generated", false),
            "a nested file re-includes"
        );
        assert!(rules.is_ignored("backend/api.generated", false));
        assert!(rules.is_ignored("frontend/types.generated", false));
    }

    #[test]
    fn a_path_under_an_ignored_directory_is_ignored() {
        let mut rules = IgnoreRules::default();
        rules.add_gitignore("", "target/\n");
        rules.add_gitignore("frontend", "dist/\n");
        assert!(rules.is_ignored("target/debug/deps/g5.rlib", false));
        assert!(rules.is_ignored("frontend/dist/assets", true));
        assert!(!rules.is_ignored("frontend/src/dist.ts", false));
    }

    #[test]
    fn a_re_included_parent_does_not_re_include_an_excluded_entry() {
        let mut rules = IgnoreRules::default();
        rules.add_gitignore("", "node_modules/\nsrc/\n");
        rules.add_gitignore("frontend", "!src/\n");
        assert!(
            !rules.is_ignored("frontend/src", true),
            "the nested file re-includes src/"
        );
        assert!(
            rules.is_ignored("frontend/src/node_modules", true),
            "the root rule still names the entry"
        );
        assert!(!rules.is_ignored("frontend/src/main.ts", false));
        assert!(rules.is_ignored("backend/src", true));
    }

    #[test]
    fn nothing_is_re_included_below_an_excluded_directory() {
        let mut rules = IgnoreRules::default();
        rules.add_gitignore("", "build/\n!build/keep.txt\n");
        assert!(rules.is_ignored("build", true));
        assert!(
            rules.is_ignored("build/keep.txt", false),
            "Git cannot re-include under build/"
        );
        assert!(!rules.is_ignored("keep.txt", false));
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_first_rule() {
        let mut rules = IgnoreRules::default();
        rules.add_gitignore("", "\u{feff}target/\nnode_modules/\n");
        assert!(rules.is_ignored("target", true));
        assert!(rules.is_ignored("node_modules", true));
    }

    #[test]
    fn an_invalid_line_keeps_the_rest_of_the_file() {
        let mut rules = IgnoreRules::default();
        rules.add_gitignore("", "[\ntarget/\n");
        assert!(rules.is_ignored("target", true));
    }

    #[test]
    fn nothing_is_ignored_without_rules() {
        let rules = IgnoreRules::default();
        assert!(!rules.is_ignored("node_modules", true));
    }

    /// An in-memory checkout: path → file content.
    struct MemorySource(BTreeMap<&'static str, &'static str>);

    impl TreeSource for MemorySource {
        type Error = Infallible;

        fn list(&self, directory: &str) -> Result<Listing, Infallible> {
            if directory == "wide" {
                return Ok(Listing::TooWide);
            }
            let prefix = if directory.is_empty() {
                String::new()
            } else {
                format!("{directory}/")
            };
            let mut names = BTreeMap::new();
            for (path, content) in &self.0 {
                let Some(relative) = path.strip_prefix(&prefix) else {
                    continue;
                };
                match relative.split_once('/') {
                    None => {
                        names.insert(relative.to_string(), (false, content.len() as u64));
                    }
                    Some((child, _)) => {
                        names.entry(child.to_string()).or_insert((true, 0));
                    }
                }
            }
            if names.is_empty() && !directory.is_empty() {
                return Ok(Listing::Missing);
            }
            Ok(Listing::Entries(
                names
                    .into_iter()
                    .map(|(name, (is_dir, size))| SourceEntry { name, is_dir, size })
                    .collect(),
            ))
        }

        fn read_gitignore(&self, directory: &str) -> Option<String> {
            let path = join(directory, GITIGNORE);
            self.0
                .get(path.as_str())
                .map(|content| (*content).to_string())
        }
    }

    fn checkout() -> MemorySource {
        MemorySource(BTreeMap::from([
            (
                ".gitignore",
                "target/\nnode_modules/\n*.log\n!keep.log\n/dist\n*.md\n",
            ),
            ("README.md", "tracked although *.md is ignored\n"),
            ("notes.md", "untracked and ignored\n"),
            ("src/main.rs", "fn main() {}\n"),
            ("keep.log", "re-included\n"),
            ("debug.log", "ignored\n"),
            ("dist/bundle.js", "ignored by /dist\n"),
            ("target/debug/deps/g5.rlib", "build output\n"),
            ("frontend/.gitignore", "dist/\n!*.log\n"),
            ("frontend/build.log", "re-included by the nested file\n"),
            ("frontend/dist/app.js", "ignored by the nested dist/\n"),
            ("frontend/node_modules/pkg/index.js", "dependency\n"),
            ("frontend/src/main.ts", "export {}\n"),
            (
                "backend/dist/keep.rs",
                "the nested dist/ rule stays under frontend/\n",
            ),
            (".git/HEAD", "hidden everywhere\n"),
        ]))
    }

    const ROOMY: WalkLimits = WalkLimits::reply(10_000, usize::MAX);

    fn paths(walk: &TreeWalk) -> Vec<&str> {
        walk.entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect()
    }

    #[test]
    fn the_walk_prunes_ignored_trees_and_keeps_tracked_paths() {
        let tracked = BTreeSet::from(["README.md".to_string()]);
        let walk = walk_tree(&checkout(), "", -1, &tracked, ROOMY).unwrap();
        assert!(!walk.truncated);
        let listed = paths(&walk);
        for expected in [
            "README.md",
            "src",
            "src/main.rs",
            "keep.log",
            "frontend",
            "frontend/build.log",
            "frontend/src/main.ts",
            "backend/dist/keep.rs",
        ] {
            assert!(listed.contains(&expected), "{expected} missing: {listed:?}");
        }
        for hidden in [
            "notes.md",
            "debug.log",
            "dist",
            "target",
            "frontend/dist",
            "frontend/node_modules",
            ".gitignore",
            ".git",
        ] {
            assert!(
                !listed
                    .iter()
                    .any(|path| *path == hidden || path.starts_with(&format!("{hidden}/"))),
                "{hidden} listed: {listed:?}"
            );
        }
    }

    #[test]
    fn the_walk_shows_design_artifacts_and_prunes_runtime_metadata() {
        let source = MemorySource(BTreeMap::from([
            (".g5/pr_ontology_diff_design/topic/ontology-plan.md", "plan"),
            (".g5/pr_ontology_diff_design/topic/mocks/page.html", "mock"),
            (
                ".g5/pr_ontology_diff_design/topic/screenshots/files.png",
                "screenshot",
            ),
            (".g5/pr_ontology_diff_design/.g5/state.json", "runtime"),
            (
                ".g5/pr_ontology_diff_design/topic/.g5/state.json",
                "runtime",
            ),
            (
                ".g5/pr_ontology_diff_design/topic/.git/config",
                "git config",
            ),
            (".g5/pr_ontology_diff_design_old/plan.md", "look-alike"),
            (".g5/auth_state.json", "runtime"),
        ]));
        let walk = walk_tree(&source, "", -1, &BTreeSet::new(), ROOMY).unwrap();
        assert!(!walk.truncated);
        let mut listed = paths(&walk);
        listed.sort_unstable();
        assert_eq!(
            listed,
            [
                ".g5",
                ".g5/pr_ontology_diff_design",
                ".g5/pr_ontology_diff_design/topic",
                ".g5/pr_ontology_diff_design/topic/mocks",
                ".g5/pr_ontology_diff_design/topic/mocks/page.html",
                ".g5/pr_ontology_diff_design/topic/ontology-plan.md",
                ".g5/pr_ontology_diff_design/topic/screenshots",
                ".g5/pr_ontology_diff_design/topic/screenshots/files.png",
            ]
        );
    }

    #[test]
    fn a_subtree_walk_inherits_the_rules_above_it() {
        let walk = walk_tree(&checkout(), "frontend", -1, &BTreeSet::new(), ROOMY).unwrap();
        let listed = paths(&walk);
        assert_eq!(
            listed,
            ["frontend/build.log", "frontend/src", "frontend/src/main.ts"]
        );
    }

    #[test]
    fn depth_bounds_the_walk() {
        let walk = walk_tree(&checkout(), "", 1, &BTreeSet::new(), ROOMY).unwrap();
        let listed = paths(&walk);
        assert!(listed.contains(&"src"));
        assert!(!listed.contains(&"src/main.rs"));
    }

    #[test]
    fn more_entries_than_the_cap_report_truncation() {
        let limits = WalkLimits::reply(3, usize::MAX);
        let walk = walk_tree(&checkout(), "", -1, &BTreeSet::new(), limits).unwrap();
        assert!(walk.truncated);
        assert_eq!(
            walk.entries.len(),
            4,
            "the walk stops at the first entry past the cap"
        );
    }

    #[test]
    fn a_directory_that_vanished_is_skipped() {
        let walk = walk_tree(
            &MemorySource(BTreeMap::from([("src/main.rs", "")])),
            "missing",
            -1,
            &BTreeSet::new(),
            ROOMY,
        )
        .unwrap();
        assert!(walk.entries.is_empty());
        assert!(!walk.truncated);
    }

    #[test]
    fn long_paths_cross_the_byte_bound_below_the_entry_cap() {
        // Forty entries whose paths alone are 4 KiB: well under the entry
        // cap, far over a 64 KiB byte budget.
        let files: BTreeMap<&'static str, &'static str> = (0..40)
            .map(|index| {
                let name: &'static str =
                    Box::leak(format!("{index:02}{}", "x".repeat(4094)).into_boxed_str());
                (name, "")
            })
            .collect();
        let limits = WalkLimits::reply(10_000, 64 * 1024);
        let walk = walk_tree(&MemorySource(files), "", -1, &BTreeSet::new(), limits).unwrap();
        assert!(walk.truncated, "the byte bound stops the walk");
        assert!(
            walk.entries.len() < 40,
            "the walk stopped before listing everything: {}",
            walk.entries.len()
        );
        let budgeted: usize = walk
            .entries
            .iter()
            .map(|entry| entry_bytes(&entry.path))
            .sum();
        assert!(budgeted > limits.max_bytes);
    }

    #[test]
    fn the_byte_budget_covers_json_escaping() {
        // Quotes, backslashes and control characters serialize to more
        // bytes than they occupy; the budget must never undercount them.
        let files: BTreeMap<&'static str, &'static str> = (0..200)
            .map(|index| {
                let name: &'static str = Box::leak(
                    format!(
                        "{index:03}{}\\\\{}\t\u{1}",
                        "\"".repeat(120),
                        "é".repeat(20)
                    )
                    .into_boxed_str(),
                );
                (name, "")
            })
            .collect();
        let walk = walk_tree(&MemorySource(files), "", -1, &BTreeSet::new(), ROOMY).unwrap();
        let entries: Vec<serde_json::Value> = walk
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({"path": entry.path, "type": if entry.is_dir { "directory" } else { "file" }, "size": u64::MAX})
            })
            .collect();
        let serialized = serde_json::to_vec(&entries).unwrap().len();
        let budgeted: usize = walk
            .entries
            .iter()
            .map(|entry| entry_bytes(&entry.path))
            .sum();
        assert!(
            budgeted >= serialized,
            "budget {budgeted} undercounts the {serialized}-byte reply"
        );
    }

    #[test]
    fn a_directory_too_wide_to_list_truncates_the_walk() {
        let walk = walk_tree(
            &MemorySource(BTreeMap::from([("src/main.rs", ""), ("wide/one", "")])),
            "",
            -1,
            &BTreeSet::new(),
            ROOMY,
        )
        .unwrap();
        assert!(walk.truncated, "a wide directory counts as past the cap");
        assert!(paths(&walk).contains(&"src/main.rs"));
    }

    #[test]
    fn a_directory_bound_counts_directories_and_not_files() {
        // Two directories holding many files: fine for a watch bounded at
        // ten directories, past a reply bounded at ten entries.
        let files: BTreeMap<&'static str, &'static str> = (0..300)
            .map(|index| {
                let name: &'static str =
                    Box::leak(format!("bucket{}/file{index:03}", index % 2).into_boxed_str());
                (name, "")
            })
            .collect();
        let source = MemorySource(files);
        let watch = walk_tree(
            &source,
            "",
            -1,
            &BTreeSet::new(),
            WalkLimits::directories(10),
        )
        .unwrap();
        assert!(!watch.truncated);
        assert_eq!(watch.entries.iter().filter(|entry| entry.is_dir).count(), 2);
        let watch = walk_tree(
            &source,
            "",
            -1,
            &BTreeSet::new(),
            WalkLimits::directories(1),
        )
        .unwrap();
        assert!(watch.truncated, "the third directory would cross the bound");
        let reply = walk_tree(
            &source,
            "",
            -1,
            &BTreeSet::new(),
            WalkLimits::reply(10, usize::MAX),
        )
        .unwrap();
        assert!(reply.truncated);
    }
}
