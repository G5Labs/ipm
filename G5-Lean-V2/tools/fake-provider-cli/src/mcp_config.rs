//! Locating the MCP server the fakes must talk to.
//!
//! Claude reads `--mcp-config` (JSON files or inline JSON with a top-level
//! `mcpServers` map); Codex reads `[mcp_servers.<name>]` tables from
//! `$CODEX_HOME/config.toml`. Both describe a stdio server by `command`,
//! `args`, optional `cwd` and `env`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

/// How to spawn one MCP stdio server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerSpec {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    /// How long to wait for `initialize` to answer.
    pub startup_timeout: Duration,
    /// How long to wait for a `tools/call` to answer.
    pub tool_timeout: Duration,
}

/// Codex defaults (`startup_timeout_sec = 10`, `tool_timeout_sec = 60`).
pub const CODEX_DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
pub const CODEX_DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(60);
/// Claude's `MCP_TOOL_TIMEOUT` is milliseconds; the fake's default when the
/// variable is unset.
pub const CLAUDE_DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(30);
pub const CLAUDE_DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// Parse every `--mcp-config` value (file path or inline JSON) into server
/// specs, later values overriding earlier ones with the same name.
pub fn claude_servers(
    configs: &[String],
    base_dir: &Path,
    tool_timeout: Duration,
) -> Result<BTreeMap<String, McpServerSpec>, String> {
    let mut servers = BTreeMap::new();
    for config in configs {
        let trimmed = config.trim();
        let (document, origin): (Value, String) = if trimmed.starts_with('{') {
            (
                serde_json::from_str(trimmed)
                    .map_err(|err| format!("--mcp-config inline JSON is invalid: {err}"))?,
                "--mcp-config <inline>".to_string(),
            )
        } else {
            let path = base_dir.join(trimmed);
            let text = fs::read_to_string(&path).map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    // The pinned CLI's message (exit 1, before any frame).
                    format!(
                        "Error: Invalid MCP configuration:\nMCP config file not found: {}",
                        path.display()
                    )
                } else {
                    format!("cannot read --mcp-config {}: {err}", path.display())
                }
            })?;
            (
                serde_json::from_str(&text).map_err(|err| {
                    format!("--mcp-config {} is invalid JSON: {err}", path.display())
                })?,
                path.display().to_string(),
            )
        };
        let Some(map) = document.get("mcpServers").and_then(Value::as_object) else {
            return Err(format!("{origin} has no \"mcpServers\" object"));
        };
        for (name, entry) in map {
            let spec = spec_from_json(name, entry, base_dir, tool_timeout)
                .map_err(|err| format!("{origin}: server {name:?}: {err}"))?;
            servers.insert(name.clone(), spec);
        }
    }
    Ok(servers)
}

fn spec_from_json(
    name: &str,
    entry: &Value,
    base_dir: &Path,
    tool_timeout: Duration,
) -> Result<McpServerSpec, String> {
    let object = entry
        .as_object()
        .ok_or_else(|| "server entry must be an object".to_string())?;
    if let Some(kind) = object.get("type").and_then(Value::as_str) {
        if kind != "stdio" {
            return Err(format!(
                "only stdio servers are supported, got type {kind:?}"
            ));
        }
    }
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| "\"command\" must be a string".to_string())?
        .to_string();
    let args = match object.get("args") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(String::from)
                    .ok_or_else(|| "\"args\" must be an array of strings".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err("\"args\" must be an array of strings".to_string()),
    };
    let cwd = match object.get("cwd") {
        None | Some(Value::Null) => None,
        Some(Value::String(dir)) => Some(resolve_dir(base_dir, dir)),
        Some(_) => return Err("\"cwd\" must be a string".to_string()),
    };
    let env = match object.get("env") {
        None | Some(Value::Null) => BTreeMap::new(),
        Some(Value::Object(map)) => map
            .iter()
            .map(|(k, v)| {
                v.as_str()
                    .map(|s| (k.clone(), s.to_string()))
                    .ok_or_else(|| "\"env\" values must be strings".to_string())
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?,
        Some(_) => return Err("\"env\" must be an object".to_string()),
    };
    Ok(McpServerSpec {
        name: name.to_string(),
        command,
        args,
        cwd,
        env,
        startup_timeout: CLAUDE_DEFAULT_STARTUP_TIMEOUT,
        tool_timeout,
    })
}

/// Read `$CODEX_HOME/config.toml` and return its `[mcp_servers.*]` tables.
pub fn codex_servers(codex_home: &Path) -> Result<BTreeMap<String, McpServerSpec>, String> {
    let path = codex_home.join("config.toml");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(err) => return Err(format!("cannot read {}: {err}", path.display())),
    };
    let document: toml::Value = toml::from_str(&text)
        .map_err(|err| format!("{} is not valid TOML: {err}", path.display()))?;
    let mut servers = BTreeMap::new();
    let Some(table) = document.get("mcp_servers").and_then(toml::Value::as_table) else {
        return Ok(servers);
    };
    for (name, entry) in table {
        let spec = spec_from_toml(name, entry, codex_home)
            .map_err(|err| format!("{}: [mcp_servers.{name}]: {err}", path.display()))?;
        servers.insert(name.clone(), spec);
    }
    Ok(servers)
}

/// Native `-c mcp_servers.*` overrides are part of the provider contract too.
pub fn codex_override_servers(
    overrides: &[(String, String)],
    base: &Path,
) -> Result<BTreeMap<String, McpServerSpec>, String> {
    let text = overrides
        .iter()
        .filter(|(key, _)| key.starts_with("mcp_servers."))
        .map(|(key, value)| format!("{key}={value}\n"))
        .collect::<String>();
    if text.is_empty() {
        return Ok(BTreeMap::new());
    }
    let document: toml::Value = toml::from_str(&text).map_err(|e| e.to_string())?;
    document
        .get("mcp_servers")
        .and_then(toml::Value::as_table)
        .ok_or("MCP override table missing".to_string())?
        .iter()
        .map(|(name, entry)| Ok((name.clone(), spec_from_toml(name, entry, base)?)))
        .collect()
}

fn spec_from_toml(
    name: &str,
    entry: &toml::Value,
    base_dir: &Path,
) -> Result<McpServerSpec, String> {
    let table = entry
        .as_table()
        .ok_or_else(|| "must be a table".to_string())?;
    if table
        .get("enabled")
        .and_then(toml::Value::as_bool)
        .is_some_and(|enabled| !enabled)
    {
        return Err("server is disabled".to_string());
    }
    let command = table
        .get("command")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| "command must be a string (only stdio servers are supported)".to_string())?
        .to_string();
    let args = match table.get("args") {
        None => Vec::new(),
        Some(toml::Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(String::from)
                    .ok_or_else(|| "args must be an array of strings".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err("args must be an array of strings".to_string()),
    };
    let cwd = match table.get("cwd") {
        None => None,
        Some(toml::Value::String(dir)) => Some(resolve_dir(base_dir, dir)),
        Some(_) => return Err("cwd must be a string".to_string()),
    };
    let env = match table.get("env") {
        None => BTreeMap::new(),
        Some(toml::Value::Table(map)) => map
            .iter()
            .map(|(k, v)| {
                v.as_str()
                    .map(|s| (k.clone(), s.to_string()))
                    .ok_or_else(|| "env values must be strings".to_string())
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?,
        Some(_) => return Err("env must be a table".to_string()),
    };
    let startup_timeout =
        timeout_secs(table, "startup_timeout_sec")?.unwrap_or(CODEX_DEFAULT_STARTUP_TIMEOUT);
    let tool_timeout =
        timeout_secs(table, "tool_timeout_sec")?.unwrap_or(CODEX_DEFAULT_TOOL_TIMEOUT);
    Ok(McpServerSpec {
        name: name.to_string(),
        command,
        args,
        cwd,
        env,
        startup_timeout,
        tool_timeout,
    })
}

fn timeout_secs(table: &toml::Table, key: &str) -> Result<Option<Duration>, String> {
    match table.get(key) {
        None => Ok(None),
        Some(toml::Value::Integer(secs)) if *secs >= 0 => {
            Ok(Some(Duration::from_secs(*secs as u64)))
        }
        Some(toml::Value::Float(secs)) if *secs >= 0.0 => Ok(Some(Duration::from_secs_f64(*secs))),
        Some(_) => Err(format!("{key} must be a non-negative number of seconds")),
    }
}

/// A relative `cwd` in a server entry is resolved against the fake's working
/// directory (Claude) or `CODEX_HOME` (Codex); `.` therefore means the
/// process cwd for Claude, matching how the profile's `.mcp.json` is written.
fn resolve_dir(base_dir: &Path, dir: &str) -> PathBuf {
    let candidate = Path::new(dir);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base_dir.join(candidate)
    }
}

/// `MCP_TOOL_TIMEOUT` (milliseconds) as the real Claude CLI reads it.
pub fn claude_tool_timeout_from_env() -> Result<Duration, String> {
    match std::env::var("MCP_TOOL_TIMEOUT") {
        Ok(raw) if !raw.trim().is_empty() => raw
            .trim()
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| {
                format!("MCP_TOOL_TIMEOUT must be an integer number of milliseconds, got {raw:?}")
            }),
        _ => Ok(CLAUDE_DEFAULT_TOOL_TIMEOUT),
    }
}

/// The Codex home directory: `CODEX_HOME`, else `$HOME/.codex`.
pub fn codex_home() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("CODEX_HOME") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".codex"))
        .map_err(|_| "neither CODEX_HOME nor HOME is set".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_config_file_and_inline_json_parse() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".mcp.json");
        fs::write(
            &file,
            r#"{"mcpServers":{"g5-tools":{"command":"/opt/g5/bin/g5d","args":["mcp","intent-grounding"],"cwd":".","env":{"LD_LIBRARY_PATH":"/opt/g5/bin"}}}}"#,
        )
        .unwrap();
        let servers = claude_servers(
            &[
                ".mcp.json".to_string(),
                r#"{"mcpServers":{"other":{"type":"stdio","command":"x"}}}"#.to_string(),
            ],
            dir.path(),
            Duration::from_secs(5),
        )
        .unwrap();
        let g5 = &servers["g5-tools"];
        assert_eq!(g5.command, "/opt/g5/bin/g5d");
        assert_eq!(g5.args, vec!["mcp", "intent-grounding"]);
        assert_eq!(g5.cwd.as_deref(), Some(dir.path().join(".").as_path()));
        assert_eq!(
            g5.env.get("LD_LIBRARY_PATH").map(String::as_str),
            Some("/opt/g5/bin")
        );
        assert_eq!(g5.tool_timeout, Duration::from_secs(5));
        assert!(servers.contains_key("other"));
        assert!(claude_servers(
            &["missing.json".to_string()],
            dir.path(),
            Duration::from_secs(1)
        )
        .is_err());
        assert!(claude_servers(
            &[r#"{"mcpServers":{"h":{"type":"http","url":"x"}}}"#.to_string()],
            dir.path(),
            Duration::from_secs(1)
        )
        .is_err());
    }

    #[test]
    fn codex_config_toml_parses_with_timeouts() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("config.toml"),
            r#"
approval_policy = "never"
[mcp_servers.g5-tools]
command = "/opt/g5/bin/g5d"
args = ["mcp", "intent-grounding"]
cwd = "/work"
required = true
startup_timeout_sec = 120
tool_timeout_sec = 900
[mcp_servers.g5-tools.env]
LD_LIBRARY_PATH = "/opt/g5/bin"
[mcp_servers.g5-tools.tools.echo]
approval_mode = "approve"
"#,
        )
        .unwrap();
        let servers = codex_servers(dir.path()).unwrap();
        let g5 = &servers["g5-tools"];
        assert_eq!(g5.cwd.as_deref(), Some(Path::new("/work")));
        assert_eq!(g5.startup_timeout, Duration::from_secs(120));
        assert_eq!(g5.tool_timeout, Duration::from_secs(900));
        assert_eq!(g5.env.len(), 1);
        assert!(codex_servers(&dir.path().join("nope")).unwrap().is_empty());
    }
}
