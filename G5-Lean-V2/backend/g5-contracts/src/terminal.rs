//! The browser terminal protocol on a workspace PTY stream: newline-delimited
//! JSON control messages, then raw PTY output. The API and the guest
//! dispatcher share these types; the browser speaks the same JSON, which the
//! tests below pin.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const MAX_TERMINAL_ID_BYTES: usize = 128;

/// The longest `open` line either end accepts: the shared secret budget plus
/// fixed headroom for the other fields, so a policy-valid secret set always
/// fits (`g5.terminal.secret-admission`).
pub const TERMINAL_OPEN_FRAME_MAX_BYTES: usize =
    crate::types::task_env::MAX_TASK_ENV_SERIALIZED_BYTES + 4 * 1024;

/// A message toward the guest. The API sends `open` once when the stream
/// opens. `input`, `resize` and `close` come from the browser, or `input` from
/// the API for a binary WebSocket frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalClientMessage {
    Open {
        cols: u16,
        rows: u16,
        #[serde(default)]
        terminal_id: Option<String>,
        /// Omitted when absent: dispatchers without replay offsets reject
        /// unknown `open` fields.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        offset: Option<u64>,
        #[serde(default)]
        secrets: HashMap<String, String>,
        #[cfg(feature = "terminal-test-hooks")]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        test_hook: Option<TerminalTestHook>,
    },
    Input {
        data: String,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    Close,
}

impl TerminalClientMessage {
    /// The `open` message.
    #[must_use]
    pub fn open(
        cols: u16,
        rows: u16,
        terminal_id: Option<String>,
        offset: Option<u64>,
        secrets: HashMap<String, String>,
    ) -> Self {
        Self::Open {
            cols,
            rows,
            terminal_id,
            offset,
            secrets,
            #[cfg(feature = "terminal-test-hooks")]
            test_hook: None,
        }
    }

    /// The message as one JSON line.
    pub fn to_line(&self) -> serde_json::Result<Vec<u8>> {
        json_line(self)
    }
}

/// Dispatcher test fixtures carried on `open`; never built into production.
#[cfg(feature = "terminal-test-hooks")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalTestHook {
    LateAttachment,
    FailCloseOnce,
}

/// The guest's control line before any PTY output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalServerMessage {
    /// The PTY is attached; its output from `offset` follows.
    Opened {
        terminal_id: String,
        reattached: bool,
        offset: u64,
        reset: bool,
    },
    /// The terminal cannot be attached; the stream then closes. `detail` is
    /// the owner-facing sentence for a refusal the owner can correct.
    Rejected {
        reason: TerminalRejection,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// Sent after `opened` and any output, as the stream's last frame, when
    /// the guest closes it for a reason the browser must act on. Only hosted
    /// (read-only) sessions send it: a shell stream still closes silently and
    /// the browser learns of its end on the next `open`. It is written as one
    /// mux frame on its own so the relay delivers it as one message.
    Closed { reason: TerminalClose },
}

impl TerminalServerMessage {
    /// The message as one JSON line.
    pub fn to_line(&self) -> serde_json::Result<Vec<u8>> {
        json_line(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalRejection {
    TerminalUnavailable,
    /// The owner's effective Runtime Config secrets violate the shared
    /// exportable-environment policy; `detail` names the key and the limit.
    SecretsInvalid,
}

/// Why the guest ended an attached stream itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalClose {
    /// The hosted session's output is complete: its last writer closed the
    /// terminal and every retained byte was delivered. The session stays
    /// attachable for replay, but the viewer should stop reconnecting.
    SessionEnded,
    /// A newer attach replaced this viewer (one viewer at a time,
    /// `g5.preview.output.live-tail.viewer`). The session goes on without it;
    /// the viewer should stop reconnecting, since each reconnect would
    /// supersede the newer viewer in turn, and may reattach at its offset
    /// only on the owner's request.
    Superseded,
}

/// The namespace of hosted (read-only, dispatcher-owned) terminal sessions.
/// An interactive `open` can never create an identity in it; attaching to
/// one requires that the dispatcher already hosts it.
pub const HOSTED_TERMINAL_PREFIX: &str = "preview:";

/// The hosted session identity of a preview task's application output.
#[must_use]
pub fn preview_terminal_id(task_id: &str) -> String {
    format!("{HOSTED_TERMINAL_PREFIX}{task_id}")
}

fn json_line(message: &impl Serialize) -> serde_json::Result<Vec<u8>> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn line_value(line: &[u8]) -> Value {
        assert_eq!(line.last(), Some(&b'\n'), "one JSON line");
        serde_json::from_slice(line).expect("the line is JSON")
    }

    /// The JSON both ends and the browser exchanged before these types:
    /// `open` without an offset carries no `offset` key, the browser's own
    /// messages parse, unknown `open` fields are still refused, and the
    /// guest's control lines keep their shape.
    #[test]
    fn terminal_messages_keep_their_json() {
        let secrets = HashMap::from([("TOKEN".to_string(), "x".to_string())]);
        assert_eq!(
            line_value(
                &TerminalClientMessage::open(80, 24, None, None, secrets.clone())
                    .to_line()
                    .unwrap()
            ),
            json!({"type":"open","cols":80,"rows":24,"terminal_id":null,"secrets":{"TOKEN":"x"}})
        );
        assert_eq!(
            line_value(
                &TerminalClientMessage::open(120, 40, Some("t1".into()), Some(42), HashMap::new())
                    .to_line()
                    .unwrap()
            ),
            json!({"type":"open","cols":120,"rows":40,"terminal_id":"t1","offset":42,"secrets":{}})
        );
        assert_eq!(
            serde_json::to_value(TerminalClientMessage::Input {
                data: "ls\n".into()
            })
            .unwrap(),
            json!({"type":"input","data":"ls\n"})
        );

        for (browser, message) in [
            (
                r#"{"type":"input","data":"ls\n"}"#,
                TerminalClientMessage::Input {
                    data: "ls\n".into(),
                },
            ),
            (
                r#"{"type":"resize","cols":100,"rows":30}"#,
                TerminalClientMessage::Resize {
                    cols: 100,
                    rows: 30,
                },
            ),
            (r#"{"type":"close"}"#, TerminalClientMessage::Close),
            (
                r#"{"type":"open","cols":80,"rows":24,"secrets":{}}"#,
                TerminalClientMessage::open(80, 24, None, None, HashMap::new()),
            ),
        ] {
            assert_eq!(
                serde_json::from_str::<TerminalClientMessage>(browser).unwrap(),
                message,
                "{browser}"
            );
        }
        assert!(serde_json::from_str::<TerminalClientMessage>(
            r#"{"type":"open","cols":80,"rows":24,"secrets":{},"later":1}"#
        )
        .is_err());

        assert_eq!(
            line_value(
                &TerminalServerMessage::Opened {
                    terminal_id: "t1".into(),
                    reattached: true,
                    offset: 7,
                    reset: false,
                }
                .to_line()
                .unwrap()
            ),
            json!({"type":"opened","terminal_id":"t1","reattached":true,"offset":7,"reset":false})
        );
        assert_eq!(
            line_value(
                &TerminalServerMessage::Rejected {
                    reason: TerminalRejection::TerminalUnavailable,
                    detail: None,
                }
                .to_line()
                .unwrap()
            ),
            json!({"type":"rejected","reason":"terminal_unavailable"})
        );
        // A correctable refusal carries the owner-facing sentence; older
        // browsers ignore the field and older guests never emit it.
        assert_eq!(
            line_value(
                &TerminalServerMessage::Rejected {
                    reason: TerminalRejection::SecretsInvalid,
                    detail: Some("Runtime Config secret `x` is reserved".into()),
                }
                .to_line()
                .unwrap()
            ),
            json!({"type":"rejected","reason":"secrets_invalid","detail":"Runtime Config secret `x` is reserved"})
        );
        assert_eq!(
            serde_json::from_str::<TerminalServerMessage>(
                r#"{"type":"rejected","reason":"terminal_unavailable"}"#
            )
            .unwrap(),
            TerminalServerMessage::Rejected {
                reason: TerminalRejection::TerminalUnavailable,
                detail: None,
            }
        );
        assert_eq!(
            line_value(
                &TerminalServerMessage::Closed {
                    reason: TerminalClose::SessionEnded,
                }
                .to_line()
                .unwrap()
            ),
            json!({"type":"closed","reason":"session_ended"})
        );
        assert_eq!(
            line_value(
                &TerminalServerMessage::Closed {
                    reason: TerminalClose::Superseded,
                }
                .to_line()
                .unwrap()
            ),
            json!({"type":"closed","reason":"superseded"})
        );
        assert_eq!(
            preview_terminal_id("0192c7e4-1111-7000-8000-000000000001"),
            "preview:0192c7e4-1111-7000-8000-000000000001"
        );
    }
}
