use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeUseCategory {
    Legitimate,
    KnownDefect,
}

#[derive(Debug, Clone, Copy)]
struct ExpectedLegacyUse {
    path: &'static str,
    function: &'static str,
    needle: &'static str,
    expected_count: usize,
    category: ScopeUseCategory,
    reason: &'static str,
    removal: &'static str,
}

#[derive(Debug, Clone)]
struct Finding {
    path: String,
    function: String,
    needle: &'static str,
    line: usize,
    line_text: String,
}

const LEGACY_SCOPE_NEEDLES: &[&str] = &[
    "resolve_org_id(&auth)",
    "resolve_org_id(auth)",
    "auth.org_uuid()",
    "self.org_uuid()",
    "auth.selected_org",
    "claims.org",
    "claims.org_id",
];

macro_rules! legacy_use {
    ($category:expr, $path:literal, $function:literal, $needle:literal, $expected_count:literal, $reason:literal, $removal:literal) => {
        ExpectedLegacyUse {
            path: $path,
            function: $function,
            needle: $needle,
            expected_count: $expected_count,
            category: $category,
            reason: $reason,
            removal: $removal,
        }
    };
}

const LEGITIMATE_SELECTED_ORG_USES: &[ExpectedLegacyUse] = &[
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/mod.rs",
        "ws_adapter",
        "claims.org",
        1,
        "The event socket retains the validated ticket's organization for membership checks and revocation; resource subscriptions independently resolve their target organization.",
        "Remove if event connections no longer retain a selected organization or get it from another authenticated credential."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/git.rs",
        "require_read",
        "claims.org_id",
        2,
        "After a host-source grant or workspace incarnation passed its own read authorization for this repository, its signed org names the repository's storage directory (g5.repo.storage-tenancy); a repository of another org resolves to a directory that does not exist.",
        "Remove if Git reads resolve storage from a replacement authenticated execution identity."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/git.rs",
        "require_incarnation_ref_updates",
        "claims.org_id",
        3,
        "The signed workspace-incarnation org scopes immutable flush lookup and is copied only after current_incarnation_repo_target cross-checks it against authoritative workspace state.",
        "Remove if flush receipts and push targets derive their tenant from a replacement authenticated execution identity."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/internal.rs",
        "scoped_db_for_workspace_incarnation",
        "claims.org_id",
        1,
        "The service scope is opened from the signed workspace-incarnation org and immediately fenced against the target workspace, owner, VM and epoch.",
        "Remove if incarnation-scoped database capabilities are minted from another authenticated execution identity."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/internal.rs",
        "save_source_control_internal",
        "claims.org_id",
        1,
        "The post-Save checkpoint capability uses the signed incarnation org, locks the workspace before its share fence, then revalidates the exact workspace/owner/VM/epoch before advancing the checkpoint.",
        "Remove if Save checkpoint publication no longer uses the signed workspace-incarnation capability."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/internal.rs",
        "resolve_internal_issue_actor",
        "claims.org_id",
        1,
        "The returned org is from the signed workspace-incarnation identity after scoped_db_for_workspace_incarnation validates it against the requested workspace.",
        "Remove if internal platform issue intake no longer uses workspace-incarnation authentication."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/telemetry.rs",
        "ingest_ui_events",
        "claims.org",
        1,
        "UI-telemetry audit events are labeled with the org the user was acting in — the selected JWT org — by design (issue #1552, routes/telemetry [D3]); events describe browser activity, there is no target entity to derive scope from, and the value only becomes a g5_audit log field (no DB access is scoped by it).",
        "Remove if UI events ever gain an entity-derived org scope or start gating persistence/DB access."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/permissions.rs",
        "get_org_default_scopes",
        "resolve_org_id(&auth)",
        1,
        "Org default scopes are an explicit current-org administration surface and reject mismatched path orgs.",
        "Remove if org administration routes derive authorization from the path org instead of active org."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/permissions.rs",
        "set_org_default_scopes",
        "resolve_org_id(&auth)",
        1,
        "Org default-scope updates are an explicit current-org administration surface and reject mismatched path orgs.",
        "Remove if org administration routes derive authorization from the path org instead of active org."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/permissions.rs",
        "get_user_scopes",
        "resolve_org_id(&auth)",
        1,
        "Org member scope reads are an explicit current-org administration surface and reject mismatched path orgs.",
        "Remove if org administration routes derive authorization from the path org instead of active org."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/permissions.rs",
        "set_user_scopes",
        "resolve_org_id(&auth)",
        1,
        "Org member scope updates are an explicit current-org administration surface and reject mismatched path orgs.",
        "Remove if org administration routes derive authorization from the path org instead of active org."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/routes/permissions.rs",
        "scoped_system_db_for_effective_scopes",
        "resolve_org_id(auth)",
        1,
        "Self-scope endpoints fall back to the current selected org only when the request has no workspace_id or other target entity.",
        "Remove if all self-scope calls become workspace/path/entity-scoped."
    ),
    legacy_use!(
        ScopeUseCategory::Legitimate,
        "src/ws/channels.rs",
        "authorize_subscribe",
        "claims.org",
        2,
        "Org and user WebSocket channels check current membership in the ticket's selected organization; workspace, job, task and chat channels derive authorization from the target entity.",
        "Remove if org and user channel admission no longer binds to the ticket's selected organization."
    ),
];

const KNOWN_SELECTED_ORG_DEFECTS: &[ExpectedLegacyUse] = &[];

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).expect("read source directory");
    for entry in entries {
        let entry = entry.expect("read source entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path
            .extension()
            .is_some_and(|ext| ext == std::ffi::OsStr::new("rs"))
        {
            files.push(path);
        }
    }
}

fn collect_scanned_files(workspace_root: &Path) -> Vec<(PathBuf, PathBuf)> {
    let routes_root = workspace_root.join("g5-api-routes");
    let core_root = workspace_root.join("g5-api-core");
    let mut files = Vec::new();

    let mut route_files = Vec::new();
    collect_rust_files(&routes_root.join("src/routes"), &mut route_files);
    files.extend(
        route_files
            .into_iter()
            .map(|path| (routes_root.clone(), path)),
    );

    let mut websocket_files = Vec::new();
    collect_rust_files(&core_root.join("src/ws"), &mut websocket_files);
    files.extend(
        websocket_files
            .into_iter()
            .map(|path| (core_root.clone(), path)),
    );
    files.sort_by(|left, right| left.1.cmp(&right.1));
    files
}

fn relative_path(crate_root: &Path, path: &Path) -> String {
    path.strip_prefix(crate_root)
        .expect("scanned file should live under crate root")
        .to_string_lossy()
        .replace('\\', "/")
}

fn parse_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let fn_idx = trimmed.find("fn ")?;
    let prefix = trimmed[..fn_idx].trim();
    if !prefix.split_whitespace().all(|part| {
        matches!(part, "pub" | "async" | "unsafe" | "const")
            || (part.starts_with("pub(") && part.ends_with(')'))
    }) {
        return None;
    }

    let name_start = fn_idx + "fn ".len();
    let name = trimmed[name_start..]
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();

    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn scan_file(crate_root: &Path, path: &Path) -> Vec<Finding> {
    let content = fs::read_to_string(path).expect("read Rust source file");
    let relative = relative_path(crate_root, path);
    let mut current_function = String::from("<module>");
    let mut findings = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        if let Some(function) = parse_fn_name(line) {
            current_function = function;
        }

        for &needle in LEGACY_SCOPE_NEEDLES {
            if line_contains_legacy_needle(line, needle) {
                findings.push(Finding {
                    path: relative.clone(),
                    function: current_function.clone(),
                    needle,
                    line: line_idx + 1,
                    line_text: line.trim().to_string(),
                });
            }
        }
    }

    findings
}

fn line_contains_legacy_needle(line: &str, needle: &str) -> bool {
    if needle != "claims.org" {
        return line.contains(needle);
    }

    line.match_indices(needle).any(|(index, _)| {
        let next_index = index + needle.len();
        line.as_bytes()
            .get(next_index)
            .is_none_or(|byte| *byte != b'_')
    })
}

fn scan_for_legacy_selected_org_uses(workspace_root: &Path) -> Vec<Finding> {
    collect_scanned_files(workspace_root)
        .iter()
        .flat_map(|(crate_root, path)| scan_file(crate_root, path))
        .collect()
}

fn all_expected_uses() -> impl Iterator<Item = &'static ExpectedLegacyUse> {
    LEGITIMATE_SELECTED_ORG_USES
        .iter()
        .chain(KNOWN_SELECTED_ORG_DEFECTS.iter())
}

fn expected_entry_for(finding: &Finding) -> Option<&'static ExpectedLegacyUse> {
    all_expected_uses().find(|entry| {
        entry.path == finding.path
            && entry.function == finding.function
            && entry.needle == finding.needle
    })
}

fn validate_expected_entry(entry: &ExpectedLegacyUse) -> Option<String> {
    let mut missing = Vec::new();
    if entry.reason.trim().is_empty() {
        missing.push("reason");
    }
    if entry.removal.trim().is_empty() {
        missing.push("removal");
    }
    if entry.expected_count == 0 {
        missing.push("expected_count");
    }
    if missing.is_empty() {
        None
    } else {
        Some(format!(
            "{}::{} `{}` has empty required fields: {}",
            entry.path,
            entry.function,
            entry.needle,
            missing.join(", ")
        ))
    }
}

fn count_matches(findings: &[Finding], entry: &ExpectedLegacyUse) -> usize {
    findings
        .iter()
        .filter(|finding| {
            entry.path == finding.path
                && entry.function == finding.function
                && entry.needle == finding.needle
        })
        .count()
}

fn render_findings(findings: &[&Finding]) -> String {
    findings
        .iter()
        .map(|finding| {
            format!(
                "{}:{} {} `{}`\n  {}",
                finding.path, finding.line, finding.function, finding.needle, finding.line_text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn selected_org_scope_legacy_uses_are_allowlisted() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api must live directly under the generation workspace");
    let findings = scan_for_legacy_selected_org_uses(workspace_root);

    let unexpected = findings
        .iter()
        .filter(|finding| expected_entry_for(finding).is_none())
        .collect::<Vec<_>>();

    let mut stale_or_invalid_entries = Vec::new();
    let mut expected_keys = HashSet::new();
    for entry in KNOWN_SELECTED_ORG_DEFECTS {
        if entry.category != ScopeUseCategory::KnownDefect {
            stale_or_invalid_entries.push(format!(
                "{}::{} `{}` is in the known-defect allow-list but is not categorized as KnownDefect",
                entry.path, entry.function, entry.needle
            ));
        }
    }
    for entry in all_expected_uses() {
        if !expected_keys.insert((entry.path, entry.function, entry.needle)) {
            stale_or_invalid_entries.push(format!(
                "{}::{} `{}` appears more than once in the allow-list",
                entry.path, entry.function, entry.needle
            ));
        }
        if let Some(message) = validate_expected_entry(entry) {
            stale_or_invalid_entries.push(message);
        }
        let count = count_matches(&findings, entry);
        if count != entry.expected_count {
            stale_or_invalid_entries.push(format!(
                "{:?} {}::{} `{}` expected {} match(es), found {}. Reason: {} Removal: {}",
                entry.category,
                entry.path,
                entry.function,
                entry.needle,
                entry.expected_count,
                count,
                entry.reason,
                entry.removal
            ));
        }
    }

    assert!(
        unexpected.is_empty() && stale_or_invalid_entries.is_empty(),
        "selected-org tenant scope sources must be explicitly classified. \
         Entity-owned routes should derive org/scope from the target entity, not from the selected JWT org.\n\n\
         Unexpected findings:\n{}\n\n\
         Stale or invalid allow-list entries:\n{}",
        render_findings(&unexpected),
        stale_or_invalid_entries.join("\n")
    );
}
