//! Static audit for `g5.repo.storage-guard`: only the repository store's
//! holder (`TenantRepoStorage` in `services/git/repo_store.rs`) reaches the
//! raw store, so every repository path is keyed for an organization the caller
//! is scoped to. No other production or test source may name the raw store,
//! its constructors or its path methods, build the store from configuration,
//! or assemble a repository path under the storage base by hand.

use std::{
    fs,
    path::{Path, PathBuf},
};

use regex::Regex;
use walkdir::WalkDir;

/// Files allowed to reach the raw store: the store's own crate, its holder,
/// and this audit.
const ALLOWED: &[&str] = &[
    "g5-storage/",
    "g5-api-core/src/services/git/repo_store.rs",
    "g5-api/tests/repo_storage_tenancy_guard.rs",
];

/// The storage crate's items other modules may name: its configuration and
/// the plain values its holder returns. The raw store, its implementations
/// and `RepoKey` stay behind the holder.
const NAMEABLE_STORAGE_ITEMS: &[&str] = &[
    "StorageConfig",
    "StorageError",
    "StorageEntry",
    "ByteStream",
];

/// The attachment store is a separate store with its own owner and
/// organization check (`g5.issue.native.attachment-store`); only that owner
/// and the state that composes it name its items.
const ATTACHMENT_STORE_ITEMS: &[&str] = &[
    "AttachmentKey",
    "AttachmentStore",
    "DirectoryAttachmentStore",
    "StoredObject",
];
const ATTACHMENT_STORE_OWNERS: &[&str] = &[
    "g5-api-core/src/services/git/native_issues/attachments.rs",
    "g5-api-core/src/services/mod.rs",
];

/// What no other file may contain, with the reason the audit reports. Rules
/// run over the whole comment-stripped file, so a statement or `use` list
/// split across lines is still matched.
fn rules() -> Vec<(Regex, &'static str)> {
    [
        (
            r"(^|[^A-Za-z0-9_])(RepoStorage|LocalRepoStorage|FilestoreRepoStorage)([^A-Za-z0-9_]|$)",
            "names the raw repository store; reach it through TenantRepoStorage",
        ),
        (
            r"(^|[^A-Za-z0-9_])g5_storage\s+as\s",
            "aliases the storage crate, which hides what it names",
        ),
        (
            r"\.(local_path|review_artifact_dir)\s*\(",
            "calls a raw store path method; use TenantRepoStorage::path or artifact_dir",
        ),
        (
            r"(storage\s*|StorageConfig\b[^;{]*?)\.\s*build\s*\(",
            "builds the raw store from configuration; use TenantRepoStorage::from_config",
        ),
        (
            r#"\.join\(\s*&?\s*(repo_id|repo\.id|row\.repo_id|member\.repo_id)\b|\.join\(\s*&?\s*key\.repo_id\(\)|\.join\(\s*format!\(\s*"\{repo_id\}"#,
            "assembles a repository path by hand; ask the store for the keyed path",
        ),
        (
            r"G5_STORAGE_(BASE_DIR|MOUNT_POINT|TENANT_PREFIX)|StorageConfig\s*::\s*from_env",
            "reads the storage base directly; only configuration and the store may",
        ),
    ]
    .into_iter()
    .map(|(pattern, reason)| (Regex::new(pattern).unwrap(), reason))
    .collect()
}

/// Configuration parsing may read the storage environment; nothing else may.
const STORAGE_ENV_OWNERS: &[&str] = &["g5-api-foundation/src/config.rs"];

/// Verified machine credentials vouch for their organization
/// (`OrgScope`); only the token crate may make one. Test sources mint them
/// to sign fixtures' tokens.
const CREDENTIAL_OWNERS: &[&str] = &["g5-auth/"];

fn backend_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api crate must be beneath backend")
        .to_path_buf()
}

/// `source` with `//` comments blanked, keeping offsets and line numbers. A
/// `//` inside a string literal (such as `https://`) is not a comment.
fn strip_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let mut in_string = false;
            let mut previous = '\0';
            for (index, ch) in line.char_indices() {
                match ch {
                    '"' if previous != '\\' => in_string = !in_string,
                    '/' if !in_string && previous == '/' => {
                        let start = index - 1;
                        return format!("{}{}", &line[..start], " ".repeat(line.len() - start));
                    }
                    _ => {}
                }
                previous = ch;
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every violation in `source`, as `line: reason`, ignoring comments.
fn violations(relative: &str, source: &str) -> Vec<String> {
    if ALLOWED.iter().any(|allowed| relative.starts_with(allowed)) {
        return Vec::new();
    }
    let code = strip_comments(source);
    let line_of = |offset: usize| code[..offset].matches('\n').count();
    let report = |offset: usize, reason: &str| {
        let line = line_of(offset);
        format!(
            "{relative}:{}: {reason}: {}",
            line + 1,
            source.lines().nth(line).unwrap_or_default().trim()
        )
    };
    let mut found = Vec::new();
    for (rule, reason) in &rules() {
        if reason.starts_with("reads the storage base")
            && STORAGE_ENV_OWNERS
                .iter()
                .any(|owner| relative.starts_with(owner))
        {
            continue;
        }
        for hit in rule.find_iter(&code) {
            found.push(report(hit.start(), reason));
        }
    }
    // Every item named through the crate path, including `use` lists and
    // globs, must be one other modules may name.
    let paths = Regex::new(r"g5_storage\s*::\s*(\{[^}]*\}|[A-Za-z_*][A-Za-z0-9_]*)").unwrap();
    let item = Regex::new(r"[A-Za-z_*][A-Za-z0-9_]*").unwrap();
    let attachment_owner = ATTACHMENT_STORE_OWNERS.contains(&relative);
    for hit in paths.captures_iter(&code) {
        let named = hit.get(1).unwrap();
        let refused = item
            .find_iter(named.as_str())
            .map(|name| name.as_str())
            .filter(|name| *name != "self")
            .any(|name| {
                !NAMEABLE_STORAGE_ITEMS.contains(&name)
                    && !(attachment_owner && ATTACHMENT_STORE_ITEMS.contains(&name))
            });
        if refused {
            found.push(report(
                hit.get(0).unwrap().start(),
                "names a storage item other than its configuration and values; keys and the raw store stay behind TenantRepoStorage",
            ));
        }
    }
    if !relative.contains("/tests/")
        && !CREDENTIAL_OWNERS
            .iter()
            .any(|owner| relative.starts_with(owner))
    {
        let credential =
            Regex::new(r"(^|[^A-Za-z0-9_])(WorkspaceIncarnationClaims|HostSourceClaims)\s*\{")
                .unwrap();
        for hit in credential.find_iter(&code) {
            // `struct`/`impl` headers and return types name the type without
            // making one.
            let before = code[..hit.start()].trim_end();
            if ["struct", "for", "impl", "->"]
                .iter()
                .any(|header| before.ends_with(header))
            {
                continue;
            }
            found.push(report(
                hit.start(),
                "makes a machine credential outside the token crate; only a verified credential may vouch for an organization",
            ));
        }
    }
    found
}

#[test]
fn only_the_holder_reaches_repository_storage() {
    let backend = backend_dir();
    let mut found = Vec::new();
    let mut scanned = 0;
    for entry in WalkDir::new(&backend)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            name != "target" && name != "node_modules" && !name.starts_with('.')
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
    {
        let relative = entry
            .path()
            .strip_prefix(&backend)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let source = fs::read_to_string(entry.path()).unwrap();
        scanned += 1;
        found.extend(violations(&relative, &source));
    }
    assert!(
        scanned > 500,
        "the audit scanned the backend ({scanned} files)"
    );
    assert!(
        found.is_empty(),
        "repository storage is reached outside its holder:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_audit_catches_raw_store_access() {
    for planted in [
        "let path = state.raw.local_path(&repo_id.to_string());",
        "let dir = storage.review_artifact_dir(&key);",
        "fn f(store: &dyn RepoStorage) {}",
        "use g5_storage::{LocalRepoStorage, StorageConfig};",
        "let key = g5_storage::RepoKey { org_id, repo_id };",
        "let store = config.storage.build()?;",
        "let store = g5_storage::StorageConfig::local(dir).build()?;",
        "let path = base.join(repo_id.to_string());",
        r#"let path = base.join(format!("{repo_id}"));"#,
        r#"let base = std::env::var("G5_STORAGE_BASE_DIR")?;"#,
        "let store = g5_storage::StorageConfig::from_env()?.build()?;",
        "let store = StorageConfig::local(PathBuf::from(dir)).build()?;",
        "use g5_storage::{\n    RepoKey,\n    StorageConfig,\n};",
        "use g5_storage::*;",
        "use g5_storage as gs;",
        "let path = base.join(key.repo_id().to_string());",
        r#"let url = "https://example.test"; let path = state.raw.local_path(&key);"#,
        "let claims = g5_auth::HostSourceClaims { org_id, ..claims };",
        "use g5_storage::{AttachmentKey, AttachmentStore};",
    ] {
        assert!(
            !violations("g5-api-routes/src/routes/planted.rs", planted).is_empty(),
            "not caught: {planted}"
        );
    }
    for allowed in [
        "let path = state.repo_storage.repo_path(&db, org_id, repo_id)?;",
        "let repos = state.repo_storage.scoped(&db);",
        "let storage: TenantRepoStorage = TenantRepoStorage::from_config(&config.storage)?;",
        "let scoped: &ScopedRepoStorage = &repos;",
        "// a comment may mention RepoStorage and .local_path(",
        "let dir = repo_storage.artifact_dir(&key);",
        "use g5_storage::{StorageConfig, StorageEntry};",
        "fn f(entry: g5_storage::StorageEntry) {}",
        "impl OrgScope for HostSourceClaims {}",
    ] {
        assert!(
            violations("g5-api-routes/src/routes/planted.rs", allowed).is_empty(),
            "wrongly refused: {allowed}"
        );
    }
    assert!(violations(
        "g5-api-core/src/services/git/native_issues/attachments.rs",
        "use g5_storage::{AttachmentKey, AttachmentStore, StorageError, StoredObject};"
    )
    .is_empty());
    assert!(!violations(
        "g5-api-core/src/services/git/native_issues/attachments.rs",
        "use g5_storage::RepoKey;"
    )
    .is_empty());
    assert!(violations(
        "g5-api-core/src/services/git/repo_store.rs",
        "self.backend.local_path(&key.0)"
    )
    .is_empty());
}
