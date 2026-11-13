//! Workspace configuration, CLI value types, and compatibility errors.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use g5d_foundation::semlift_config::{GroundingConfig, ViewsConfig};

// ---------------------------------------------------------------------------
// G5dError — g5d error type (config-relevant variants)
// ---------------------------------------------------------------------------

/// Error type for g5d operations.
///
/// For this module, only `Config` and `WorkspaceConfig` variants are used.
/// Other variants will be added by other modules.
#[derive(Debug, Error)]
pub enum G5dError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("workspace config error: {0}")]
    WorkspaceConfig(String),
}

/// Compatibility shape for the partial `semlift.yaml` configuration.
///
/// The dependency-neutral runtime crate owns the parser, while this outer CLI
/// boundary preserves the historical `G5dError::Config` load signature.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemliftConfig {
    /// View workflow behavior.
    #[serde(default)]
    pub views: ViewsConfig,
    /// Grounding workflow behavior.
    #[serde(default)]
    pub grounding: GroundingConfig,
}

impl SemliftConfig {
    /// Load the partial configuration while preserving the public g5d error contract.
    pub fn load(workspace_path: &Path) -> Result<Option<Self>, G5dError> {
        g5d_foundation::semlift_config::SemliftConfig::load(workspace_path)
            .map(|config| config.map(Self::from))
            .map_err(|error| G5dError::Config(error.to_string()))
    }
}

impl From<g5d_foundation::semlift_config::SemliftConfig> for SemliftConfig {
    fn from(config: g5d_foundation::semlift_config::SemliftConfig) -> Self {
        Self {
            views: config.views,
            grounding: config.grounding,
        }
    }
}

// ---------------------------------------------------------------------------
// Role — worker role enum
// ---------------------------------------------------------------------------

/// Role this g5d instance runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Worker role: executes agent tasks, runs ReAct loop, provides dev environment.
    Worker,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Worker => write!(f, "worker"),
        }
    }
}

impl FromStr for Role {
    type Err = G5dError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "worker" => Ok(Role::Worker),
            other => Err(G5dError::Config(format!(
                "unrecognized role: '{}'. Expected 'worker'",
                other
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// IsolationMode — re-exported from the canonical isolation module
// ---------------------------------------------------------------------------

/// Re-export the canonical `IsolationMode` enum. The single definition lives
/// in `crate::cli::isolation::mod` and is shared by the worker and isolation
/// manager so there is exactly one enum across the daemon.
pub use g5d_isolation::IsolationMode;

// ---------------------------------------------------------------------------
// CloneMethod — clone strategy enum
// ---------------------------------------------------------------------------

/// Clone method for workspace repos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloneMethod {
    /// Copy-on-write clone (APFS clonefile / XFS reflinks). Preferred.
    Cow,
    /// Full git clone (fallback when COW unsupported).
    Full,
}

impl fmt::Display for CloneMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloneMethod::Cow => write!(f, "cow"),
            CloneMethod::Full => write!(f, "full"),
        }
    }
}

impl FromStr for CloneMethod {
    type Err = G5dError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cow" => Ok(CloneMethod::Cow),
            "full" => Ok(CloneMethod::Full),
            other => Err(G5dError::Config(format!(
                "unrecognized clone method: '{}'. Expected 'cow' or 'full'",
                other
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// ForkStrategy — fork strategy enum
// ---------------------------------------------------------------------------

/// Fork strategy for workspace repos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForkStrategy {
    Fork,
    Mirror,
    None,
}

impl fmt::Display for ForkStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ForkStrategy::Fork => write!(f, "fork"),
            ForkStrategy::Mirror => write!(f, "mirror"),
            ForkStrategy::None => write!(f, "none"),
        }
    }
}

impl FromStr for ForkStrategy {
    type Err = G5dError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fork" => Ok(ForkStrategy::Fork),
            "mirror" => Ok(ForkStrategy::Mirror),
            "none" => Ok(ForkStrategy::None),
            other => Err(G5dError::Config(format!(
                "unrecognized fork strategy: '{}'. Expected 'fork', 'mirror', or 'none'",
                other
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// WorkspaceConfig — .g5/workspace.yaml schema
// ---------------------------------------------------------------------------

/// Represents the `.g5/workspace.yaml` file structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub remote: WorkspaceRemoteConfig,
    #[serde(default)]
    pub clone: WorkspaceCloneConfig,
    #[serde(default)]
    pub fork: WorkspaceForkConfig,
    #[serde(default)]
    pub isolation: WorkspaceIsolationConfig,
    #[serde(default)]
    pub cache: WorkspaceCacheConfig,
    #[serde(default)]
    pub docs: Option<WorkspaceDocsConfig>,
    #[serde(default)]
    pub repos: Vec<WorkspaceRepoEntry>,
    /// `preview:` block — the customer's app boot config for first-class
    /// `preview` tasks (`g5/g5d/host` D9, `g5/g5d/cli` D34). Present only when the
    /// workspace opts into previews. `g5d preview-serve` reads it authoritatively
    /// in-VM at boot; the host supervisor reads only `port` to configure forwarding.
    /// Absent (`None`) for workspaces that do not declare a preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewWorkspaceConfig>,
    /// Top-level `base_image:` entry — the workspace VM guest image
    /// (`software_architecture/g5d/config` D33). The
    /// highest-precedence source in the `workspace.yaml` > reserved
    /// `org.yaml` > database-pin fold
    /// (`system_invariants/microvm_execution_boundary` D13). This module only
    /// parses it; the host supervisor resolves it before workspace
    /// materialization, at the g5d-host resolution seam
    /// (`assignment_translation` D25) — it is deliberately NOT merged into
    /// `G5dConfig`/`IsolationConfig.base_image`, which configure the host
    /// process's own default. Absent (`None`) means the workspace expresses no
    /// image preference and the database pin applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_image: Option<String>,
}

/// The `.g5/workspace.yaml` `preview:` block — how a `preview` task boots the
/// customer's app inside the workspace VM (`g5/g5d/host` D9, `g5/g5d/cli`
/// D34). All values are the customer's own, non-secret config: NO G5 credential
/// is ever placed here, and the preview guest is launched with no G5 identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreviewWorkspaceConfig {
    /// Shell command that launches the app, run via `/bin/sh -c` in-VM.
    pub command: String,
    /// TCP port the app LISTENS on inside the guest (1..=65535). Passt forwarding publishes this guest listener to a host port the supervisor
    /// leases (`g5/g5d/host` D9).
    pub port: u16,
    /// Working directory for the command, relative to `/work` unless absolute.
    /// Defaults to the `/work` root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Extra non-secret environment variables for the app process.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, String>,
    /// Optional shell command run after the app stops — on Close, and after the
    /// app exits on its own — with the same shell, cwd and env as `command`.
    /// Tears down state the app left behind in the shared VM (containers,
    /// daemons), e.g. `just dev-down` for G5 itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_command: Option<String>,
}

impl PreviewWorkspaceConfig {
    /// Validate the preview block. Mirrors the g5-api task-admission presence gate
    /// (`command` non-empty, `port` in 1..=65535) as defense in depth — the host
    /// and the in-VM `preview-serve` both fail closed on a malformed block even
    /// though g5-api gated it, because the materialized ref could differ.
    pub fn validate(&self) -> Result<(), String> {
        if self.command.trim().is_empty() {
            return Err("`preview.command` must be a non-empty string".to_string());
        }
        if self.port == 0 {
            return Err("`preview.port` must be in 1..=65535".to_string());
        }
        if self
            .cleanup_command
            .as_deref()
            .is_some_and(|command| command.trim().is_empty())
        {
            return Err("`preview.cleanup_command` must be non-empty when set".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceRemoteConfig {
    #[serde(default)]
    pub enabled: bool,
    pub api_url: Option<String>,
    pub project_id: Option<String>,
}

fn default_clone_method() -> String {
    "cow".to_string()
}

fn default_clone_base_dir() -> String {
    "~/.g5/cache/repos".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCloneConfig {
    #[serde(default = "default_clone_method")]
    pub method: String,
    #[serde(default = "default_clone_base_dir")]
    pub base_dir: String,
    /// When true (`g5d clone --checkout-remote-names`), each remote-tracking
    /// repo is checked out under its upstream `remote_branch` label instead of
    /// the canonical G5 local name. Server-truth stays canonical; push/pull map
    /// back to canonical on the `origin` proxy. Persisted so `refresh` /
    /// `switch` / `push` keep the same local labels.
    #[serde(default)]
    pub checkout_remote_names: bool,
}

impl Default for WorkspaceCloneConfig {
    fn default() -> Self {
        Self {
            method: default_clone_method(),
            base_dir: default_clone_base_dir(),
            checkout_remote_names: false,
        }
    }
}

fn default_fork_strategy() -> String {
    "fork".to_string()
}

fn default_fork_upstream() -> String {
    "origin".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceForkConfig {
    #[serde(default = "default_fork_strategy")]
    pub strategy: String,
    #[serde(default = "default_fork_upstream")]
    pub upstream: String,
}

impl Default for WorkspaceForkConfig {
    fn default() -> Self {
        Self {
            strategy: default_fork_strategy(),
            upstream: default_fork_upstream(),
        }
    }
}

fn default_base_image() -> String {
    "g5/worker:base".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceIsolationConfig {
    /// Workspace-level isolation mode. **Optional**: when absent the
    /// resolver falls through to the deployment-mode default (hosted →
    /// `microvm`, local → `process`). Baking in a `default = "microvm"`
    /// here would force a hardware VM on every local-dev workspace, which is
    /// not what a local `g5d host` does in process-mode runs.
    #[serde(default)]
    pub mode: Option<String>,
    pub fallback: Option<String>,
    #[serde(default = "default_base_image")]
    pub base_image: String,
    #[serde(default)]
    pub resources: Option<WorkspaceIsolationResources>,
}

impl Default for WorkspaceIsolationConfig {
    fn default() -> Self {
        Self {
            mode: None,
            fallback: None,
            base_image: default_base_image(),
            resources: None,
        }
    }
}

fn default_cpus() -> u32 {
    4
}

fn default_memory_gb() -> u32 {
    16
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceIsolationResources {
    #[serde(default = "default_cpus")]
    pub cpus: u32,
    #[serde(default = "default_memory_gb")]
    pub memory_gb: u32,
}

fn default_cache_mode() -> String {
    "local".to_string()
}

/// Legacy workspace cache settings; they do not select a runtime cache tier.
/// Retired `gcs` configuration is accepted as an unknown field and ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCacheConfig {
    #[serde(default = "default_cache_mode")]
    pub mode: String,
    pub local_dir: Option<String>,
}

impl Default for WorkspaceCacheConfig {
    fn default() -> Self {
        Self {
            mode: default_cache_mode(),
            local_dir: None,
        }
    }
}

fn default_docs_branch() -> String {
    "main".to_string()
}

fn default_docs_tracking() -> String {
    "independent".to_string()
}

fn default_docs_path() -> String {
    "repos/docs".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDocsConfig {
    pub repo_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    #[serde(default = "default_docs_branch")]
    pub branch: String,
    #[serde(default = "default_docs_tracking")]
    pub tracking_mode: String,
    #[serde(default = "default_docs_path")]
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

fn default_branch() -> String {
    "main".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRepoEntry {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    /// Canonical local branch name (server-truth), e.g. `<name-slug>-<8-hex workspace-id>`.
    #[serde(default = "default_branch")]
    pub branch: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Upstream remote ref name (e.g. `main`) for remote-tracking repos,
    /// surfaced from the manifest (workspace_service D15). `None` for
    /// non-remote-tracking repos. Drives the optional remote-name checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
}

impl WorkspaceConfig {
    /// Load workspace config from a YAML file.
    /// Returns Ok(None) if the file does not exist.
    /// Returns Err if the file exists but cannot be parsed.
    pub fn load(workspace_path: &Path) -> Result<Option<Self>, G5dError> {
        let config_path = workspace_path.join(".g5").join("workspace.yaml");
        if !config_path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&config_path).map_err(|e| {
            G5dError::WorkspaceConfig(format!("failed to read {}: {}", config_path.display(), e))
        })?;
        Self::parse_str(&content).map(Some).map_err(|e| {
            G5dError::WorkspaceConfig(format!("failed to parse {}: {e}", config_path.display()))
        })
    }

    /// Parse workspace config from YAML text — the same document `load` reads
    /// from disk.
    pub fn parse_str(content: &str) -> Result<Self, G5dError> {
        let config: Self =
            serde_yaml::from_str(content).map_err(|e| G5dError::WorkspaceConfig(e.to_string()))?;
        Ok(config)
    }

    /// Load and validate the `preview:` block from `<workspace_path>/.g5/workspace.yaml`.
    ///
    /// Used by BOTH the host supervisor (to read the guest `port` for forwarding — `g5/g5d/host` D9) and the in-VM `g5d preview-serve` (to read
    /// `command`/`port`/`working_dir`/`env` authoritatively — `g5/g5d/cli` D34).
    /// Fails closed if `.g5/workspace.yaml` is absent, has no `preview:` block,
    /// or the block is invalid.
    pub fn load_preview(workspace_path: &Path) -> Result<PreviewWorkspaceConfig, G5dError> {
        let config = Self::load(workspace_path)?.ok_or_else(|| {
            G5dError::WorkspaceConfig(format!(
                ".g5/workspace.yaml not found under {} — a preview task requires a `preview:` block \
                 with `command` and `port`",
                workspace_path.display()
            ))
        })?;
        let preview = config.preview.ok_or_else(|| {
            G5dError::WorkspaceConfig(
                ".g5/workspace.yaml has no `preview:` block (need `command` and `port`)"
                    .to_string(),
            )
        })?;
        preview.validate().map_err(G5dError::WorkspaceConfig)?;
        Ok(preview)
    }

    /// Load and validate a named preview profile from
    /// `<workspace_path>/.g5/preview_profiles/<profile>.yaml` (`g5/g5d/config`
    /// D32, `g5/g5-common/types` D18).
    ///
    /// A profile file's TOP-LEVEL fields are exactly the `PreviewWorkspaceConfig`
    /// shape (`command`, `port`, `working_dir`, `env`) — there is no enclosing
    /// `preview:` key. The `profile` name is re-validated as a safe file-stem
    /// before the path join so it can never escape the `preview_profiles/`
    /// directory. Used by BOTH the host supervisor (guest `port` for forwarding) and the in-VM `g5d preview-serve` (full app config) when a
    /// profile is selected; `load_preview` is the back-compat path otherwise.
    /// Fails closed if the name is unsafe, the file is absent, or the parsed
    /// profile is invalid.
    pub fn load_preview_profile(
        workspace_path: &Path,
        profile: &str,
    ) -> Result<PreviewWorkspaceConfig, G5dError> {
        g5_contracts::types::validate_preview_profile_name(profile)
            .map_err(|e| G5dError::WorkspaceConfig(format!("invalid preview profile name: {e}")))?;

        let profile_path = workspace_path
            .join(".g5")
            .join("preview_profiles")
            .join(format!("{profile}.yaml"));
        if !profile_path.exists() {
            return Err(G5dError::WorkspaceConfig(format!(
                "preview profile not found at {} (need a `.g5/preview_profiles/{}.yaml` with `command` and `port`)",
                profile_path.display(),
                profile
            )));
        }
        let content = std::fs::read_to_string(&profile_path).map_err(|e| {
            G5dError::WorkspaceConfig(format!("failed to read {}: {}", profile_path.display(), e))
        })?;
        let preview: PreviewWorkspaceConfig = serde_yaml::from_str(&content).map_err(|e| {
            G5dError::WorkspaceConfig(format!("failed to parse {}: {}", profile_path.display(), e))
        })?;
        preview.validate().map_err(G5dError::WorkspaceConfig)?;
        Ok(preview)
    }
}

// ---------------------------------------------------------------------------
// RepoConfig — g5.yaml schema
// ---------------------------------------------------------------------------

/// Represents the per-repo `g5.yaml` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub name: String,
    #[serde(default)]
    pub language: Vec<String>,
    #[serde(default)]
    pub setup: Option<RepoSetupConfig>,
    #[serde(default)]
    pub oci: Option<RepoOciConfig>,
    #[serde(default)]
    pub runtime: Option<RepoRuntimeConfig>,
    #[serde(default)]
    pub tooling: Option<RepoToolingConfig>,
    #[serde(default)]
    pub paths: Option<RepoPathsConfig>,
    #[serde(default)]
    pub open_in_ide: Option<RepoIdeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSetupConfig {
    #[serde(default)]
    pub bootstrap: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoOciConfig {
    pub image: Option<String>,
    pub build: Option<RepoOciBuildConfig>,
    pub push: Option<RepoOciPushConfig>,
    pub remote_host: Option<RepoOciRemoteHost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoOciBuildConfig {
    pub context: String,
    pub dockerfile: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoOciPushConfig {
    pub registry: String,
    pub repository: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoOciRemoteHost {
    pub url: String,
    pub auth_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRuntimeConfig {
    /// Runtime kind: "native", "docker", "compose", "custom".
    pub kind: String,
    pub command: Option<String>,
    pub env_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoToolingConfig {
    pub build: Option<String>,
    pub test: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoPathsConfig {
    pub workdir: Option<String>,
    #[serde(default)]
    pub ignores: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIdeConfig {
    #[serde(default)]
    pub recommended: Vec<String>,
    pub workspace_root: Option<String>,
}

impl RepoConfig {
    /// Load repo config from g5.yaml at the given repo root.
    /// Returns Ok(None) if the file does not exist.
    /// Returns Err if the file exists but cannot be parsed.
    pub fn load(repo_root: &Path) -> Result<Option<Self>, G5dError> {
        let config_path = repo_root.join("g5.yaml");
        if !config_path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&config_path).map_err(|e| {
            G5dError::Config(format!("failed to read {}: {}", config_path.display(), e))
        })?;
        let config: RepoConfig = serde_yaml::from_str(&content).map_err(|e| {
            G5dError::Config(format!("failed to parse {}: {}", config_path.display(), e))
        })?;
        Ok(Some(config))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Preview workspace-config tests (`g5/g5d/host` D9, `g5/g5d/cli` D34) --

    #[test]
    fn workspace_config_parses_preview_block() {
        let yaml = "preview:\n  command: \"npm start\"\n  port: 3000\n  working_dir: app\n  env:\n    NODE_ENV: production\n";
        let cfg: WorkspaceConfig =
            serde_yaml::from_str(yaml).expect("parse preview workspace.yaml");
        let preview = cfg.preview.expect("preview block present");
        assert_eq!(preview.command, "npm start");
        assert_eq!(preview.port, 3000);
        assert_eq!(preview.working_dir.as_deref(), Some("app"));
        assert_eq!(
            preview.env.get("NODE_ENV").map(String::as_str),
            Some("production")
        );
        preview.validate().expect("valid preview block");
    }

    #[test]
    fn workspace_config_without_preview_block_is_none() {
        let cfg: WorkspaceConfig =
            serde_yaml::from_str("workspace_id: w-1\n").expect("parse workspace.yaml");
        assert!(cfg.preview.is_none());
    }

    #[test]
    fn preview_config_validate_rejects_empty_command_and_zero_port() {
        let empty_cmd = PreviewWorkspaceConfig {
            command: "   ".to_string(),
            port: 3000,
            working_dir: None,
            env: std::collections::BTreeMap::new(),
            cleanup_command: None,
        };
        assert!(empty_cmd.validate().is_err(), "empty command rejected");

        let zero_port = PreviewWorkspaceConfig {
            command: "npm start".to_string(),
            port: 0,
            working_dir: None,
            env: std::collections::BTreeMap::new(),
            cleanup_command: None,
        };
        assert!(zero_port.validate().is_err(), "port 0 rejected");
    }

    #[test]
    fn preview_config_accepts_port_boundaries_and_rejects_overflow() {
        for port in [1, u16::MAX] {
            let preview = PreviewWorkspaceConfig {
                command: "npm start".to_string(),
                port,
                working_dir: None,
                env: std::collections::BTreeMap::new(),
                cleanup_command: None,
            };
            preview.validate().expect("port boundary must be valid");
        }

        let overflow = serde_yaml::from_str::<WorkspaceConfig>(
            "preview:\n  command: \"npm start\"\n  port: 65536\n",
        );
        assert!(overflow.is_err(), "port above u16::MAX must fail to parse");
    }

    #[test]
    fn load_preview_fails_closed_without_block() {
        let tmp = tempfile::tempdir().unwrap();
        let g5 = tmp.path().join(".g5");
        std::fs::create_dir_all(&g5).unwrap();
        // A workspace.yaml that exists but has no `preview:` block.
        std::fs::write(g5.join("workspace.yaml"), "workspace_id: w-1\n").unwrap();
        let err = WorkspaceConfig::load_preview(tmp.path())
            .expect_err("missing preview block must fail closed");
        assert!(matches!(err, G5dError::WorkspaceConfig(_)));
    }

    #[test]
    fn load_preview_reads_block_from_workspace_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let g5 = tmp.path().join(".g5");
        std::fs::create_dir_all(&g5).unwrap();
        std::fs::write(
            g5.join("workspace.yaml"),
            "preview:\n  command: \"./serve.sh\"\n  port: 8080\n",
        )
        .unwrap();
        let preview = WorkspaceConfig::load_preview(tmp.path()).expect("load preview block");
        assert_eq!(preview.command, "./serve.sh");
        assert_eq!(preview.port, 8080);
    }

    #[test]
    fn load_preview_profile_reads_named_file() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles = tmp.path().join(".g5").join("preview_profiles");
        std::fs::create_dir_all(&profiles).unwrap();
        // A profile file's TOP-LEVEL fields are the PreviewWorkspaceConfig shape
        // (no enclosing `preview:` key).
        std::fs::write(
            profiles.join("web.yaml"),
            "command: \"npm run dev\"\nport: 5173\nworking_dir: client\ncleanup_command: \"npm run down\"\n",
        )
        .unwrap();
        let preview =
            WorkspaceConfig::load_preview_profile(tmp.path(), "web").expect("load named profile");
        assert_eq!(preview.command, "npm run dev");
        assert_eq!(preview.port, 5173);
        assert_eq!(preview.working_dir.as_deref(), Some("client"));
        assert_eq!(preview.cleanup_command.as_deref(), Some("npm run down"));
    }

    #[test]
    fn preview_config_rejects_blank_cleanup_command() {
        let blank = PreviewWorkspaceConfig {
            command: "npm start".to_string(),
            port: 3000,
            working_dir: None,
            env: std::collections::BTreeMap::new(),
            cleanup_command: Some("  ".to_string()),
        };
        assert!(blank.validate().is_err(), "blank cleanup_command rejected");
    }

    #[test]
    fn load_preview_profile_fails_closed_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".g5").join("preview_profiles")).unwrap();
        let err = WorkspaceConfig::load_preview_profile(tmp.path(), "missing")
            .expect_err("absent profile must fail closed");
        assert!(matches!(err, G5dError::WorkspaceConfig(_)));
    }

    #[test]
    fn load_preview_profile_rejects_unsafe_name() {
        let tmp = tempfile::tempdir().unwrap();
        // An unsafe name is rejected before any path join, so traversal is
        // impossible even if such a file existed.
        for bad in ["../escape", "a/b", "with.dot", ""] {
            let err = WorkspaceConfig::load_preview_profile(tmp.path(), bad)
                .expect_err("unsafe profile name must be rejected");
            assert!(matches!(err, G5dError::WorkspaceConfig(_)));
        }
    }

    // -- Role tests --

    #[test]
    fn test_role_display() {
        assert_eq!(Role::Worker.to_string(), "worker");
    }

    #[test]
    fn test_role_from_str_case_insensitive() {
        assert_eq!(<Role as FromStr>::from_str("worker").unwrap(), Role::Worker);
        assert_eq!(<Role as FromStr>::from_str("Worker").unwrap(), Role::Worker);
        assert_eq!(<Role as FromStr>::from_str("WORKER").unwrap(), Role::Worker);
    }

    #[test]
    fn test_role_from_str_invalid() {
        assert!(<Role as FromStr>::from_str("editor").is_err());
        assert!(<Role as FromStr>::from_str("unknown").is_err());
    }

    #[test]
    fn test_role_serde_roundtrip() {
        let json = serde_json::to_string(&Role::Worker).unwrap();
        assert_eq!(json, "\"worker\"");
        let parsed: Role = serde_json::from_str("\"worker\"").unwrap();
        assert_eq!(parsed, Role::Worker);
    }

    // -- IsolationMode tests --

    #[test]
    fn test_isolation_mode_display() {
        assert_eq!(IsolationMode::MicroVm.to_string(), "microvm");
        assert_eq!(IsolationMode::Process.to_string(), "process");
    }

    #[test]
    fn test_isolation_mode_from_str_case_insensitive() {
        assert_eq!(
            <IsolationMode as FromStr>::from_str("microvm").unwrap(),
            IsolationMode::MicroVm
        );
        assert_eq!(
            <IsolationMode as FromStr>::from_str("MICROVM").unwrap(),
            IsolationMode::MicroVm
        );
        assert_eq!(
            <IsolationMode as FromStr>::from_str("process").unwrap(),
            IsolationMode::Process
        );
        assert_eq!(
            <IsolationMode as FromStr>::from_str("Process").unwrap(),
            IsolationMode::Process
        );
    }

    #[test]
    fn test_isolation_mode_from_str_invalid() {
        assert!(<IsolationMode as FromStr>::from_str("docker").is_err());
    }

    #[test]
    fn test_isolation_mode_serde_roundtrip() {
        let json = serde_json::to_string(&IsolationMode::MicroVm).unwrap();
        assert_eq!(json, "\"microvm\"");
        let json = serde_json::to_string(&IsolationMode::Process).unwrap();
        assert_eq!(json, "\"process\"");
    }

    // -- CloneMethod tests --

    #[test]
    fn test_clone_method_display() {
        assert_eq!(CloneMethod::Cow.to_string(), "cow");
        assert_eq!(CloneMethod::Full.to_string(), "full");
    }

    #[test]
    fn test_clone_method_from_str() {
        assert_eq!(CloneMethod::from_str("cow").unwrap(), CloneMethod::Cow);
        assert_eq!(CloneMethod::from_str("full").unwrap(), CloneMethod::Full);
        assert!(CloneMethod::from_str("shallow").is_err());
    }

    // -- ForkStrategy tests --

    #[test]
    fn test_fork_strategy_display() {
        assert_eq!(ForkStrategy::Fork.to_string(), "fork");
        assert_eq!(ForkStrategy::Mirror.to_string(), "mirror");
        assert_eq!(ForkStrategy::None.to_string(), "none");
    }

    #[test]
    fn test_fork_strategy_from_str() {
        assert_eq!(ForkStrategy::from_str("fork").unwrap(), ForkStrategy::Fork);
        assert_eq!(
            ForkStrategy::from_str("mirror").unwrap(),
            ForkStrategy::Mirror
        );
        assert_eq!(ForkStrategy::from_str("none").unwrap(), ForkStrategy::None);
        assert!(ForkStrategy::from_str("copy").is_err());
    }

    // -- expand_tilde tests --

    // -- G5dConfig test_default tests --

    // -- WorkspaceConfig tests --

    #[test]
    fn test_workspace_config_load_missing_file() {
        let result = WorkspaceConfig::load(Path::new("/nonexistent/path"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_workspace_config_deserialize_minimal() {
        let yaml = "workspace_id: ws-123\n";
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.workspace_id, Some("ws-123".to_string()));
        assert!(!config.remote.enabled);
        assert_eq!(config.clone.method, "cow");
        assert_eq!(config.fork.strategy, "fork");
        // isolation.mode is optional — absent in the minimal yaml falls
        // through to the deployment-mode default at resolution time.
        assert_eq!(config.isolation.mode, None);
        assert_eq!(config.cache.mode, "local");
    }

    #[test]
    fn workspace_config_parses_top_level_base_image() {
        // Contract test_contract/workspace_yaml_task_image D1 (present arm):
        // a workspace.yaml carrying the task-image entry deserializes it.
        let yaml = "workspace_id: ws-img\nbase_image: \"registry.example.com/g5/task-runtime@sha256:5e6b4850aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaafeccc2aa\"\n";
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.base_image.as_deref(),
            Some(
                "registry.example.com/g5/task-runtime@sha256:5e6b4850aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaafeccc2aa"
            )
        );
    }

    #[test]
    fn workspace_config_without_base_image_is_none() {
        // Contract D1 (absent arm): no entry yields None for the field and
        // changes nothing else.
        let yaml = "workspace_id: ws-no-img\n";
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.base_image.is_none());
        assert_eq!(config.workspace_id, Some("ws-no-img".to_string()));
    }

    #[test]
    fn workspace_config_load_reads_base_image_and_malformed_yaml_errors() {
        // Contract D1 (load + malformed arms): a real `.g5/workspace.yaml`
        // with the entry loads it; malformed YAML remains a hard error; a
        // missing file remains Ok(None) (covered again beside the existing
        // missing-file test for the same load path).
        let dir = tempfile::tempdir().unwrap();
        let g5_dir = dir.path().join(".g5");
        std::fs::create_dir_all(&g5_dir).unwrap();
        std::fs::write(
            g5_dir.join("workspace.yaml"),
            "workspace_id: ws-load\nbase_image: registry.example.com/g5/task-runtime:v42\n",
        )
        .unwrap();
        let loaded = WorkspaceConfig::load(dir.path()).unwrap().unwrap();
        assert_eq!(
            loaded.base_image.as_deref(),
            Some("registry.example.com/g5/task-runtime:v42")
        );

        std::fs::write(g5_dir.join("workspace.yaml"), "base_image: [unclosed\n").unwrap();
        let err = WorkspaceConfig::load(dir.path());
        assert!(matches!(err, Err(G5dError::WorkspaceConfig(_))));

        let missing = tempfile::tempdir().unwrap();
        assert!(WorkspaceConfig::load(missing.path()).unwrap().is_none());
    }

    #[test]
    fn test_workspace_docs_tracking_defaults_to_independent() {
        let yaml = r#"
workspace_id: ws-docs
docs:
  repo_url: https://example.test/git/docs
"#;
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.docs.unwrap().tracking_mode, "independent");
    }

    #[test]
    fn test_workspace_docs_tracking_accepts_legacy_submodule_value() {
        let yaml = r#"
workspace_id: ws-legacy-docs
docs:
  repo_url: https://example.test/git/docs
  tracking_mode: submodule
"#;
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.docs.unwrap().tracking_mode, "submodule");
    }

    #[test]
    fn test_workspace_isolation_mode_omitted_section_defaults_to_none() {
        // Even when the workspace.yaml omits the entire `isolation:` block,
        // `WorkspaceIsolationConfig::default()` must produce `mode: None`
        // so the resolver can fall through to deployment defaults.
        let cfg = WorkspaceIsolationConfig::default();
        assert!(cfg.mode.is_none());
    }

    #[test]
    fn test_grounding_config_defaults_chat_intents_enabled() {
        let config = GroundingConfig::default();
        assert!(config.chat_intents_enabled);
    }

    #[test]
    fn test_semlift_config_parses_chat_intents_disabled() {
        let yaml = r#"
grounding:
  chat_intents_enabled: false
"#;
        let config: SemliftConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.grounding.chat_intents_enabled);
        assert!(config.views.fan_out_sync);
    }

    #[test]
    fn test_workspace_config_deserialize_full() {
        let yaml = r#"
workspace_id: ws-456
remote:
  enabled: true
  api_url: "https://api.g5.dev"
  project_id: "proj-123"
clone:
  method: full
  base_dir: /data/repos
fork:
  strategy: mirror
  upstream: upstream
isolation:
  mode: process
  fallback: process
  base_image: custom/image:latest
  resources:
    cpus: 8
    memory_gb: 32
cache:
  mode: local
  local_dir: /cache
repos:
  - name: my-repo
    url: https://github.com/org/repo
    branch: develop
    path: repos/my-repo
"#;
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.workspace_id, Some("ws-456".to_string()));
        assert!(config.remote.enabled);
        assert_eq!(
            config.remote.api_url,
            Some("https://api.g5.dev".to_string())
        );
        assert_eq!(config.clone.method, "full");
        assert_eq!(config.fork.strategy, "mirror");
        assert_eq!(config.fork.upstream, "upstream");
        assert_eq!(config.isolation.mode, Some("process".to_string()));
        assert_eq!(config.isolation.fallback, Some("process".to_string()));
        assert_eq!(config.isolation.base_image, "custom/image:latest");
        let resources = config.isolation.resources.as_ref().unwrap();
        assert_eq!(resources.cpus, 8);
        assert_eq!(resources.memory_gb, 32);
        assert_eq!(config.cache.mode, "local");
        assert_eq!(config.cache.local_dir, Some("/cache".to_string()));
        assert_eq!(config.repos.len(), 1);
        assert_eq!(config.repos[0].name, "my-repo");
        assert_eq!(config.repos[0].branch, "develop");
    }

    #[test]
    fn workspace_config_load_tolerates_retired_gcs_settings() {
        let dir = tempfile::tempdir().unwrap();
        let g5_dir = dir.path().join(".g5");
        std::fs::create_dir(&g5_dir).unwrap();
        // Retired settings do not block live preview/repository configuration,
        // even when they no longer conform to the removed GCS schema.
        for legacy_gcs in ["{bucket: old-bucket, prefix: old-prefix}", "{}", "unused"] {
            let yaml = format!(
                "workspace_id: ws-legacy\ncache:\n  mode: gcs\n  local_dir: /cache\n  gcs: {legacy_gcs}\npreview:\n  command: npm start\n  port: 8080\n"
            );
            std::fs::write(g5_dir.join("workspace.yaml"), &yaml).unwrap();
            let config = WorkspaceConfig::load(dir.path()).unwrap().unwrap();
            assert_eq!(config.workspace_id.as_deref(), Some("ws-legacy"));
            assert_eq!(config.cache.mode, "gcs");
            assert_eq!(config.cache.local_dir.as_deref(), Some("/cache"));
            let preview = WorkspaceConfig::load_preview(dir.path()).unwrap();
            assert_eq!(preview.command, "npm start");
            assert_eq!(preview.port, 8080);
            let serialized =
                crate::cli::workspace_manifest::serialize_workspace_manifest(&config).unwrap();
            let value: serde_yaml::Value = serde_yaml::from_str(&serialized).unwrap();
            assert!(value["cache"].get("gcs").is_none());
            let reparsed = WorkspaceConfig::parse_str(&serialized).unwrap();
            assert_eq!(reparsed.cache.mode, "gcs");
            assert_eq!(reparsed.preview.unwrap().port, 8080);
        }
    }

    #[test]
    fn test_workspace_config_remote_name_checkout_fields() {
        // clone.checkout_remote_names persists the opt-in; a repo entry carries
        // the upstream remote_branch from the manifest. An unknown manifest
        // field (remote_repo_name) is tolerated/ignored by WorkspaceRepoEntry.
        let yaml = r#"
workspace_id: ws-789
clone:
  method: full
  base_dir: /data/repos
  checkout_remote_names: true
repos:
  - name: backend
    url: https://api.g5.dev/git/00000000-0000-0000-0000-000000000000
    branch: feat/login
    path: repos/backend
    remote_branch: main
    remote_repo_name: octocat/hello-world
"#;
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.clone.checkout_remote_names);
        assert_eq!(config.repos.len(), 1);
        // `branch` stays the canonical local name; `remote_branch` is upstream.
        assert_eq!(config.repos[0].branch, "feat/login");
        assert_eq!(config.repos[0].remote_branch, Some("main".to_string()));

        // Round-trip preserves the persisted opt-in + remote_branch.
        let serialized = serde_yaml::to_string(&config).unwrap();
        let reparsed: WorkspaceConfig = serde_yaml::from_str(&serialized).unwrap();
        assert!(reparsed.clone.checkout_remote_names);
        assert_eq!(reparsed.repos[0].remote_branch, Some("main".to_string()));
    }

    #[test]
    fn test_workspace_config_remote_name_checkout_defaults() {
        // Omitted → opt-in defaults false and remote_branch defaults None
        // (backward-compatible with pre-feature workspace.yaml files).
        let yaml = r#"
workspace_id: ws-000
repos:
  - name: backend
    url: https://api.g5.dev/git/repo
    branch: main
    path: repos/backend
"#;
        let config: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.clone.checkout_remote_names);
        assert_eq!(config.repos[0].remote_branch, None);
    }

    // -- RepoConfig tests --

    #[test]
    fn test_repo_config_load_missing_file() {
        let result = RepoConfig::load(Path::new("/nonexistent/path"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_repo_config_deserialize() {
        let yaml = r#"
name: my-project
language:
  - rust
  - python
setup:
  bootstrap:
    - "cargo build"
    - "pip install -r requirements.txt"
  env:
    RUST_LOG: debug
runtime:
  kind: native
  command: "cargo run"
tooling:
  build: "cargo build --release"
  test: "cargo test"
paths:
  workdir: src
  ignores:
    - target/
    - node_modules/
open_in_ide:
  recommended:
    - vscode
    - neovim
  workspace_root: .
"#;
        let config: RepoConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "my-project");
        assert_eq!(config.language, vec!["rust", "python"]);
        let setup = config.setup.unwrap();
        assert_eq!(setup.bootstrap.len(), 2);
        assert_eq!(setup.env.get("RUST_LOG"), Some(&"debug".to_string()));
        let runtime = config.runtime.unwrap();
        assert_eq!(runtime.kind, "native");
        let tooling = config.tooling.unwrap();
        assert_eq!(tooling.build, Some("cargo build --release".to_string()));
        let paths = config.paths.unwrap();
        assert_eq!(paths.workdir, Some("src".to_string()));
        assert_eq!(paths.ignores.len(), 2);
        let ide = config.open_in_ide.unwrap();
        assert_eq!(ide.recommended, vec!["vscode", "neovim"]);
    }

    // -- G5dError tests --

    #[test]
    fn test_g5d_error_display() {
        let err = G5dError::Config("test error".to_string());
        assert_eq!(err.to_string(), "configuration error: test error");

        let err = G5dError::WorkspaceConfig("bad yaml".to_string());
        assert_eq!(err.to_string(), "workspace config error: bad yaml");
    }
}
