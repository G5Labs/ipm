//! Session files the fakes leave behind, in the layouts design v6 §8.4
//! describes, so session-file harvesting can be tested against the fakes:
//!
//! * Claude: `$CLAUDE_CONFIG_DIR/projects/<escaped cwd>/<session id>.jsonl`
//! * Codex: `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<timestamp>-<id>.jsonl`

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::scenario::{civil_from_days, rfc3339_now};

/// Claude Code's project key for a working directory: every character that is
/// not ASCII alphanumeric or `-` becomes `-` (observed on 2.1.270, where
/// `/private/tmp/x.y` maps to `-private-tmp-x-y`).
pub fn claude_project_key(cwd: &Path) -> String {
    cwd.display()
        .to_string()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

/// Path of the Claude transcript for `session_id` under `config_dir`.
pub fn claude_transcript_path(config_dir: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    config_dir
        .join("projects")
        .join(claude_project_key(cwd))
        .join(format!("{session_id}.jsonl"))
}

/// Append the turn's frames to the Claude transcript. `messages` are the
/// user message(s) read from stdin followed by the emitted frames.
pub fn write_claude_transcript(
    config_dir: &Path,
    cwd: &Path,
    session_id: &str,
    messages: &[Value],
) -> Result<PathBuf, String> {
    let path = claude_transcript_path(config_dir, cwd, session_id);
    append_jsonl(&path, messages)?;
    Ok(path)
}

/// Path of a new Codex rollout file for `session_id` under `codex_home`.
pub fn codex_rollout_path(codex_home: &Path, session_id: &str) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let day_secs = secs.rem_euclid(86_400);
    let stamp = format!(
        "{year:04}-{month:02}-{day:02}T{:02}-{:02}-{:02}",
        day_secs / 3600,
        (day_secs % 3600) / 60,
        day_secs % 60
    );
    codex_home
        .join("sessions")
        .join(format!("{year:04}"))
        .join(format!("{month:02}"))
        .join(format!("{day:02}"))
        .join(format!("rollout-{stamp}-{session_id}.jsonl"))
}

/// Find an existing rollout for `session_id` under `codex_home`.
pub fn find_codex_rollout(codex_home: &Path, session_id: &str) -> Option<PathBuf> {
    let suffix = format!("-{session_id}.jsonl");
    let mut stack = vec![codex_home.join("sessions")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(&suffix))
            {
                return Some(path);
            }
        }
    }
    None
}

/// Write (or append to) the Codex rollout for this turn.
pub fn write_codex_rollout(
    codex_home: &Path,
    cwd: &Path,
    session_id: &str,
    cli_version: &str,
    frames: &[Value],
) -> Result<PathBuf, String> {
    let (path, is_new) = match find_codex_rollout(codex_home, session_id) {
        Some(existing) => (existing, false),
        None => (codex_rollout_path(codex_home, session_id), true),
    };
    let mut lines = Vec::new();
    if is_new {
        lines.push(json!({
            "timestamp": rfc3339_now(),
            "type": "session_meta",
            "payload": {
                "id": session_id,
                "timestamp": rfc3339_now(),
                "cwd": cwd.display().to_string(),
                "originator": "fake-codex",
                "cli_version": cli_version,
                "instructions": null
            }
        }));
    }
    for frame in frames {
        lines.push(json!({
            "timestamp": rfc3339_now(),
            "type": "event_msg",
            "payload": frame
        }));
    }
    append_jsonl(&path, &lines)?;
    Ok(path)
}

fn append_jsonl(path: &Path, lines: &[Value]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("cannot create {}: {err}", parent.display()))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("cannot open {}: {err}", path.display()))?;
    for line in lines {
        writeln!(file, "{}", serde_json::to_string(line).expect("json"))
            .map_err(|err| format!("cannot write {}: {err}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_key_matches_observed_escaping() {
        assert_eq!(
            claude_project_key(Path::new(
                "/private/tmp/claude-501/-Users-zliu-Workspace-G5-Lean-wt-P3.1"
            )),
            "-private-tmp-claude-501--Users-zliu-Workspace-G5-Lean-wt-P3-1"
        );
        assert_eq!(
            claude_project_key(Path::new("/work/repo_a")),
            "-work-repo-a"
        );
    }

    #[test]
    fn transcript_and_rollout_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path =
            write_claude_transcript(dir.path(), Path::new("/work"), "sess", &[json!({"a": 1})])
                .unwrap();
        assert_eq!(
            path,
            dir.path().join("projects").join("-work").join("sess.jsonl")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"a\":1}\n");

        let rollout = write_codex_rollout(
            dir.path(),
            Path::new("/work"),
            "thread-1",
            "0.0.0",
            &[json!({"type": "turn.started"})],
        )
        .unwrap();
        assert!(rollout.starts_with(dir.path().join("sessions")));
        assert!(rollout
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("-thread-1.jsonl"));
        let text = fs::read_to_string(&rollout).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().next().unwrap().contains("session_meta"));
        assert_eq!(
            find_codex_rollout(dir.path(), "thread-1"),
            Some(rollout.clone())
        );
        // A second turn appends to the same rollout.
        let again = write_codex_rollout(
            dir.path(),
            Path::new("/work"),
            "thread-1",
            "0.0.0",
            &[json!({"type": "turn.completed"})],
        )
        .unwrap();
        assert_eq!(again, rollout);
        assert_eq!(fs::read_to_string(&rollout).unwrap().lines().count(), 3);
        assert_eq!(find_codex_rollout(dir.path(), "missing"), None);
    }
}
