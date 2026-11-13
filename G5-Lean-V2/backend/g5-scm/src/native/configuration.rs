//! Task-private Git configuration; callers select the path and exact repository cut.

use g5_contracts::G5Error;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Write a new task-private global Git config containing one exact
/// `safe.directory` entry per assigned repository.
///
/// The destination must not already exist. This prevents a task-controlled
/// symlink or stale config from redirecting or widening the protected trust
/// file. The parent is likewise required to be a physical directory.
pub fn write_task_safe_directory_config(
    config_path: &Path,
    safe_directories: &[PathBuf],
) -> Result<(), G5Error> {
    let body = task_safe_directory_config_body(safe_directories)?;
    write_new_task_safe_directory_config(config_path, &body)
}

/// Replace-by-recreation variant of [`write_task_safe_directory_config`] for a
/// task running against a REUSED long-lived workspace tree — the preview
/// chat-attach drive, whose every turn is a new task over the preview's
/// materialized workspace (provided_agent_runtime D79 replacement scoping;
/// live test 2026-08-23, chat intent 8b883305: the second attached turn hit
/// EEXIST on `gitconfig.host`). A pre-existing file — regular or symlink — is
/// removed via `symlink_metadata` + `remove_file` (absent tolerated), then the
/// SAME exclusive-create write runs, so an existing file is never followed,
/// appended to, or truncated through, and every other D79 protection holds.
pub fn replace_task_safe_directory_config(
    config_path: &Path,
    safe_directories: &[PathBuf],
) -> Result<(), G5Error> {
    match std::fs::symlink_metadata(config_path) {
        Ok(_) => std::fs::remove_file(config_path).map_err(|error| G5Error::Internal {
            message: format!(
                "Failed to remove prior task Git config '{}': {error}",
                config_path.display()
            ),
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(G5Error::Internal {
                message: format!(
                    "Failed to inspect prior task Git config '{}': {error}",
                    config_path.display()
                ),
            })
        }
    }
    write_task_safe_directory_config(config_path, safe_directories)
}

/// Ensure an existing task-private config is byte-identical to the exact
/// assigned repository set, or create it when absent. A mismatched existing
/// file fails closed rather than being replaced after the task has started.
pub fn ensure_task_safe_directory_config(
    config_path: &Path,
    safe_directories: &[PathBuf],
) -> Result<(), G5Error> {
    let expected = task_safe_directory_config_body(safe_directories)?;
    match std::fs::symlink_metadata(config_path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "configPath",
                        "path": config_path,
                        "message": "Existing task Git config must be a physical file",
                    }),
                });
            }
            let actual =
                std::fs::read_to_string(config_path).map_err(|error| G5Error::Internal {
                    message: format!(
                        "Failed to read task Git config '{}': {error}",
                        config_path.display()
                    ),
                })?;
            if actual != expected {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "configPath",
                        "path": config_path,
                        "message": "Existing task Git config does not match the complete assigned repository cut",
                    }),
                });
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_new_task_safe_directory_config(config_path, &expected)
        }
        Err(error) => Err(G5Error::Internal {
            message: format!(
                "Failed to inspect task Git config '{}': {error}",
                config_path.display()
            ),
        }),
    }
}

fn task_safe_directory_config_body(safe_directories: &[PathBuf]) -> Result<String, G5Error> {
    if safe_directories.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "safeDirectories",
                "message": "Task Git trust requires at least one exact repository root",
            }),
        });
    }

    let mut seen = HashSet::with_capacity(safe_directories.len());
    let mut body = String::from("[safe]\n");
    for directory in safe_directories {
        if !directory.is_absolute() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "safeDirectories",
                    "path": directory,
                    "message": "Task Git trust requires absolute repository roots",
                }),
            });
        }
        let value = directory.to_str().ok_or_else(|| G5Error::ValidationError {
            details: serde_json::json!({
                "field": "safeDirectories",
                "path": directory,
                "message": "Task Git trust requires UTF-8 repository roots",
            }),
        })?;
        if value == "*" || !seen.insert(value.to_string()) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "safeDirectories",
                    "path": directory,
                    "message": "Task Git trust requires unique exact repository roots",
                }),
            });
        }
        body.push_str("\tdirectory = \"");
        write_git_config_quoted(&mut body, value)?;
        body.push_str("\"\n");
    }

    Ok(body)
}

fn write_new_task_safe_directory_config(config_path: &Path, body: &str) -> Result<(), G5Error> {
    let parent = config_path
        .parent()
        .ok_or_else(|| G5Error::ValidationError {
            details: serde_json::json!({
                "field": "configPath",
                "path": config_path,
                "message": "Task Git config path requires a parent directory",
            }),
        })?;
    std::fs::create_dir_all(parent).map_err(|error| G5Error::Internal {
        message: format!(
            "Failed to create task Git config directory '{}': {error}",
            parent.display()
        ),
    })?;
    let parent_metadata = std::fs::symlink_metadata(parent).map_err(|error| G5Error::Internal {
        message: format!(
            "Failed to inspect task Git config directory '{}': {error}",
            parent.display()
        ),
    })?;
    if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "configPath",
                "path": config_path,
                "message": "Task Git config parent must be a physical directory",
            }),
        });
    }

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(config_path)
        .map_err(|error| G5Error::Internal {
            message: format!(
                "Failed to create task Git config '{}': {error}",
                config_path.display()
            ),
        })?;
    file.write_all(body.as_bytes())
        .map_err(|error| G5Error::Internal {
            message: format!(
                "Failed to write task Git config '{}': {error}",
                config_path.display()
            ),
        })?;
    Ok(())
}

fn write_git_config_quoted(output: &mut String, value: &str) -> Result<(), G5Error> {
    for character in value.chars() {
        match character {
            '\0' | '\n' | '\r' => {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "safeDirectories",
                        "message": "Task Git trust rejects repository roots with line-breaking characters",
                    }),
                });
            }
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\t' => output.push_str("\\t"),
            '\u{0008}' => output.push_str("\\b"),
            _ => output.push(character),
        }
    }
    Ok(())
}

/// All paths are selected under the runtime's private state directory by its
/// owner. The assigned roots and excludes policy are inputs, not SCM authority.
pub struct MemberConfiguration {
    pub global_config: PathBuf,
    pub transport_config: PathBuf,
    pub excludes_file: PathBuf,
    pub safe_directories: Vec<PathBuf>,
    pub author: crate::GitAuthor,
    pub excluded_roots: Vec<String>,
    pub excluded_file_patterns: Vec<String>,
}

/// Install private trust, author and exclude configuration by replacement.
/// No user's or system's Git configuration is modified.
pub fn install_member_configuration(
    configuration: &MemberConfiguration,
) -> Result<(), super::NativeGitError> {
    let operation = "install member configuration";
    let mut body = task_safe_directory_config_body(&configuration.safe_directories)
        .map_err(|error| super::invalid(operation, &error.to_string()))?;
    body.push_str(&format!(
        "[user]\n\tname = {}\n\temail = {}\n[core]\n\texcludesFile = {}\n[include]\n\tpath = {}\n",
        member_quote(&configuration.author.name, operation)?,
        member_quote(&configuration.author.email, operation)?,
        member_path(&configuration.excludes_file, operation)?,
        member_path(&configuration.transport_config, operation)?,
    ));
    let mut excludes = String::new();
    for path in &configuration.excluded_roots {
        member_quote(path, operation)?;
        excludes.push('/');
        excludes.push_str(path);
        excludes.push('\n');
    }
    for pattern in &configuration.excluded_file_patterns {
        member_quote(pattern, operation)?;
        excludes.push_str(pattern);
        excludes.push('\n');
    }
    write_member_files(
        operation,
        &[
            (&configuration.excludes_file, excludes.as_bytes()),
            (&configuration.transport_config, b""),
            (&configuration.global_config, body.as_bytes()),
        ],
    )
}

/// Runtime-owned routing for exactly one assigned checkout. The relay lease
/// remains with the caller and must outlive every use of this configuration.
pub struct MemberRemoteConfiguration {
    pub checkout: PathBuf,
    pub config_path: PathBuf,
    pub canonical_url: String,
    pub transport_url: String,
    pub branch: String,
}

/// Replace the per-member includes and their common private include index.
/// Same-named branches in other members never inherit this member's upstream.
pub fn configure_member_remotes(
    include_path: &Path,
    members: &[MemberRemoteConfiguration],
) -> Result<(), super::NativeGitError> {
    let operation = "configure member remotes";
    let mut includes = String::new();
    let mut bodies = Vec::new();
    for member in members {
        let root = std::fs::canonicalize(&member.checkout)
            .map_err(|_| super::invalid(operation, "Cannot resolve assigned member checkout"))?
            .join(".git");
        let body = format!(
            "[url {}]\n\tinsteadOf = {}\n[branch {}]\n\tremote = origin\n\tmerge = {}\n[push]\n\tdefault = simple\n",
            member_quote(&member.transport_url, operation)?,
            member_quote(&member.canonical_url, operation)?,
            member_quote(&member.branch, operation)?,
            member_quote(&format!("refs/heads/{}", member.branch), operation)?,
        );
        includes.push_str(&format!(
            "[includeIf {}]\n\tpath = {}\n",
            member_quote(&format!("gitdir:{}", root.to_string_lossy()), operation)?,
            member_path(&member.config_path, operation)?,
        ));
        bodies.push((&member.config_path, body));
    }
    let mut files: Vec<(&Path, &[u8])> = bodies
        .iter()
        .map(|(path, body)| (path.as_path(), body.as_bytes()))
        .collect();
    files.push((include_path, includes.as_bytes()));
    write_member_files(operation, &files)
}

fn member_path(path: &Path, operation: &'static str) -> Result<String, super::NativeGitError> {
    member_quote(
        path.to_str()
            .ok_or_else(|| super::invalid(operation, "Member configuration path must be UTF-8"))?,
        operation,
    )
}

fn member_quote(value: &str, operation: &'static str) -> Result<String, super::NativeGitError> {
    if value.chars().any(char::is_control) {
        return Err(super::invalid(
            operation,
            "Git configuration values cannot contain control characters",
        ));
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn write_member_files(
    operation: &'static str,
    files: &[(&Path, &[u8])],
) -> Result<(), super::NativeGitError> {
    // Validate all parent locations before writing any target.
    for (path, _) in files {
        let parent = path.parent().ok_or_else(|| {
            super::invalid(
                operation,
                "Private Git configuration requires a parent directory",
            )
        })?;
        let metadata = std::fs::symlink_metadata(parent).map_err(|_| {
            super::invalid(
                operation,
                "Private Git configuration directory is unavailable",
            )
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(super::invalid(
                operation,
                "Private Git configuration requires a physical directory",
            ));
        }
    }
    for (completed, (path, body)) in files.iter().enumerate() {
        let parent = path.parent().expect("validated parent");
        let result = (|| -> std::io::Result<()> {
            let mut temporary = tempfile::Builder::new()
                .prefix(".g5-gitconfig-")
                .tempfile_in(parent)?;
            temporary.write_all(body)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                temporary
                    .as_file()
                    .set_permissions(std::fs::Permissions::from_mode(0o644))?;
            }
            temporary.as_file().sync_all()?;
            temporary.persist(path).map_err(|error| error.error)?;
            Ok(())
        })();
        if let Err(error) = result {
            return Err(super::NativeGitError {
                operation, target: Some(path.display().to_string()), kind: super::FailureKind::Io,
                effect: if completed > 0 { super::FailureEffect::Partial } else { super::FailureEffect::Unknown },
                detail: format!("{completed} private configuration files installed; cannot install this file: {error}"),
            });
        }
    }
    Ok(())
}
