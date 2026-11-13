//! `g5d chat governing` — the agent names an unchanged ontology statement that
//! governs its turn's changes, with a one-line reason
//! (`g5.chat.ontology-card.governing`).
//!
//! The command runs in the agent's shell during a workspace chat turn. It reads
//! the turn from `G5D_TASK_ID`, which the runner sets in the agent's
//! environment, finds the statement in the workspace's checked-out ontology and
//! records its ID, file and reason on the running turn through the
//! incarnation-authenticated workspace-agent route. The server accepts it only
//! for a running agent task of this workspace on this VM; the turn's changes
//! card then shows the statement's text with the reason labelled as the
//! agent's.

use std::path::{Path, PathBuf};

use g5_contracts::error::G5Error;
use g5_semantic::ontology_comparison;
use serde::Deserialize;
use serde_json::json;

use super::{stable_context, ChatArgs, ChatCommand, ChatGoverningArgs, GlobalFlags};

pub async fn execute(args: &ChatArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    match &args.command {
        ChatCommand::Governing(governing) => record_governing(governing, global).await,
    }
}

fn env_uuid(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|id| id.trim().to_owned())
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
}

async fn record_governing(args: &ChatGoverningArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    let workspace_id = env_uuid("G5D_WORKSPACE_ID").ok_or_else(|| {
        G5Error::Config(
            "`g5d chat governing` runs inside a G5 workspace VM: workspace identity and \
             credentials are not available here."
                .to_string(),
        )
    })?;
    let task_id = env_uuid("G5D_TASK_ID").ok_or_else(|| {
        G5Error::Config(
            "`g5d chat governing` runs during an agent's chat turn: G5D_TASK_ID is not set."
                .to_string(),
        )
    })?;
    let reason = args.reason.trim();
    if reason.is_empty() || reason.contains('\n') {
        return Err(G5Error::Config(
            "--reason must be one line saying why the statement governs this turn's changes"
                .to_string(),
        ));
    }
    let root = std::env::var_os("G5D_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/work"));
    let found = locate(&root, &code_members(&root), &args.add, args.file.as_deref())?;
    let client = stable_context::require_worker_client(global).map_err(|_| {
        G5Error::Config(
            "`g5d chat governing` needs workspace credentials; run it from the agent's \
             workspace VM."
                .to_string(),
        )
    })?;
    let response = client
        .workspace_agent_post(
            &workspace_id,
            &format!("chat-tasks/{task_id}/ontology/governing"),
            &json!({
                "statementId": args.add,
                "repoId": found.repo_id,
                "path": found.path,
                "reason": reason,
            }),
            true,
        )
        .await?;
    if global.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&response["data"]).unwrap_or_default()
        );
    } else {
        let count = response["data"]["governing"].as_array().map_or(0, Vec::len);
        println!(
            "Recorded `{}` ({}/{}) as governing this turn's changes ({count} of at most 8).",
            args.add, found.root_path, found.path
        );
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Member {
    repo_id: String,
    root_path: String,
}

#[derive(Deserialize)]
struct Layout {
    repos: Vec<LayoutRepo>,
}

#[derive(Deserialize)]
struct LayoutRepo {
    repo_id: String,
    root_path: String,
    #[serde(default)]
    role: Option<String>,
}

/// The workspace's code members from its layout; the docs member has no ontology.
fn code_members(root: &Path) -> Vec<Member> {
    std::fs::read(root.join(".g5/layout.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Layout>(&bytes).ok())
        .map(|layout| {
            layout
                .repos
                .into_iter()
                .filter(|repo| repo.role.as_deref().unwrap_or("code") == "code")
                .filter(|repo| {
                    !repo.root_path.is_empty()
                        && Path::new(&repo.root_path)
                            .components()
                            .all(|c| matches!(c, std::path::Component::Normal(_)))
                })
                .map(|repo| Member {
                    repo_id: repo.repo_id,
                    root_path: repo.root_path,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, PartialEq, Eq)]
struct Found {
    repo_id: String,
    root_path: String,
    path: String,
}

fn ontology_files(dir: &Path, relative: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{relative}/{name}");
        if kind.is_dir() {
            ontology_files(&entry.path(), &path, out);
        } else if kind.is_file() && name.ends_with(".g5.md") {
            out.push(path);
        }
    }
}

/// The one ontology file that holds `id`, optionally narrowed by `--file`
/// (`.g5/ontology/…` or `<member>/.g5/ontology/…`).
fn locate(root: &Path, members: &[Member], id: &str, file: Option<&str>) -> Result<Found, G5Error> {
    let mut found = Vec::new();
    for member in members {
        let mut files = Vec::new();
        ontology_files(
            &root.join(&member.root_path).join(".g5/ontology"),
            ".g5/ontology",
            &mut files,
        );
        for path in files {
            if let Some(wanted) = file {
                let qualified = format!("{}/{path}", member.root_path);
                if wanted != path && wanted != qualified {
                    continue;
                }
            }
            let Ok(text) = std::fs::read_to_string(root.join(&member.root_path).join(&path)) else {
                continue;
            };
            let Ok(statements) = ontology_comparison::parse_version(&text) else {
                continue;
            };
            if statements.iter().any(|s| s.id == id) {
                found.push(Found {
                    repo_id: member.repo_id.clone(),
                    root_path: member.root_path.clone(),
                    path,
                });
            }
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(G5Error::Config(format!(
            "statement `{id}` is not in the workspace's ontology{}; name an existing, \
             unchanged statement",
            file.map(|f| format!(" file {f}")).unwrap_or_default()
        ))),
        _ => Err(G5Error::Config(format!(
            "statement `{id}` is in several files ({}); pass --file <member>/<path>",
            found
                .iter()
                .map(|f| format!("{}/{}", f.root_path, f.path))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, text: &str) {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    const PAGE: &str =
        "# P\n\n<a id=\"x.a\"></a>\n[I · x.a]\n\nAlpha.\n\n## Metadata\n\n### x.a\n\n- agent\n";

    #[test]
    fn finds_a_statement_in_the_code_members_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            ".g5/layout.json",
            r#"{"repos":[{"repo_id":"c1","root_path":"code","role":"code"},{"repo_id":"d1","root_path":"docs","role":"docs"}]}"#,
        );
        write(root, "code/.g5/ontology/features/a.g5.md", PAGE);
        write(root, "docs/.g5/ontology/a.g5.md", PAGE);
        let members = code_members(root);
        assert_eq!(members.len(), 1);
        assert_eq!(
            locate(root, &members, "x.a", None).unwrap(),
            Found {
                repo_id: "c1".into(),
                root_path: "code".into(),
                path: ".g5/ontology/features/a.g5.md".into(),
            }
        );
        assert!(locate(root, &members, "x.missing", None).is_err());
        assert!(locate(root, &members, "x.a", Some(".g5/ontology/other.g5.md")).is_err());
    }

    #[test]
    fn an_ambiguous_id_needs_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            ".g5/layout.json",
            r#"{"repos":[{"repo_id":"c1","root_path":"code","role":"code"}]}"#,
        );
        write(root, "code/.g5/ontology/a.g5.md", PAGE);
        write(root, "code/.g5/ontology/b.g5.md", PAGE);
        let members = code_members(root);
        assert!(locate(root, &members, "x.a", None).is_err());
        assert_eq!(
            locate(root, &members, "x.a", Some("code/.g5/ontology/b.g5.md"))
                .unwrap()
                .path,
            ".g5/ontology/b.g5.md"
        );
    }
}
