//! Deterministic provider fixtures for citation, workspace writes, task completion,
//! question answers, meeting extraction and native-Git publication. Directives
//! apply only to the current turn; recovered history cannot replay side effects.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::Value;

#[path = "native_git_workflows.rs"]
mod native_git;

pub const CITATION_PREFIX: &str = "G5_CITATION_E2E:";
pub const WORKSPACE_WRITE_PREFIX: &str = "G5_FS_WRITE_E2E:";
pub const WORKSPACE_WRITE_SET_PREFIX: &str = "G5_FS_WRITE_SET_E2E:";
pub const IMAGE_SHA256_PREFIX: &str = "G5_IMAGE_SHA256_E2E:";

/// An explicit synthetic-provider assertion used by the public attachment
/// flow. Historical recovery text cannot request a second image delivery.
pub fn verify_image_payload(prompt: &str, images: &[Vec<u8>]) -> Result<(), String> {
    use sha2::{Digest, Sha256};

    let current = prompt
        .rsplit_once(RECOVERY_CURRENT_INPUT_MARKER)
        .map_or(prompt, |(_, current)| current);
    let Some(expected) = current
        .split_whitespace()
        .find_map(|word| word.strip_prefix(IMAGE_SHA256_PREFIX))
    else {
        if prompt.contains(IMAGE_SHA256_PREFIX) && !images.is_empty() {
            return Err("historical image bytes were replayed as current input".into());
        }
        return Ok(());
    };
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("image fixture names an invalid SHA-256 digest".into());
    }
    if images.len() != 1
        || format!("{:x}", Sha256::digest(&images[0])) != expected.to_ascii_lowercase()
    {
        return Err("image fixture did not receive exactly the expected bytes".into());
    }
    Ok(())
}
const WAIT_FOR_FILE_PREFIX: &str = "G5_WAIT_FOR_FILE_E2E:";
pub const TASK_COMPLETE_PREFIX: &str = "G5_TASK_COMPLETE_E2E:";
pub const WORKSPACE_SAVE_DIRECTIVE: &str = "G5_WORKSPACE_SAVE_E2E";
/// `G5_STREAM_DELTAS_E2E:<count>:<bytes_per_delta>[:<delay_ms>]`: the turn's
/// reply streams as `count` text deltas of exactly `bytes_per_delta` bytes
/// each (`stream_delta_text`), `delay_ms` apart, before the usual final text
/// block and result, so the live relay is driven by a long provider answer
/// (chat g5.chat.live-delivery, g5.chat.live-tail-recovery acceptance).
pub const STREAM_DELTAS_PREFIX: &str = "G5_STREAM_DELTAS_E2E:";
/// Five-digit delta indexes keep every delta exactly `bytes_per_delta` bytes.
pub const STREAM_DELTAS_MAX_COUNT: usize = 99_999;
/// `w<index>` plus one trailing space needs at least this many bytes.
pub const STREAM_DELTAS_MIN_BYTES: usize = 8;
pub const STREAM_DELTAS_MAX_BYTES: usize = 4096;
/// The whole reply stays well under the guest's output accounting.
pub const STREAM_DELTAS_MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
pub const STREAM_DELTAS_MAX_DELAY_MS: u64 = 10_000;

/// A parsed `G5_STREAM_DELTAS_E2E` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamDeltas {
    pub count: usize,
    pub bytes_per_delta: usize,
    /// Slept before every delta frame; 0 streams as one burst.
    pub delay_ms: u64,
}

impl StreamDeltas {
    /// The delta texts in order; their concatenation is the reply.
    pub fn deltas(&self) -> Vec<String> {
        (1..=self.count)
            .map(|index| stream_delta_text(index, self.bytes_per_delta))
            .collect()
    }

    /// The reply the deltas concatenate to.
    pub fn text(&self) -> String {
        self.deltas().concat()
    }
}

/// Delta `index` (1-based) of exactly `bytes_per_delta` bytes: `w` and the
/// zero-padded five-digit index, lower-case filler letters, one space. Each
/// delta is a distinct word, so a rendered or stored copy proves order and
/// detects a missing or duplicated delta. The browser flow replicates this
/// (`frontend/e2e/flows/chat-large-response.spec.ts`).
pub fn stream_delta_text(index: usize, bytes_per_delta: usize) -> String {
    const FILLER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    let head = format!("w{index:05}");
    let mut text = head.clone();
    while text.len() + 1 < bytes_per_delta {
        text.push(FILLER[(text.len() - head.len()) % FILLER.len()] as char);
    }
    text.push(' ');
    text
}

/// The prompt's single stream-deltas directive, if any. Malformed values or
/// several directives fail the turn, like the other fixtures.
pub fn stream_deltas_directive(prompt: &str) -> Result<Option<StreamDeltas>, String> {
    let tokens = directive_tokens(prompt, STREAM_DELTAS_PREFIX);
    let token = match tokens.as_slice() {
        [] => return Ok(None),
        [token] => *token,
        _ => return Err(format!("{STREAM_DELTAS_PREFIX} must appear exactly once")),
    };
    let parts: Vec<&str> = token.split(':').collect();
    let (count, bytes, delay) = match parts.as_slice() {
        [count, bytes] => (*count, *bytes, "0"),
        [count, bytes, delay] => (*count, *bytes, *delay),
        _ => {
            return Err(format!(
            "{STREAM_DELTAS_PREFIX} expects <count>:<bytes_per_delta>[:<delay_ms>], got {token:?}"
        ))
        }
    };
    let number = |name: &str, value: &str| -> Result<u64, String> {
        value
            .parse::<u64>()
            .map_err(|_| format!("{STREAM_DELTAS_PREFIX} {name} must be a non-negative integer"))
    };
    let count = number("count", count)? as usize;
    let bytes_per_delta = number("bytes_per_delta", bytes)? as usize;
    let delay_ms = number("delay_ms", delay)?;
    if count == 0 || count > STREAM_DELTAS_MAX_COUNT {
        return Err(format!(
            "{STREAM_DELTAS_PREFIX} count must be 1..={STREAM_DELTAS_MAX_COUNT}"
        ));
    }
    if !(STREAM_DELTAS_MIN_BYTES..=STREAM_DELTAS_MAX_BYTES).contains(&bytes_per_delta) {
        return Err(format!(
            "{STREAM_DELTAS_PREFIX} bytes_per_delta must be {STREAM_DELTAS_MIN_BYTES}..={STREAM_DELTAS_MAX_BYTES}"
        ));
    }
    if count.saturating_mul(bytes_per_delta) > STREAM_DELTAS_MAX_TOTAL_BYTES {
        return Err(format!(
            "{STREAM_DELTAS_PREFIX} count * bytes_per_delta must not exceed {STREAM_DELTAS_MAX_TOTAL_BYTES}"
        ));
    }
    if delay_ms > STREAM_DELTAS_MAX_DELAY_MS {
        return Err(format!(
            "{STREAM_DELTAS_PREFIX} delay_ms must be at most {STREAM_DELTAS_MAX_DELAY_MS}"
        ));
    }
    Ok(Some(StreamDeltas {
        count,
        bytes_per_delta,
        delay_ms,
    }))
}
/// `G5_QUESTION_CLI_E2E:<canonical base64 JSON {path, id, expectedRevision,
/// options}>`: the agent reads one referenced question set and proposes an
/// answer through `g5d question` (`g5.documents.question.agent-access`).
pub const QUESTION_CLI_PREFIX: &str = "G5_QUESTION_CLI_E2E:";
/// `G5_CHAT_GOVERNING_E2E:<canonical base64 JSON {id, reason, file?}>`: after
/// any workspace write of the same turn, the agent names a governing statement
/// through `g5d chat governing` (`g5.chat.ontology-card.governing`).
pub const CHAT_GOVERNING_PREFIX: &str = "G5_CHAT_GOVERNING_E2E:";
/// First line of the built-in `extract-decisions` workbench template
/// (`workbench_builtins/actions.yaml`).
pub const MEETING_EXTRACT_SENTINEL: &str = "Extract the decisions from the meeting transcript.";
/// The serialized `current_doc` context (`workbench_action_message`).
const CURRENT_DOC_PREFIX: &str = "Context: the current document is `";
/// The option ids the template fixes for every meeting decision.
const MEETING_OPTIONS: [(&str, &str); 3] = [
    (
        "attach",
        "Create a task and attach to the current workspace",
    ),
    ("detach", "Create a detached task"),
    ("reject", "Reject"),
];
const REPEATED_COMMIT_PREFIX: &str = "G5_REPEATED_COMMIT_E2E:";
const BUDGET_RESUME_PREFIX: &str = "G5_BUDGET_RESUME_E2E:";
const G5D_BIN_ENV: &str = "G5_FAKE_G5D_BIN";
const TASK_COMPLETE_TIMEOUT: Duration = Duration::from_secs(30);

const PROJECT_DOCS_ALIAS: &str = "project_docs";
const RECOVERY_CURRENT_INPUT_MARKER: &str = "\nEnd of historical context. Current input follows:\n";

/// What a Claude workflow turn replies with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowReply {
    pub text: String,
    pub budget_exhausted: bool,
    /// Citation only: the path the turn "reads" (a `Read` tool call) before
    /// answering.
    pub read_path: Option<String>,
    /// Stream directive only: `text` is emitted as these deltas instead of one.
    pub stream_deltas: Option<StreamDeltas>,
}

/// Run the Claude workflow the prompt selects, if any. `Err` fails the turn
/// (exit 1) with the fixture's error text.
pub fn claude_workflow(
    prompt: &str,
    cwd: &Path,
    max_turns: Option<u64>,
) -> Result<Option<WorkflowReply>, String> {
    // Recovery includes earlier directives as quoted history. The fake
    // provider's deterministic side effects belong only to the current turn.
    let prompt = current_input(prompt);
    if let Some(stream) = stream_deltas_directive(prompt)? {
        return Ok(Some(WorkflowReply {
            text: stream.text(),
            read_path: None,
            budget_exhausted: false,
            stream_deltas: Some(stream),
        }));
    }
    if prompt
        .split_whitespace()
        .any(|token| token == WORKSPACE_SAVE_DIRECTIVE)
    {
        let stdout = run_g5d(cwd, &["workspace".into(), "save".into()], None)?;
        let response: Value =
            serde_json::from_str(&stdout).map_err(|err| format!("invalid Save receipt: {err}"))?;
        let receipt = &response["data"];
        if receipt["saved"] != true || receipt["generation"].as_i64().is_none_or(|n| n <= 0) {
            return Err(format!("Save did not confirm a checkpoint: {stdout}"));
        }
        return Ok(Some(WorkflowReply {
            text: format!(
                "Fake workspace Save completed: generation {}; {}.",
                receipt["generation"],
                boundary()
            ),
            read_path: None,
            budget_exhausted: false,
            stream_deltas: None,
        }));
    }
    if let Some(reply) = question_cli(prompt, cwd)? {
        return Ok(Some(reply));
    }
    if let Some(reply) = meeting_extract(prompt, cwd)? {
        return Ok(Some(reply));
    }
    if let Some(reply) = native_git::budget_resume(prompt, cwd, max_turns)? {
        return Ok(Some(reply));
    }
    if let Some(reply) = native_git::repeated_commit(prompt, cwd)? {
        return Ok(Some(reply));
    }
    let citation = citation(prompt);
    let write = workspace_write(prompt, cwd)?;
    let governing = chat_governing(prompt, cwd)?;
    wait_for_file(prompt, cwd)?;
    let completed = if let Some(summary) = task_complete_directive(prompt)? {
        run_task_complete(&summary, cwd)?;
        true
    } else {
        false
    };
    let boundary = boundary();
    let text = if let Some((_, canonical)) = &citation {
        format!("Citation proof: {canonical}.")
    } else {
        let governing = governing
            .map(|id| format!("governing statement recorded: {id}; "))
            .unwrap_or_default();
        match (write, completed) {
            (Some(relative), true) => format!(
                "Fake workspace write completed: {relative}; {governing}task completion recorded; {boundary}."
            ),
            (Some(relative), false) => {
                format!("Fake workspace write completed: {relative}; {governing}{boundary}.")
            }
            (None, true) => format!("Fake task completion recorded; {governing}{boundary}."),
            (None, false) if !governing.is_empty() => {
                format!("Fake {governing}{boundary}.")
            }
            (None, false) => return Ok(None),
        }
    };
    Ok(Some(WorkflowReply {
        text,
        read_path: citation.map(|(observed, _)| observed),
        budget_exhausted: false,
        stream_deltas: None,
    }))
}

fn current_input(prompt: &str) -> &str {
    prompt
        .rsplit_once(RECOVERY_CURRENT_INPUT_MARKER)
        .map_or(prompt, |(_, current)| current)
}

/// The queue/restart journeys use the same bounded file-write and release
/// directives for Codex. Other Claude workflow commands remain Claude-only.
pub fn codex_workspace_workflow(prompt: &str, cwd: &Path) -> Result<Option<String>, String> {
    let prompt = current_input(prompt);
    let Some(written) = workspace_write(prompt, cwd)? else {
        return Ok(None);
    };
    let governing = chat_governing(prompt, cwd)?
        .map(|id| format!("governing statement recorded: {id}; "))
        .unwrap_or_default();
    wait_for_file(prompt, cwd)?;
    Ok(Some(format!(
        "Fake workspace write completed: {written}; {governing}{}.",
        boundary()
    )))
}

/// The citation directive's `(observed, canonical)` paths (both fakes).
pub fn citation(prompt: &str) -> Option<(String, String)> {
    for token in directive_tokens(prompt, CITATION_PREFIX) {
        let Some((observed, rest)) = token.split_once('|') else {
            continue;
        };
        let canonical = rest.split('|').next().unwrap_or("");
        if !observed.is_empty() && !canonical.is_empty() {
            return Some((observed.to_string(), canonical.to_string()));
        }
    }
    None
}

/// `boundary=g5d=<G5_FAKE_G5D_BIN>,claude=<this executable>`.
fn boundary() -> String {
    let g5d = std::env::var(G5D_BIN_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "unset".to_string());
    let exe = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    format!("boundary=g5d={g5d},claude={exe}")
}

/// Tokens following `prefix` where it starts the prompt or follows
/// whitespace, each running to the next whitespace (the fixtures' regexes).
fn env_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1" | "true" | "yes")
    )
}

fn directive_tokens<'a>(prompt: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut tokens = Vec::new();
    let mut search = 0;
    while let Some(found) = prompt[search..].find(prefix) {
        let start = search + found;
        let at_boundary = prompt[..start]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        let token_start = start + prefix.len();
        let token_end = prompt[token_start..]
            .find(char::is_whitespace)
            .map_or(prompt.len(), |offset| token_start + offset);
        if at_boundary {
            tokens.push(&prompt[token_start..token_end]);
        }
        search = token_start;
    }
    tokens
}

fn is_base64_token(token: &str) -> bool {
    let body = token.trim_end_matches('=');
    token.len() - body.len() <= 2
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/')
}

fn canonical_base64(encoded: &str) -> Option<Vec<u8>> {
    let bytes = STANDARD.decode(encoded).ok()?;
    (STANDARD.encode(&bytes) == encoded).then_some(bytes)
}

fn wait_for_file(prompt: &str, cwd: &Path) -> Result<(), String> {
    let directives = prompt.matches(WAIT_FOR_FILE_PREFIX).count();
    if directives == 0 {
        return Ok(());
    }
    if directives != 1 {
        return Err("fake workspace wait requires exactly one directive".to_string());
    }
    let relative = directive_tokens(prompt, WAIT_FOR_FILE_PREFIX)
        .into_iter()
        .next()
        .ok_or_else(|| "fake workspace wait directive is malformed".to_string())?;
    if relative.is_empty() || Path::new(relative).is_absolute() {
        return Err("fake workspace wait path must be relative".to_string());
    }
    let root = std::env::var_os("G5D_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd.to_path_buf());
    let root = normalize(&root);
    let target = normalize(&root.join(relative));
    if escapes(&root, &target) {
        return Err("fake workspace wait path escapes the workspace".to_string());
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    while !target.is_file() {
        if Instant::now() >= deadline {
            return Err(format!(
                "fake workspace wait timed out after 120 seconds: {relative}"
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn workspace_write(prompt: &str, cwd: &Path) -> Result<Option<String>, String> {
    let sets = prompt.matches(WORKSPACE_WRITE_SET_PREFIX).count();
    if sets > 0 {
        if sets != 1 || prompt.contains(WORKSPACE_WRITE_PREFIX) {
            return Err("fake workspace write set requires exactly one directive".to_string());
        }
        let token = directive_tokens(prompt, WORKSPACE_WRITE_SET_PREFIX)
            .into_iter()
            .find(|token| is_base64_token(token))
            .ok_or_else(|| "fake workspace write set directive is malformed".to_string())?;
        let bytes = canonical_base64(token)
            .ok_or_else(|| "fake workspace write set must be canonical base64".to_string())?;
        let writes: Vec<Value> = serde_json::from_slice(&bytes)
            .map_err(|_| "fake workspace write set must be a JSON array".to_string())?;
        if writes.len() != 2 {
            return Err("fake workspace write set requires two files".to_string());
        }
        let root = std::env::var_os("G5D_WORKSPACE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| cwd.to_path_buf());
        let root = normalize(&root);
        let mut pending = Vec::with_capacity(writes.len());
        for write in writes {
            let relative = write["path"]
                .as_str()
                .ok_or_else(|| "fake workspace write set path is missing".to_string())?;
            let encoded = write["contentBase64"]
                .as_str()
                .ok_or_else(|| "fake workspace write set content is missing".to_string())?;
            if relative.is_empty() || Path::new(relative).is_absolute() {
                return Err("fake workspace write set path must be relative".to_string());
            }
            let target = normalize(&root.join(relative));
            if escapes(&root, &target) {
                return Err("fake workspace write set path escapes the workspace".to_string());
            }
            let content = canonical_base64(encoded).ok_or_else(|| {
                "fake workspace write set content must be canonical base64".to_string()
            })?;
            if pending
                .iter()
                .any(|(prior, _): &(PathBuf, Vec<u8>)| prior == &target)
            {
                return Err("fake workspace write set paths must be distinct".to_string());
            }
            pending.push((target, content));
        }
        for (target, content) in &pending {
            write_file(target, content)
                .map_err(|error| format!("fake workspace write set failed: {error}"))?;
        }
        return Ok(Some("two workspace files".to_string()));
    }
    let directives = prompt.matches(WORKSPACE_WRITE_PREFIX).count();
    if directives == 0 {
        return Ok(None);
    }
    if directives != 1 {
        return Err("fake workspace write requires exactly one directive".to_string());
    }
    let parsed = directive_tokens(prompt, WORKSPACE_WRITE_PREFIX)
        .into_iter()
        .find_map(|token| {
            let (relative, encoded) = token.split_once('|')?;
            (!relative.is_empty() && is_base64_token(encoded)).then_some((relative, encoded))
        });
    let Some((relative, encoded)) = parsed else {
        return Err("fake workspace write directive is malformed".to_string());
    };
    if Path::new(relative).is_absolute() {
        return Err("fake workspace write path must be relative".to_string());
    }
    let root = normalize(cwd);
    let target = normalize(&root.join(relative));
    if escapes(&root, &target) {
        return Err("fake workspace write path escapes the stable workspace".to_string());
    }
    let Some(bytes) = canonical_base64(encoded) else {
        return Err("fake workspace write content must be canonical base64".to_string());
    };
    write_file(&target, &bytes).map_err(|err| format!("fake workspace write failed: {err}"))?;
    Ok(Some(relative.to_string()))
}

fn task_complete_directive(prompt: &str) -> Result<Option<String>, String> {
    let directives = prompt.matches(TASK_COMPLETE_PREFIX).count();
    if directives == 0 {
        return Ok(None);
    }
    if directives != 1 {
        return Err("fake task completion requires exactly one directive".to_string());
    }
    let Some(encoded) = directive_tokens(prompt, TASK_COMPLETE_PREFIX)
        .into_iter()
        .find(|token| is_base64_token(token))
    else {
        return Err("fake task completion directive is malformed".to_string());
    };
    let Some(bytes) = canonical_base64(encoded) else {
        return Err("fake task completion summary must be canonical base64".to_string());
    };
    let Ok(summary) = String::from_utf8(bytes) else {
        return Err("fake task completion summary must be valid UTF-8".to_string());
    };
    if summary.trim().is_empty() {
        return Err("fake task completion summary must be non-empty".to_string());
    }
    Ok(Some(summary))
}

/// Run `$G5_FAKE_G5D_BIN task complete -m <summary>` in `cwd` (inheriting the
/// environment), bounded like the retired fixture (30 s).
/// The referenced-question workflow: `g5d question list --path` then
/// `g5d question answer` with the directive's options, through the g5d on
/// `G5_FAKE_G5D_BIN` or else on PATH (it is on PATH in a workspace VM).
fn question_cli(prompt: &str, cwd: &Path) -> Result<Option<WorkflowReply>, String> {
    let tokens = directive_tokens(prompt, QUESTION_CLI_PREFIX);
    let Some(token) = tokens.first() else {
        return Ok(None);
    };
    if tokens.len() != 1 || !is_base64_token(token) {
        return Err("question CLI fixture needs exactly one canonical base64 directive".into());
    }
    let bytes = canonical_base64(token).ok_or("question CLI directive is not canonical base64")?;
    let spec: Value = serde_json::from_slice(&bytes)
        .map_err(|err| format!("question CLI directive is not JSON: {err}"))?;
    let path = spec["path"]
        .as_str()
        .ok_or("question CLI directive needs path")?;
    let id = spec["id"]
        .as_str()
        .ok_or("question CLI directive needs id")?;
    let revision = spec["expectedRevision"]
        .as_u64()
        .ok_or("question CLI directive needs expectedRevision")?;
    let run = |args: &[String]| run_g5d(cwd, args, None);
    let listed = run(&[
        "question".into(),
        "list".into(),
        "--path".into(),
        path.to_string(),
    ])?;
    if !listed.contains(id) {
        return Err(format!("g5d question list did not show {id}: {listed}"));
    }
    let mut answer = vec![
        "question".to_string(),
        "answer".into(),
        "--path".into(),
        path.to_string(),
        "--id".into(),
        id.to_string(),
        "--expected-revision".into(),
        revision.to_string(),
    ];
    for option in spec["options"].as_array().into_iter().flatten() {
        if let Some(option) = option.as_str() {
            answer.push("--option".into());
            answer.push(option.to_string());
        }
    }
    let answered = run(&answer)?;
    Ok(Some(WorkflowReply {
        text: format!(
            "Fake question proposal recorded: {}; {}.",
            answered.trim(),
            boundary()
        ),
        read_path: None,
        budget_exhausted: false,
        stream_deltas: None,
    }))
}

/// The governing-statement workflow: `g5d chat governing --add <id> --reason
/// <reason> [--file <file>]`, returning the statement ID it recorded.
fn chat_governing(prompt: &str, cwd: &Path) -> Result<Option<String>, String> {
    let tokens = directive_tokens(prompt, CHAT_GOVERNING_PREFIX);
    let Some(token) = tokens.first() else {
        return Ok(None);
    };
    if tokens.len() != 1 || !is_base64_token(token) {
        return Err("governing fixture needs exactly one canonical base64 directive".into());
    }
    let bytes = canonical_base64(token).ok_or("governing directive is not canonical base64")?;
    let spec: Value = serde_json::from_slice(&bytes)
        .map_err(|err| format!("governing directive is not JSON: {err}"))?;
    let id = spec["id"].as_str().ok_or("governing directive needs id")?;
    let reason = spec["reason"]
        .as_str()
        .ok_or("governing directive needs reason")?;
    let mut args = vec![
        "chat".to_string(),
        "governing".into(),
        "--add".into(),
        id.to_string(),
        "--reason".into(),
        reason.to_string(),
    ];
    if let Some(file) = spec["file"].as_str() {
        args.push("--file".into());
        args.push(file.to_string());
    }
    run_g5d(cwd, &args, None)?;
    Ok(Some(id.to_string()))
}

/// Run the g5d on `G5_FAKE_G5D_BIN` (else on PATH, where a workspace VM has
/// it) with `args` and an optional stdin, returning its stdout; a non-zero
/// exit is the fixture's error with the command's stderr.
fn run_g5d(cwd: &Path, args: &[String], stdin: Option<&[u8]>) -> Result<String, String> {
    let binary = std::env::var(G5D_BIN_ENV)
        .ok()
        .filter(|value| Path::new(value).is_absolute())
        .unwrap_or_else(|| "g5d".to_string());
    let mut command = Command::new(&binary);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|err| format!("fake g5d question failed to start: {err}"))?;
    if let Some(bytes) = stdin {
        let mut handle = child.stdin.take().ok_or("fake g5d question has no stdin")?;
        std::io::Write::write_all(&mut handle, bytes)
            .map_err(|err| format!("fake g5d question stdin failed: {err}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("fake g5d question failed: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        Err(format!(
            "fake g5d question failed with {}: {} {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
            stdout.trim()
        ))
    }
}

/// The destination of `relative` inside the workspace's Project Docs folder,
/// reached through the VM's `project_docs` alias: the alias must be a symlink
/// whose canonical target lies inside the canonical workspace, and the
/// destination must stay inside that target. Used by the meeting fixture; `what` names the fixture in errors.
fn project_docs_destination(cwd: &Path, relative: &str, what: &str) -> Result<PathBuf, String> {
    let root = normalize(cwd);
    let alias = root.join(PROJECT_DOCS_ALIAS);
    let invalid_path = || format!("fake {what} output path is invalid");
    if escapes(&alias, &normalize(&alias.join(relative))) {
        return Err(invalid_path());
    }
    let unavailable = || format!("fake {what} Project Docs alias is unavailable");
    let metadata = std::fs::symlink_metadata(&alias).map_err(|_| unavailable())?;
    if !metadata.file_type().is_symlink() {
        return Err(format!("fake {what} Project Docs alias is invalid"));
    }
    let canonical_root = std::fs::canonicalize(&root).map_err(|_| unavailable())?;
    let docs_root = std::fs::canonicalize(&alias).map_err(|_| unavailable())?;
    if escapes(&canonical_root, &docs_root) {
        return Err(format!("fake {what} Project Docs alias is invalid"));
    }
    let destination = normalize(&docs_root.join(relative));
    if escapes(&docs_root, &destination) {
        return Err(invalid_path());
    }
    Ok(destination)
}

/// The review-meeting extraction template (`g5.workbench.meeting`): read the
/// transcript path from the `current_doc` context, create the meeting's
/// question set through `g5d question create` (two fixed decisions, the three
/// meeting options, free text for the comment) and write
/// `meetings/<name>.decisions.md` beside the transcript through the
/// `project_docs` alias, one `g5-question-ref` fence per question. Nothing is
/// answered: the owner decides from the cards. For this fixture, the alias must be the VM's symlink to the workspace's Project
/// Docs folder.
fn meeting_extract(prompt: &str, cwd: &Path) -> Result<Option<WorkflowReply>, String> {
    if !prompt.contains(MEETING_EXTRACT_SENTINEL) {
        return Ok(None);
    }
    let transcript = prompt
        .split(CURRENT_DOC_PREFIX)
        .nth(1)
        .and_then(|rest| rest.split('`').next())
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or("fake meeting extraction needs the current document context")?;
    let mut parts = transcript.split('/');
    let (Some("docs"), Some(workspace), Some("meetings"), Some(file), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return Err(format!(
            "fake meeting extraction needs a transcript under docs/<workspace>/meetings/: {transcript}"
        ));
    };
    if !is_canonical_uuid(workspace) {
        return Err("fake meeting extraction transcript workspace is invalid".to_string());
    }
    let stem = file
        .strip_suffix(".md")
        .filter(|stem| !stem.is_empty() && !stem.contains(".."))
        .ok_or("fake meeting extraction transcript must be a Markdown document")?;
    let slug: String = stem
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    // The template's rule, exactly: the first 56 characters of the slug, so
    // the set id stays within the store's 64-character limit with its prefix.
    let slug: String = slug.chars().take(56).collect();
    let set_id = format!("meeting-{slug}");
    let title = format!("Meeting {stem}");
    let options: Vec<Value> = MEETING_OPTIONS
        .iter()
        .map(|(id, label)| serde_json::json!({"id": id, "label": label}))
        .collect();
    // Stable ids: a re-run of the template over the same transcript edits
    // these entries instead of appending two more, as the template asks.
    let questions = serde_json::json!([
        {
            "id": "timeline-landing-page",
            "prompt": "The project timeline becomes the landing page",
            "context": format!(
                "Detail: the overview page goes away; the timeline shows status and history. Decided by Tim at 00:28. Target: docs/{workspace}/requirements.md"
            ),
            "allowFreeText": true,
            "options": options,
        },
        {
            "id": "ribbon-templated-chats",
            "prompt": "Ribbon buttons are templated chats, not bespoke features",
            "context": format!(
                "Detail: each button starts a predefined prompt under a persona. Decided by Tim at 00:19. Target: docs/{workspace}/requirements.md"
            ),
            "allowFreeText": true,
            "options": options,
        },
    ]);
    // Like an agent whose command failed, a failure from here on is the
    // turn's reply rather than a crashed provider: the worker publishes no
    // provider stderr, so this is the only place the error is visible.
    let relative = format!("meetings/{stem}.decisions.md");
    let record = || -> Result<WorkflowReply, String> {
        // The alias is checked before g5d writes anything, so a bad alias
        // creates no set.
        let destination = project_docs_destination(cwd, &relative, "meeting extraction")?;
        let args: Vec<String> = [
            "--json", "question", "create", "--set", &set_id, "--title", &title, "--file", "-",
        ]
        .iter()
        .map(|arg| arg.to_string())
        .collect();
        let stdout = run_g5d(cwd, &args, Some(questions.to_string().as_bytes()))?;
        let created: Value = serde_json::from_str(&stdout)
            .map_err(|err| format!("fake g5d question create output is not JSON: {err}"))?;
        let set_path = created["data"]["set"]["path"]
            .as_str()
            .filter(|path| !path.is_empty())
            .ok_or("fake g5d question create output names no set path")?
            .to_string();
        let ids: Vec<String> = created["data"]["questionIds"]
            .as_array()
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if ids.len() != 2 {
            return Err(format!(
                "fake g5d question create returned {} question ids, expected 2",
                ids.len()
            ));
        }
        let mut lines = vec![
            format!("# Decisions — {title}"),
            String::new(),
            format!("Transcript: [{file}](./{file})"),
            String::new(),
        ];
        for (index, (id, question)) in ids
            .iter()
            .zip(questions.as_array().into_iter().flatten())
            .enumerate()
        {
            lines.push(format!(
                "[Q{}. {}]",
                index + 1,
                question["prompt"].as_str().unwrap_or("")
            ));
            lines.push(String::new());
            lines.push("```g5-question-ref".to_string());
            lines.push(serde_json::json!({"path": set_path, "id": id}).to_string());
            lines.push("```".to_string());
            lines.push(String::new());
        }
        let markdown = lines.join("\n");
        write_file(&destination, markdown.as_bytes())
            .map_err(|err| format!("fake meeting extraction write failed: {err}"))?;
        Ok(WorkflowReply {
        text: format!(
            "Fake meeting decisions recorded: docs/{workspace}/{relative}; {} decisions in {set_path}; {}.",
            ids.len(),
            boundary()
        ),
        read_path: None,
        budget_exhausted: false,
        stream_deltas: None,
    })
    };
    Ok(Some(record().unwrap_or_else(|err| WorkflowReply {
        text: format!("Fake meeting extraction failed: {err}; {}.", boundary()),
        read_path: None,
        budget_exhausted: false,
        stream_deltas: None,
    })))
}

fn run_task_complete(summary: &str, cwd: &Path) -> Result<(), String> {
    let binary = std::env::var(G5D_BIN_ENV).unwrap_or_default();
    if binary.trim().is_empty() || !Path::new(&binary).is_absolute() {
        return Err(
            "fake task completion requires G5_FAKE_G5D_BIN to be an absolute executable path"
                .to_string(),
        );
    }
    let mut child = Command::new(&binary)
        .args(["task", "complete", "-m", summary])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("fake g5d task completion failed: {err}"))?;
    let stdout = child.stdout.take().map(read_in_background);
    let stderr = child.stderr.take().map(read_in_background);
    let deadline = Instant::now() + TASK_COMPLETE_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("fake g5d task completion failed: timed out after 30 s".to_string());
            }
            Err(err) => return Err(format!("fake g5d task completion failed: {err}")),
        }
    };
    if status.success() {
        return Ok(());
    }
    let mut details = vec![format!("command failed with {status}")];
    for (label, handle) in [("stderr", stderr), ("stdout", stdout)] {
        let text = handle.map(joined_text).unwrap_or_default();
        if !text.trim().is_empty() {
            details.push(format!("{label}: {}", text.trim()));
        }
    }
    Err(format!(
        "fake g5d task completion failed: {}",
        details.join("; ")
    ))
}

fn read_in_background<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        buffer
    })
}

fn joined_text(handle: std::thread::JoinHandle<Vec<u8>>) -> String {
    String::from_utf8_lossy(&handle.join().unwrap_or_default()).into_owned()
}

fn is_canonical_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, len)| {
            group.len() == len && group.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
        })
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// True unless `target` lies strictly inside `root`.
fn escapes(root: &Path, target: &Path) -> bool {
    target
        .strip_prefix(root)
        .map_or(true, |rest| rest.as_os_str().is_empty())
}

fn write_file(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, bytes)
}

/// Deterministic formal-review fixture: real MCP reads and the sole output
/// writer, with an explicit synthetic verdict marker confined to test prompts.
/// The schema 6 report's last section from the frozen assignment: each
/// repository's name and ID with its source, target, workspace-base and
/// merge-base cuts. The synthetic policy names no domains or posture.
fn review_scope(assignment: &Value) -> String {
    let head = |key: &str, repo: &str| {
        assignment[key]
            .as_array()
            .and_then(|heads| heads.iter().find(|head| head["repoId"] == repo))
            .and_then(|head| head["headCommit"].as_str())
            .unwrap_or("unavailable")
            .to_string()
    };
    let merge_base = |repo: &str| {
        assignment["mergeBase"]["repositories"]
            .as_array()
            .and_then(|repos| repos.iter().find(|entry| entry["repoId"] == repo))
            .and_then(|entry| entry["gitSha"].as_str())
            .unwrap_or("unavailable")
            .to_string()
    };
    let repositories = assignment["sourceHeads"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|head| head["repoId"].as_str())
        .map(|repo| {
            let name = assignment["repositoryNames"][repo].as_str().unwrap_or(repo);
            format!(
                "- {name} ({repo}): source {}, target {}, workspace base {}, merge base {}.",
                head("sourceHeads", repo),
                head("targetHeads", repo),
                head("workspaceBaseHeads", repo),
                merge_base(repo),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Scope: the assigned merge-base to source comparison.\n\n{repositories}\n\nDomains and posture: not specified by the synthetic review policy."
    )
}

pub fn formal_approval(
    prompt: &str,
    servers: &std::collections::BTreeMap<String, crate::mcp_config::McpServerSpec>,
) -> Result<Option<String>, String> {
    let result = if prompt.contains("G5_FORMAL_APPROVAL_E2E:approved") {
        "approved"
    } else if prompt.contains("G5_FORMAL_APPROVAL_E2E:rejected") {
        "rejected"
    } else {
        return Ok(None);
    };
    let spec = servers
        .get("formal_review")
        .ok_or("formal approval MCP surface missing")?;
    let mut client = crate::mcp::McpClient::connect(spec, "formal-review-test-provider")
        .map_err(|e| e.to_string())?;
    if let Some((_, assignment)) = prompt.split_once("Immutable assignment metadata (read only):") {
        let metadata = assignment
            .split_once("```json\n")
            .and_then(|(_, s)| s.split_once("\n```"))
            .map(|(s, _)| s)
            .ok_or("assignment missing")?;
        let assignment: Value = serde_json::from_str(metadata).map_err(|e| e.to_string())?;
        let comparison = client
            .call_tool(
                "read_review",
                &serde_json::json!({"operation":"comparison"}),
                Duration::from_secs(30),
            )
            .map_err(|e| e.to_string())?;
        if comparison.is_error {
            return Err(comparison.text());
        }
        for source in assignment["sourceHeads"]
            .as_array()
            .ok_or("source missing")?
        {
            let read = client
                .call_tool(
                    "read_review",
                    &serde_json::json!({"operation":"diff","repoId":source["repoId"]}),
                    Duration::from_secs(30),
                )
                .map_err(|e| e.to_string())?;
            if read.is_error {
                return Err(read.text());
            }
        }
        let mut report = serde_json::json!({"result":result,"body":"Synthetic provider transport inspected the frozen comparison and captured Git objects through the real confined tools."});
        // Schema 6 orders the report body under fixed headings.
        if assignment["schemaVersion"] == 6 {
            // A rejection names the synthetic criterion that blocks it, as the
            // v5 guidance requires; an approval has no blocking findings.
            let (criteria, blocking) = if result == "rejected" {
                (
                    "Synthetic policy criterion — FAILED: the delimited policy requires this review to reject. Evidence: the frozen comparison and captured Git objects were read through the real confined tools.",
                    "1. The synthetic policy criterion failed.",
                )
            } else {
                (
                    "Synthetic policy criterion — PASSED. Evidence: the frozen comparison and captured Git objects were read through the real confined tools.",
                    "None.",
                )
            };
            report["body"] = serde_json::json!(format!(
                "## Governing prompt\n\nThe delimited synthetic review policy.\n\n## Verdict\n\n{result}\n\n## Required criteria and supporting evidence\n\n{criteria}\n\n## Blocking findings\n\n{blocking}\n\n## Advisory findings\n\nNone.\n\n## Waivers and non-applicable dimensions\n\nNone.\n\n## Limitations\n\nSynthetic provider transport.\n\n## Review scope and provenance\n\n{scope}",
                scope = review_scope(&assignment),
            ));
        }
        // Schemas 5 and 6 add the separately authored summary.
        if matches!(assignment["schemaVersion"].as_u64(), Some(5 | 6)) {
            report["summary"] = serde_json::json!(format!(
                "Synthetic review {result}: the frozen merge-base comparison was read through the confined tools."
            ));
        }
        let written = client
            .call_tool("write_report", &report, Duration::from_secs(30))
            .map_err(|e| e.to_string())?;
        if written.is_error {
            return Err(written.text());
        }
        client.shutdown();
        return Ok(Some("Assigned formal review artifact complete.".into()));
    }
    let source = prompt
        .split_once("Source cut (authoritative):")
        .and_then(|(_, s)| s.split_once("```json\n"))
        .and_then(|(_, s)| s.split_once("\n```"))
        .map(|(s, _)| s)
        .ok_or("source vector missing")?;
    let source: Vec<Value> = serde_json::from_str(source).map_err(|e| e.to_string())?;
    for repo in source {
        for arguments in [
            serde_json::json!({"repoId":repo["repoId"],"operation":"paths"}),
            serde_json::json!({"repoId":repo["repoId"],"operation":"diff"}),
        ] {
            let read = client
                .call_tool("read_review", &arguments, Duration::from_secs(30))
                .map_err(|e| e.to_string())?;
            if read.is_error {
                return Err(format!("formal review read failed: {}", read.text()));
            }
        }
    }
    let assigned_skeleton = [3, 2].into_iter().find_map(|schema| {
        let heading = format!("Canonical schemaVersion {schema} frontmatter — {result} outcome:");
        prompt.split_once(&heading)
    });
    let skeleton = assigned_skeleton
        .and_then(|(_, s)| s.split_once("```yaml\n"))
        .and_then(|(_, s)| s.split_once("\n```"))
        .map(|(s, _)| s)
        .ok_or("canonical skeleton missing")?;
    let path = prompt
        .split_once("- Required output path: `")
        .and_then(|(_, s)| s.split_once('`'))
        .map(|(s, _)| s)
        .ok_or("assigned output path missing")?;
    let document=format!("{skeleton}\n\nSynthetic provider transport inspected the assigned Git-object inventory and diff through the real confined tools. Fixture verdict: {result}.\n");
    let written = client
        .call_tool(
            "write_report",
            &serde_json::json!({"path":path,"document":document}),
            Duration::from_secs(30),
        )
        .map_err(|e| e.to_string())?;
    if written.is_error {
        return Err(format!("formal output refused: {}", written.text()));
    }
    client.shutdown();
    Ok(Some("Assigned formal review artifact complete.".into()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn image_fixture_checks_exact_current_bytes() {
        use sha2::{Digest, Sha256};
        let image = b"image".to_vec();
        let digest = format!("{:x}", Sha256::digest(&image));
        let prompt = format!("Inspect G5_IMAGE_SHA256_E2E:{digest}");
        assert!(super::verify_image_payload(&prompt, std::slice::from_ref(&image)).is_ok());
        assert!(super::verify_image_payload(&prompt, &[b"wrong".to_vec()]).is_err());
        assert!(super::verify_image_payload(&prompt, &[]).is_err());
        let historical = format!("{prompt}{}Continue", super::RECOVERY_CURRENT_INPUT_MARKER);
        assert!(super::verify_image_payload(&historical, &[]).is_ok());
        assert!(super::verify_image_payload(&historical, &[image]).is_err());
    }
    use super::*;

    fn b64(text: &str) -> String {
        STANDARD.encode(text)
    }

    #[test]
    fn citation_reads_the_first_complete_directive() {
        assert_eq!(
            citation("see G5_CITATION_E2E:repo/a.rs|docs/x/a.rs please"),
            Some(("repo/a.rs".to_string(), "docs/x/a.rs".to_string()))
        );
        assert_eq!(citation("xG5_CITATION_E2E:a|b"), None, "needs a boundary");
        assert_eq!(citation("G5_CITATION_E2E:a|"), None);
    }

    #[test]
    fn task_completion_directive_validates_like_the_fixture() {
        assert_eq!(
            task_complete_directive(&format!("do it G5_TASK_COMPLETE_E2E:{}", b64("all done")))
                .unwrap(),
            Some("all done".to_string())
        );
        assert_eq!(task_complete_directive("nothing here").unwrap(), None);
        assert!(task_complete_directive("G5_TASK_COMPLETE_E2E:%%")
            .unwrap_err()
            .contains("malformed"));
        assert!(task_complete_directive("G5_TASK_COMPLETE_E2E:YQ")
            .unwrap_err()
            .contains("canonical base64"));
        assert!(task_complete_directive(&format!(
            "G5_TASK_COMPLETE_E2E:{} G5_TASK_COMPLETE_E2E:{}",
            b64("a"),
            b64("b")
        ))
        .unwrap_err()
        .contains("exactly one"));
        assert!(
            task_complete_directive(&format!("G5_TASK_COMPLETE_E2E:{}", b64("  ")))
                .unwrap_err()
                .contains("non-empty")
        );
    }

    #[test]
    fn stream_deltas_directive_parses_count_bytes_and_optional_delay() {
        assert_eq!(
            stream_deltas_directive("please G5_STREAM_DELTAS_E2E:10000:24 now").unwrap(),
            Some(StreamDeltas {
                count: 10_000,
                bytes_per_delta: 24,
                delay_ms: 0
            })
        );
        assert_eq!(
            stream_deltas_directive("G5_STREAM_DELTAS_E2E:3:8:250").unwrap(),
            Some(StreamDeltas {
                count: 3,
                bytes_per_delta: 8,
                delay_ms: 250
            })
        );
        assert_eq!(stream_deltas_directive("nothing here").unwrap(), None);
        assert_eq!(
            stream_deltas_directive("xG5_STREAM_DELTAS_E2E:3:8").unwrap(),
            None,
            "needs a boundary"
        );
        for malformed in [
            "G5_STREAM_DELTAS_E2E:3",
            "G5_STREAM_DELTAS_E2E:3:8:1:2",
            "G5_STREAM_DELTAS_E2E:x:8",
            "G5_STREAM_DELTAS_E2E:0:8",
            "G5_STREAM_DELTAS_E2E:100000:8",
            "G5_STREAM_DELTAS_E2E:3:7",
            "G5_STREAM_DELTAS_E2E:3:4097",
            "G5_STREAM_DELTAS_E2E:99999:4096",
            "G5_STREAM_DELTAS_E2E:3:8:10001",
            "G5_STREAM_DELTAS_E2E:3:8 G5_STREAM_DELTAS_E2E:4:8",
        ] {
            assert!(
                stream_deltas_directive(malformed).is_err(),
                "{malformed} should be rejected"
            );
        }
        // Recovery history never replays the directive.
        let historical = format!(
            "G5_STREAM_DELTAS_E2E:3:8{}Continue",
            RECOVERY_CURRENT_INPUT_MARKER
        );
        let reply = claude_workflow(&historical, Path::new("/nonexistent"), None).unwrap();
        assert_eq!(reply, None);
    }

    #[test]
    fn stream_delta_text_is_exact_sized_distinct_and_concatenates_to_the_reply() {
        assert_eq!(stream_delta_text(1, 8), "w00001a ");
        assert_eq!(stream_delta_text(12, 24), "w00012abcdefghijklmnopq ");
        assert_eq!(stream_delta_text(99_999, 8), "w99999a ");
        let spec = StreamDeltas {
            count: 10_000,
            bytes_per_delta: 24,
            delay_ms: 0,
        };
        let deltas = spec.deltas();
        assert_eq!(deltas.len(), 10_000);
        assert!(deltas.iter().all(|delta| delta.len() == 24));
        let distinct: std::collections::HashSet<&String> = deltas.iter().collect();
        assert_eq!(distinct.len(), deltas.len());
        assert_eq!(spec.text(), deltas.concat());
        assert_eq!(spec.text().len(), 240_000);
        let reply = claude_workflow(
            "G5_STREAM_DELTAS_E2E:10000:24",
            Path::new("/nonexistent"),
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(reply.text, spec.text());
        assert_eq!(reply.stream_deltas, Some(spec));
        assert_eq!(reply.read_path, None);
        assert!(!reply.budget_exhausted);
    }

    #[test]
    fn paths_that_leave_the_root_escape() {
        let root = Path::new("/work");
        assert!(escapes(root, &normalize(&root.join("../etc"))));
        assert!(escapes(root, &normalize(&root.join("."))));
        assert!(!escapes(root, &normalize(&root.join("a/../b.txt"))));
    }

    #[test]
    fn review_scope_reports_each_repository_cut_from_the_assignment() {
        let assignment = serde_json::json!({
            "sourceHeads": [{"repoId": "r1", "headCommit": "s1"}],
            "targetHeads": [{"repoId": "r1", "headCommit": "t1"}],
            "workspaceBaseHeads": [{"repoId": "r1", "headCommit": "w1"}],
            "mergeBase": {"repositories": [{"repoId": "r1", "repoName": "G5-Lean", "gitSha": "m1"}]},
            "repositoryNames": {"r1": "G5-Lean"},
        });
        let scope = super::review_scope(&assignment);
        assert!(scope
            .contains("- G5-Lean (r1): source s1, target t1, workspace base w1, merge base m1."));
        assert!(scope.contains("not specified by the synthetic review policy"));
    }
}
