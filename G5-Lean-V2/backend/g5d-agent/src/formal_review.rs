//! The formal-review tool boundary. No arbitrary commands, working-tree reads,
//! network tools, delegation or caller-selected output paths are exposed.
use crate::{AgentTurnInputV1, ProviderId};
use base64::Engine;
use g5_contracts::{
    types::{
        validate_agent_approval_markdown_v2, FormalApprovalInvocation,
        AGENT_APPROVAL_OUTPUT_MAX_BYTES, AGENT_APPROVAL_SUMMARY_MAX_CHARS,
    },
    G5Error,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub(crate) fn invocation(
    input: &AgentTurnInputV1,
) -> Result<Option<FormalApprovalInvocation>, G5Error> {
    input
        .provider
        .options
        .get("formal_review")
        .map(|value| {
            let review: FormalApprovalInvocation =
                serde_json::from_value(value.clone()).map_err(error)?;
            review.validate().map_err(error)?;
            if input.task.attempt != 1
                || input.continuation.is_some()
                || input.resume_prompt.is_some()
                || input.native_question_reply.is_some()
                || !input.attachments.is_empty()
                || input.prompt.text != review.user_prompt
            {
                return Err(error("formal review requires one fresh frozen input"));
            }
            Ok(review)
        })
        .transpose()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Repository {
    repo_id: String,
    root_path: String,
    #[serde(default)]
    role: Option<String>,
    /// Read by the ontology changes card, not by reviews.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    git_url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolConfig {
    invocation: FormalApprovalInvocation,
    repositories: BTreeMap<String, PathBuf>,
    output: PathBuf,
}

pub(crate) struct Environment {
    _directory: tempfile::TempDir,
    pub home: PathBuf,
    pub cwd: PathBuf,
    config: ToolConfig,
}

impl Environment {
    pub fn prepare(
        input: &AgentTurnInputV1,
        assignment: &FormalApprovalInvocation,
        workspace: &Path,
        original_home: &Path,
    ) -> Result<Self, G5Error> {
        let workspace = workspace.canonicalize().map_err(error)?;
        let directory = tempfile::Builder::new()
            .prefix("g5-formal-review-")
            .tempdir_in("/tmp")
            .map_err(error)?;
        if directory
            .path()
            .canonicalize()
            .map_err(error)?
            .starts_with(&workspace)
        {
            return Err(error(
                "formal configuration must be outside the reviewed workspace",
            ));
        }
        let home = directory.path().join("home");
        let cwd = directory.path().join("launch");
        for path in [
            &home,
            &cwd,
            &home.join(".g5"),
            &home.join(".codex"),
            &home.join(".claude"),
        ] {
            std::fs::create_dir_all(path).map_err(error)?;
        }
        let members: Vec<Repository> =
            serde_json::from_str(&std::env::var("G5D_AGENT_REPOSITORIES").map_err(error)?)
                .map_err(error)?;
        let mut repositories = BTreeMap::new();
        for head in &assignment.resolution.source_heads {
            let member = members
                .iter()
                .find(|m| m.repo_id == head.repo_id)
                .ok_or_else(|| error("review source member is unavailable"))?;
            let path = Path::new(&member.root_path);
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
            {
                return Err(error("review member path escapes workspace"));
            }
            let root = workspace.join(path);
            if root.canonicalize().map_err(error)? != root {
                return Err(error("review member path contains a symlink"));
            }
            repositories.insert(head.repo_id.clone(), root);
        }
        if assignment
            .resolution
            .target_heads
            .iter()
            .any(|head| !repositories.contains_key(&head.repo_id))
        {
            return Err(error("review target has a different repository set"));
        }
        // Subscription transport uses only the selected native credential. No
        // user configuration, instructions, hooks or MCP server is inherited.
        if input.provider.auth_mode
            == Some(g5_contracts::types::AgentAuthModeV1::ChatGptSubscription)
        {
            let auth = std::fs::read(original_home.join(".codex/auth.json"))
                .map_err(|_| error("formal review subscription is not authenticated"))?;
            private_write(&home.join(".codex/auth.json"), &auth)?;
        }
        for provider in [".claude", ".codex"] {
            let skill = home.join(provider).join("skills/g5-agent-approval-review");
            std::fs::create_dir_all(skill.join("references")).map_err(error)?;
            private_write(&skill.join("SKILL.md"), assignment.skill().as_bytes())?;
            private_write(
                &skill.join("references/agent-prompt.md"),
                assignment.prompt_guide().as_bytes(),
            )?;
        }
        let output = directory.path().join("report.md");
        let config = ToolConfig {
            invocation: assignment.clone(),
            repositories,
            output,
        };
        private_write(
            &home.join(".g5/approval-tools.json"),
            &serde_json::to_vec(&config).map_err(error)?,
        )?;
        let system = format!("{}\n\nInstalled sole skill (read in full):\n{}\n\nSkill reference:\n{}\n\nThe formal_review MCP tools are the complete tool surface. They read only the assigned Git objects. Use write_report according to the assigned schema to submit the single result; no lifecycle or shell tool exists.", assignment.system_prompt, assignment.skill(), assignment.prompt_guide());
        private_write(&home.join(".g5/review-system.txt"), system.as_bytes())?;
        let executable = std::env::current_exe().map_err(error)?;
        let tool_env = supervised_environment();
        let server = json!({"command": executable, "args": ["approval-review-tools", "--config", home.join(".g5/approval-tools.json")], "env": tool_env});
        private_write(
            &home.join(".g5/review-mcp.json"),
            &serde_json::to_vec(&json!({"mcpServers":{"formal_review":server}})).map_err(error)?,
        )?;
        Ok(Self {
            _directory: directory,
            home,
            cwd,
            config,
        })
    }

    /// Called only after the provider exited. A new random directory and a
    /// create-new writer make stale output impossible. Refuse links and races
    /// using the opened descriptor, not a preceding pathname stat alone.
    pub fn harvest(&self) -> Result<Value, G5Error> {
        harvest(&self.config)
    }
}

fn private_write(path: &Path, bytes: &[u8]) -> Result<(), G5Error> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path).map_err(error)?;
    file.write_all(bytes).map_err(error)?;
    file.sync_all().map_err(error)
}

fn harvest(config: &ToolConfig) -> Result<Value, G5Error> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(&config.output).map_err(error)?;
    let before = file.metadata().map_err(error)?;
    if !before.is_file() || before.len() > AGENT_APPROVAL_OUTPUT_MAX_BYTES as u64 {
        return Err(error("review output must be a bounded regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.nlink() != 1 {
            return Err(error("review output has multiple links"));
        }
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take((AGENT_APPROVAL_OUTPUT_MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    let after = file.metadata().map_err(error)?;
    if bytes.len() as u64 != before.len()
        || after.len() != before.len()
        || before.modified().map_err(error)? != after.modified().map_err(error)?
    {
        return Err(error("review output changed while being read"));
    }
    let completed = std::fs::read_to_string(config.output.with_extension("complete"))
        .map_err(|_| error("review output writer has not completed"))?;
    if completed != format!("{}:{:x}", bytes.len(), Sha256::digest(&bytes)) {
        return Err(error("review output completion receipt does not match"));
    }
    validate_agent_approval_markdown_v2(&bytes, &config.invocation.resolution).map_err(error)?;
    Ok(
        json!({"contractVersion": config.invocation.contract_version, "resolutionId":config.invocation.resolution.resolution_id, "contentBase64":base64::engine::general_purpose::STANDARD.encode(&bytes), "sha256":format!("{:x}",Sha256::digest(&bytes))}),
    )
}

/// The CLI entrypoint is internal; its config is created by the trusted turn
/// supervisor, in its new private directory. The provider receives no config
/// mutation tool and cannot choose a repository, commit or output location.
pub async fn serve(config_path: &Path) -> Result<(), G5Error> {
    let config: ToolConfig =
        serde_json::from_slice(&std::fs::read(config_path).map_err(error)?).map_err(error)?;
    config.invocation.validate().map_err(error)?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut output = tokio::io::stdout();
    while let Some(line) = lines.next_line().await.map_err(error)? {
        if line.len() > AGENT_APPROVAL_OUTPUT_MAX_BYTES * 8 {
            return Err(error("review tool request too large"));
        }
        let message: Value = serde_json::from_str(&line).map_err(error)?;
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let result = match message["method"].as_str() {
            Some("initialize") => {
                json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"formal_review","version":"1"}})
            }
            Some("ping") => json!({}),
            Some("tools/list") => {
                json!({"tools": tools(config.invocation.resolution.schema_version)})
            }
            Some("tools/call") => match call(&config, &message["params"]).await {
                Ok(value) => json!({"content":[{"type":"text","text":value}],"isError":false}),
                Err(cause) => {
                    json!({"content":[{"type":"text","text":cause.to_string()}],"isError":true})
                }
            },
            _ => {
                let response = json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unknown review method"}});
                output
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .map_err(error)?;
                continue;
            }
        };
        output
            .write_all(format!("{}\n", json!({"jsonrpc":"2.0","id":id,"result":result})).as_bytes())
            .await
            .map_err(error)?;
        output.flush().await.map_err(error)?;
    }
    Ok(())
}

/// The tool surface for one assignment schema. Each schema keeps its own
/// published `write_report` shape for frozen retries.
fn tools(schema_version: u8) -> Vec<Value> {
    let read = json!({"name":"read_review","description":"Read immutable assigned Git cuts or the frozen comparison vector.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"repoId":{"type":"string"},"operation":{"enum":["comparison","file","paths","diff"]},"cut":{"enum":["source","mergeBase","parent","workspaceBase"]},"path":{"type":"string"}},"required":["operation"]}});
    if matches!(schema_version, 5 | 6) {
        // Schema 6 keeps schema 5's write_report shape; only its body
        // guidance (the skill's section order) is new.
        let body_order = if schema_version == 6 {
            " The body starts with ## Governing prompt and follows the skill's section order."
        } else {
            ""
        };
        return vec![
            read,
            json!({"name":"write_report","description":format!("Submit the verdict, summary and Markdown body once. The summary is one plain-text line of at most 250 characters, written separately from the body, that states what was reviewed and why it was approved or rejected.{body_order} The tool copies immutable assignment metadata and writes submittedAt."),"inputSchema":{"type":"object","additionalProperties":false,"properties":{"result":{"enum":["approved","rejected"]},"summary":{"type":"string","maxLength":AGENT_APPROVAL_SUMMARY_MAX_CHARS},"body":{"type":"string"}},"required":["result","summary","body"]}}),
        ];
    }
    if schema_version == 4 {
        return vec![
            read,
            json!({"name":"write_report","description":"Submit the verdict and Markdown body once. The tool copies immutable assignment metadata and writes submittedAt.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"result":{"enum":["approved","rejected"]},"body":{"type":"string"}},"required":["result","body"]}}),
        ];
    }
    vec![
        json!({"name":"read_review","description":"Read a file, list paths, or diff only the frozen source/base Git objects. Source is the default cut. No worktree, shell, network or other revision is accessible.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"repoId":{"type":"string"},"operation":{"enum":["file","paths","diff"]},"cut":{"enum":["source","target"]},"path":{"type":"string"}},"required":["repoId","operation"]}}),
        json!({"name":"write_report","description":"Write the sole assigned report exactly once. Copy a canonical v2 skeleton from the assignment, add the nonblank review body, and pass the assigned logical output path.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"path":{"type":"string"},"document":{"type":"string"}},"required":["path","document"]}}),
    ]
}

async fn call(config: &ToolConfig, params: &Value) -> Result<String, G5Error> {
    let arguments = params["arguments"]
        .as_object()
        .ok_or_else(|| error("Expected tool arguments"))?;
    let get = |key: &str| {
        arguments
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| error(format!("Missing {key}")))
    };
    match params["name"].as_str() {
        Some("write_report") => {
            let canonical;
            let resolution = &config.invocation.resolution;
            let document = if resolution.has_assignment_metadata() {
                let summary = resolution.has_authored_summary();
                let expected: &[&str] = if summary {
                    &["result", "summary", "body"]
                } else {
                    &["result", "body"]
                };
                if arguments.len() != expected.len()
                    || !expected.iter().all(|key| arguments.contains_key(*key))
                {
                    return Err(error(if summary {
                        "Expected only result, summary and body"
                    } else {
                        "Expected only result and body"
                    }));
                }
                let result = match get("result")? {
                    "approved" => g5_contracts::types::AgentApprovalOutcome::Approved,
                    "rejected" => g5_contracts::types::AgentApprovalOutcome::Rejected,
                    _ => return Err(error("Expected approved or rejected")),
                };
                let submitted_at =
                    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
                canonical = if summary {
                    g5_contracts::types::render_agent_approval_report_v5(
                        resolution,
                        result,
                        get("summary")?,
                        get("body")?,
                        &submitted_at,
                    )
                } else {
                    g5_contracts::types::render_agent_approval_report_v4(
                        resolution,
                        result,
                        get("body")?,
                        &submitted_at,
                    )
                }
                .map_err(error)?;
                canonical.as_str()
            } else {
                if arguments.len() != 2
                    || get("path")? != config.invocation.resolution.output_doc_path
                {
                    return Err(error("Unassigned output path or fields"));
                }
                get("document")?
            };
            validate_agent_approval_markdown_v2(document.as_bytes(), &config.invocation.resolution)
                .map_err(error)?;
            private_write(&config.output, document.as_bytes())?;
            private_write(
                &config.output.with_extension("complete"),
                format!(
                    "{}:{:x}",
                    document.len(),
                    Sha256::digest(document.as_bytes())
                )
                .as_bytes(),
            )?;
            Ok("Assigned report written. End the turn.".into())
        }
        Some("read_review") => {
            if arguments
                .keys()
                .any(|k| !["repoId", "operation", "cut", "path"].contains(&k.as_str()))
            {
                return Err(error("Unknown review read field"));
            }
            if config.invocation.resolution.has_assignment_metadata()
                && get("operation")? == "comparison"
            {
                return serde_json::to_string(&config.invocation.resolution.merge_base)
                    .map_err(error);
            }
            let repo = get("repoId")?;
            let root = config
                .repositories
                .get(repo)
                .ok_or_else(|| error("Unassigned repository"))?;
            let source = config
                .invocation
                .resolution
                .source_heads
                .iter()
                .find(|h| h.repo_id == repo)
                .ok_or_else(|| error("Source member missing"))?;
            let resolution = &config.invocation.resolution;
            let base = resolution
                .comparison_sha(repo)
                .ok_or_else(|| error("Comparison member missing"))?;
            let name = arguments
                .get("cut")
                .and_then(Value::as_str)
                .unwrap_or("source");
            let cut = match name {
                "source" => source.head_commit.as_str(),
                "target" if !resolution.has_assignment_metadata() => base,
                "mergeBase" if resolution.has_assignment_metadata() => base,
                "parent" if resolution.has_assignment_metadata() => resolution
                    .target_heads
                    .iter()
                    .find(|h| h.repo_id == repo)
                    .map(|h| h.head_commit.as_str())
                    .ok_or_else(|| error("Parent member missing"))?,
                "workspaceBase" if resolution.has_assignment_metadata() => resolution
                    .workspace_base_heads
                    .as_ref()
                    .and_then(|hs| hs.iter().find(|h| h.repo_id == repo))
                    .map(|h| h.head_commit.as_str())
                    .ok_or_else(|| error("Workspace base member missing"))?,
                _ => return Err(error("Unassigned cut")),
            };
            use g5_scm::native::ReviewSnapshotRead;
            let read = match get("operation")? {
                "file" => ReviewSnapshotRead::File {
                    commit: cut,
                    path: get("path")?,
                },
                "paths" => ReviewSnapshotRead::Paths { commit: cut },
                "diff" => ReviewSnapshotRead::Diff {
                    base,
                    source: &source.head_commit,
                    path: arguments.get("path").and_then(Value::as_str),
                },
                _ => return Err(error("Unknown read operation")),
            };
            let git = g5_scm::native::NativeGit::cli(Duration::from_secs(15)).map_err(error)?;
            let bytes = git.read_review_snapshot(root, read).await.map_err(error)?;
            String::from_utf8(bytes).map_err(error)
        }
        _ => Err(error("Unknown formal review tool")),
    }
}

fn supervised_environment() -> BTreeMap<String, String> {
    use g5d_foundation::runtime_environment as runtime;
    let mut env: BTreeMap<String, String> = runtime::native_loader_environment()
        .into_iter()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect();
    env.insert(
        "PATH".into(),
        // The common image's perl-free Git is exposed at /usr/local/bin;
        // Docker ENV does not reach a scrubbed native launch.
        format!(
            "{}:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            runtime::runtime_bin_dir().display()
        ),
    );
    for key in [
        runtime::PROCESS_INCARNATION_ENV,
        runtime::PROCESS_INCARNATION_OUTER_ENV,
    ] {
        if let Ok(value) = std::env::var(key) {
            env.insert(key.into(), value);
        }
    }
    env
}

pub(crate) fn sanitize_environment(env: &mut BTreeMap<String, String>) {
    env.retain(|key, _| {
        matches!(
            key.as_str(),
            "PATH"
                | "HOME"
                | "CODEX_HOME"
                | "LANG"
                | "LC_ALL"
                | "TERM"
                | "SSL_CERT_FILE"
                | "SSL_CERT_DIR"
        ) || g5_contracts::types::FORMAL_APPROVAL_CREDENTIAL_KEYS.contains(&key.as_str())
    });
    env.extend(supervised_environment());
    env.insert("CLAUDE_CODE_DISABLE_AUTO_MEMORY".into(), "1".into());
    // Since 2.1.304 custom endpoints enable tool search even when --tools
    // excludes it, adding a deferred placeholder. Reviews expose only their
    // assigned MCP tools, loaded upfront, regardless of inherited defaults.
    env.insert("ENABLE_TOOL_SEARCH".into(), "false".into());
}
fn error(value: impl std::fmt::Display) -> G5Error {
    G5Error::Internal {
        message: format!("formal review: {value}"),
    }
}

pub(crate) fn launch(
    input: &AgentTurnInputV1,
    home: &Path,
    fake: bool,
) -> Result<(PathBuf, Vec<String>, Vec<u8>), G5Error> {
    let system_path = home.join(".g5/review-system.txt");
    let system = std::fs::read_to_string(&system_path).map_err(error)?;
    let mut args = Vec::<String>::new();
    let program = match input.provider.provider_id {
        ProviderId::ClaudeCode => {
            args.extend(
                [
                    "-p",
                    "--output-format",
                    "stream-json",
                    "--input-format",
                    "stream-json",
                    "--verbose",
                    "--include-partial-messages",
                    "--tools",
                    "",
                    "--permission-mode",
                    "dontAsk",
                    "--permission-prompts",
                    "none",
                    "--setting-sources",
                    "",
                    "--strict-mcp-config",
                    "--mcp-config",
                ]
                .map(str::to_string),
            );
            args.push(
                home.join(".g5/review-mcp.json")
                    .to_string_lossy()
                    .into_owned(),
            );
            args.extend(
                [
                    "--allowedTools",
                    "mcp__formal_review__read_review,mcp__formal_review__write_report",
                    "--system-prompt",
                ]
                .map(str::to_string),
            );
            args.push(system);
            let mut settings = json!({"disableAllHooks": true});
            crate::runner::claude::apply_model_overrides(&input.provider.model_id, &mut settings)
                .map_err(error)?;
            args.extend(["--settings".into(), settings.to_string()]);
            args.extend(crate::runner::claude::model_arguments(
                &input.provider.model_id,
                input.provider.options.get("effort").and_then(Value::as_str),
            ));
            if fake {
                std::env::var_os("G5_CLAUDE_PATH")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        g5d_foundation::runtime_environment::runtime_bin_dir().join("fake-claude")
                    })
            } else {
                g5d_foundation::runtime_environment::runtime_bin_dir().join("claude")
            }
        }
        ProviderId::Codex => {
            // App Server has no exec-only isolation flags. The fresh
            // private CODEX_HOME supplies no user config or rules, and the
            // thread itself is ephemeral (`codex_app_server_adapter`).
            args.extend(
                ["app-server", "--listen", "stdio://", "--strict-config"].map(str::to_string),
            );
            for setting in [
                "approval_policy=\"never\"",
                "project_doc_max_bytes=0",
                "default_permissions=\"g5_formal_review\"",
                "permissions.g5_formal_review.extends=\":read-only\"",
                "permissions.g5_formal_review.filesystem={\":root\"=\"deny\"}",
                "skills.bundled.enabled=false",
                "skills.include_instructions=false",
                "tools.update_plan.enabled=false",
                "tools.experimental_request_user_input.enabled=false",
                "web_search=\"disabled\"",
                "developer_instructions=\"\"",
                "include_apps_instructions=false",
                "include_collaboration_mode_instructions=false",
            ] {
                args.extend(["-c".into(), setting.into()]);
            }
            for feature in [
                "shell_tool",
                "shell_snapshot",
                "shell_snapshot_v2",
                "undo",
                "unified_exec",
                "multi_agent",
                "multi_agent_v2",
                "multi_agent_mode",
                "agent_message_board",
                "apps",
                "plugins",
                "remote_plugin",
                "plugin_hooks",
                "hooks",
                "browser_use",
                "browser_use_external",
                "computer_use",
                "view_image",
                "image_generation",
                "imagegenext",
                "in_app_browser",
                "skill_search",
                "tool_search",
                "skill_mcp_dependency_install",
                "memories",
                "code_mode",
                "code_mode_host",
                "goals",
            ] {
                args.extend(["--disable".into(), feature.into()]);
            }
            let command = std::env::current_exe().map_err(error)?;
            let toml_string = |value: &str| serde_json::to_string(value).map_err(error);
            for setting in [
                format!(
                    "model_instructions_file={}",
                    toml_string(&system_path.to_string_lossy())?
                ),
                format!(
                    "mcp_servers.formal_review.command={}",
                    toml_string(&command.to_string_lossy())?
                ),
                format!(
                    "mcp_servers.formal_review.args=[\"approval-review-tools\",\"--config\",{}]",
                    toml_string(&home.join(".g5/approval-tools.json").to_string_lossy())?
                ),
                "mcp_servers.formal_review.required=true".into(),
                "mcp_servers.formal_review.omit_tools_from=[\"deferred\"]".into(),
                "mcp_servers.formal_review.tools.read_review.approval_mode=\"approve\"".into(),
                "mcp_servers.formal_review.tools.write_report.approval_mode=\"approve\"".into(),
            ] {
                args.extend(["-c".into(), setting]);
            }
            args.extend(crate::runner::codex_app_server::feature_configuration());
            args.extend(crate::memory::codex_configuration(
                &crate::memory::Binding::Disabled,
                false,
            ));
            for (key, value) in supervised_environment() {
                args.extend([
                    "-c".into(),
                    format!(
                        "mcp_servers.formal_review.env.{key}={}",
                        toml_string(&value)?
                    ),
                ]);
            }
            if fake {
                std::env::var_os("G5_CODEX_PATH")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        g5d_foundation::runtime_environment::runtime_bin_dir().join("fake-codex")
                    })
            } else {
                g5d_foundation::runtime_environment::runtime_bin_dir().join("codex")
            }
        }
    };
    let bytes = match input.provider.provider_id {
        ProviderId::ClaudeCode => {
            crate::runner::claude::build_stdin_checked(&input.prompt.text, &[]).map_err(error)?
        }
        ProviderId::Codex => {
            let mut bytes = serde_json::to_vec(&crate::runner::codex_app_server::initialize())
                .map_err(error)?;
            bytes.push(b'\n');
            bytes
        }
    };
    Ok((program, args, bytes))
}

/// The formal review's single App Server thread and turn. No `sandbox` is
/// sent: it would replace the launch's root-denying permission profile.
pub(crate) fn codex_app_server_adapter(
    input: &AgentTurnInputV1,
    cwd: &Path,
) -> crate::runner::codex_app_server::AppServer {
    let thread = json!({
        "model": input.provider.model_id,
        "cwd": cwd,
        "approvalPolicy": "never",
        "ephemeral": true,
    });
    let mut turn = json!({
        "input": [{"type": "text", "text": input.prompt.text}],
        "model": input.provider.model_id,
    });
    if let Some(effort) = input.provider.options.get("effort").and_then(Value::as_str) {
        turn["effort"] = json!(effort);
    }
    crate::runner::codex_app_server::AppServer::formal(thread, turn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use g5_contracts::types::{
        render_agent_approval_frontmatter_v2, AgentApprovalOutcome, ApprovalReviewResolutionV2,
        MergeSourceHead,
    };
    pub(super) fn assignment() -> FormalApprovalInvocation {
        let source = uuid::Uuid::new_v4();
        let target = uuid::Uuid::new_v4();
        let repo = uuid::Uuid::new_v4();
        FormalApprovalInvocation::render(
            ApprovalReviewResolutionV2 {
                assigned_at: None,
                merge_base: None,
                workspace_base_heads: None,
                repository_names: Default::default(),
                schema_version: 2,
                resolution_id: format!("apr_{}", "a".repeat(64)),
                source_workspace_id: source,
                source_heads: vec![MergeSourceHead {
                    repo_id: repo.to_string(),
                    workspace_id: source.to_string(),
                    branch_name: "source".into(),
                    head_commit: "a".repeat(40),
                }],
                target_workspace_id: target,
                target_heads: vec![MergeSourceHead {
                    repo_id: repo.to_string(),
                    workspace_id: target.to_string(),
                    branch_name: "target".into(),
                    head_commit: "b".repeat(40),
                }],
                definition_id: uuid::Uuid::new_v4(),
                definition_version: 1,
                output_doc_id: uuid::Uuid::new_v4(),
                output_repo_id: repo,
                output_doc_path: format!(
                    "docs/{source}/reviews/agent-approval-{}.md",
                    uuid::Uuid::new_v4()
                ),
            },
            "Review",
            "Check behavior.",
        )
        .unwrap()
    }
    fn config(root: &Path) -> ToolConfig {
        ToolConfig {
            invocation: assignment(),
            repositories: BTreeMap::new(),
            output: root.join("report.md"),
        }
    }
    fn document(config: &ToolConfig, result: AgentApprovalOutcome) -> String {
        format!(
            "---\n{}\n---\n\nExact evidence: é.\n",
            render_agent_approval_frontmatter_v2(&config.invocation.resolution, result).unwrap()
        )
    }
    #[tokio::test]
    async fn sole_output_preserves_exact_bytes_and_digest_for_both_verdicts() {
        for result in [
            AgentApprovalOutcome::Approved,
            AgentApprovalOutcome::Rejected,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let cfg = config(temp.path());
            let text = document(&cfg, result);
            let call_args = json!({"name":"write_report","arguments":{"path":cfg.invocation.resolution.output_doc_path,"document":text}});
            call(&cfg, &call_args).await.unwrap();
            let read = harvest(&cfg).unwrap();
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(read["contentBase64"].as_str().unwrap())
                    .unwrap(),
                text.as_bytes()
            );
            assert_eq!(
                read["sha256"],
                format!("{:x}", Sha256::digest(text.as_bytes()))
            );
            assert!(
                call(&cfg, &call_args).await.is_err(),
                "a late callback cannot rewrite the report"
            );
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 2);
        }
    }
    /// A schema 4, 5 or 6 assignment with frozen merge bases.
    fn merge_base_config(root: &Path, schema_version: u8) -> ToolConfig {
        let mut resolution = assignment().resolution;
        let repo = resolution.source_heads[0].repo_id.clone();
        resolution.schema_version = schema_version;
        resolution.workspace_base_heads = Some(resolution.source_heads.clone());
        resolution
            .repository_names
            .insert(repo.clone(), "App".into());
        resolution.assigned_at = Some("2026-10-30T10:00:00Z".into());
        resolution.merge_base = Some(g5_contracts::types::ApprovalMergeBaseCut {
            repositories: vec![g5_contracts::types::ApprovalMergeBaseRepository {
                repo_id: repo,
                repo_name: "App".into(),
                git_sha: "c".repeat(40),
            }],
        });
        ToolConfig {
            invocation: FormalApprovalInvocation::render(resolution, "Review", "Check behavior.")
                .unwrap(),
            repositories: BTreeMap::new(),
            output: root.join("report.md"),
        }
    }
    #[tokio::test]
    async fn schema5_and_6_report_tools_write_the_authored_summary_and_unchanged_body() {
        let names = |schema: u8| {
            tools(schema)
                .iter()
                .find(|tool| tool["name"] == "write_report")
                .unwrap()["inputSchema"]["required"]
                .clone()
        };
        assert_eq!(names(5), json!(["result", "summary", "body"]));
        assert_eq!(names(6), names(5));
        let description = |schema: u8| {
            tools(schema)
                .iter()
                .find(|tool| tool["name"] == "write_report")
                .unwrap()["description"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert!(description(6).contains("## Governing prompt"));
        assert!(!description(5).contains("Governing prompt"));
        assert_eq!(names(4), json!(["result", "body"]));
        assert_eq!(names(3), json!(["path", "document"]));
        let summary =
            "Approved: the retry flow converges on one decision; tests cover cancellation.";
        let body = "# Scope\n\nReviewed the frozen merge-base comparison.";
        let mut frontmatters = Vec::new();
        for (schema, result, verdict) in [
            (5, "approved", AgentApprovalOutcome::Approved),
            (5, "rejected", AgentApprovalOutcome::Rejected),
            (6, "approved", AgentApprovalOutcome::Approved),
            (6, "rejected", AgentApprovalOutcome::Rejected),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let cfg = merge_base_config(temp.path(), schema);
            for invalid in [
                json!({"result":result,"body":body}),
                json!({"result":result,"summary":"","body":body}),
                json!({"result":result,"summary":"x".repeat(251),"body":body}),
                json!({"result":result,"summary":"two\nlines","body":body}),
                json!({"result":result,"summary":summary,"body":" "}),
                json!({"result":result,"summary":summary,"body":body,"extra":true}),
                json!({"result":"deferred","summary":summary,"body":body}),
            ] {
                assert!(
                    call(&cfg, &json!({"name":"write_report","arguments":invalid}))
                        .await
                        .is_err(),
                    "{invalid}"
                );
                assert!(!cfg.output.exists());
            }
            let args = json!({"name":"write_report","arguments":{"result":result,"summary":summary,"body":body}});
            call(&cfg, &args).await.unwrap();
            let report = std::fs::read(&cfg.output).unwrap();
            let parsed =
                validate_agent_approval_markdown_v2(&report, &cfg.invocation.resolution).unwrap();
            assert_eq!(parsed.outcome, verdict);
            assert_eq!(parsed.summary, summary);
            let text = String::from_utf8(report.clone()).unwrap();
            assert!(text.ends_with(&format!("\n---\n\n{body}\n")));
            assert!(text.contains("submittedAt:"));
            // Schema 6 writes schema 5's frontmatter fields, in the same
            // order, with the same summary; only the version differs.
            assert!(text.starts_with(&format!("---\nschemaVersion: {schema}\n")));
            frontmatters.push(
                text.lines()
                    .map(|line| match line.split_once(": ") {
                        Some(("summary", _)) => line.to_string(),
                        Some((key, _)) => key.to_string(),
                        None => line.to_string(),
                    })
                    .collect::<Vec<_>>(),
            );
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(harvest(&cfg).unwrap()["contentBase64"].as_str().unwrap())
                    .unwrap(),
                report
            );
            assert!(
                call(&cfg, &args).await.is_err(),
                "a late callback cannot rewrite the report"
            );
        }
        assert_eq!(frontmatters[0], frontmatters[2]);
        assert_eq!(frontmatters[1], frontmatters[3]);
        // Schema 4 keeps its published tool: a summary argument is refused.
        let temp = tempfile::tempdir().unwrap();
        let cfg = merge_base_config(temp.path(), 4);
        assert!(call(
            &cfg,
            &json!({"name":"write_report","arguments":{"result":"approved","summary":summary,"body":body}})
        )
        .await
        .is_err());
        call(
            &cfg,
            &json!({"name":"write_report","arguments":{"result":"approved","body":body}}),
        )
        .await
        .unwrap();
        let report = String::from_utf8(std::fs::read(&cfg.output).unwrap()).unwrap();
        assert!(!report.contains("summary"));
    }
    #[tokio::test]
    async fn report_tool_denies_other_paths_unknown_fields_and_invalid_assignments() {
        let temp = tempfile::tempdir().unwrap();
        let cfg = config(temp.path());
        let text = document(&cfg, AgentApprovalOutcome::Approved);
        for args in [
            json!({"path":"../escape","document":text}),
            json!({"path":cfg.invocation.resolution.output_doc_path,"document":text,"extra":true}),
            json!({"path":cfg.invocation.resolution.output_doc_path,"document":text.replace("result: approved","result: deferred")}),
            json!({"path":cfg.invocation.resolution.output_doc_path,"document":text.replace(&cfg.invocation.resolution.resolution_id,"apr_bad")}),
        ] {
            assert!(call(&cfg, &json!({"name":"write_report","arguments":args}))
                .await
                .is_err());
        }
        assert!(!cfg.output.exists());
    }
    #[test]
    fn harvesting_refuses_missing_nonregular_links_and_oversize_output() {
        let temp = tempfile::tempdir().unwrap();
        let cfg = config(temp.path());
        assert!(harvest(&cfg).is_err());
        std::fs::create_dir(&cfg.output).unwrap();
        assert!(harvest(&cfg).is_err());
        std::fs::remove_dir(&cfg.output).unwrap();
        std::fs::write(&cfg.output, vec![b'x'; AGENT_APPROVAL_OUTPUT_MAX_BYTES + 1]).unwrap();
        assert!(harvest(&cfg).is_err());
        std::fs::remove_file(&cfg.output).unwrap();
        #[cfg(unix)]
        {
            let outside = temp.path().join("outside");
            std::fs::write(&outside, document(&cfg, AgentApprovalOutcome::Approved)).unwrap();
            std::os::unix::fs::symlink(&outside, &cfg.output).unwrap();
            assert!(harvest(&cfg).is_err());
            std::fs::remove_file(&cfg.output).unwrap();
            std::fs::hard_link(&outside, &cfg.output).unwrap();
            assert!(harvest(&cfg).is_err());
        }
    }
    #[tokio::test]
    async fn reads_stay_on_assigned_git_objects_despite_dirty_and_moved_heads() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repo");
        std::fs::create_dir(&repository).unwrap();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .current_dir(&repository)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        };
        git(&["init", "-q"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "user.email", "test@example.invalid"]);
        std::fs::write(repository.join("review.txt"), "base").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);
        let base = git(&["rev-parse", "HEAD"]);
        std::fs::write(repository.join("review.txt"), "assigned source").unwrap();
        git(&["commit", "-qam", "source"]);
        let source = git(&["rev-parse", "HEAD"]);
        let mut cfg = config(temp.path());
        let repo = cfg.invocation.resolution.source_heads[0].repo_id.clone();
        cfg.repositories.insert(repo.clone(), repository.clone());
        cfg.invocation.resolution.source_heads[0].head_commit = source.clone();
        cfg.invocation.resolution.target_heads[0].head_commit = source.clone();
        let mut comparison = cfg.invocation.resolution.source_heads[0].clone();
        comparison.head_commit = base;
        cfg.invocation.resolution.schema_version = 3;
        cfg.invocation.resolution.workspace_base_heads = Some(vec![comparison]);
        cfg.invocation
            .resolution
            .repository_names
            .insert(repo.clone(), "Readable app".into());
        std::fs::write(repository.join("review.txt"), "later pushed content").unwrap();
        git(&["commit", "-qam", "later"]);
        std::fs::write(repository.join("review.txt"), "dirty unpushed content").unwrap();
        let read = |arguments: Value| json!({"name":"read_review","arguments":arguments});
        assert_eq!(
            call(
                &cfg,
                &read(json!({"repoId":repo,"operation":"file","path":"review.txt"}))
            )
            .await
            .unwrap(),
            "assigned source"
        );
        assert_eq!(
            call(
                &cfg,
                &read(json!({"repoId":repo,"operation":"file","cut":"target","path":"review.txt"}))
            )
            .await
            .unwrap(),
            "base"
        );
        for args in [
            json!({"repoId":repo,"operation":"file","path":"../../secret"}),
            json!({"repoId":repo,"operation":"file","path":"review.txt","commit":"HEAD"}),
            json!({"repoId":"foreign","operation":"paths"}),
        ] {
            assert!(call(&cfg, &read(args)).await.is_err());
        }
        assert_eq!(
            std::fs::read_to_string(repository.join("review.txt")).unwrap(),
            "dirty unpushed content"
        );
        assert_ne!(source, git(&["rev-parse", "HEAD"]));
    }
    #[test]
    fn supervisor_environment_does_not_inherit_hooks_tools_or_platform_credentials() {
        let mut environment = BTreeMap::from([
            ("LD_PRELOAD".into(), "malicious.so".into()),
            ("G5D_WORKSPACE_TOKEN".into(), "secret".into()),
            ("BASH_ENV".into(), "script".into()),
            ("ANTHROPIC_API_KEY".into(), "selected".into()),
            ("HOME".into(), "/fresh".into()),
            ("PATH".into(), "/untrusted/project/bin".into()),
            ("CODEX_HOME".into(), "/fresh/.codex".into()),
            ("ENABLE_TOOL_SEARCH".into(), "true".into()),
        ]);
        sanitize_environment(&mut environment);
        assert_eq!(
            environment.get("ANTHROPIC_API_KEY").map(String::as_str),
            Some("selected")
        );
        assert_eq!(
            environment.get("CODEX_HOME").map(String::as_str),
            Some("/fresh/.codex")
        );
        assert!(!environment["PATH"].contains("/untrusted/project/bin"));
        assert!(!environment.contains_key("LD_PRELOAD"));
        assert!(!environment.contains_key("G5D_WORKSPACE_TOKEN"));
        assert!(!environment.contains_key("BASH_ENV"));
        assert_eq!(environment["ENABLE_TOOL_SEARCH"], "false");
    }
}

#[cfg(test)]
mod comparison_parity {
    use super::tests::assignment;
    use super::*;
    #[tokio::test]
    async fn frozen_assignment_and_confined_diff_match_the_shared_git_matrix() {
        let fixture = g5_scm::comparison_fixture::comparison_fixture();
        for case in &fixture.scenarios {
            let git = g5_scm::GitRepo::open(fixture.directory.path()).unwrap();
            let source = &fixture.heads[case.source];
            let parent = &fixture.heads[case.parent];
            let fork = &fixture.heads[case.base];
            let capture = git.pushed_comparison(source, parent, Some(fork), 250);
            let Some(_) = case.merge_base else {
                assert!(capture.is_err());
                continue;
            };
            let capture = capture.unwrap();
            let output = tempfile::tempdir().unwrap();
            let mut invocation = assignment();
            let resolution = &mut invocation.resolution;
            let repo = resolution.source_heads[0].repo_id.clone();
            resolution.schema_version = 4;
            resolution.source_heads[0].head_commit = source.clone();
            resolution.target_heads[0].head_commit = parent.clone();
            resolution.workspace_base_heads = Some(vec![g5_contracts::types::MergeSourceHead {
                head_commit: fork.clone(),
                ..resolution.source_heads[0].clone()
            }]);
            resolution
                .repository_names
                .insert(repo.clone(), "App".into());
            resolution.assigned_at = Some("2026-10-27T10:00:00Z".into());
            resolution.merge_base = Some(g5_contracts::types::ApprovalMergeBaseCut {
                repositories: vec![g5_contracts::types::ApprovalMergeBaseRepository {
                    repo_id: repo.clone(),
                    repo_name: "App".into(),
                    git_sha: capture.merge_base.clone(),
                }],
            });
            invocation = FormalApprovalInvocation::render(
                invocation.resolution,
                "Review",
                "Review exact source work",
            )
            .unwrap();
            let cfg = ToolConfig {
                invocation,
                repositories: BTreeMap::from([(repo.clone(), fixture.directory.path().to_owned())]),
                output: output.path().join("report.md"),
            };
            let read = call(
                &cfg,
                &json!({"name":"read_review","arguments":{"repoId":repo,"operation":"diff"}}),
            )
            .await
            .unwrap();
            let actual = g5_scm::source_control_patch(read.as_bytes()).unwrap();
            let expected = git
                .source_control_diff(&capture.merge_base, source)
                .unwrap();
            assert_eq!(
                serde_json::to_value(&actual).unwrap(),
                serde_json::to_value(&expected).unwrap(),
                "{}",
                case.name
            );
            let frozen = call(
                &cfg,
                &json!({"name":"read_review","arguments":{"operation":"comparison"}}),
            )
            .await
            .unwrap();
            assert!(frozen.contains(&capture.merge_base));
            assert!(call(
                &cfg,
                &json!({"name":"write_report","arguments":{"result":"approved","body":""}})
            )
            .await
            .is_err());
            call(&cfg,&json!({"name":"write_report","arguments":{"result":"approved","body":if expected.is_empty(){"Empty comparison; no branch changes."}else{"Reviewed only the frozen merge-base to source comparison."}}})).await.unwrap();
            let report = std::fs::read(&cfg.output).unwrap();
            validate_agent_approval_markdown_v2(&report, &cfg.invocation.resolution).unwrap();
            assert!(String::from_utf8(report.clone())
                .unwrap()
                .contains("submittedAt:"));
            let harvested = harvest(&cfg).unwrap();
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(harvested["contentBase64"].as_str().unwrap())
                    .unwrap(),
                report
            );
        }
    }
}
