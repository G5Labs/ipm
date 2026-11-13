//! The turn's ontology start snapshot and its changes card
//! (`g5.chat.ontology-card`).
//!
//! When a workspace chat turn starts, each code member's ontology
//! (`.g5/ontology/**/*.g5.md`) is recorded as a commit the server already has,
//! a zero-context patch of the tracked files against it and the text of new
//! untracked files. Nothing in the checkout's Git state is written: no commit,
//! stash, ref or index change. When the turn ends, the start files are rebuilt
//! from that record and compared with the files as they stand under the shared
//! statement rule. Every Git call goes through g5-scm's typed operations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use g5_scm::native::{FileSelection, NativeGit, NativeObjectKind};
use g5_semantic::ontology_comparison::{self, OntologyStatement, StatementChangeKind};
use serde::{Deserialize, Serialize};

/// The ontology files of a code member.
const ONTOLOGY: FileSelection<'static> = FileSelection {
    directory: ONTOLOGY_DIR,
    suffix: ONTOLOGY_SUFFIX,
};
const ONTOLOGY_DIR: &str = ".g5/ontology";
const ONTOLOGY_SUFFIX: &str = ".g5.md";
/// One Git call; a stuck call refuses the member rather than the turn.
const GIT_TIMEOUT: Duration = Duration::from_secs(30);
/// The snapshot travels with the turn; beyond this the member is refused.
pub const MAX_BASELINE_BYTES: usize = 4 * 1024 * 1024;
/// The start commit's ontology files, read back to rebuild the start.
const MAX_START_FILES_BYTES: usize = 64 * 1024 * 1024;

/// A code member of the workspace, from the dispatcher's member vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub repo_id: String,
    pub root_path: String,
    pub root: PathBuf,
    /// The member's canonical `origin` URL, for refetching a start commit.
    pub git_url: Option<String>,
}

#[derive(Deserialize)]
struct LayoutMember {
    repo_id: String,
    root_path: String,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    git_url: Option<String>,
}

/// Code members named by `G5D_AGENT_REPOSITORIES`; the docs member has no
/// ontology. A member path that is absolute or escapes the workspace is skipped.
pub fn code_members(workspace_root: &Path, repositories_json: &str) -> Vec<Member> {
    let Ok(members) = serde_json::from_str::<Vec<LayoutMember>>(repositories_json) else {
        return Vec::new();
    };
    members
        .into_iter()
        .filter(|m| m.role.as_deref().unwrap_or("code") == "code")
        .filter(|m| {
            let path = Path::new(&m.root_path);
            !m.root_path.is_empty()
                && path
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
        })
        .map(|m| Member {
            root: workspace_root.join(&m.root_path),
            repo_id: m.repo_id,
            root_path: m.root_path,
            git_url: m.git_url.filter(|url| !url.is_empty()),
        })
        .collect()
}

/// The turn's start, recorded once and reused by its later attempts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OntologyBaselineV1 {
    pub members: Vec<MemberBaseline>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberBaseline {
    pub repo_id: String,
    /// The newest commit HEAD shares with the pushed branch, else HEAD; absent
    /// for an unborn or unreadable repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    /// `git diff -U0` of the tracked ontology files from `base_commit` to the
    /// working tree: staged, unstaged, deleted and unpushed changes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub patch: String,
    /// Full text of files the base cannot supply: untracked files, or every
    /// file when there is no base commit.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, String>,
    /// Why this member's start could not be recorded; its changed files are refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
    /// For an unavailable member, each file's SHA-256, so a turn that leaves
    /// them unchanged still has no card.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub digests: BTreeMap<String, String>,
}

impl OntologyBaselineV1 {
    pub fn encoded_len(&self) -> usize {
        serde_json::to_vec(self).map_or(usize::MAX, |bytes| bytes.len())
    }
}

/// The shared native runner, through the member relay the guest runtime
/// configures when there is one, as the workspace CLI does.
fn git() -> Result<NativeGit, String> {
    let runner = match (
        std::env::var_os(g5_contracts::workspace_git::SOCKET_ENV),
        std::env::var_os("GIT_CONFIG_GLOBAL"),
    ) {
        (Some(_), Some(config)) => NativeGit::workspace_cli(GIT_TIMEOUT, config.into()),
        _ => NativeGit::cli(GIT_TIMEOUT),
    };
    runner.map_err(|e| e.public_message())
}

fn digest(text: &str) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read_text(root: &Path, path: &str) -> Option<String> {
    let full = root.join(path);
    let meta = std::fs::symlink_metadata(&full).ok()?;
    if !meta.file_type().is_file() {
        return None;
    }
    std::fs::read(full)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Record one member's ontology as it stands now.
pub async fn capture_member(member: &Member) -> MemberBaseline {
    let mut baseline = MemberBaseline {
        repo_id: member.repo_id.clone(),
        ..MemberBaseline::default()
    };
    if let Err(reason) = capture_into(member, &mut baseline).await {
        return unavailable_member(member, reason).await;
    }
    let size = baseline.patch.len() + baseline.files.values().map(String::len).sum::<usize>();
    if size > MAX_BASELINE_BYTES {
        return unavailable_member(member, too_large()).await;
    }
    baseline
}

fn too_large() -> String {
    "the ontology's uncommitted and unpushed changes exceed 4 MiB".into()
}

/// A member whose start cannot be kept: only its files' digests, so a turn
/// that leaves them unchanged still has no card.
async fn unavailable_member(member: &Member, reason: String) -> MemberBaseline {
    MemberBaseline {
        repo_id: member.repo_id.clone(),
        unavailable: Some(reason),
        digests: current_files(member)
            .await
            .map(|files| files.iter().map(|(p, t)| (p.clone(), digest(t))).collect())
            .unwrap_or_default(),
        ..MemberBaseline::default()
    }
}

async fn capture_into(member: &Member, baseline: &mut MemberBaseline) -> Result<(), String> {
    let root = &member.root;
    if !root.join(".git").exists() {
        return Err("not a Git checkout".into());
    }
    let git = git()?;
    baseline.base_commit = git
        .pushed_base(root, "origin")
        .await
        .map_err(|e| e.public_message())?;
    match &baseline.base_commit {
        Some(base) => {
            // `diff-index`, not `git diff`, which rewrites the index for
            // stat-dirty files.
            let patch = git
                .worktree_patch(root, base, ONTOLOGY, MAX_BASELINE_BYTES)
                .await
                .map_err(|e| e.public_message())?
                .ok_or_else(too_large)?;
            baseline.patch =
                String::from_utf8(patch).map_err(|_| "an ontology file is not UTF-8 text")?;
            let untracked = git
                .worktree_files(root, ONTOLOGY, false)
                .await
                .map_err(|e| e.public_message())?;
            for path in untracked {
                if let Some(text) = read_text(root, &path) {
                    baseline.files.insert(path, text);
                }
            }
        }
        None => {
            for path in working_paths(&git, root).await? {
                if let Some(text) = read_text(root, &path) {
                    baseline.files.insert(path, text);
                }
            }
        }
    }
    Ok(())
}

/// Tracked and untracked, not ignored, ontology files present in the working tree.
async fn working_paths(git: &NativeGit, root: &Path) -> Result<BTreeSet<String>, String> {
    let listed = git
        .worktree_files(root, ONTOLOGY, true)
        .await
        .map_err(|e| e.public_message())?;
    Ok(listed
        .into_iter()
        .filter(|path| {
            std::fs::symlink_metadata(root.join(path)).is_ok_and(|m| m.file_type().is_file())
        })
        .collect())
}

/// The member's ontology files as they stand now.
pub async fn current_files(member: &Member) -> Result<BTreeMap<String, String>, String> {
    let git = git()?;
    let mut files = BTreeMap::new();
    for path in working_paths(&git, &member.root).await? {
        if let Some(text) = read_text(&member.root, &path) {
            files.insert(path, text);
        }
    }
    Ok(files)
}

/// Rebuild the member's start files from its record.
pub async fn start_files(
    member: &Member,
    baseline: &MemberBaseline,
) -> Result<BTreeMap<String, String>, String> {
    if let Some(reason) = &baseline.unavailable {
        return Err(reason.clone());
    }
    let mut files = BTreeMap::new();
    if let Some(base) = &baseline.base_commit {
        let root = &member.root;
        let git = git()?;
        let present = git
            .object_kind(root, base)
            .await
            .map_err(|e| e.public_message())?
            == Some(NativeObjectKind::Commit);
        if !present {
            // Lost locally, for example after a rebase and pruning: the server
            // still has it, as the base was chosen to be durable.
            let url = member
                .git_url
                .as_deref()
                .ok_or("the turn's start commit is unavailable")?;
            git.fetch_commit(root, "origin", url, base, &g5_scm::GitAuth::None)
                .await
                .map_err(|e| {
                    format!(
                        "the turn's start commit is unavailable: {}",
                        e.public_message()
                    )
                })?;
        }
        let blobs = git
            .commit_files(root, base, ONTOLOGY, MAX_START_FILES_BYTES)
            .await
            .map_err(|e| e.public_message())?
            .ok_or("the turn's start files are too large to read")?;
        for (path, blob) in blobs {
            files.insert(path, String::from_utf8_lossy(&blob).into_owned());
        }
        apply_patch(&mut files, &baseline.patch)?;
    }
    for (path, text) in &baseline.files {
        files.insert(path.clone(), text.clone());
    }
    Ok(files)
}

fn unquote(path: &str) -> String {
    let Some(inner) = path.strip_prefix('"').and_then(|p| p.strip_suffix('"')) else {
        return path.to_owned();
    };
    let mut out = Vec::new();
    let mut chars = inner.bytes().peekable();
    while let Some(b) = chars.next() {
        if b != b'\\' {
            out.push(b);
            continue;
        }
        match chars.next() {
            Some(b'n') => out.push(b'\n'),
            Some(b't') => out.push(b'\t'),
            Some(b'"') => out.push(b'"'),
            Some(b'\\') => out.push(b'\\'),
            Some(d @ b'0'..=b'7') => {
                let mut value = u32::from(d - b'0');
                for _ in 0..2 {
                    if let Some(&n @ b'0'..=b'7') = chars.peek() {
                        value = value * 8 + u32::from(n - b'0');
                        chars.next();
                    }
                }
                out.push(value as u8);
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn patch_path(spec: &str, prefix: &str) -> Option<String> {
    if spec == "/dev/null" {
        return None;
    }
    let path = unquote(spec.trim_end_matches('\t'));
    path.strip_prefix(prefix).map(str::to_owned)
}

/// Apply a `git diff -U0` patch made from exactly these files. Every removed
/// line must match, so a patch that does not belong to its base is refused.
pub fn apply_patch(files: &mut BTreeMap<String, String>, patch: &str) -> Result<(), String> {
    let lines: Vec<&str> = patch.split_inclusive('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        if !lines[i].starts_with("diff --git ") {
            i += 1;
            continue;
        }
        i += 1;
        let (mut old, mut new) = (None, None);
        let mut saw_headers = false;
        while i < lines.len() && !lines[i].starts_with("@@") && !lines[i].starts_with("diff --git ")
        {
            let line = lines[i].trim_end_matches('\n');
            if let Some(spec) = line.strip_prefix("--- ") {
                old = patch_path(spec, "a/");
                saw_headers = true;
            } else if let Some(spec) = line.strip_prefix("+++ ") {
                new = patch_path(spec, "b/");
            }
            i += 1;
        }
        if !saw_headers {
            // A mode-only change carries no content.
            continue;
        }
        let base = match &old {
            Some(path) => Some(
                files
                    .get(path)
                    .cloned()
                    .ok_or_else(|| format!("the start patch names a missing file: {path}"))?,
            ),
            None => None,
        };
        let source: Vec<&str> = base
            .as_deref()
            .map(|b| b.split_inclusive('\n').collect())
            .unwrap_or_default();
        let mut out = String::new();
        let mut cursor = 0usize;
        while i < lines.len() && lines[i].starts_with("@@") {
            let header = lines[i].trim_end_matches('\n');
            let (old_start, old_len) = hunk_range(header, '-')?;
            i += 1;
            // With zero context, a pure insertion follows line `old_start`.
            let take_until = if old_len == 0 {
                old_start
            } else {
                old_start - 1
            };
            if take_until < cursor || take_until > source.len() {
                return Err("the start patch does not match its commit".into());
            }
            for line in &source[cursor..take_until] {
                out.push_str(line);
            }
            cursor = take_until;
            let mut last_added: Option<usize> = None;
            let mut last_was_removal = false;
            while i < lines.len() {
                let line = lines[i];
                if let Some(removed) = line.strip_prefix('-') {
                    if source.get(cursor).map(|s| strip_newline(s)) != Some(strip_newline(removed))
                    {
                        return Err("the start patch does not match its commit".into());
                    }
                    cursor += 1;
                    last_was_removal = true;
                } else if let Some(added) = line.strip_prefix('+') {
                    last_added = Some(out.len());
                    out.push_str(added);
                    last_was_removal = false;
                } else if line.starts_with('\\') {
                    // "\ No newline at end of file" ends the previous line.
                    if !last_was_removal && last_added.is_some() && out.ends_with('\n') {
                        out.pop();
                    }
                } else {
                    break;
                }
                i += 1;
            }
        }
        for line in &source[cursor.min(source.len())..] {
            out.push_str(line);
        }
        match (&old, &new) {
            (_, None) => {
                if let Some(path) = &old {
                    files.remove(path);
                }
            }
            (_, Some(path)) => {
                if let Some(prev) = &old {
                    if prev != path {
                        files.remove(prev);
                    }
                }
                files.insert(path.clone(), out);
            }
        }
    }
    Ok(())
}

fn strip_newline(line: &str) -> &str {
    line.strip_suffix('\n').unwrap_or(line)
}

fn hunk_range(header: &str, sign: char) -> Result<(usize, usize), String> {
    let spec = header
        .split(' ')
        .find(|part| part.starts_with(sign))
        .ok_or("malformed start patch")?;
    let spec = &spec[1..];
    let (start, len) = match spec.split_once(',') {
        Some((s, l)) => (s, l),
        None => (spec, "1"),
    };
    Ok((
        start.parse().map_err(|_| "malformed start patch")?,
        len.parse().map_err(|_| "malformed start patch")?,
    ))
}

/// One statement version on the card: its authored source (anchor to the
/// statement's end) and its footer metadata entry. `"{source}## Metadata\n\n### {id}{metadata}"`
/// parses back to the same statement under both implementations of the rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardStatement {
    pub source: String,
    pub metadata: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardChange {
    pub id: String,
    pub change: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<CardStatement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<CardStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardFile {
    pub repo_id: String,
    pub root_path: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<CardChange>,
}

fn card_statement(statement: &OntologyStatement) -> CardStatement {
    CardStatement {
        source: statement.source.clone(),
        metadata: statement.metadata.clone(),
    }
}

/// Compare one member's start and current files. Files are listed in path
/// order; a file whose versions cannot be compared carries its refusal.
pub fn compare_member(
    member: &Member,
    start: Result<&BTreeMap<String, String>, &str>,
    current: Result<&BTreeMap<String, String>, &str>,
) -> Vec<CardFile> {
    let file = |path: &str| CardFile {
        repo_id: member.repo_id.clone(),
        root_path: member.root_path.clone(),
        path: path.to_owned(),
        refusal: None,
        changes: Vec::new(),
    };
    let (start, current) = match (start, current) {
        (Ok(s), Ok(c)) => (s, c),
        (Err(reason), _) | (_, Err(reason)) => {
            return vec![CardFile {
                refusal: Some(format!("Statement comparison unavailable: {reason}.")),
                ..file(ONTOLOGY_DIR)
            }];
        }
    };
    compare_files(member, start, current)
}

/// Files of a member whose start is unavailable: only those that changed,
/// each refused, since their start versions cannot be shown.
pub fn refuse_changed(
    member: &Member,
    baseline: &MemberBaseline,
    current: &BTreeMap<String, String>,
) -> Vec<CardFile> {
    let reason = baseline.unavailable.as_deref().unwrap_or("unavailable");
    let paths: BTreeSet<&String> = baseline.digests.keys().chain(current.keys()).collect();
    paths
        .into_iter()
        .filter(|path| baseline.digests.get(*path) != current.get(*path).map(|t| digest(t)).as_ref())
        .map(|path| CardFile {
            repo_id: member.repo_id.clone(),
            root_path: member.root_path.clone(),
            path: path.clone(),
            refusal: Some(format!(
                "Statement comparison unavailable: the turn's start was not recorded because {reason}."
            )),
            changes: Vec::new(),
        })
        .collect()
}

fn compare_files(
    member: &Member,
    start: &BTreeMap<String, String>,
    current: &BTreeMap<String, String>,
) -> Vec<CardFile> {
    let file = |path: &str| CardFile {
        repo_id: member.repo_id.clone(),
        root_path: member.root_path.clone(),
        path: path.to_owned(),
        refusal: None,
        changes: Vec::new(),
    };
    let paths: BTreeSet<&String> = start.keys().chain(current.keys()).collect();
    let mut out = Vec::new();
    for path in paths {
        let before = start.get(path).map(String::as_str);
        let after = current.get(path).map(String::as_str);
        if before == after {
            continue;
        }
        match ontology_comparison::compare(before, after) {
            Ok(comparison) if comparison.changes.is_empty() => {}
            Ok(comparison) => out.push(CardFile {
                changes: comparison
                    .changes
                    .iter()
                    .map(|change| CardChange {
                        id: change.id.clone(),
                        change: match change.change {
                            StatementChangeKind::Added => "added",
                            StatementChangeKind::Modified => "modified",
                            StatementChangeKind::Deleted => "deleted",
                        }
                        .into(),
                        before: change.before.as_ref().map(card_statement),
                        after: change.after.as_ref().map(card_statement),
                    })
                    .collect(),
                ..file(path)
            }),
            Err(refusal) => out.push(CardFile {
                refusal: Some(refusal.to_string()),
                ..file(path)
            }),
        }
    }
    out
}

/// The card event's bound, below the dispatcher journal's 1 MiB event limit.
pub const MAX_CARD_BYTES: usize = 768 * 1024;
/// At most this long before the provider starts.
const BEGIN_TIMEOUT: Duration = Duration::from_secs(45);
/// Building the card never holds up the turn's outcome for longer.
const CARD_TIMEOUT: Duration = Duration::from_secs(45);
/// The platform keeps a turn's start up to 6 MiB of JSON; this leaves room
/// for the request around it.
const MAX_TOTAL_BASELINE_BYTES: usize = 6 * 1024 * 1024 - 64 * 1024;

/// A workspace chat turn's ontology record, kept with the turn by the API.
pub struct TurnOntology {
    client: g5_internal_api::InternalApiClient,
    workspace_id: String,
    task_id: String,
    members: Vec<Member>,
    baseline: OntologyBaselineV1,
}

fn turn_path(task_id: &str, rest: &str) -> String {
    format!("chat-tasks/{task_id}/ontology{rest}")
}

impl TurnOntology {
    /// The turn's start: the one an earlier attempt recorded, else the
    /// ontology as it stands now, recorded before the provider starts. `None`
    /// when the platform cannot keep it, so the turn has no card.
    pub async fn begin(
        client: &g5_internal_api::InternalApiClient,
        workspace_id: &str,
        task_id: &str,
        attempt: u32,
        members: Vec<Member>,
    ) -> Option<Self> {
        if members.is_empty() {
            return None;
        }
        let work = async {
            let current = client
                .workspace_agent_get(workspace_id, &turn_path(task_id, ""))
                .await
                .map_err(|e| e.to_string())?;
            let stored = &current["data"]["baseline"];
            let baseline = if stored.is_null() {
                let mut baseline = OntologyBaselineV1::default();
                for member in &members {
                    baseline.members.push(capture_member(member).await);
                }
                // Within what the platform keeps: drop the largest members'
                // starts to their digests until the whole start fits.
                while baseline.encoded_len() > MAX_TOTAL_BASELINE_BYTES {
                    let Some(index) = baseline
                        .members
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| m.unavailable.is_none())
                        .max_by_key(|(_, m)| serde_json::to_vec(m).map_or(0, |b| b.len()))
                        .map(|(index, _)| index)
                    else {
                        break;
                    };
                    baseline.members[index] = unavailable_member(
                        &members[index],
                        "the workspace's uncommitted and unpushed ontology changes are too large"
                            .into(),
                    )
                    .await;
                }
                let recorded = client
                    .workspace_agent_post(
                        workspace_id,
                        &turn_path(task_id, "/baseline"),
                        &serde_json::json!({"attempt": attempt, "baseline": baseline}),
                        true,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                // Another attempt may have recorded first; the first one stands.
                serde_json::from_value(recorded["data"]["baseline"].clone())
                    .map_err(|e| e.to_string())?
            } else {
                serde_json::from_value(stored.clone()).map_err(|e| e.to_string())?
            };
            Ok::<_, String>(baseline)
        };
        match tokio::time::timeout(BEGIN_TIMEOUT, work).await {
            Ok(Ok(baseline)) => Some(Self {
                client: client.clone(),
                workspace_id: workspace_id.to_owned(),
                task_id: task_id.to_owned(),
                members,
                baseline,
            }),
            Ok(Err(error)) => {
                eprintln!("ontology changes card unavailable for this turn: {error}");
                None
            }
            Err(_) => {
                eprintln!("ontology changes card unavailable for this turn: start timed out");
                None
            }
        }
    }

    /// The card of a successful attempt, or `None` when no statement changed.
    /// A card over `MAX_CARD_BYTES` is replaced by its too-large marker.
    pub async fn card(&self) -> Option<serde_json::Value> {
        match tokio::time::timeout(CARD_TIMEOUT, self.build_card()).await {
            Ok(card) => card,
            Err(_) => {
                eprintln!("ontology changes card skipped: building it timed out");
                None
            }
        }
    }

    async fn build_card(&self) -> Option<serde_json::Value> {
        let mut files = Vec::new();
        let mut current_by_member = Vec::new();
        for member in &self.members {
            let Some(start) = self
                .baseline
                .members
                .iter()
                .find(|m| m.repo_id == member.repo_id)
            else {
                continue;
            };
            let current = current_files(member).await;
            if start.unavailable.is_some() {
                if let Ok(current) = &current {
                    files.extend(refuse_changed(member, start, current));
                }
            } else {
                let start_files = start_files(member, start).await;
                files.extend(compare_member(
                    member,
                    start_files.as_ref().map_err(String::as_str),
                    current.as_ref().map_err(String::as_str),
                ));
            }
            current_by_member.push((member, current.unwrap_or_default()));
        }
        if files.is_empty() {
            return None;
        }
        let governing = self.governing(&current_by_member).await;
        let card = serde_json::json!({
            "schema": "g5.ontology-card.v1",
            "files": files,
            "governing": governing,
        });
        if serde_json::to_vec(&card).map_or(usize::MAX, |b| b.len()) <= MAX_CARD_BYTES {
            Some(card)
        } else {
            Some(serde_json::json!({"schema": "g5.ontology-card.v1", "tooLarge": true}))
        }
    }

    /// The governing statements the agent recorded, resolved in the files as
    /// they stand; a name that cannot be found keeps no statement.
    async fn governing(
        &self,
        current: &[(&Member, BTreeMap<String, String>)],
    ) -> Vec<serde_json::Value> {
        let listed = match self
            .client
            .workspace_agent_get(&self.workspace_id, &turn_path(&self.task_id, ""))
            .await
        {
            Ok(value) => value["data"]["governing"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            Err(error) => {
                eprintln!("governing statements unavailable: {error}");
                return Vec::new();
            }
        };
        listed
            .into_iter()
            .map(|entry| {
                let id = entry["statementId"].as_str().unwrap_or_default();
                let repo_id = entry["repoId"].as_str().unwrap_or_default();
                let path = entry["path"].as_str().unwrap_or_default();
                let statement = current
                    .iter()
                    .find(|(m, _)| m.repo_id == repo_id)
                    .and_then(|(_, files)| files.get(path))
                    .and_then(|text| ontology_comparison::parse_version(text).ok())
                    .and_then(|all| all.into_iter().find(|s| s.id == id))
                    .map(|s| card_statement(&s));
                serde_json::json!({
                    "id": id,
                    "repoId": repo_id,
                    "path": path,
                    "reason": entry["reason"],
                    "statement": statement,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn applies_zero_context_patches_exactly() {
        let mut files = map(&[(".g5/ontology/a.g5.md", "one\ntwo\nthree\n")]);
        let patch = "diff --git a/.g5/ontology/a.g5.md b/.g5/ontology/a.g5.md\nindex 1..2 100644\n--- a/.g5/ontology/a.g5.md\n+++ b/.g5/ontology/a.g5.md\n@@ -2 +2 @@\n-two\n+TWO\n@@ -3,0 +4,2 @@\n+four\n+five\n";
        apply_patch(&mut files, patch).unwrap();
        assert_eq!(
            files[".g5/ontology/a.g5.md"],
            "one\nTWO\nthree\nfour\nfive\n"
        );
    }

    #[test]
    fn refuses_a_patch_that_does_not_match_its_base() {
        let mut files = map(&[(".g5/ontology/a.g5.md", "one\nother\n")]);
        let patch = "diff --git a/.g5/ontology/a.g5.md b/.g5/ontology/a.g5.md\n--- a/.g5/ontology/a.g5.md\n+++ b/.g5/ontology/a.g5.md\n@@ -2 +2 @@\n-two\n+TWO\n";
        assert!(apply_patch(&mut files, patch).is_err());
    }

    #[test]
    fn applies_additions_deletions_and_missing_final_newlines() {
        let mut files = map(&[
            (".g5/ontology/gone.g5.md", "x\n"),
            (".g5/ontology/tail.g5.md", "a\nb"),
        ]);
        let patch = concat!(
            "diff --git a/.g5/ontology/gone.g5.md b/.g5/ontology/gone.g5.md\ndeleted file mode 100644\n--- a/.g5/ontology/gone.g5.md\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n",
            "diff --git a/.g5/ontology/new.g5.md b/.g5/ontology/new.g5.md\nnew file mode 100644\n--- /dev/null\n+++ b/.g5/ontology/new.g5.md\n@@ -0,0 +1,2 @@\n+n1\n+n2\n\\ No newline at end of file\n",
            "diff --git a/.g5/ontology/tail.g5.md b/.g5/ontology/tail.g5.md\n--- a/.g5/ontology/tail.g5.md\n+++ b/.g5/ontology/tail.g5.md\n@@ -2 +2,2 @@\n-b\n\\ No newline at end of file\n+b\n+c\n",
        );
        apply_patch(&mut files, patch).unwrap();
        assert!(!files.contains_key(".g5/ontology/gone.g5.md"));
        assert_eq!(files[".g5/ontology/new.g5.md"], "n1\nn2");
        assert_eq!(files[".g5/ontology/tail.g5.md"], "a\nb\nc\n");
    }

    #[test]
    fn unquotes_c_style_paths() {
        assert_eq!(unquote("\"a/x\\303\\251.g5.md\""), "a/xé.g5.md");
        assert_eq!(unquote("a/plain"), "a/plain");
    }

    fn sh(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn write(root: &Path, path: &str, text: &str) {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    fn statement(id: &str, body: &str) -> String {
        format!("<a id=\"{id}\"></a>\n[I · {id}]\n\n{body}\n\n")
    }

    fn page(statements: &[(&str, &str)]) -> String {
        let mut text = String::from("# Page\n\n");
        for (id, body) in statements {
            text.push_str(&statement(id, body));
        }
        text.push_str("## Metadata\n\n");
        for (id, _) in statements {
            text.push_str(&format!("### {id}\n\n- agent\n\n"));
        }
        text
    }

    /// A pushed checkout: `origin` is a bare repository and the branch tracks it.
    fn checkout() -> (tempfile::TempDir, Member) {
        let dir = tempfile::tempdir().unwrap();
        let remote = dir.path().join("remote.git");
        let root = dir.path().join("code");
        std::fs::create_dir_all(&root).unwrap();
        sh(
            dir.path(),
            &["init", "-q", "--bare", remote.to_str().unwrap()],
        );
        sh(&root, &["init", "-q", "-b", "main"]);
        write(
            &root,
            ".g5/ontology/a.g5.md",
            &page(&[("x.a", "Alpha."), ("x.b", "Beta.")]),
        );
        write(
            &root,
            ".g5/ontology/sub/c.g5.md",
            &page(&[("x.c", "Gamma.")]),
        );
        write(&root, "src/code.rs", "fn main() {}\n");
        sh(&root, &["add", "-A"]);
        sh(&root, &["commit", "-q", "-m", "init"]);
        sh(
            &root,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        sh(&root, &["push", "-q", "origin", "main"]);
        sh(&root, &["fetch", "-q", "origin"]);
        let member = Member {
            repo_id: "code-id".into(),
            root_path: "code".into(),
            root,
            git_url: Some(remote.to_str().unwrap().to_owned()),
        };
        (dir, member)
    }

    fn git_state(root: &Path) -> (String, String, Vec<u8>) {
        (
            sh(root, &["for-each-ref"]),
            sh(root, &["stash", "list"]),
            std::fs::read(root.join(".git/index")).unwrap(),
        )
    }

    #[tokio::test]
    async fn a_pushed_clean_checkout_records_only_its_commit() {
        let (_dir, member) = checkout();
        let baseline = capture_member(&member).await;
        assert_eq!(
            baseline.base_commit.as_deref(),
            Some(sh(&member.root, &["rev-parse", "HEAD"]).as_str())
        );
        assert!(
            baseline.patch.is_empty()
                && baseline.files.is_empty()
                && baseline.unavailable.is_none()
        );
        let start = start_files(&member, &baseline).await.unwrap();
        assert_eq!(start, current_files(&member).await.unwrap());
    }

    #[tokio::test]
    async fn a_touched_but_unchanged_file_leaves_the_index_unwritten() {
        let (_dir, member) = checkout();
        let file = member.root.join(".g5/ontology/a.g5.md");
        // Stat-dirty: same content, a later modification time.
        let later = std::time::SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(later)
            .unwrap();
        let before = git_state(&member.root);
        let baseline = capture_member(&member).await;
        assert!(baseline.patch.is_empty() && baseline.unavailable.is_none());
        assert_eq!(git_state(&member.root), before);
    }

    #[tokio::test]
    async fn a_start_that_cannot_be_captured_keeps_digests_so_an_unchanged_turn_has_no_card() {
        let (_dir, member) = checkout();
        let unavailable = unavailable_member(&member, "test".into()).await;
        let current = current_files(&member).await.unwrap();
        assert_eq!(unavailable.digests.len(), current.len());
        assert!(refuse_changed(&member, &unavailable, &current).is_empty());
    }

    #[tokio::test]
    async fn every_kind_of_difference_is_rebuilt_exactly_without_touching_git_state() {
        let (_dir, member) = checkout();
        let root = member.root.clone();
        // An unpushed commit changing the ontology.
        write(
            &root,
            ".g5/ontology/a.g5.md",
            &page(&[("x.a", "Alpha, committed."), ("x.b", "Beta.")]),
        );
        sh(&root, &["commit", "-q", "-am", "unpushed"]);
        // Staged, unstaged, deleted, renamed and untracked files.
        write(
            &root,
            ".g5/ontology/staged.g5.md",
            &page(&[("x.s", "Staged.")]),
        );
        sh(&root, &["add", ".g5/ontology/staged.g5.md"]);
        write(
            &root,
            ".g5/ontology/a.g5.md",
            &page(&[("x.a", "Alpha, unstaged."), ("x.b", "Beta.")]),
        );
        sh(
            &root,
            &[
                "mv",
                ".g5/ontology/sub/c.g5.md",
                ".g5/ontology/sub/renamed.g5.md",
            ],
        );
        write(
            &root,
            ".g5/ontology/new.g5.md",
            &page(&[("x.n", "Untracked.")]),
        );
        write(
            &root,
            ".g5/ontology/no-newline.g5.md",
            "<a id=\"x.t\"></a>\n[I · x.t]\n\nTail.",
        );
        let before = git_state(&root);
        let baseline = capture_member(&member).await;
        assert_eq!(git_state(&root), before, "capture writes no Git state");
        let expected = current_files(&member).await.unwrap();
        assert_eq!(start_files(&member, &baseline).await.unwrap(), expected);
        // The base is the pushed commit, not the unpushed HEAD.
        assert_eq!(
            baseline.base_commit.as_deref(),
            Some(sh(&root, &["rev-parse", "origin/main"]).as_str())
        );
        // The turn then commits, resets and switches branches; the start stays.
        sh(&root, &["add", "-A"]);
        sh(&root, &["commit", "-q", "-m", "during the turn"]);
        sh(&root, &["checkout", "-q", "-b", "other"]);
        write(
            &root,
            ".g5/ontology/a.g5.md",
            &page(&[("x.a", "Changed in the turn."), ("x.b", "Beta.")]),
        );
        assert_eq!(start_files(&member, &baseline).await.unwrap(), expected);
        let files = compare_member(
            &member,
            Ok(&expected),
            Ok(&current_files(&member).await.unwrap()),
        );
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, ".g5/ontology/a.g5.md");
        assert_eq!(files[0].changes[0].id, "x.a");
        assert_eq!(files[0].changes[0].change, "modified");
        assert!(files[0].changes[0]
            .before
            .as_ref()
            .unwrap()
            .source
            .contains("Alpha, unstaged."));
    }

    #[tokio::test]
    async fn a_start_commit_lost_locally_is_fetched_from_the_server() {
        let (_dir, member) = checkout();
        let root = member.root.clone();
        write(
            &root,
            ".g5/ontology/a.g5.md",
            &page(&[("x.a", "Pushed later."), ("x.b", "Beta.")]),
        );
        sh(&root, &["commit", "-q", "-am", "second"]);
        sh(&root, &["push", "-q", "origin", "main"]);
        sh(&root, &["fetch", "-q", "origin"]);
        let baseline = capture_member(&member).await;
        let expected = current_files(&member).await.unwrap();
        // Reset away and prune: the start commit is gone from this checkout.
        sh(&root, &["reset", "-q", "--hard", "HEAD~1"]);
        sh(&root, &["update-ref", "-d", "refs/remotes/origin/main"]);
        sh(&root, &["reflog", "expire", "--expire=now", "--all"]);
        sh(&root, &["gc", "-q", "--prune=now"]);
        let base = baseline.base_commit.clone().unwrap();
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["cat-file", "-e", &base])
                .status()
                .unwrap()
                .code()
                != Some(0)
        );
        assert_eq!(start_files(&member, &baseline).await.unwrap(), expected);
    }

    #[tokio::test]
    async fn an_unborn_repository_records_full_text() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("code");
        std::fs::create_dir_all(&root).unwrap();
        sh(&root, &["init", "-q"]);
        write(&root, ".g5/ontology/a.g5.md", &page(&[("x.a", "Alpha.")]));
        let member = Member {
            repo_id: "r".into(),
            root_path: "code".into(),
            root,
            git_url: None,
        };
        let baseline = capture_member(&member).await;
        assert!(baseline.base_commit.is_none());
        assert_eq!(baseline.files.len(), 1);
        assert_eq!(
            start_files(&member, &baseline).await.unwrap(),
            current_files(&member).await.unwrap()
        );
    }

    #[test]
    fn metadata_only_and_unparseable_files_are_handled_per_file() {
        let member = Member {
            repo_id: "r".into(),
            root_path: "code".into(),
            root: PathBuf::from("/nonexistent"),
            git_url: None,
        };
        let start = map(&[
            (".g5/ontology/a.g5.md", &page(&[("x.a", "Alpha.")])),
            (".g5/ontology/b.g5.md", &page(&[("x.b", "Beta.")])),
        ]);
        let mut meta = page(&[("x.a", "Alpha.")]);
        meta.push_str("- Source: a new note.\n");
        let current = map(&[
            (".g5/ontology/a.g5.md", &meta),
            (
                ".g5/ontology/b.g5.md",
                "<a id=\"x.b\"></a>\n[I · x.b]\n\n```\nunclosed\n",
            ),
        ]);
        let files = compare_member(&member, Ok(&start), Ok(&current));
        assert_eq!(
            files.len(),
            1,
            "a metadata-only change is no statement change"
        );
        assert_eq!(files[0].path, ".g5/ontology/b.g5.md");
        assert!(files[0].refusal.is_some());
    }

    #[test]
    fn an_unrecorded_start_refuses_only_files_the_turn_changed() {
        let member = Member {
            repo_id: "r".into(),
            root_path: "code".into(),
            root: PathBuf::from("/nonexistent"),
            git_url: None,
        };
        let current = map(&[
            (".g5/ontology/a.g5.md", "same"),
            (".g5/ontology/b.g5.md", "changed"),
            (".g5/ontology/new.g5.md", "new"),
        ]);
        let baseline = MemberBaseline {
            repo_id: "r".into(),
            unavailable: Some("too large".into()),
            digests: [
                (".g5/ontology/a.g5.md".to_owned(), digest("same")),
                (".g5/ontology/b.g5.md".to_owned(), digest("before")),
                (".g5/ontology/gone.g5.md".to_owned(), digest("gone")),
            ]
            .into(),
            ..MemberBaseline::default()
        };
        let refused: Vec<String> = refuse_changed(&member, &baseline, &current)
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(
            refused,
            [
                ".g5/ontology/b.g5.md",
                ".g5/ontology/gone.g5.md",
                ".g5/ontology/new.g5.md"
            ]
        );
        let unchanged = map(&[
            (".g5/ontology/a.g5.md", "same"),
            (".g5/ontology/b.g5.md", "before"),
            (".g5/ontology/gone.g5.md", "gone"),
        ]);
        assert!(refuse_changed(&member, &baseline, &unchanged).is_empty());
    }

    #[test]
    fn reads_only_safe_code_members() {
        let members = code_members(
            Path::new("/work"),
            r#"[{"repo_id":"c","root_path":"code","role":"code","git_url":"https://g5.test/git/c"},{"repo_id":"d","root_path":"docs","role":"docs"},{"repo_id":"x","root_path":"../x","role":"code"},{"repo_id":"o","root_path":"older"}]"#,
        );
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].root, PathBuf::from("/work/code"));
        assert_eq!(members[0].git_url.as_deref(), Some("https://g5.test/git/c"));
        // An older dispatcher names no URL: a lost start commit is not refetched.
        assert_eq!(members[1].git_url, None);
    }
}
