//! Argument parsers that accept the real CLIs' argv.
//!
//! `parse_claude` follows the commander conventions of the Claude Code CLI
//! (`--flag=value`, variadic options that swallow following non-option
//! tokens, optional-value options, `--` terminator); `parse_codex` follows the
//! clap conventions of `codex exec`. Both reject unknown options the way the
//! real binaries do so a runner that passes a flag the pinned CLI lacks fails
//! in tests rather than in production.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arity {
    Flag,
    Value,
    OptionalValue,
    Variadic,
}

/// Parsed argv: options by canonical long name, boolean flags, positionals.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ParsedArgs {
    pub options: BTreeMap<String, Vec<String>>,
    pub flags: BTreeSet<String>,
    pub positional: Vec<String>,
}

impl ParsedArgs {
    /// Whether a boolean flag (or an optional-value option given without a
    /// value) was present.
    pub fn has(&self, name: &str) -> bool {
        self.flags.contains(name) || self.options.contains_key(name)
    }

    /// Last value of a valued option.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.options
            .get(name)
            .and_then(|values| values.last())
            .map(String::as_str)
    }

    /// All values of a valued option.
    pub fn values(&self, name: &str) -> &[String] {
        self.options.get(name).map(Vec::as_slice).unwrap_or(&[])
    }
}

const CLAUDE_OPTIONS: &[(&str, Arity)] = &[
    ("--print", Arity::Flag),
    ("--verbose", Arity::Flag),
    ("--include-partial-messages", Arity::Flag),
    ("--strict-mcp-config", Arity::Flag),
    ("--replay-user-messages", Arity::Flag),
    ("--no-session-persistence", Arity::Flag),
    ("--include-hook-events", Arity::Flag),
    ("--forward-subagent-text", Arity::Flag),
    ("--dangerously-skip-permissions", Arity::Flag),
    ("--allow-dangerously-skip-permissions", Arity::Flag),
    ("--bare", Arity::Flag),
    ("--restricted", Arity::Flag),
    ("--fork-session", Arity::Flag),
    ("--continue", Arity::Flag),
    ("--exclude-dynamic-system-prompt-sections", Arity::Flag),
    ("--brief", Arity::Flag),
    ("--chrome", Arity::Flag),
    ("--no-chrome", Arity::Flag),
    ("--ide", Arity::Flag),
    ("--disable-slash-commands", Arity::Flag),
    ("--safe-mode", Arity::Flag),
    ("--ax-screen-reader", Arity::Flag),
    ("--bg", Arity::Flag),
    ("--tmux", Arity::Flag),
    ("--version", Arity::Flag),
    ("--help", Arity::Flag),
    ("--output-format", Arity::Value),
    ("--input-format", Arity::Value),
    ("--session-id", Arity::Value),
    ("--permission-mode", Arity::Value),
    ("--permission-prompts", Arity::Value),
    ("--permission-prompt-tool", Arity::Value),
    ("--settings", Arity::Value),
    ("--setting-sources", Arity::Value),
    ("--append-system-prompt", Arity::Value),
    ("--append-system-prompt-file", Arity::Value),
    ("--system-prompt", Arity::Value),
    ("--system-prompt-file", Arity::Value),
    ("--system-prompt-snapshot", Arity::Value),
    ("--max-turns", Arity::Value),
    ("--model", Arity::Value),
    ("--effort", Arity::Value),
    ("--agents", Arity::Value),
    ("--agent", Arity::Value),
    ("--max-budget-usd", Arity::Value),
    ("--fallback-model", Arity::Value),
    ("--json-schema", Arity::Value),
    ("--debug-file", Arity::Value),
    ("--name", Arity::Value),
    ("--autocompact", Arity::Value),
    ("--environment", Arity::Value),
    ("--remote-control-session-name-prefix", Arity::Value),
    ("--plugin-dir", Arity::Value),
    ("--plugin-url", Arity::Value),
    ("--resume", Arity::OptionalValue),
    ("--debug", Arity::OptionalValue),
    ("--prompt-suggestions", Arity::OptionalValue),
    ("--cloud", Arity::OptionalValue),
    ("--from-pr", Arity::OptionalValue),
    ("--teleport", Arity::OptionalValue),
    ("--worktree", Arity::OptionalValue),
    ("--remote-control", Arity::OptionalValue),
    ("--mcp-config", Arity::Variadic),
    ("--disallowedTools", Arity::Variadic),
    ("--allowedTools", Arity::Variadic),
    ("--add-dir", Arity::Variadic),
    ("--tools", Arity::Variadic),
    ("--betas", Arity::Variadic),
    ("--file", Arity::Variadic),
];

const CLAUDE_ALIASES: &[(&str, &str)] = &[
    ("-p", "--print"),
    ("-r", "--resume"),
    ("-c", "--continue"),
    ("-d", "--debug"),
    ("-n", "--name"),
    ("-v", "--version"),
    ("-h", "--help"),
    ("-w", "--worktree"),
    ("--disallowed-tools", "--disallowedTools"),
    ("--allowed-tools", "--allowedTools"),
    ("--background", "--bg"),
];

fn claude_canonical(token: &str) -> Option<(&'static str, Arity)> {
    let name = CLAUDE_ALIASES
        .iter()
        .find(|(alias, _)| *alias == token)
        .map(|(_, canonical)| *canonical)
        .unwrap_or(token);
    CLAUDE_OPTIONS
        .iter()
        .find(|(option, _)| *option == name)
        .map(|(option, arity)| (*option, *arity))
}

/// Parse Claude Code argv (without the program name).
pub fn parse_claude(args: &[String]) -> Result<ParsedArgs, String> {
    let mut parsed = ParsedArgs::default();
    let mut index = 0;
    while index < args.len() {
        let token = args[index].as_str();
        index += 1;
        if token == "--" {
            parsed.positional.extend(args[index..].iter().cloned());
            break;
        }
        if !token.starts_with('-') || token == "-" {
            parsed.positional.push(token.to_string());
            continue;
        }
        let (name_part, inline_value) = match token.split_once('=') {
            Some((name, value)) if token.starts_with("--") => (name, Some(value.to_string())),
            _ => (token, None),
        };
        let Some((canonical, arity)) = claude_canonical(name_part) else {
            return Err(format!("error: unknown option '{name_part}'"));
        };
        match arity {
            Arity::Flag => {
                if inline_value.is_some() {
                    return Err(format!("error: option '{canonical}' does not take a value"));
                }
                parsed.flags.insert(canonical.to_string());
            }
            Arity::Value => {
                let value = match inline_value {
                    Some(value) => value,
                    None => {
                        let next = args.get(index).cloned().ok_or_else(|| {
                            format!("error: option '{canonical} <value>' argument missing")
                        })?;
                        index += 1;
                        next
                    }
                };
                parsed
                    .options
                    .entry(canonical.to_string())
                    .or_default()
                    .push(value);
            }
            Arity::OptionalValue => {
                let entry = parsed.options.entry(canonical.to_string()).or_default();
                match inline_value {
                    Some(value) => entry.push(value),
                    None => {
                        if let Some(next) = args.get(index).filter(|next| !next.starts_with('-')) {
                            entry.push(next.clone());
                            index += 1;
                        }
                    }
                }
            }
            Arity::Variadic => {
                let entry = parsed.options.entry(canonical.to_string()).or_default();
                match inline_value {
                    Some(value) => entry.push(value),
                    None => {
                        let mut consumed = 0;
                        while let Some(next) = args.get(index + consumed) {
                            if next.starts_with('-') && next != "-" {
                                break;
                            }
                            entry.push(next.clone());
                            consumed += 1;
                        }
                        if consumed == 0 {
                            return Err(format!(
                                "error: option '{canonical} <values...>' argument missing"
                            ));
                        }
                        index += consumed;
                    }
                }
            }
        }
    }
    Ok(parsed)
}

/// The `codex exec` invocation the fake understands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexArgs {
    /// `codex exec resume <id>`: the session id to resume.
    pub resume: Option<String>,
    /// `--last` on `exec resume`.
    pub resume_last: bool,
    pub json: bool,
    pub skip_git_repo_check: bool,
    pub full_auto: bool,
    pub bypass_approvals_and_sandbox: bool,
    pub ephemeral: bool,
    pub ignore_user_config: bool,
    pub oss: bool,
    pub search: bool,
    pub output_last_message: Option<String>,
    pub output_schema: Option<String>,
    pub model: Option<String>,
    pub profile: Option<String>,
    pub sandbox: Option<String>,
    pub ask_for_approval: Option<String>,
    pub cd: Option<String>,
    pub color: Option<String>,
    pub local_provider: Option<String>,
    pub add_dirs: Vec<String>,
    /// `-c key=value` overrides in order.
    pub config_overrides: Vec<(String, String)>,
    /// `-i` image paths (comma-separated values are split).
    pub images: Vec<String>,
    /// The positional prompt, if any (`-` means stdin, like no prompt).
    pub prompt: Option<String>,
}

/// What a `codex` argv asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexInvocation {
    Version,
    Help { subcommand: Option<String> },
    Exec(Box<CodexArgs>),
}

/// Parse `codex` argv (without the program name).
pub fn parse_codex(args: &[String]) -> Result<CodexInvocation, String> {
    let mut index = 0;
    // Global options may precede the subcommand.
    let mut exec = CodexArgs::default();
    let mut subcommand: Option<String> = None;
    let mut positionals: Vec<String> = Vec::new();
    let mut help = false;
    while index < args.len() {
        let token = args[index].as_str();
        index += 1;
        if token == "--" {
            positionals.extend(args[index..].iter().cloned());
            break;
        }
        match token {
            "-V" | "--version" => return Ok(CodexInvocation::Version),
            "--strict-config" => continue,
            "-h" | "--help" => {
                help = true;
                continue;
            }
            _ => {}
        }
        if token.starts_with('-') && token != "-" {
            let (name, inline_value) = match token.split_once('=') {
                Some((name, value)) if token.starts_with("--") => (name, Some(value.to_string())),
                _ => (token, None),
            };
            let take_value = |index: &mut usize| -> Result<String, String> {
                if let Some(value) = inline_value.clone() {
                    return Ok(value);
                }
                let value = args.get(*index).cloned().ok_or_else(|| {
                    format!("error: a value is required for '{name}' but none was supplied")
                })?;
                *index += 1;
                Ok(value)
            };
            match name {
                "--json" => exec.json = true,
                "--skip-git-repo-check" => exec.skip_git_repo_check = true,
                "--full-auto" => exec.full_auto = true,
                "--dangerously-bypass-approvals-and-sandbox" => {
                    exec.bypass_approvals_and_sandbox = true
                }
                "--ephemeral" => exec.ephemeral = true,
                "--ignore-user-config" => exec.ignore_user_config = true,
                "--ignore-rules" => {}
                "--disable" | "--enable" => {
                    let feature = take_value(&mut index)?;
                    exec.config_overrides.push((
                        format!("features.{feature}"),
                        (name == "--enable").to_string(),
                    ));
                }
                "--oss" => exec.oss = true,
                "--search" => exec.search = true,
                "--last" => exec.resume_last = true,
                "-o" | "--output-last-message" => {
                    exec.output_last_message = Some(take_value(&mut index)?)
                }
                "--output-schema" => exec.output_schema = Some(take_value(&mut index)?),
                "-m" | "--model" => exec.model = Some(take_value(&mut index)?),
                "-p" | "--profile" => exec.profile = Some(take_value(&mut index)?),
                "-s" | "--sandbox" => exec.sandbox = Some(take_value(&mut index)?),
                "-a" | "--ask-for-approval" => {
                    exec.ask_for_approval = Some(take_value(&mut index)?)
                }
                "-C" | "--cd" => exec.cd = Some(take_value(&mut index)?),
                "--color" => exec.color = Some(take_value(&mut index)?),
                "--local-provider" => exec.local_provider = Some(take_value(&mut index)?),
                "--add-dir" => exec.add_dirs.push(take_value(&mut index)?),
                "-c" | "--config" => {
                    let raw = take_value(&mut index)?;
                    let (key, value) = raw.split_once('=').ok_or_else(|| {
                        format!("error: invalid value '{raw}' for '--config <key=value>': expected key=value")
                    })?;
                    exec.config_overrides
                        .push((key.to_string(), value.to_string()));
                }
                "-i" | "--image" => {
                    let raw = take_value(&mut index)?;
                    exec.images.extend(
                        raw.split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(String::from),
                    );
                }
                other => {
                    return Err(format!("error: unexpected argument '{other}' found"));
                }
            }
            continue;
        }
        if subcommand.is_none() {
            subcommand = Some(token.to_string());
        } else {
            positionals.push(token.to_string());
        }
    }
    if help {
        return Ok(CodexInvocation::Help { subcommand });
    }
    match subcommand.as_deref() {
        Some("exec") | Some("e") => {}
        Some(other) => {
            return Err(format!(
                "error: fake-codex only implements 'codex exec'; got subcommand '{other}'"
            ))
        }
        None => {
            return Err(
                "error: fake-codex only implements 'codex exec'; no subcommand given".to_string(),
            )
        }
    }
    let mut rest = positionals.into_iter();
    let first = rest.next();
    if first.as_deref() == Some("resume") {
        if !exec.resume_last {
            let id = rest.next().ok_or_else(|| {
                "error: 'codex exec resume' needs a session id or --last".to_string()
            })?;
            exec.resume = Some(id);
        }
        exec.prompt = rest.next();
    } else {
        exec.prompt = first;
    }
    if let Some(extra) = rest.next() {
        return Err(format!("error: unexpected argument '{extra}' found"));
    }
    if exec.prompt.as_deref() == Some("-") {
        exec.prompt = None;
    }
    Ok(CodexInvocation::Exec(Box::new(exec)))
}

/// Codex `-c key=value` overrides applied on top of `config.toml`, as a map.
pub fn codex_overrides_map(overrides: &[(String, String)]) -> BTreeMap<String, String> {
    overrides
        .iter()
        .map(|(k, v)| (k.clone(), unquote_toml_string(v)))
        .collect()
}

/// `-c approval_policy="never"` carries TOML quoting; strip one layer.
pub fn unquote_toml_string(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_the_design_claude_argv() {
        let parsed = parse_claude(&argv(&[
            "-p",
            "--output-format",
            "stream-json",
            "--input-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--session-id",
            "7b4a0b9e-3b70-4a4e-9a8d-1f3a2c5d6e7f",
            "--permission-mode",
            "default",
            "--permission-prompts",
            "host",
            "--permission-prompt-tool",
            "stdio",
            "--mcp-config",
            "/work/.mcp.json",
            "--strict-mcp-config",
            "--settings",
            "/work/.claude/settings.json",
            "--setting-sources",
            "project",
            "--disallowedTools",
            "AskUserQuestion",
            "--model",
            "claude-sonnet-4-5",
            "--max-turns",
            "40",
            "--append-system-prompt",
            "## Current runtime boundary",
        ]))
        .expect("parses");
        assert!(parsed.has("--print"));
        assert!(parsed.has("--verbose"));
        assert!(parsed.has("--strict-mcp-config"));
        assert_eq!(parsed.value("--output-format"), Some("stream-json"));
        assert_eq!(parsed.value("--input-format"), Some("stream-json"));
        assert_eq!(
            parsed.value("--session-id"),
            Some("7b4a0b9e-3b70-4a4e-9a8d-1f3a2c5d6e7f")
        );
        assert_eq!(
            parsed.values("--mcp-config"),
            &["/work/.mcp.json".to_string()]
        );
        assert_eq!(
            parsed.values("--disallowedTools"),
            &["AskUserQuestion".to_string()]
        );
        assert_eq!(parsed.value("--max-turns"), Some("40"));
        assert_eq!(
            parsed.value("--append-system-prompt"),
            Some("## Current runtime boundary")
        );
        assert!(parsed.positional.is_empty());
    }

    #[test]
    fn variadic_and_optional_value_options_follow_commander() {
        let parsed = parse_claude(&argv(&[
            "--mcp-config",
            "a.json",
            "b.json",
            "--disallowed-tools",
            "Bash(git *)",
            "Edit",
            "--resume",
            "-p",
            "--allowedTools=Read",
            "prompt text",
        ]))
        .unwrap();
        assert_eq!(
            parsed.values("--mcp-config"),
            &["a.json".to_string(), "b.json".to_string()]
        );
        assert_eq!(
            parsed.values("--disallowedTools"),
            &["Bash(git *)".to_string(), "Edit".to_string()]
        );
        assert!(parsed.has("--resume"));
        assert_eq!(parsed.value("--resume"), None);
        assert!(parsed.has("--print"));
        assert_eq!(parsed.values("--allowedTools"), &["Read".to_string()]);
        assert_eq!(parsed.positional, vec!["prompt text".to_string()]);

        let resumed = parse_claude(&argv(&["-r", "abc", "-p"])).unwrap();
        assert_eq!(resumed.value("--resume"), Some("abc"));
    }

    #[test]
    fn rejects_unknown_options_and_missing_values() {
        assert_eq!(
            parse_claude(&argv(&["--bogus"])).unwrap_err(),
            "error: unknown option '--bogus'"
        );
        assert!(parse_claude(&argv(&["--model"])).is_err());
        assert!(parse_claude(&argv(&["--mcp-config", "--verbose"])).is_err());
        assert!(parse_claude(&argv(&["--verbose=1"])).is_err());
    }

    #[test]
    fn double_dash_ends_option_parsing() {
        let parsed = parse_claude(&argv(&["-p", "--", "--not-an-option"])).unwrap();
        assert_eq!(parsed.positional, vec!["--not-an-option".to_string()]);
    }

    #[test]
    fn parses_the_design_codex_argv() {
        let parsed = parse_codex(&argv(&[
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--output-last-message",
            "/tmp/last.txt",
            "--model",
            "gpt-5.5",
            "-c",
            "approval_policy=\"never\"",
            "-c",
            "sandbox_mode=\"workspace-write\"",
            "-c",
            "model_reasoning_effort=\"high\"",
            "-i",
            "a.png,b.png",
            "-i",
            "c.png",
        ]))
        .unwrap();
        let CodexInvocation::Exec(exec) = parsed else {
            panic!("expected exec");
        };
        assert!(exec.json && exec.skip_git_repo_check);
        assert_eq!(exec.output_last_message.as_deref(), Some("/tmp/last.txt"));
        assert_eq!(exec.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(exec.images, vec!["a.png", "b.png", "c.png"]);
        assert_eq!(exec.prompt, None);
        let map = codex_overrides_map(&exec.config_overrides);
        assert_eq!(
            map.get("approval_policy").map(String::as_str),
            Some("never")
        );
        assert_eq!(
            map.get("sandbox_mode").map(String::as_str),
            Some("workspace-write")
        );
        assert_eq!(
            map.get("model_reasoning_effort").map(String::as_str),
            Some("high")
        );
    }

    #[test]
    fn parses_codex_resume_and_prompt_forms() {
        let CodexInvocation::Exec(exec) =
            parse_codex(&argv(&["exec", "resume", "sess-1", "--json", "follow up"])).unwrap()
        else {
            panic!("expected exec");
        };
        assert_eq!(exec.resume.as_deref(), Some("sess-1"));
        assert_eq!(exec.prompt.as_deref(), Some("follow up"));

        let CodexInvocation::Exec(exec) = parse_codex(&argv(&["exec", "-"])).unwrap() else {
            panic!("expected exec");
        };
        assert_eq!(exec.prompt, None);

        let CodexInvocation::Exec(exec) =
            parse_codex(&argv(&["exec", "resume", "--last", "--json"])).unwrap()
        else {
            panic!("expected exec");
        };
        assert!(exec.resume_last);
        assert_eq!(exec.resume, None);
    }

    #[test]
    fn codex_version_help_and_errors() {
        assert_eq!(
            parse_codex(&argv(&["--version"])).unwrap(),
            CodexInvocation::Version
        );
        assert_eq!(
            parse_codex(&argv(&["exec", "--help"])).unwrap(),
            CodexInvocation::Help {
                subcommand: Some("exec".to_string())
            }
        );
        assert!(parse_codex(&argv(&["exec", "--bogus"])).is_err());
        assert!(parse_codex(&argv(&["login"])).is_err());
        assert!(parse_codex(&argv(&["exec", "resume"])).is_err());
        assert!(parse_codex(&argv(&["exec", "-c", "novalue"])).is_err());
    }

    #[test]
    fn toml_quotes_are_stripped_once() {
        assert_eq!(unquote_toml_string("\"never\""), "never");
        assert_eq!(unquote_toml_string("'x'"), "x");
        assert_eq!(unquote_toml_string("plain"), "plain");
        assert_eq!(unquote_toml_string("\""), "\"");
    }
}
