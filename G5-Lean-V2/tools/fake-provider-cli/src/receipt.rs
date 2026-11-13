//! The receipt a fake writes to `G5_FAKE_RECEIPT` so a test can assert what
//! the runner actually sent: argv, working directory, the prompt read from
//! stdin, the MCP calls made and the files written.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

/// Environment variables copied into the receipt by value. Credentials are
/// reported by presence only (see [`Receipt::credentials_present`]).
pub const RECORDED_ENV: &[&str] = &[
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "MCP_TOOL_TIMEOUT",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "OPENAI_BASE_URL",
    "HOME",
    "G5D_SESSION_ID",
    "G5D_API_URL",
    "G5_FAKE_SCENARIO",
];

/// Credential variables whose presence (never value) is recorded.
pub const CREDENTIAL_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "OPENROUTER_API_KEY",
    "G5D_TOKEN",
];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct McpCallRecord {
    pub server: String,
    pub tool: String,
    pub arguments: Value,
    pub is_error: bool,
    pub content: Value,
    pub transport_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PermissionRecord {
    pub tool_name: String,
    pub input: Value,
    pub prompt_tool: Option<String>,
    pub behavior: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub provider: String,
    pub version: String,
    pub argv: Vec<String>,
    pub cwd: String,
    /// Parsed options (Claude: canonical long names; Codex: field names).
    pub options: Value,
    pub positional: Vec<String>,
    pub stdin: String,
    /// Parsed `stream-json` input messages (Claude) or an empty list.
    pub stdin_messages: Vec<Value>,
    pub prompt: String,
    pub session_id: String,
    pub resume: Option<String>,
    pub env: BTreeMap<String, String>,
    pub credentials_present: Vec<String>,
    pub mcp_calls: Vec<McpCallRecord>,
    pub permissions: Vec<PermissionRecord>,
    pub written_files: Vec<String>,
    pub frames_emitted: usize,
    pub exit_code: i32,
}

impl Receipt {
    /// Start a receipt for `provider` with the environment snapshot taken now.
    pub fn new(provider: &str, version: &str, argv: Vec<String>, cwd: &Path) -> Self {
        let env = RECORDED_ENV
            .iter()
            .filter_map(|key| {
                std::env::var(key)
                    .ok()
                    .map(|value| (key.to_string(), value))
            })
            .collect();
        let credentials_present = CREDENTIAL_ENV
            .iter()
            .filter(|key| std::env::var(key).is_ok_and(|value| !value.is_empty()))
            .map(|key| key.to_string())
            .collect();
        Self {
            provider: provider.to_string(),
            version: version.to_string(),
            argv,
            cwd: cwd.display().to_string(),
            options: Value::Null,
            positional: Vec::new(),
            stdin: String::new(),
            stdin_messages: Vec::new(),
            prompt: String::new(),
            session_id: String::new(),
            resume: None,
            env,
            credentials_present,
            mcp_calls: Vec::new(),
            permissions: Vec::new(),
            written_files: Vec::new(),
            frames_emitted: 0,
            exit_code: 0,
        }
    }

    /// Write the receipt to the path named by `G5_FAKE_RECEIPT`, if set.
    pub fn write_if_requested(&self) -> Result<(), String> {
        let Some(path) = std::env::var_os("G5_FAKE_RECEIPT") else {
            return Ok(());
        };
        let path = Path::new(&path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("cannot create {}: {err}", parent.display()))?;
            }
        }
        let text = serde_json::to_string_pretty(self).expect("receipt serialises");
        std::fs::write(path, text)
            .map_err(|err| format!("cannot write receipt {}: {err}", path.display()))
    }
}
