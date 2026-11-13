//! A deterministic native-resume probe. The first turn writes a random fact
//! only into a tool result; the second turn can answer only by reading the
//! provider's persisted transcript or rollout. Historical message text cannot
//! supply the fact.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde_json::Value;

use crate::session_files;

pub const RECORD: &str = "G5_TOOL_ONLY_FACT_E2E:record";
pub const RECALL: &str = "G5_TOOL_ONLY_FACT_E2E:recall";
pub const SKIP_PERSISTENCE: &str = "G5_TOOL_ONLY_FACT_E2E:skip-persistence";
pub const FAIL_SAVE: &str = "G5_TOOL_ONLY_FACT_E2E:fail-save";
pub const BLOCK_AFTER_SAVE: &str = "G5_TOOL_ONLY_FACT_E2E:block-after-save";
pub const REFUSE_RESUME: &str = "G5_TOOL_ONLY_FACT_E2E:refuse-resume";
const READY_PREFIX: &str = "G5_TOOL_ONLY_FACT_E2E:ready:";
pub const FACT_PREFIX: &str = "tool-only-e2e-";
const CLAUDE_RESULT_NAME: &str = "fake-tool-result.txt";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Record,
    Recall,
}

fn current_input(prompt: &str) -> &str {
    prompt
        .rsplit_once("\nEnd of historical context. Current input follows:\n")
        .map_or(prompt, |(_, current)| current)
}

pub fn action(prompt: &str) -> Option<Action> {
    let input = current_input(prompt);
    if input.split_whitespace().any(|word| word == RECALL) {
        Some(Action::Recall)
    } else if input.split_whitespace().any(|word| word == RECORD) {
        Some(Action::Record)
    } else {
        None
    }
}

pub fn skip_persistence(prompt: &str) -> bool {
    current_input(prompt)
        .split_whitespace()
        .any(|word| word == SKIP_PERSISTENCE)
}

pub fn fail_save(prompt: &str) -> bool {
    current_input(prompt)
        .split_whitespace()
        .any(|word| word == FAIL_SAVE)
}

pub fn block_after_save(prompt: &str) -> bool {
    current_input(prompt)
        .split_whitespace()
        .any(|word| word == BLOCK_AFTER_SAVE)
}

pub fn refuse_resume(prompt: &str) -> bool {
    current_input(prompt)
        .split_whitespace()
        .any(|word| word == REFUSE_RESUME)
}

/// Let the browser stop a fake only after its native bytes have been written.
/// The marker lives in a repository beneath the fake's workspace cwd so the
/// public workspace-file route can observe it.
pub fn mark_native_ready(prompt: &str, cwd: &Path, session_id: &str) -> Result<(), String> {
    let Some(raw_path) = current_input(prompt)
        .split_whitespace()
        .find_map(|word| word.strip_prefix(READY_PREFIX))
    else {
        return Ok(());
    };
    let cwd = cwd.canonicalize().map_err(|error| error.to_string())?;
    let path = Path::new(raw_path);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let parent = path.parent().ok_or("native-ready marker has no parent")?;
    let parent = parent.canonicalize().map_err(|error| error.to_string())?;
    if !parent.starts_with(&cwd) {
        return Err("native-ready marker escaped the workspace".into());
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(session_id.as_bytes()))
        .map_err(|error| error.to_string())
}

#[cfg(unix)]
pub fn plant_unsafe_session_file(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink("missing-native-session", path).map_err(|error| error.to_string())
}

pub fn new_fact() -> String {
    format!("{FACT_PREFIX}{}", uuid::Uuid::new_v4())
}

fn read_fact(path: &Path, extract: impl Fn(&Value) -> Option<&str>) -> Option<String> {
    fs::read_to_string(path)
        .ok()?
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|frame| extract(&frame).map(str::to_owned))
        .find(|value| value.starts_with(FACT_PREFIX))
}

pub fn claude_fact(config_dir: &Path, cwd: &Path, session_id: &str) -> Option<String> {
    let path = session_files::claude_transcript_path(config_dir, cwd, session_id);
    read_fact(&path, |frame| {
        (frame["type"] == "user")
            .then_some(())
            .and_then(|()| frame.pointer("/message/content")?.as_array())?
            .iter()
            .find(|block| block["type"] == "tool_result")?
            .get("content")?
            .as_str()
    })
    .or_else(|| {
        let marker = format!("tool-results/{CLAUDE_RESULT_NAME}");
        let reference = fs::read_to_string(&path)
            .ok()?
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|frame| {
                frame
                    .pointer("/message/content/0/content")?
                    .as_str()
                    .map(str::to_owned)
            })
            .find(|content| content.contains(&marker))?;
        let _ = reference;
        let result = path
            .parent()?
            .join(session_id)
            .join("tool-results")
            .join(CLAUDE_RESULT_NAME);
        let fact = fs::read_to_string(result).ok()?;
        fact.starts_with(FACT_PREFIX).then_some(fact)
    })
}

/// Keep the emitted tool result visible to the timeline, but make the saved
/// transcript point at the separate file as Claude does for large outputs.
pub fn externalize_claude_fact(
    config_dir: &Path,
    cwd: &Path,
    session_id: &str,
    transcript: &mut [Value],
) -> Result<(), String> {
    let result = session_files::claude_transcript_path(config_dir, cwd, session_id)
        .parent()
        .ok_or("missing Claude project directory")?
        .join(session_id)
        .join("tool-results")
        .join(CLAUDE_RESULT_NAME);
    for frame in transcript {
        let Some(blocks) = frame
            .pointer_mut("/message/content")
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for block in blocks {
            if block["type"] != "tool_result" {
                continue;
            }
            let Some(fact) = block["content"]
                .as_str()
                .filter(|value| value.starts_with(FACT_PREFIX))
            else {
                continue;
            };
            fs::create_dir_all(result.parent().ok_or("missing tool-results directory")?)
                .map_err(|error| error.to_string())?;
            fs::write(&result, fact).map_err(|error| error.to_string())?;
            block["content"] = serde_json::json!(format!(
                "<persisted-output> Full output saved to: {}",
                result.display()
            ));
            return Ok(());
        }
    }
    Err("fake Claude record turn had no tool-only result to externalize".into())
}

pub fn codex_fact(codex_home: &Path, session_id: &str) -> Option<String> {
    let path = session_files::find_codex_rollout(codex_home, session_id)?;
    read_fact(&path, |frame| {
        (frame.pointer("/payload/type")?.as_str()? == "item.completed"
            && frame.pointer("/payload/item/type")?.as_str()? == "command_execution")
            .then(|| frame.pointer("/payload/item/aggregated_output")?.as_str())
            .flatten()
    })
}

pub fn recall_reply(fact: Option<String>) -> String {
    match fact {
        Some(fact) => format!("Earlier tool-only fact: {fact}"),
        None => "Earlier tool-only fact unavailable.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_directive_does_not_start_a_second_recording() {
        let prompt =
            format!("{RECORD}\nEnd of historical context. Current input follows:\nContinue");
        assert_eq!(action(&prompt), None);
        assert!(!skip_persistence(&prompt));
        assert!(!fail_save(&format!(
            "{FAIL_SAVE}\nEnd of historical context. Current input follows:\nContinue"
        )));
        assert_eq!(action(RECALL), Some(Action::Recall));
        assert!(fail_save(FAIL_SAVE));
    }

    #[test]
    fn native_ready_marker_is_written_only_beneath_the_workspace() {
        let workspace = tempfile::tempdir().unwrap();
        let repo = workspace.path().join("repo");
        fs::create_dir(&repo).unwrap();
        let marker = repo.join("ready.txt");
        let prompt = format!("{BLOCK_AFTER_SAVE} {READY_PREFIX}{}", marker.display());
        mark_native_ready(&prompt, workspace.path(), "native-session").unwrap();
        assert_eq!(fs::read_to_string(marker).unwrap(), "native-session");

        let relative = format!("{READY_PREFIX}repo/relative.txt");
        mark_native_ready(&relative, workspace.path(), "second-session").unwrap();
        assert_eq!(
            fs::read_to_string(repo.join("relative.txt")).unwrap(),
            "second-session"
        );

        let outside = tempfile::tempdir().unwrap();
        let prompt = format!(
            "{READY_PREFIX}{}",
            outside.path().join("ready.txt").display()
        );
        assert!(mark_native_ready(&prompt, workspace.path(), "native-session").is_err());
    }
}
