//! Preview-profile values shared by routes and workspace services.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProfileDTO {
    pub name: String,
    pub command: String,
    pub port: u16,
    pub working_dir: Option<String>,
    pub description: Option<String>,
    pub env_keys: Vec<String>,
    /// Shell command the guest runs after the app stops (Close or self-exit).
    pub cleanup_command: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PreviewApplicationSource {
    WorkspaceDefault,
    NamedProfile,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewApplicationDTO {
    pub source: PreviewApplicationSource,
    pub profile: Option<String>,
    pub config_path: String,
    pub command: String,
    pub port: u16,
    pub working_dir: Option<String>,
    pub description: Option<String>,
    pub env_keys: Vec<String>,
    pub cleanup_command: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreviewProfileInput {
    pub name: String,
    pub command: String,
    pub port: u16,
    pub working_dir: Option<String>,
    pub env: BTreeMap<String, String>,
    pub description: Option<String>,
    pub cleanup_command: Option<String>,
    pub expected_head_commit: Option<String>,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitPreviewProfileResult {
    pub head_commit: String,
    pub profile: PreviewProfileDTO,
}
