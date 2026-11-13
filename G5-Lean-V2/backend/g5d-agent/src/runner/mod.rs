//! Provider wire adapters and session-file helpers shared with `agent.turn`.
//! Process launch, bounded IO, deadlines, and outcome settlement belong to
//! the `agent.turn` supervisor in `runtime` under dispatcher custody.

pub mod claude;
pub mod codex;
pub mod codex_app_server;
pub mod events;
pub mod session_files;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use events::{ProjectionState, ProviderLabel, TimelineProjection, TurnEvent, TurnUsage};

/// The two provider CLIs supported by these wire adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    ClaudeCode,
    Codex,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::ClaudeCode => "claude_code",
            Provider::Codex => "codex",
        }
    }

    pub fn label(self) -> ProviderLabel {
        match self {
            Provider::ClaudeCode => ProviderLabel::CLAUDE,
            Provider::Codex => ProviderLabel::CODEX,
        }
    }
}

/// A file attached to the turn (already on disk where the process can read
/// it; image attachments become content blocks for Claude and `-i` files for
/// Codex).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub path: PathBuf,
    pub media_type: Option<String>,
}
