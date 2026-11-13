//! Bind native memory directly to the owner's personal repository
//! (`g5.personal.memory`). There is no startup copy or exit-time merge; the
//! dispatcher commits and pushes it after code pushes, and native session/auth
//! state stays at home.

use crate::types::ProviderId;
use g5_scm::native::NativeGit;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

#[derive(Debug)]
pub(crate) enum Binding {
    Bound(PathBuf),
    Skipped(&'static str),
    /// Off by design (formal review): silent, unlike `Skipped`.
    Disabled,
}

impl Binding {
    pub(crate) fn is_bound(&self) -> bool {
        matches!(self, Binding::Bound(_))
    }
}

/// Bind memory in the personal repository checked out at `personal_root`
/// (workspace-relative), which the dispatcher names only when Start checked
/// it out. Without it memory is off: a workspace repository would share it
/// with that repository's readers.
pub(crate) fn prepare(
    workspace: &Path,
    personal_root: Option<&str>,
    provider: ProviderId,
    writable: bool,
) -> io::Result<Binding> {
    let Some(personal_root) = personal_root.filter(|root| !root.is_empty()) else {
        return Ok(Binding::Skipped(
            "The owner's personal repository is not in this workspace, so memory is off.",
        ));
    };
    let workspace = workspace.canonicalize()?;
    let path = Path::new(personal_root);
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid("invalid personal repository root"));
    }
    let root = workspace.join(path);
    match root.canonicalize() {
        // An alias must not turn the personal checkout into another checkout
        // or a private home.
        Ok(canonical) if canonical != root => {
            return Err(invalid("personal repository root contains a symlink"));
        }
        Ok(_) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Binding::Skipped(
                "The owner's personal repository is not checked out, so memory is off.",
            ));
        }
        Err(error) => return Err(error),
    }
    // Check before creating anything. Native memory is ordinary Git content;
    // a broad ignore rule must not silently discard new lessons.
    let provider = match provider {
        ProviderId::ClaudeCode => "claude",
        ProviderId::Codex => "codex",
    };
    let relative = Path::new("memory").join(provider);
    ensure_git_eligible(&root, &relative.join("MEMORY.md"))?;
    if provider == "codex" {
        ensure_git_eligible(&root, &relative.join("memory_summary.md"))?;
    }
    let mut directory = root.clone();
    for component in ["memory", provider] {
        directory.push(component);
        match std::fs::create_dir(&directory) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error),
        }
        if !std::fs::symlink_metadata(&directory)?.file_type().is_dir() {
            return Err(invalid(
                "native memory path must be a directory without symlinks",
            ));
        }
    }
    if writable {
        ensure_writable(&directory)?;
    }
    validate_tree(&root, &directory, writable)?;
    Ok(Binding::Bound(directory))
}

/// The pinned Codex stores memory at CODEX_HOME/memories and has no memory-only
/// directory setting. Link just that tree; never move auth, sessions or SQLite
/// state into Git, and never overwrite an existing local memory collection.
pub(crate) fn bind_codex(home: &Path, binding: &Binding) -> io::Result<()> {
    let Binding::Bound(directory) = binding else {
        return Ok(());
    };
    let codex_home = home.join(".codex");
    std::fs::create_dir_all(&codex_home)?;
    if !std::fs::symlink_metadata(&codex_home)?.file_type().is_dir() {
        return Err(invalid("Codex home must be a directory without symlinks"));
    }
    let link = codex_home.join("memories");
    match std::fs::symlink_metadata(&link) {
        Ok(metadata) if metadata.is_dir() => {
            // Disabled native launches can leave an empty directory behind.
            // remove_dir refuses a populated collection without touching it.
            if let Err(error) = std::fs::remove_dir(&link) {
                if error.kind() != io::ErrorKind::NotFound {
                    // Another launch may have replaced the empty directory
                    // with our link after symlink_metadata observed it.
                    return validate_codex_link(&link, directory).map_err(|_| {
                        io::Error::new(error.kind(), "Codex has an existing local memory directory; preserve its contents in the personal memory folder and remove the local directory before retrying")
                    });
                }
            }
        }
        Ok(_) => return validate_codex_link(&link, directory),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    #[cfg(unix)]
    match std::os::unix::fs::symlink(directory, &link) {
        Ok(()) => Ok(()),
        // Another launch can establish the same binding concurrently.
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            validate_codex_link(&link, directory)
        }
        Err(error) => Err(error),
    }
    #[cfg(not(unix))]
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Codex memory binding requires Unix",
    ))
}

fn validate_codex_link(link: &Path, directory: &Path) -> io::Result<()> {
    if std::fs::read_link(link).is_ok_and(|target| target == directory) {
        Ok(())
    } else {
        Err(invalid("Codex memory path already exists outside the personal memory binding; preserve it and remove the conflicting path before retrying"))
    }
}

pub(crate) fn codex_configuration(binding: &Binding, writable: bool) -> Vec<String> {
    let enabled = binding.is_bound();
    let mut settings = vec![
        format!("features.memories={enabled}"),
        format!("memories.generate_memories={}", enabled && writable),
        format!("memories.use_memories={enabled}"),
    ];
    if let Binding::Bound(directory) = binding {
        if writable {
            // The personal repository is outside the code checkout. Permit
            // native tool writes to memory under workspace-write, without
            // granting writes to the rest of the owner's personal repository.
            settings.push(format!(
                "sandbox_workspace_write.writable_roots=[{}]",
                toml_edit::Value::from(directory.to_string_lossy().into_owned())
            ));
        }
    }
    settings
        .into_iter()
        .flat_map(|setting| ["-c".into(), setting])
        .collect()
}

fn validate_tree(root: &Path, directory: &Path, writable: bool) -> io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if entry.file_name() == ".git" || (!kind.is_dir() && !kind.is_file()) {
            return Err(invalid(
                "native memory contains a symlink, private Git state or special file",
            ));
        }
        if kind.is_dir() {
            if writable {
                ensure_writable(&entry.path())?;
            }
            validate_tree(root, &entry.path(), writable)?;
        } else {
            ensure_git_eligible(root, entry.path().strip_prefix(root).unwrap())?;
            if writable {
                ensure_writable(&entry.path())?;
            }
        }
    }
    Ok(())
}

fn ensure_git_eligible(root: &Path, path: &Path) -> io::Result<()> {
    // Flush seeds its private index from HEAD, not the user's staging area.
    // Already committed files remain capturable even if ignore rules match.
    let runner = NativeGit::cli(std::time::Duration::from_secs(30)).map_err(io::Error::other)?;
    match runner
        .path_is_capture_eligible_blocking(root, path)
        .map_err(|error| {
            io::Error::other(format!("could not check committed native memory: {error}"))
        })? {
        true => Ok(()),
        false => Err(invalid(
            "repository ignore rules exclude native memory; update them before launching",
        )),
    }
}

/// Native index-content injection replaces MEMORY.md even when the directory
/// is bound correctly. Neither an empty nor a null overlay unsets it. Inspect
/// supported file settings for this incompatible override; native Claude still
/// parses/applies all settings itself and the originals are never rewritten.
pub(crate) fn validate_claude_settings(cwd: &Path, home: &Path) -> io::Result<()> {
    reject_index_override(&cwd.join(".claude/settings.json"))?;
    reject_index_override(&home.join(".claude/settings.json"))?;
    reject_index_override(&home.join(".claude/remote-settings.json"))?;
    let policy_root = if cfg!(target_os = "macos") {
        Path::new("/Library/Application Support/ClaudeCode")
    } else {
        Path::new("/etc/claude-code")
    };
    reject_index_override(&policy_root.join("managed-settings.json"))?;
    match std::fs::read_dir(policy_root.join("managed-settings.d")) {
        Ok(entries) => {
            for entry in entries {
                let path = entry?.path();
                if path.extension().is_some_and(|ext| ext == "json")
                    && !path.file_name().unwrap().to_string_lossy().starts_with('.')
                    && path.is_file()
                {
                    reject_index_override(&path)?;
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    Ok(())
}

fn reject_index_override(path: &Path) -> io::Result<()> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    // The pinned parser accepts whitespace-only files and a leading BOM, then
    // uses strict JSON. Invalid files remain subject to native validation.
    // Read only keys: native JSON.parse accepts numbers outside Rust's f64
    // range, and unrelated values must not bypass this check. Maps also retain
    // the last duplicate key, matching native JSON parsing.
    let Ok(settings) = serde_json::from_str::<
        std::collections::HashMap<String, &serde_json::value::RawValue>,
    >(contents.trim_start_matches('\u{feff}')) else {
        return Ok(());
    };
    if settings
        .get("env")
        .and_then(|env| {
            serde_json::from_str::<std::collections::HashMap<String, serde::de::IgnoredAny>>(
                env.get(),
            )
            .ok()
        })
        .is_some_and(|env| env.contains_key("CLAUDE_COWORK_MEMORY_INDEX_CONTENT"))
    {
        return Err(invalid("CLAUDE_COWORK_MEMORY_INDEX_CONTENT replaces repository memory; remove it from native settings before launching"));
    }
    Ok(())
}

pub(crate) fn claude_settings(home: &Path, binding: &Binding) -> String {
    // Let --setting-sources user,project parse native settings (including empty
    // files). Flag settings take precedence over native settings.env, whose
    // overrides are applied after the child process environment is installed.
    let mut settings = serde_json::json!({"env": {
        "CLAUDE_CONFIG_DIR": home.join(".claude"),
        "CLAUDE_CODE_REMOTE_MEMORY_DIR": "",
        "CLAUDE_COWORK_MEMORY_PATH_OVERRIDE": "",
        "CLAUDE_MEMORY_STORES": ""
    }});
    match binding {
        Binding::Bound(directory) => {
            settings["autoMemoryDirectory"] = serde_json::json!(directory);
        }
        Binding::Skipped(_) | Binding::Disabled => {
            settings["autoMemoryEnabled"] = Value::Bool(false);
            settings["env"]["CLAUDE_CODE_DISABLE_AUTO_MEMORY"] = Value::String("1".into());
        }
    }
    settings.to_string()
}

fn ensure_writable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let access = if path.is_dir() {
            libc::W_OK | libc::X_OK
        } else {
            libc::W_OK
        };
        let path = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| invalid("invalid native memory path"))?;
        // The module runs with its real and effective identity equal. Check
        // access without leaving a probe/staging file in repository memory.
        if unsafe { libc::access(path.as_ptr(), access) } != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "native memory is not writable",
            ));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native memory write checks require Unix",
        ))
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::process::Command;

    /// A Git checkout at `personal`, where Start leaves the personal repository.
    fn personal(work: &Path) -> PathBuf {
        let root = work.join("personal");
        std::fs::create_dir_all(&root).unwrap();
        assert!(Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(&root)
            .status()
            .unwrap()
            .success());
        root
    }

    fn prepare(workspace: &Path) -> io::Result<Binding> {
        super::prepare(workspace, Some("personal"), ProviderId::ClaudeCode, true)
    }

    fn bound(binding: Binding) -> PathBuf {
        match binding {
            Binding::Bound(path) => path,
            Binding::Skipped(reason) => panic!("{reason}"),
            Binding::Disabled => panic!("memory disabled"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn codex_binding_shares_only_memory_and_preserves_local_state() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        personal(work);
        let binding = super::prepare(work, Some("personal"), ProviderId::Codex, true).unwrap();
        let memory =
            bound(super::prepare(work, Some("personal"), ProviderId::Codex, true).unwrap());
        assert!(memory.ends_with("personal/memory/codex"));
        let home = work.join("home");
        let native = home.join(".codex");
        std::fs::create_dir_all(native.join("memories")).unwrap();
        std::fs::write(native.join("auth.json"), "private credential fixture").unwrap();
        std::fs::write(native.join("state.sqlite"), "private state fixture").unwrap();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| bind_codex(&home, &binding).unwrap());
            }
        });
        bind_codex(&home, &binding).unwrap();
        std::fs::write(native.join("memories/memory_summary.md"), "saved lesson").unwrap();
        assert_eq!(
            std::fs::read_to_string(memory.join("memory_summary.md")).unwrap(),
            "saved lesson"
        );
        assert_eq!(std::fs::read_dir(&memory).unwrap().count(), 1);
        assert_eq!(
            std::fs::read_to_string(native.join("auth.json")).unwrap(),
            "private credential fixture"
        );
        assert_eq!(
            std::fs::read_to_string(native.join("state.sqlite")).unwrap(),
            "private state fixture"
        );
        std::fs::remove_file(native.join("memories/memory_summary.md")).unwrap();
        bind_codex(&home, &binding).unwrap();
        assert!(!memory.join("memory_summary.md").exists());

        std::fs::remove_file(native.join("memories")).unwrap();
        std::fs::create_dir(native.join("memories")).unwrap();
        std::fs::write(native.join("memories/local.md"), "keep local lesson").unwrap();
        assert!(bind_codex(&home, &binding).is_err());
        assert_eq!(
            std::fs::read_to_string(native.join("memories/local.md")).unwrap(),
            "keep local lesson"
        );
        std::fs::remove_file(native.join("memories/local.md")).unwrap();
        std::fs::remove_dir(native.join("memories")).unwrap();
        std::os::unix::fs::symlink(work.join("elsewhere"), native.join("memories")).unwrap();
        assert!(bind_codex(&home, &binding).is_err());
        assert_eq!(
            std::fs::read_link(native.join("memories")).unwrap(),
            work.join("elsewhere")
        );

        let fresh = work.join("fresh");
        bind_codex(&fresh, &Binding::Skipped("no personal repository")).unwrap();
        bind_codex(&fresh, &Binding::Disabled).unwrap();
        assert!(!fresh.exists());
    }

    #[test]
    fn codex_binding_checks_the_summary_and_native_tree() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        let root = personal(work);
        std::fs::write(root.join(".gitignore"), "memory/codex/memory_summary.md\n").unwrap();
        assert!(super::prepare(work, Some("personal"), ProviderId::Codex, true).is_err());
        assert!(!root.join("memory").exists());
        std::fs::remove_file(root.join(".gitignore")).unwrap();
        let memory =
            bound(super::prepare(work, Some("personal"), ProviderId::Codex, true).unwrap());
        std::fs::create_dir(memory.join(".git")).unwrap();
        assert!(super::prepare(work, Some("personal"), ProviderId::Codex, true).is_err());
    }

    #[test]
    fn memory_binds_only_the_personal_repository() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        let code = work.join("code");
        std::fs::create_dir_all(&code).unwrap();
        assert!(Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(&code)
            .status()
            .unwrap()
            .success());
        // No personal repository named, or none checked out: memory is off
        // and no workspace repository receives it.
        assert!(matches!(
            super::prepare(work, None, ProviderId::ClaudeCode, true).unwrap(),
            Binding::Skipped(_)
        ));
        assert!(matches!(prepare(work).unwrap(), Binding::Skipped(_)));
        assert!(!work.join("personal").exists());
        personal(work);
        assert!(bound(prepare(work).unwrap()).ends_with("personal/memory/claude"));
        for root in ["../personal", "/personal", "personal/../code", "."] {
            assert!(
                super::prepare(work, Some(root), ProviderId::ClaudeCode, true).is_err(),
                "{root}"
            );
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&code, work.join("alias")).unwrap();
            assert!(super::prepare(work, Some("alias"), ProviderId::ClaudeCode, true).is_err());
        }
        assert!(!code.join("memory").exists());
        assert!(!code.join(".g5").exists());
    }

    #[test]
    fn repeated_and_concurrent_setup_preserves_native_changes_and_project_settings() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        personal(work);
        let memory = bound(prepare(work).unwrap());
        std::fs::write(memory.join("MEMORY.md"), "current lesson").unwrap();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    prepare(work).unwrap();
                });
            }
        });
        assert_eq!(
            std::fs::read_to_string(memory.join("MEMORY.md")).unwrap(),
            "current lesson"
        );
        std::fs::remove_file(memory.join("MEMORY.md")).unwrap();
        prepare(work).unwrap();
        assert!(!memory.join("MEMORY.md").exists());
        let cwd = work.join("personal");
        std::fs::create_dir(cwd.join(".claude")).unwrap();
        let original = json!({"autoMemoryDirectory":"/old", "autoMemoryEnabled":false, "permissions":{"deny":["Write"]},"hooks":{}}).to_string();
        std::fs::write(cwd.join(".claude/settings.json"), &original).unwrap();
        let settings: Value =
            serde_json::from_str(&claude_settings(&cwd, &Binding::Bound(memory.clone()))).unwrap();
        assert_eq!(settings["autoMemoryDirectory"], json!(memory));
        assert!(settings.get("autoMemoryEnabled").is_none());
        assert!(settings.get("permissions").is_none());
        assert_eq!(settings["env"]["CLAUDE_COWORK_MEMORY_PATH_OVERRIDE"], "");
        assert_eq!(
            std::fs::read_to_string(cwd.join(".claude/settings.json")).unwrap(),
            original
        );
        let skipped: Value =
            serde_json::from_str(&claude_settings(&cwd, &Binding::Skipped("test"))).unwrap();
        assert_eq!(skipped["autoMemoryEnabled"], false);
        assert!(skipped.get("autoMemoryDirectory").is_none());
        let disabled: Value =
            serde_json::from_str(&claude_settings(&cwd, &Binding::Disabled)).unwrap();
        assert_eq!(disabled["autoMemoryEnabled"], false);
        assert_eq!(disabled["env"]["CLAUDE_CODE_DISABLE_AUTO_MEMORY"], "1");
        assert!(disabled.get("autoMemoryDirectory").is_none());
    }

    #[test]
    fn user_settings_cannot_replace_the_repository_memory_index() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let cwd = temp.path().join("code");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::create_dir_all(&cwd).unwrap();
        let settings = home.join(".claude/settings.json");
        let original = r#"{"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"foreign"}}"#;
        std::fs::write(&settings, original).unwrap();
        assert!(validate_claude_settings(&cwd, &home).is_err());
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), original);
    }

    #[test]
    fn native_index_override_is_rejected_without_rewriting_settings() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("code");
        std::fs::create_dir_all(cwd.join(".claude")).unwrap();
        let settings = cwd.join(".claude/settings.json");
        for value in [
            json!("foreign"),
            json!(""),
            Value::Null,
            json!(false),
            json!(12),
        ] {
            let text = format!(
                "\u{feff}{}",
                json!({"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":value}})
            );
            std::fs::write(&settings, &text).unwrap();
            assert!(reject_index_override(&settings).is_err());
            assert_eq!(std::fs::read_to_string(&settings).unwrap(), text);
        }
        for text in [
            "",
            "   \n",
            "\u{feff}{}",
            "{",
            r#"{"permissions":{"deny":["Write"]}}"#,
            r#"{"env":{"EXAMPLE":1e400}}"#,
            r#"{"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"old"},"env":{}}"#,
        ] {
            std::fs::write(&settings, text).unwrap();
            reject_index_override(&settings).unwrap();
            assert_eq!(std::fs::read_to_string(&settings).unwrap(), text);
        }
        // Only the native top-level env object applies. Unrelated extension
        // metadata is not a provider environment override.
        std::fs::write(
            &settings,
            json!({"example":{"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"documentation"}}})
                .to_string(),
        )
        .unwrap();
        reject_index_override(&settings).unwrap();
        for text in [
            r#"{"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"foreign","EXAMPLE":1e400}}"#,
            r#"{"env":null,"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"foreign"}}"#,
            r#"{"example":1e400,"env":{"CLAUDE_COWORK_MEMORY_INDEX_CONTENT":"foreign"}}"#,
        ] {
            std::fs::write(&settings, text).unwrap();
            assert!(reject_index_override(&settings).is_err());
            assert_eq!(std::fs::read_to_string(&settings).unwrap(), text);
        }
    }

    #[test]
    fn ignored_memory_fails_without_overriding_policy_or_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        personal(work);
        std::fs::write(work.join("personal/.gitignore"), "memory/\n").unwrap();
        assert!(prepare(work).unwrap_err().to_string().contains("ignore"));
        assert!(!work.join("personal/memory").exists());
        std::fs::write(work.join("personal/.gitignore"), "*.topic\n").unwrap();
        let memory = bound(prepare(work).unwrap());
        std::fs::write(memory.join("private.topic"), "keep").unwrap();
        assert!(prepare(work).is_err());
        assert_eq!(
            std::fs::read_to_string(memory.join("private.topic")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn ignored_memory_uses_head_tracking_not_the_staging_index() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        personal(work);
        let root = work.join("personal");
        let memory = bound(prepare(work).unwrap());
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        std::fs::write(memory.join("MEMORY.md"), "committed lesson").unwrap();
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Memory Test",
            "-c",
            "user.email=memory@example.invalid",
            "commit",
            "-qm",
            "memory",
        ]);
        std::fs::write(root.join(".gitignore"), "memory/claude/*.md\n").unwrap();
        std::fs::write(memory.join("MEMORY.md"), "current lesson").unwrap();
        prepare(work).unwrap();
        assert_eq!(
            std::fs::read_to_string(memory.join("MEMORY.md")).unwrap(),
            "current lesson"
        );

        std::fs::write(memory.join("topic.md"), "uncommitted lesson").unwrap();
        assert!(prepare(work).is_err());
        git(&["add", "-f", "memory/claude/topic.md"]);
        assert!(prepare(work).is_err());
        std::fs::write(root.join(".gitignore"), "memory/claude/MEMORY.md\n").unwrap();
        prepare(work).unwrap();
        assert_eq!(
            std::fs::read_to_string(memory.join("topic.md")).unwrap(),
            "uncommitted lesson"
        );

        let outside = tempfile::tempdir().unwrap();
        assert!(ensure_git_eligible(outside.path(), Path::new("MEMORY.md"))
            .unwrap_err()
            .to_string()
            .contains("committed native memory"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_unwritable_memory_but_allows_restricted_reads() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::getuid() } == 0 {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        personal(work);
        let memory = bound(prepare(work).unwrap());
        std::fs::write(memory.join("MEMORY.md"), "existing").unwrap();
        std::fs::set_permissions(&memory, std::fs::Permissions::from_mode(0o555)).unwrap();
        let writable = prepare(work);
        let readonly = super::prepare(work, Some("personal"), ProviderId::ClaudeCode, false);
        std::fs::set_permissions(&memory, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            writable.unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(matches!(readonly.unwrap(), Binding::Bound(_)));
        assert_eq!(
            std::fs::read_to_string(memory.join("MEMORY.md")).unwrap(),
            "existing"
        );
        std::fs::remove_file(memory.join("MEMORY.md")).unwrap();
        std::fs::set_permissions(&memory, std::fs::Permissions::from_mode(0o666)).unwrap();
        let unsearchable = prepare(work);
        std::fs::set_permissions(&memory, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            unsearchable.unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinks_and_nested_git_without_mutating_their_targets() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        personal(work);
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), work.join("personal/memory")).unwrap();
        assert!(prepare(work).is_err());
        assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());
        std::fs::remove_file(work.join("personal/memory")).unwrap();
        let memory = bound(prepare(work).unwrap());
        symlink(outside.path(), memory.join("linked")).unwrap();
        assert!(prepare(work).is_err());
        std::fs::remove_file(memory.join("linked")).unwrap();
        std::fs::create_dir(memory.join(".git")).unwrap();
        assert!(prepare(work).is_err());
        assert!(memory.join(".git").exists());
    }
}
