//! Preview configuration as a workspace declares it: the `preview` block
//! of `.g5/workspace.yaml` or a `.g5/preview_profiles/<name>.yaml` file.
//! One parser serves the listing and the launch, so a listing offers only
//! what a launch accepts.

use g5_api_foundation::contracts::preview_profiles::{
    PreviewApplicationDTO, PreviewApplicationSource,
};
use g5_contracts::error::G5Error;
use serde::Deserialize;
use serde_json::json;

use crate::errors::AppError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreviewProfileConfig {
    pub(crate) command: String,
    pub(crate) port: u16,
    #[serde(default)]
    pub(crate) working_dir: Option<String>,
    #[serde(default)]
    pub(crate) env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) cleanup_command: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkspacePreviewConfigDocument {
    preview: Option<PreviewProfileConfig>,
}

pub(crate) fn parse_preview_config(
    content: &str,
    named_profile: bool,
) -> Result<PreviewProfileConfig, AppError> {
    let invalid = |message: String| {
        AppError::from_error(G5Error::ValidationError {
            details: json!({"field":"profile","message":message}),
        })
    };
    let config = if named_profile {
        serde_yaml::from_str::<PreviewProfileConfig>(content)
            .map_err(|error| invalid(error.to_string()))?
    } else {
        serde_yaml::from_str::<WorkspacePreviewConfigDocument>(content)
            .map_err(|error| invalid(error.to_string()))?
            .preview
            .ok_or_else(|| invalid(".g5/workspace.yaml has no preview block".to_string()))?
    };
    if config.command.trim().is_empty() || config.port == 0 {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({"field":"profile","message":"preview command and port are required"}),
        }));
    }
    if config
        .cleanup_command
        .as_deref()
        .is_some_and(|command| command.trim().is_empty())
    {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: json!({"field":"profile","message":"preview cleanup_command must be non-empty when set"}),
        }));
    }
    // Parsing these fields is deliberate even though the API only persists the
    // routing port; the dispatcher re-reads the same complete profile before
    // launch and owns command/env/cleanup application.
    let _ = (&config.working_dir, &config.env);
    Ok(config)
}

/// The repository-relative file a preview application is declared in: the
/// workspace default or a named profile. The live and committed sources read
/// the same path.
pub(crate) fn preview_config_path(profile: Option<&str>) -> String {
    profile.map_or_else(
        || ".g5/workspace.yaml".to_string(),
        |profile| format!(".g5/preview_profiles/{profile}.yaml"),
    )
}

/// Read a preview configuration with the parser launch uses, so a listing
/// offers only what a launch accepts. `profile` names a
/// `.g5/preview_profiles/` file; `None` reads `.g5/workspace.yaml`'s
/// `preview` block.
pub(crate) fn read_preview_application(
    profile: Option<String>,
    config_path: String,
    content: &str,
) -> Result<PreviewApplicationDTO, AppError> {
    let source = if profile.is_some() {
        PreviewApplicationSource::NamedProfile
    } else {
        PreviewApplicationSource::WorkspaceDefault
    };
    let config = parse_preview_config(content, profile.is_some())?;
    Ok(preview_application(source, profile, config_path, config))
}

pub(crate) fn preview_application(
    source: PreviewApplicationSource,
    profile: Option<String>,
    config_path: String,
    config: PreviewProfileConfig,
) -> PreviewApplicationDTO {
    PreviewApplicationDTO {
        source,
        profile,
        config_path,
        command: config.command.trim().to_string(),
        port: config.port,
        working_dir: config.working_dir,
        description: config.description,
        env_keys: config.env.into_keys().collect(),
        cleanup_command: config
            .cleanup_command
            .map(|command| command.trim().to_string()),
    }
}
