//! The pushed comparison shared by Source Control and formal approval capture.
//! All inputs are immutable object IDs; policy and ref resolution stay with callers.
use crate::history_index::{history_index, HistoryKey};
use crate::{GitRepo, SourceCommit, SourceFilePatch};
use g5_contracts::G5Error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushedComparison {
    pub merge_base: String,
    pub merge_base_commit: SourceHistoryCommit,
    pub commits: Vec<SourceCommit>,
    pub truncated: bool,
    pub parent_merges: Vec<String>,
    pub workspace_base: Option<SourceCommit>,
    pub workspace_base_in_history: bool,
    pub since_workspace_base_count: usize,
    // Only an in-process result can be reused. Serialized comparisons are data,
    // not evidence that this reader evaluated the same repository and cuts.
    #[serde(skip)]
    read_cut: Option<ComparisonReadCut>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct ComparisonReadCut {
    repository: std::path::PathBuf,
    source: String,
    parent: String,
    workspace_base: Option<String>,
    limit: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonHistoryPage {
    pub commits: Vec<SourceHistoryCommit>,
    pub next_cursor: Option<String>,
}
/// History browsing needs identities and labels; paths are resolved on selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceHistoryCommit {
    pub sha: String,
    pub summary: String,
    pub author_name: String,
    pub committed_at: i64,
    pub parents: Vec<String>,
}
fn history_entry(commit: &git2::Commit<'_>) -> SourceHistoryCommit {
    SourceHistoryCommit {
        sha: commit.id().to_string(),
        summary: commit.summary().ok().flatten().unwrap_or("").into(),
        author_name: commit.author().name().unwrap_or("").into(),
        committed_at: commit.time().seconds(),
        parents: commit.parent_ids().map(|id| id.to_string()).collect(),
    }
}
fn unavailable(reason: impl Into<String>) -> G5Error {
    G5Error::Conflict {
        reason: reason.into(),
    }
}
fn oid(value: &str) -> Result<git2::Oid, G5Error> {
    if value.len() != 40 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(unavailable("Comparison requires full captured commit IDs"));
    }
    git2::Oid::from_str(value).map_err(crate::backend_error)
}
fn entry(repo: &git2::Repository, commit: &git2::Commit<'_>) -> Result<SourceCommit, G5Error> {
    let tree = commit.tree().map_err(crate::backend_error)?;
    let parent = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .map_err(crate::backend_error)?
                .tree()
                .map_err(crate::backend_error)?,
        )
    } else {
        None
    };
    let diff = repo
        .diff_tree_to_tree(parent.as_ref(), Some(&tree), None)
        .map_err(crate::backend_error)?;
    let paths = diff
        .deltas()
        .flat_map(|d| {
            [
                d.old_file()
                    .path()
                    .map(|p| p.to_string_lossy().into_owned()),
                d.new_file()
                    .path()
                    .map(|p| p.to_string_lossy().into_owned()),
            ]
        })
        .flatten()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(SourceCommit {
        sha: commit.id().to_string(),
        summary: commit.summary().ok().flatten().unwrap_or("").into(),
        author_name: commit.author().name().unwrap_or("").into(),
        committed_at: commit.time().seconds(),
        parents: commit.parent_ids().map(|p| p.to_string()).collect(),
        paths,
    })
}
fn ancestor(repo: &git2::Repository, a: git2::Oid, b: git2::Oid) -> Result<bool, G5Error> {
    Ok(a == b
        || repo
            .graph_descendant_of(b, a)
            .map_err(crate::backend_error)?)
}
impl GitRepo {
    /// Validate a selected history revision without reading its tree or patch.
    /// Only a missing commit or a proven ancestry mismatch means unavailable;
    /// unreadable repositories/graphs remain errors rather than lost choices.
    pub fn source_control_available_commit(
        &self,
        tip: &str,
        sha: &str,
    ) -> Result<Option<SourceHistoryCommit>, G5Error> {
        let repo = self.repo()?;
        let commit = match repo.find_commit(oid(sha)?) {
            Ok(commit) => commit,
            Err(error) if error.code() == git2::ErrorCode::NotFound => return Ok(None),
            Err(error) => return Err(crate::backend_error(error)),
        };
        let tip = repo.find_commit(oid(tip)?).map_err(crate::backend_error)?;
        if !ancestor(&repo, commit.id(), tip.id())? {
            return Ok(None);
        }
        Ok(Some(history_entry(&commit)))
    }

    pub fn source_control_commit(&self, tip: &str, sha: &str) -> Result<SourceCommit, G5Error> {
        if sha != tip && !self.is_ancestor_of(sha, tip)? {
            return Err(G5Error::StaleHistory);
        }
        let repo = self.repo()?;
        let commit = repo.find_commit(oid(sha)?).map_err(crate::backend_error)?;
        entry(&repo, &commit)
    }

    /// The comparison boundary without the default commit list or history count.
    /// Net-file readers need the same unique base and readable endpoint trees.
    pub fn pushed_comparison_base(&self, source: &str, parent: &str) -> Result<String, G5Error> {
        let bases = self.merge_bases(source, parent)?;
        if bases.len() != 1 {
            return Err(unavailable(if bases.is_empty() {
                "No Git merge base is available"
            } else {
                "Git merge base is ambiguous"
            }));
        }
        let repo = self.repo()?;
        for sha in [parent, &bases[0], source] {
            repo.find_commit(oid(sha)?)
                .map_err(crate::backend_error)?
                .tree()
                .map_err(crate::backend_error)?;
        }
        Ok(bases[0].clone())
    }

    /// Recheck an earlier successful read in the same request. Policy owners
    /// still recapture current access, refs and live authority independently.
    /// Classification/counts depend on immutable objects; represented commit
    /// trees and the comparison boundary must remain readable on this read.
    pub fn reuse_pushed_comparison(
        &self,
        source: &str,
        parent: &str,
        workspace_base: Option<&str>,
        limit: usize,
        previous: &PushedComparison,
    ) -> Result<Option<PushedComparison>, G5Error> {
        let cut = ComparisonReadCut {
            repository: self.repo()?.path().to_path_buf(),
            source: source.into(),
            parent: parent.into(),
            workspace_base: workspace_base.map(str::to_owned),
            limit,
        };
        if previous.read_cut.as_ref() != Some(&cut)
            || (workspace_base.is_some() && previous.workspace_base.is_none())
        {
            return Ok(None);
        }
        let base = self.pushed_comparison_base(source, parent)?;
        if previous.merge_base != base {
            return Err(unavailable("Comparison boundary changed during the read"));
        }
        let repo = self.repo()?;
        for row in previous.commits.iter().chain(&previous.workspace_base) {
            let commit = repo
                .find_commit(oid(&row.sha)?)
                .map_err(crate::backend_error)?;
            entry(&repo, &commit)?;
        }
        Ok(Some(previous.clone()))
    }

    pub fn pushed_comparison(
        &self,
        source: &str,
        parent: &str,
        workspace_base: Option<&str>,
        limit: usize,
    ) -> Result<PushedComparison, G5Error> {
        let merge_base = self.pushed_comparison_base(source, parent)?;
        let repo = self.repo()?;
        let read_cut = ComparisonReadCut {
            repository: repo.path().to_path_buf(),
            source: source.into(),
            parent: parent.into(),
            workspace_base: workspace_base.map(str::to_owned),
            limit,
        };
        let parent = oid(parent)?;
        let source = oid(source)?;
        let mut current = repo.find_commit(source).map_err(crate::backend_error)?;
        let mut commits = Vec::new();
        let mut defaults = BTreeSet::new();
        let mut parent_merges = Vec::new();
        while !ancestor(&repo, current.id(), parent)? {
            defaults.insert(current.id());
            if current.parent_count() > 1 {
                let mut from_parent = true;
                for incoming in current.parent_ids().skip(1) {
                    from_parent &= ancestor(&repo, incoming, parent)?;
                }
                if from_parent {
                    parent_merges.push(current.id().to_string());
                }
            }
            if commits.len() < limit {
                commits.push(entry(&repo, &current)?);
            }
            if current.parent_count() == 0 {
                break;
            }
            current = current.parent(0).map_err(crate::backend_error)?;
        }
        let base_oid = oid(&merge_base)?;
        let merge_base_commit =
            history_entry(&repo.find_commit(base_oid).map_err(crate::backend_error)?);
        let workspace_base = workspace_base
            .and_then(|b| oid(b).ok())
            .and_then(|b| repo.find_commit(b).ok());
        let in_history = workspace_base
            .as_ref()
            .map(|b| ancestor(&repo, b.id(), base_oid))
            .transpose()?
            .unwrap_or(false);
        let mut since_count = 0;
        if let Some(base) = workspace_base.as_ref().filter(|_| in_history) {
            let mut walk = repo.revwalk().map_err(crate::backend_error)?;
            walk.push(source).map_err(crate::backend_error)?;
            walk.hide(base.id()).map_err(crate::backend_error)?;
            for item in walk {
                let id = item.map_err(crate::backend_error)?;
                if id != base_oid && !defaults.contains(&id) {
                    since_count += 1;
                }
            }
        }
        Ok(PushedComparison {
            merge_base,
            merge_base_commit,
            truncated: defaults.len() > commits.len(),
            commits,
            parent_merges,
            workspace_base: workspace_base.as_ref().and_then(|c| entry(&repo, c).ok()),
            workspace_base_in_history: in_history,
            since_workspace_base_count: since_count,
            read_cut: Some(read_cut),
        })
    }

    /// Stable pages are bound to the captured cuts and a commit cursor, never a
    /// moving branch. A section expansion is independent of patch selection.
    pub fn comparison_history(
        &self,
        source: &str,
        parent: Option<&str>,
        workspace_base: Option<&str>,
        section: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<ComparisonHistoryPage, G5Error> {
        let scope = format!(
            "{source}:{}:{}:{section}",
            parent.unwrap_or(""),
            workspace_base.unwrap_or("")
        );
        let scope = format!("{:x}", Sha256::digest(scope.as_bytes()));
        let repository = {
            let repo = self.repo()?;
            // An index never substitutes for available repository objects.
            repo.find_commit(oid(source)?)
                .map_err(crate::backend_error)?;
            if let Some(parent) = parent {
                repo.find_commit(oid(parent)?)
                    .map_err(crate::backend_error)?;
            }
            repo.path()
                .canonicalize()
                .map_err(|e| unavailable(e.to_string()))?
        };
        let order = history_index(
            HistoryKey {
                repository,
                scope: scope.clone(),
            },
            || self.comparison_history_ids(source, parent, workspace_base, section),
        )?;
        let ids = order.ids();
        let start = match cursor {
            None => 0,
            Some(cursor) => {
                order
                    .position(oid(cursor)?)
                    .ok_or_else(|| unavailable("History cursor is outside the captured section"))?
                    + 1
            }
        };
        let end = start.saturating_add(limit.clamp(1, 100)).min(ids.len());
        let repo = self.repo()?;
        let commits = ids[start..end]
            .iter()
            .map(|id| {
                repo.find_commit(*id)
                    .map(|commit| history_entry(&commit))
                    .map_err(crate::backend_error)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ComparisonHistoryPage {
            next_cursor: (end < ids.len()).then(|| ids[end - 1].to_string()),
            commits,
        })
    }

    fn comparison_history_ids(
        &self,
        source: &str,
        parent: Option<&str>,
        workspace_base: Option<&str>,
        section: &str,
    ) -> Result<Vec<git2::Oid>, G5Error> {
        let merge_base = parent
            .map(|parent| {
                let bases = self.merge_bases(source, parent)?;
                if bases.len() != 1 {
                    return Err(unavailable("History requires a unique merge base"));
                }
                oid(&bases[0])
            })
            .transpose()?;
        let repo = self.repo()?;
        let mut excluded = BTreeSet::new();
        if let Some(parent) = parent {
            let parent = oid(parent)?;
            let mut commit = repo
                .find_commit(oid(source)?)
                .map_err(crate::backend_error)?;
            while !ancestor(&repo, commit.id(), parent)? {
                excluded.insert(commit.id());
                if commit.parent_count() == 0 {
                    break;
                }
                commit = commit.parent(0).map_err(crate::backend_error)?;
            }
        }
        if let Some(base) = merge_base {
            excluded.insert(base);
        }
        let fork = workspace_base.map(oid).transpose()?;
        let in_history = match (fork, merge_base) {
            (Some(fork), Some(base)) => {
                repo.find_commit(fork).is_ok() && ancestor(&repo, fork, base)?
            }
            _ => false,
        };
        if let Some(fork) = fork {
            excluded.insert(fork);
        }
        let mut walk = repo.revwalk().map_err(crate::backend_error)?;
        walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
            .map_err(crate::backend_error)?;
        match section {
            "since" if in_history => {
                walk.push(oid(source)?).map_err(crate::backend_error)?;
                walk.hide(fork.ok_or_else(|| unavailable("Workspace base unavailable"))?)
                    .map_err(crate::backend_error)?;
            }
            "earlier" if in_history => {
                walk.push(fork.ok_or_else(|| unavailable("Workspace base unavailable"))?)
                    .map_err(crate::backend_error)?;
            }
            "history" => {
                walk.push(oid(source)?).map_err(crate::backend_error)?;
            }
            _ => return Err(unavailable("History section unavailable for these cuts")),
        }
        walk.filter_map(|id| match id {
            Ok(id) if excluded.contains(&id) => None,
            Ok(id) => Some(Ok(id)),
            Err(error) => Some(Err(crate::backend_error(error))),
        })
        .collect()
    }

    pub fn source_control_remerge_diff(
        &self,
        tip: &str,
        sha: &str,
    ) -> Result<Vec<SourceFilePatch>, G5Error> {
        if sha != tip && !self.is_ancestor_of(sha, tip)? {
            return Err(G5Error::StaleHistory);
        }
        let sha = oid(sha)?.to_string();
        let path = self.repo()?.path().to_owned();
        let runner = crate::native::NativeGit::api(
            std::path::Path::new("git"),
            std::time::Duration::from_secs(30),
        )?;
        let output = runner.remerge_diff_blocking(&path, &sha)?;
        // Git adds a remerge diagnostic header before the normal patch headers.
        // Keep all actual patch bytes, including conflict-marker removals.
        let mut patch = Vec::new();
        for line in output.split_inclusive(|byte| *byte == b'\n') {
            if !line.starts_with(b"remerge ") {
                patch.extend_from_slice(line);
            }
        }
        crate::source_control_patch(&patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comparison_fixture::comparison_fixture;
    fn linear_history(count: usize) -> (tempfile::TempDir, String) {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let directory = tempfile::tempdir().unwrap();
        git2::Repository::init(directory.path()).unwrap();
        let mut child = Command::new("git")
            .arg("-C")
            .arg(directory.path())
            .args(["fast-import", "--quiet"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        {
            let mut input = std::io::BufWriter::new(child.stdin.take().unwrap());
            for index in 0..count {
                let message = format!("History {index}");
                writeln!(input, "commit refs/heads/main\nmark :{}\ncommitter History <history@g5.dev> {} +0000\ndata {}\n{}",
                    index + 1, 1_700_000_000 + index, message.len(), message).unwrap();
                if index > 0 {
                    writeln!(input, "from :{index}").unwrap();
                }
                let content = index.to_string();
                writeln!(
                    input,
                    "M 100644 inline history.txt\ndata {}\n{}\n",
                    content.len(),
                    content
                )
                .unwrap();
            }
            input.flush().unwrap();
        }
        assert!(child.wait().unwrap().success());
        let repository = git2::Repository::open(directory.path()).unwrap();
        let head = repository
            .refname_to_id("refs/heads/main")
            .unwrap()
            .to_string();
        (directory, head)
    }

    fn raw_commit(
        repo: &git2::Repository,
        tree: git2::Oid,
        parents: &[git2::Oid],
        message: &str,
    ) -> git2::Oid {
        let parents = parents
            .iter()
            .map(|id| format!("parent {id}\n"))
            .collect::<String>();
        let raw = format!("tree {tree}\n{parents}author Test <test@g5.dev> 1700000000 +0000\ncommitter Test <test@g5.dev> 1700000000 +0000\n\n{message}\n");
        repo.odb()
            .unwrap()
            .write(git2::ObjectType::Commit, raw.as_bytes())
            .unwrap()
    }

    #[test]
    fn comparison_boundary_validates_each_endpoint_and_unique_base() {
        let directory = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(directory.path()).unwrap();
        let tree = repo.treebuilder(None).unwrap().write().unwrap();
        let absent = oid("1234567890123456789012345678901234567890").unwrap();
        let root = raw_commit(&repo, tree, &[], "root");
        let source = raw_commit(&repo, tree, &[root], "source");
        let parent = raw_commit(&repo, tree, &[root], "parent");
        let bad_source = raw_commit(&repo, absent, &[root], "missing source tree");
        let bad_parent = raw_commit(&repo, absent, &[root], "missing parent tree");
        let bad_base = raw_commit(&repo, absent, &[], "missing base tree");
        let from_bad_base = raw_commit(&repo, tree, &[bad_base], "source on missing base tree");
        let parent_on_bad_base =
            raw_commit(&repo, tree, &[bad_base], "parent on missing base tree");
        let unrelated = raw_commit(&repo, tree, &[], "unrelated");
        let git = GitRepo::open(directory.path()).unwrap();
        assert_eq!(
            git.pushed_comparison_base(&source.to_string(), &parent.to_string())
                .unwrap(),
            root.to_string()
        );
        for (source, parent) in [
            (bad_source.to_string(), parent.to_string()),
            (source.to_string(), bad_parent.to_string()),
            (from_bad_base.to_string(), parent_on_bad_base.to_string()),
            (absent.to_string(), parent.to_string()),
            ("invalid".into(), parent.to_string()),
            (source.to_string(), unrelated.to_string()),
        ] {
            assert!(
                git.pushed_comparison_base(&source, &parent).is_err(),
                "{source} vs {parent}"
            );
        }
    }

    #[test]
    fn comparison_reuse_is_bound_to_repository_cuts_limit_and_in_process_result() {
        let (directory, source) = linear_history(8);
        let git = GitRepo::open(directory.path()).unwrap();
        let parent = git
            .repo()
            .unwrap()
            .find_commit(oid(&source).unwrap())
            .unwrap()
            .parent_id(0)
            .unwrap()
            .to_string();
        let previous = git
            .pushed_comparison(&source, &parent, Some(&parent), 250)
            .unwrap();
        let fresh = GitRepo::open(directory.path()).unwrap();
        let reused = fresh
            .reuse_pushed_comparison(&source, &parent, Some(&parent), 250, &previous)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&reused).unwrap(),
            serde_json::to_value(&previous).unwrap()
        );
        for (source, parent, fork, limit) in [
            (parent.as_str(), parent.as_str(), Some(parent.as_str()), 250),
            (source.as_str(), source.as_str(), Some(parent.as_str()), 250),
            (source.as_str(), parent.as_str(), Some(source.as_str()), 250),
            (source.as_str(), parent.as_str(), None, 250),
            (source.as_str(), parent.as_str(), Some(parent.as_str()), 1),
        ] {
            assert!(fresh
                .reuse_pushed_comparison(source, parent, fork, limit, &previous)
                .unwrap()
                .is_none());
        }
        let other_directory = tempfile::tempdir().unwrap();
        GitRepo::init(other_directory.path(), false).unwrap();
        let other = GitRepo::open(other_directory.path()).unwrap();
        assert!(other
            .reuse_pushed_comparison(&source, &parent, Some(&parent), 250, &previous)
            .unwrap()
            .is_none());
        let wire = serde_json::to_value(&previous).unwrap();
        assert!(wire.get("readCut").is_none());
        let from_wire = serde_json::from_value(wire).unwrap();
        assert!(fresh
            .reuse_pushed_comparison(&source, &parent, Some(&parent), 250, &from_wire)
            .unwrap()
            .is_none());
        let unavailable_marker = git
            .pushed_comparison(&source, &parent, Some(&"f".repeat(40)), 250)
            .unwrap();
        assert!(fresh
            .reuse_pushed_comparison(
                &source,
                &parent,
                Some(&"f".repeat(40)),
                250,
                &unavailable_marker
            )
            .unwrap()
            .is_none());
    }

    #[test]
    fn reuse_rechecks_row_trees_but_net_boundary_does_not_need_intermediate_trees() {
        let directory = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(directory.path()).unwrap();
        let mut tree = repo.treebuilder(None).unwrap();
        let base_tree = tree.write().unwrap();
        let root = raw_commit(&repo, base_tree, &[], "root");
        tree.insert("file", repo.blob(b"first").unwrap(), 0o100644)
            .unwrap();
        let middle_tree = tree.write().unwrap();
        let middle = raw_commit(&repo, middle_tree, &[root], "middle");
        tree.insert("file", repo.blob(b"second").unwrap(), 0o100644)
            .unwrap();
        let tip = raw_commit(&repo, tree.write().unwrap(), &[middle], "tip");
        let source = tip.to_string();
        let parent = root.to_string();
        let previous = GitRepo::open(directory.path())
            .unwrap()
            .pushed_comparison(&source, &parent, Some(&parent), 250)
            .unwrap();
        let object = middle_tree.to_string();
        std::fs::remove_file(
            repo.path()
                .join("objects")
                .join(&object[..2])
                .join(&object[2..]),
        )
        .unwrap();
        let fresh = GitRepo::open(directory.path()).unwrap();
        assert_eq!(
            fresh.pushed_comparison_base(&source, &parent).unwrap(),
            parent
        );
        assert_eq!(
            fresh.source_control_diff(&parent, &source).unwrap()[0].path,
            "file"
        );
        assert!(fresh
            .reuse_pushed_comparison(&source, &parent, Some(&parent), 250, &previous)
            .is_err());
        assert!(fresh
            .pushed_comparison(&source, &parent, Some(&parent), 250)
            .is_err());
    }

    #[test]
    fn history_pages_keep_all_merge_ancestry_and_legacy_cursors() {
        let fixture = comparison_fixture();
        let git = GitRepo::open(fixture.directory.path()).unwrap();
        for source in fixture.heads.values() {
            if git
                .repo()
                .unwrap()
                .find_commit(oid(source).unwrap())
                .is_err()
            {
                assert!(git
                    .comparison_history(source, None, None, "history", None, 2)
                    .is_err());
                continue;
            }
            // Independent, uncached traversal is the pre-optimization ordering.
            let expected = {
                let repo = git.repo().unwrap();
                let mut walk = repo.revwalk().unwrap();
                walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
                    .unwrap();
                walk.push(oid(source).unwrap()).unwrap();
                walk.map(|id| id.unwrap().to_string()).collect::<Vec<_>>()
            };
            let mut actual = vec![];
            let mut cursor = None;
            loop {
                let page = git
                    .comparison_history(source, None, None, "history", cursor.as_deref(), 2)
                    .unwrap();
                actual.extend(page.commits.iter().map(|commit| commit.sha.clone()));
                if let Some(last) = page.commits.last() {
                    let legacy = git
                        .comparison_history(source, None, None, "history", Some(&last.sha), 2)
                        .unwrap();
                    if let Some(next) = &page.next_cursor {
                        let indexed = git
                            .comparison_history(source, None, None, "history", Some(next), 2)
                            .unwrap();
                        assert_eq!(
                            serde_json::to_value(legacy).unwrap(),
                            serde_json::to_value(indexed).unwrap()
                        );
                    } else {
                        assert!(legacy.commits.is_empty());
                    }
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn history_cursor_is_located_in_its_captured_section() {
        let (directory, source) = linear_history(8);
        let git = GitRepo::open(directory.path()).unwrap();
        let page = git
            .comparison_history(&source, None, None, "history", None, 2)
            .unwrap();
        let cursor = page.next_cursor.unwrap();
        assert_eq!(
            cursor, page.commits[1].sha,
            "cursor wire format stays unchanged"
        );
        let all = git
            .comparison_history(&source, None, None, "history", None, 100)
            .unwrap();
        let other_source = &all.commits.last().unwrap().sha;
        for (source, parent, fork, section) in [
            (other_source.as_str(), None, None, "history"),
            (source.as_str(), None, Some(cursor.as_str()), "history"),
            (source.as_str(), Some(cursor.as_str()), None, "history"),
            (source.as_str(), None, None, "earlier"),
        ] {
            assert!(git
                .comparison_history(source, parent, fork, section, Some(&cursor), 2)
                .is_err());
        }
        for invalid in ["garbage", "ffffffffffffffffffffffffffffffffffffffff"] {
            assert!(git
                .comparison_history(&source, None, None, "history", Some(invalid), 2)
                .is_err());
        }
        let selected = git
            .source_control_commit(&source, &page.commits[0].sha)
            .unwrap();
        assert_eq!(selected.paths, vec!["history.txt"]);
        assert!(serde_json::to_value(&page.commits).unwrap()[0]
            .get("paths")
            .is_none());
    }

    #[test]
    fn history_metadata_does_not_read_commit_trees() {
        let directory = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(directory.path()).unwrap();
        let tree = "1234567890123456789012345678901234567890";
        let raw = format!("tree {tree}\nauthor Test <test@g5.dev> 1700000000 +0000\ncommitter Test <test@g5.dev> 1700000000 +0000\n\nMetadata without a tree\n");
        let id = repo
            .odb()
            .unwrap()
            .write(git2::ObjectType::Commit, raw.as_bytes())
            .unwrap();
        let git = GitRepo::open(directory.path()).unwrap();
        let page = git
            .comparison_history(&id.to_string(), None, None, "history", None, 50)
            .unwrap();
        assert_eq!(page.commits[0].summary, "Metadata without a tree");
        assert_eq!(
            git.source_control_available_commit(&id.to_string(), &id.to_string())
                .unwrap()
                .unwrap()
                .summary,
            "Metadata without a tree"
        );
        assert!(git
            .source_control_available_commit(&id.to_string(), &"0".repeat(40))
            .unwrap()
            .is_none());
        assert!(git
            .source_control_available_commit(&"0".repeat(40), &"0".repeat(40))
            .unwrap()
            .is_none());
        assert!(git
            .source_control_available_commit(&"0".repeat(40), &id.to_string())
            .is_err());
        assert!(git
            .source_control_available_commit(&id.to_string(), "invalid")
            .is_err());
        assert!(git
            .source_control_commit(&id.to_string(), &id.to_string())
            .is_err());
    }

    #[test]
    fn cache_rebuild_preserves_cursors_and_new_pushes_keep_old_cuts() {
        let (directory, source) = linear_history(24);
        let git = GitRepo::open(directory.path()).unwrap();
        let all = git
            .comparison_history(&source, None, None, "history", None, 100)
            .unwrap();
        let first = git
            .comparison_history(&source, None, None, "history", None, 2)
            .unwrap();
        let next = git
            .comparison_history(
                &source,
                None,
                None,
                "history",
                first.next_cursor.as_deref(),
                2,
            )
            .unwrap();
        for commit in &all.commits {
            git.comparison_history(&commit.sha, None, None, "history", None, 2)
                .unwrap();
        }
        let fresh_reader = GitRepo::open(directory.path()).unwrap();
        let rebuilt = fresh_reader
            .comparison_history(
                &source,
                None,
                None,
                "history",
                first.next_cursor.as_deref(),
                2,
            )
            .unwrap();
        assert_eq!(
            serde_json::to_value(next).unwrap(),
            serde_json::to_value(rebuilt).unwrap()
        );
        let newer = {
            let repo = git.repo().unwrap();
            let parent = repo.find_commit(oid(&source).unwrap()).unwrap();
            let signature = git2::Signature::now("History", "history@g5.dev").unwrap();
            let tree = parent.tree().unwrap();
            let id = repo
                .commit(
                    Some("refs/heads/main"),
                    &signature,
                    &signature,
                    "New push",
                    &tree,
                    &[&parent],
                )
                .unwrap();
            id.to_string()
        };
        let old = fresh_reader
            .comparison_history(&source, None, None, "history", None, 2)
            .unwrap();
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(old).unwrap()
        );
        let new = fresh_reader
            .comparison_history(&newer, None, None, "history", None, 2)
            .unwrap();
        assert_eq!(new.commits[0].summary, "New push");
    }

    #[test]
    #[ignore = "focused 100k-commit performance measurement"]
    fn large_base_history_benchmark() {
        use std::time::Instant;
        let (directory, source) = linear_history(100_000);
        let legacy = |skip: usize| {
            let reader = GitRepo::open(directory.path()).unwrap();
            let repo = reader.repo().unwrap();
            let mut walk = repo.revwalk().unwrap();
            walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
                .unwrap();
            walk.push(oid(&source).unwrap()).unwrap();
            walk.skip(skip)
                .take(50)
                .map(|id| {
                    entry(&repo, &repo.find_commit(id.unwrap()).unwrap())
                        .unwrap()
                        .sha
                })
                .collect::<Vec<_>>()
        };
        let start = Instant::now();
        let original_first = legacy(0);
        let old_first = start.elapsed();
        let start = Instant::now();
        let original_deep = legacy(90_000);
        let old_deep = start.elapsed();
        let git = GitRepo::open(directory.path()).unwrap();
        let start = Instant::now();
        let first = git
            .comparison_history(&source, None, None, "history", None, 50)
            .unwrap();
        let cold = start.elapsed();
        assert_eq!(
            first
                .commits
                .iter()
                .map(|c| c.sha.clone())
                .collect::<Vec<_>>(),
            original_first
        );
        let previous = {
            let repo = git.repo().unwrap();
            let mut walk = repo.revwalk().unwrap();
            walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
                .unwrap();
            walk.push(oid(&source).unwrap()).unwrap();
            walk.nth(89_999).unwrap().unwrap()
        };
        let cursor = previous.to_string();
        let start = Instant::now();
        for _ in 0..20 {
            let reader = GitRepo::open(directory.path()).unwrap();
            let page = reader
                .comparison_history(&source, None, None, "history", Some(&cursor), 50)
                .unwrap();
            assert_eq!(
                page.commits
                    .iter()
                    .map(|c| c.sha.clone())
                    .collect::<Vec<_>>(),
                original_deep
            );
        }
        let deep = start.elapsed() / 20;
        eprintln!("100000 commits, 50/page, fresh reader per request: legacy first={old_first:?}, legacy at 90000={old_deep:?}, indexed cold={cold:?}, indexed at 90000={deep:?}; retained index={} bytes for this cut", 100_000 * (std::mem::size_of::<git2::Oid>() + std::mem::size_of::<usize>()));
    }

    #[test]
    fn shared_comparison_matrix() {
        let fixture = comparison_fixture();
        for case in &fixture.scenarios {
            let git = GitRepo::open(fixture.directory.path()).unwrap();
            let source = &fixture.heads[case.source];
            let parent = &fixture.heads[case.parent];
            let fork = &fixture.heads[case.base];
            let result = git.pushed_comparison(source, parent, Some(fork), 250);
            let Some(expected) = case.merge_base else {
                assert!(result.is_err(), "{}", case.name);
                continue;
            };
            let result = result.unwrap();
            assert_eq!(result.merge_base, fixture.heads[expected], "{}", case.name);
            let mut metadata = serde_json::to_value(
                git.source_control_commit(source, &result.merge_base)
                    .unwrap(),
            )
            .unwrap();
            metadata.as_object_mut().unwrap().remove("paths");
            assert_eq!(
                serde_json::to_value(&result.merge_base_commit).unwrap(),
                metadata,
                "{}: merge-base row contains real commit metadata only",
                case.name
            );
            let fresh = GitRepo::open(fixture.directory.path()).unwrap();
            assert_eq!(
                fresh.pushed_comparison_base(source, parent).unwrap(),
                fixture.heads[expected],
                "{}",
                case.name
            );
            let reused = fresh
                .reuse_pushed_comparison(source, parent, Some(fork), 250, &result)
                .unwrap()
                .unwrap();
            assert_eq!(
                serde_json::to_value(reused).unwrap(),
                serde_json::to_value(&result).unwrap(),
                "{}",
                case.name
            );
            assert_eq!(
                result
                    .commits
                    .iter()
                    .map(|c| c.summary.as_str())
                    .collect::<Vec<_>>(),
                case.defaults,
                "{}",
                case.name
            );
            let files = git.source_control_diff(&result.merge_base, source).unwrap();
            assert_eq!(
                files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
                case.paths,
                "{}",
                case.name
            );
            let section = if result.workspace_base_in_history {
                "since"
            } else {
                "history"
            };
            let mut ids = BTreeSet::new();
            let mut cursor = None;
            loop {
                let page = git
                    .comparison_history(
                        source,
                        Some(parent),
                        Some(fork),
                        section,
                        cursor.as_deref(),
                        1,
                    )
                    .unwrap();
                for commit in page.commits {
                    assert_ne!(commit.sha, result.merge_base);
                    assert!(!result.commits.iter().any(|c| c.sha == commit.sha));
                    assert!(ids.insert(commit.sha));
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
            if result.workspace_base_in_history {
                assert_eq!(ids.len(), result.since_workspace_base_count);
                let expected = {
                    let repo = git.repo().unwrap();
                    let mut walk = repo.revwalk().unwrap();
                    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
                        .unwrap();
                    walk.push(oid(fork).unwrap()).unwrap();
                    walk.skip(1)
                        .map(|id| id.unwrap().to_string())
                        .collect::<Vec<_>>()
                };
                let mut actual = vec![];
                let mut cursor = None;
                loop {
                    let page = git
                        .comparison_history(
                            source,
                            Some(parent),
                            Some(fork),
                            "earlier",
                            cursor.as_deref(),
                            1,
                        )
                        .unwrap();
                    actual.extend(page.commits.into_iter().map(|commit| commit.sha));
                    cursor = page.next_cursor;
                    if cursor.is_none() {
                        break;
                    }
                }
                assert_eq!(actual, expected, "{} earlier history", case.name);
            }
            if case.name == "parent-merge" {
                assert_eq!(result.parent_merges, vec![source.clone()]);
                assert!(git
                    .source_control_remerge_diff(source, source)
                    .unwrap()
                    .is_empty());
            }
            if case.name == "conflict-resolution" {
                let own = git.source_control_remerge_diff(source, source).unwrap();
                assert_eq!(own.len(), 1);
                assert_eq!(own[0].path, "common.txt");
            }
            if case.name == "child-merge" {
                assert!(result.parent_merges.is_empty());
                assert_eq!(
                    git.source_control_commit_diff(source, source).unwrap()[0].path,
                    "child.txt"
                );
            }
        }
    }
}
