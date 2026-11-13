//! Scenario files: JSON Lines, one step per line, selected with
//! `G5_FAKE_SCENARIO`.
//!
//! A line is either a raw provider frame (an object that carries none of the
//! step keys; it is emitted verbatim after placeholder substitution) or one
//! step object. Every step may carry `delay_ms`, slept before the step runs.
//!
//! Recorded fixtures under `tests/fixtures/provider-frames/` are therefore
//! valid scenarios as they are; hand-written scenarios mix raw frames with
//! steps such as `write_file`, `mcp_call`, `permission` and `exit`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// One executable instruction from a scenario line.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Emit a frame verbatim (after placeholder substitution).
    Emit(Value),
    /// Write a file relative to the working directory.
    WriteFile {
        path: String,
        content: String,
        append: bool,
    },
    /// Call a tool on a configured MCP server and emit the provider's frames
    /// for the call and its result.
    McpCall {
        server: String,
        tool: String,
        arguments: Value,
        id: Option<String>,
        timeout_ms: Option<u64>,
    },
    /// Ask the permission prompt tool (Claude only) whether `tool_name` may run
    /// with `input`; on deny emit the denial and optionally jump to a label.
    Permission {
        tool_name: String,
        input: Value,
        id: Option<String>,
        on_deny: Option<String>,
    },
    /// Jump target for `on_deny`.
    Label(String),
    /// Write a line to stderr.
    Stderr(String),
    /// Stop replaying and exit with `code`.
    Exit { code: i32, stderr: Option<String> },
}

/// A step with its pre-delay.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioStep {
    pub delay_ms: u64,
    pub step: Step,
    /// 1-based line number in the scenario file, for diagnostics.
    pub line: usize,
}

/// A loaded scenario.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub path: PathBuf,
    pub steps: Vec<ScenarioStep>,
}

const STEP_KEYS: &[&str] = &[
    "emit",
    "write_file",
    "mcp_call",
    "permission",
    "label",
    "stderr",
    "exit",
];

/// The scenario file a fake should replay: the provider-specific variable
/// (`G5_FAKE_CLAUDE_SCENARIO` / `G5_FAKE_CODEX_SCENARIO`) wins over the
/// shared `G5_FAKE_SCENARIO`; `None` selects the built-in default.
pub fn scenario_path_from_env(provider_var: &str) -> Option<PathBuf> {
    [provider_var, "G5_FAKE_SCENARIO"]
        .iter()
        .find_map(|var| std::env::var_os(var).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}

impl Scenario {
    /// A scenario built in code (the built-in defaults); `label` stands in for
    /// the file path in diagnostics.
    pub fn from_steps(label: &str, steps: Vec<Step>) -> Self {
        Self {
            path: PathBuf::from(label),
            steps: steps
                .into_iter()
                .enumerate()
                .map(|(index, step)| ScenarioStep {
                    delay_ms: 0,
                    step,
                    line: index + 1,
                })
                .collect(),
        }
    }

    /// Load and parse a scenario file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path)
            .map_err(|err| format!("cannot read scenario {}: {err}", path.display()))?;
        let steps =
            parse_lines(&text).map_err(|err| format!("scenario {}: {err}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            steps,
        })
    }

    /// Index of the step labelled `label`, if any.
    pub fn label_index(&self, label: &str) -> Option<usize> {
        self.steps
            .iter()
            .position(|step| matches!(&step.step, Step::Label(name) if name == label))
    }
}

/// Parse scenario text; blank lines are skipped.
pub fn parse_lines(text: &str) -> Result<Vec<ScenarioStep>, String> {
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        if raw.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(raw)
            .map_err(|err| format!("line {line}: not valid JSON: {err}"))?;
        let step = parse_step(value).map_err(|err| format!("line {line}: {err}"))?;
        steps.push(ScenarioStep {
            delay_ms: step.0,
            step: step.1,
            line,
        });
    }
    Ok(steps)
}

fn parse_step(value: Value) -> Result<(u64, Step), String> {
    let Value::Object(mut object) = value else {
        return Err("each line must be a JSON object".to_string());
    };
    let delay_ms = match object.remove("delay_ms") {
        None | Some(Value::Null) => 0,
        Some(Value::Number(n)) => n
            .as_u64()
            .ok_or_else(|| "delay_ms must be a non-negative integer".to_string())?,
        Some(_) => return Err("delay_ms must be a non-negative integer".to_string()),
    };
    let present: Vec<&str> = STEP_KEYS
        .iter()
        .copied()
        .filter(|key| object.contains_key(*key))
        .collect();
    let step = match present.as_slice() {
        [] => {
            if object.contains_key("type") {
                Step::Emit(Value::Object(object))
            } else {
                return Err(format!(
                    "object is neither a provider frame (needs \"type\") nor a step (one of {})",
                    STEP_KEYS.join(", ")
                ));
            }
        }
        [key] => parse_keyed_step(key, object)?,
        many => {
            return Err(format!(
                "a step must carry exactly one of {}, found {}",
                STEP_KEYS.join(", "),
                many.join(", ")
            ))
        }
    };
    Ok((delay_ms, step))
}

fn parse_keyed_step(key: &str, mut object: Map<String, Value>) -> Result<Step, String> {
    let body = object.remove(key).expect("key presence checked");
    if let Some(extra) = object.keys().next() {
        return Err(format!("unexpected key {extra:?} next to {key:?}"));
    }
    match key {
        "emit" => match body {
            Value::Object(_) => Ok(Step::Emit(body)),
            _ => Err("emit must be a JSON object".to_string()),
        },
        "write_file" => {
            let body = expect_object(&body, "write_file")?;
            let path = required_string(body, "write_file", "path")?;
            validate_relative_path(&path)?;
            let content = required_string(body, "write_file", "content")?;
            let append = body
                .get("append")
                .map(|v| v.as_bool().ok_or("write_file.append must be a boolean"))
                .transpose()?
                .unwrap_or(false);
            Ok(Step::WriteFile {
                path,
                content,
                append,
            })
        }
        "mcp_call" => {
            let body = expect_object(&body, "mcp_call")?;
            let server = optional_string(body, "mcp_call", "server")?
                .unwrap_or_else(|| "g5-tools".to_string());
            let tool = required_string(body, "mcp_call", "tool")?;
            let arguments = match body.get("arguments") {
                None => Value::Object(Map::new()),
                Some(v @ Value::Object(_)) => v.clone(),
                Some(_) => return Err("mcp_call.arguments must be a JSON object".to_string()),
            };
            let id = optional_string(body, "mcp_call", "id")?;
            let timeout_ms = optional_u64(body, "mcp_call", "timeout_ms")?;
            Ok(Step::McpCall {
                server,
                tool,
                arguments,
                id,
                timeout_ms,
            })
        }
        "permission" => {
            let body = expect_object(&body, "permission")?;
            let tool_name = required_string(body, "permission", "tool_name")?;
            let input = match body.get("input") {
                None => Value::Object(Map::new()),
                Some(v @ Value::Object(_)) => v.clone(),
                Some(_) => return Err("permission.input must be a JSON object".to_string()),
            };
            let id = optional_string(body, "permission", "id")?;
            let on_deny = optional_string(body, "permission", "on_deny")?;
            Ok(Step::Permission {
                tool_name,
                input,
                id,
                on_deny,
            })
        }
        "label" => match body {
            Value::String(name) if !name.is_empty() => Ok(Step::Label(name)),
            _ => Err("label must be a non-empty string".to_string()),
        },
        "stderr" => match body {
            Value::String(text) => Ok(Step::Stderr(text)),
            _ => Err("stderr must be a string".to_string()),
        },
        "exit" => match body {
            Value::Number(n) => Ok(Step::Exit {
                code: exit_code(&Value::Number(n))?,
                stderr: None,
            }),
            Value::Object(ref map) => {
                let code = map.get("code").map(exit_code).transpose()?.unwrap_or(0);
                let stderr = optional_string(map, "exit", "stderr")?;
                Ok(Step::Exit { code, stderr })
            }
            _ => Err("exit must be a number or {\"code\": n, \"stderr\": \"...\"}".to_string()),
        },
        other => Err(format!("unknown step {other:?}")),
    }
}

fn exit_code(value: &Value) -> Result<i32, String> {
    value
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .filter(|n| (0..=255).contains(n))
        .ok_or_else(|| "exit code must be an integer between 0 and 255".to_string())
}

fn expect_object<'a>(value: &'a Value, step: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{step} must be a JSON object"))
}

fn required_string(map: &Map<String, Value>, step: &str, key: &str) -> Result<String, String> {
    optional_string(map, step, key)?.ok_or_else(|| format!("{step}.{key} is required"))
}

fn optional_string(
    map: &Map<String, Value>,
    step: &str,
    key: &str,
) -> Result<Option<String>, String> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("{step}.{key} must be a string")),
    }
}

fn optional_u64(map: &Map<String, Value>, step: &str, key: &str) -> Result<Option<u64>, String> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("{step}.{key} must be a non-negative integer")),
        Some(_) => Err(format!("{step}.{key} must be a non-negative integer")),
    }
}

/// Reject paths that could escape the working directory.
pub fn validate_relative_path(path: &str) -> Result<(), String> {
    let candidate = Path::new(path);
    if path.is_empty() {
        return Err("write_file.path must not be empty".to_string());
    }
    if candidate.is_absolute() {
        return Err(format!(
            "write_file.path must be relative to cwd, got {path:?}"
        ));
    }
    if candidate
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!(
            "write_file.path must not contain '..', got {path:?}"
        ));
    }
    Ok(())
}

/// Values substituted into `{{placeholder}}` tokens inside scenario strings.
///
/// Supported placeholders: `session_id`, `cwd`, `prompt`, `model`,
/// `timestamp` (RFC 3339 now), `uuid` (fresh v4 on every use) and
/// `uuid:<name>` (stable within one run). Unknown placeholders are left as
/// written.
pub struct Placeholders {
    pub session_id: String,
    pub cwd: String,
    pub prompt: String,
    pub model: String,
    named_uuids: RefCell<BTreeMap<String, String>>,
}

impl Placeholders {
    pub fn new(session_id: String, cwd: String, prompt: String, model: String) -> Self {
        Self {
            session_id,
            cwd,
            prompt,
            model,
            named_uuids: RefCell::new(BTreeMap::new()),
        }
    }

    /// The stable UUID for `name`, minted on first use.
    pub fn named_uuid(&self, name: &str) -> String {
        self.named_uuids
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| uuid::Uuid::new_v4().to_string())
            .clone()
    }

    /// Resolve one placeholder name (without braces).
    fn resolve(&self, name: &str) -> Option<String> {
        match name {
            "session_id" => Some(self.session_id.clone()),
            "cwd" => Some(self.cwd.clone()),
            "prompt" => Some(self.prompt.clone()),
            "model" => Some(self.model.clone()),
            "timestamp" => Some(rfc3339_now()),
            "uuid" => Some(uuid::Uuid::new_v4().to_string()),
            other => other
                .strip_prefix("uuid:")
                .filter(|slot| !slot.is_empty())
                .map(|slot| self.named_uuid(slot)),
        }
    }

    /// Substitute placeholders in a string.
    pub fn substitute_str(&self, input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            match after.find("}}") {
                Some(end) => {
                    let name = after[..end].trim();
                    match self.resolve(name) {
                        Some(value) => out.push_str(&value),
                        None => {
                            out.push_str("{{");
                            out.push_str(&after[..end]);
                            out.push_str("}}");
                        }
                    }
                    rest = &after[end + 2..];
                }
                None => {
                    out.push_str("{{");
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Substitute placeholders in every string inside a JSON value.
    pub fn substitute(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.substitute_str(s)),
            Value::Array(items) => Value::Array(items.iter().map(|v| self.substitute(v)).collect()),
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), self.substitute(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

/// Current time as an RFC 3339 UTC timestamp with millisecond precision.
pub fn rfc3339_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let millis = now.subsec_millis();
    let days = secs.div_euclid(86_400);
    let day_secs = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        day_secs / 3600,
        (day_secs % 3600) / 60,
        day_secs % 60
    )
}

/// Convert days since 1970-01-01 to a proleptic Gregorian civil date.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn placeholders() -> Placeholders {
        Placeholders::new(
            "sess-1".to_string(),
            "/work".to_string(),
            "hello".to_string(),
            "model-x".to_string(),
        )
    }

    #[test]
    fn raw_frames_and_steps_parse() {
        let text = r#"{"type":"system","subtype":"init","session_id":"{{session_id}}"}
{"delay_ms": 5, "write_file": {"path": "a.txt", "content": "x"}}

{"mcp_call": {"tool": "echo", "arguments": {"text": "q"}}}
{"permission": {"tool_name": "Write", "input": {"file_path": "a"}, "on_deny": "end"}}
{"label": "end"}
{"stderr": "diag"}
{"exit": {"code": 3, "stderr": "boom"}}
"#;
        let steps = parse_lines(text).expect("parses");
        assert_eq!(steps.len(), 7);
        assert!(matches!(steps[0].step, Step::Emit(_)));
        assert_eq!(steps[0].line, 1);
        assert_eq!(steps[1].delay_ms, 5);
        assert!(
            matches!(&steps[1].step, Step::WriteFile { path, content, append } if path == "a.txt" && content == "x" && !append)
        );
        assert!(
            matches!(&steps[2].step, Step::McpCall { server, tool, .. } if server == "g5-tools" && tool == "echo")
        );
        assert_eq!(steps[3].line, 5);
        assert!(matches!(&steps[4].step, Step::Label(name) if name == "end"));
        assert!(matches!(&steps[5].step, Step::Stderr(text) if text == "diag"));
        assert!(matches!(
            &steps[6].step,
            Step::Exit {
                code: 3,
                stderr: Some(_)
            }
        ));
    }

    #[test]
    fn exit_accepts_a_bare_number() {
        let steps = parse_lines(r#"{"exit": 2}"#).unwrap();
        assert!(matches!(
            steps[0].step,
            Step::Exit {
                code: 2,
                stderr: None
            }
        ));
    }

    #[test]
    fn rejects_ambiguous_and_malformed_lines() {
        assert!(parse_lines(r#"{"emit": {}, "exit": 1}"#).is_err());
        assert!(parse_lines(r#"{"foo": 1}"#).is_err());
        assert!(parse_lines(r#"[1,2]"#).is_err());
        assert!(parse_lines(r#"{"write_file": {"path": "../x", "content": ""}}"#).is_err());
        assert!(parse_lines(r#"{"write_file": {"path": "/abs", "content": ""}}"#).is_err());
        assert!(parse_lines(r#"{"delay_ms": -1, "exit": 0}"#).is_err());
        assert!(parse_lines(r#"{"exit": 300}"#).is_err());
        assert!(parse_lines("not json").is_err());
    }

    #[test]
    fn placeholders_substitute_recursively() {
        let p = placeholders();
        let value = json!({
            "session_id": "{{session_id}}",
            "nested": [{"text": "cwd={{cwd}} prompt={{ prompt }}"}],
            "model": "{{model}}",
            "unknown": "{{nope}} and {{",
            "num": 1
        });
        let out = p.substitute(&value);
        assert_eq!(out["session_id"], "sess-1");
        assert_eq!(out["nested"][0]["text"], "cwd=/work prompt=hello");
        assert_eq!(out["model"], "model-x");
        assert_eq!(out["unknown"], "{{nope}} and {{");
        assert_eq!(out["num"], 1);
    }

    #[test]
    fn named_uuids_are_stable_and_fresh_uuids_differ() {
        let p = placeholders();
        let a = p.substitute_str("{{uuid:t1}}");
        let b = p.substitute_str("{{uuid:t1}}");
        let c = p.substitute_str("{{uuid:t2}}");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(uuid::Uuid::parse_str(&a).is_ok());
        assert_ne!(p.substitute_str("{{uuid}}"), p.substitute_str("{{uuid}}"));
    }

    #[test]
    fn timestamps_are_rfc3339() {
        let ts = rfc3339_now();
        assert_eq!(ts.len(), 24, "{ts}");
        assert!(ts.ends_with('Z'));
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }
}
