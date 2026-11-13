//! Subscription login control. Device codes use the private HTTP channel, never
//! the dispatcher journal. Long-lived Codex credentials never leave the VM.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DEVICE_AUTH_URL: &str = "https://auth.openai.com/codex/device";
pub const DEVICE_AUTH_WAIT_MS: u64 = 300_000;

#[derive(Clone, Serialize, Deserialize)]
pub struct CodexAuthUpdate {
    pub task_id: Uuid,
    pub attempt: i32,
    #[serde(flatten)]
    pub update: CodexAuthPhase,
}

// Deliberately no Debug: Waiting contains a one-time secret.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum CodexAuthPhase {
    Checking,
    Waiting {
        user_code: String,
        deadline_unix_ms: u64,
    },
    Ready,
    Ended {
        code: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CodexAuthDisposition {
    pub proceed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

pub fn valid_device_code(code: &str) -> bool {
    (8..=16).contains(&code.len())
        && code.contains('-')
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
}

pub fn auth_notice(code: &str) -> &'static str {
    match code {
        "codex_auth_timeout" => "Device-code authorization timed out. Send any message to retry.",
        // Historical: attempts a message cancelled before follow-ups queued
        // during login; nothing writes this code for new attempts.
        "codex_auth_superseded" => "Device-code authorization was canceled because a new message arrived. Retrying with the new message.",
        "codex_auth_busy" => "Codex sign-in is already in progress in another chat. Finish that sign-in, then send a message here to retry.",
        "codex_auth_disabled" => "Device-code authorization is disabled for this ChatGPT account or workspace. Enable it in ChatGPT security settings, then send any message to retry.",
        "codex_auth_interrupted" => "Device-code authorization was interrupted. Send any message to retry.",
        _ => "Codex sign-in could not be completed. Send any message to retry.",
    }
}
