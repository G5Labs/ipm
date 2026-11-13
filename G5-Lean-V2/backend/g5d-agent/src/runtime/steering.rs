//! Immutable input bindings for one live Claude process. Completed stdin
//! write/flush confirms delivery; Claude owns when it consumes the input.
use super::*;

pub(super) const OPTION: &str = "native_mid_turn_steering";
pub(super) const MAX_CONTENT_BYTES: usize = 512 * 1024;

#[derive(Deserialize)]
struct Request {
    request_id: String,
    input_id: uuid::Uuid,
    reservation_id: uuid::Uuid,
    revision: u32,
    delivery_generation: u32,
    content: String,
}

struct Entry {
    payload: Value,
    digest: Vec<u8>,
    disposition: &'static str,
}

#[derive(Default)]
pub(super) struct Steering {
    pub enabled: bool,
    entries: HashMap<String, Entry>,
}

pub(super) enum Admission {
    Write { request_id: String, frame: Value },
    Reply { kind: &'static str, payload: Value },
}

fn digest(content: &Value) -> Vec<u8> {
    Sha256::digest(serde_json::to_vec(content).expect("JSON value")).to_vec()
}

pub(super) fn enabled(input: &AgentTurnInputV1) -> bool {
    input.provider.provider_id == ProviderId::ClaudeCode
        && input.provider.options.get(OPTION).and_then(Value::as_bool) == Some(true)
}

impl Steering {
    pub fn new(input: &AgentTurnInputV1) -> Self {
        Self {
            enabled: enabled(input),
            ..Self::default()
        }
    }

    pub fn admit(&mut self, line: &str, eligible: bool) -> Result<Admission, G5Error> {
        let request: Request =
            serde_json::from_str(line).map_err(|_| internal("invalid steering control"))?;
        let content = json!(request.content);
        let hash = digest(&content);
        let payload = json!({"request_id":request.request_id,"input_id":request.input_id,
            "reservation_id":request.reservation_id,"revision":request.revision,
            "delivery_generation":request.delivery_generation});
        if let Some(previous) = self.entries.get(&request.request_id) {
            if previous.payload != payload || previous.digest != hash {
                return Err(internal("conflicting steering binding"));
            }
            return Ok(Admission::Reply {
                kind: match previous.disposition {
                    "shipped" => "input.shipped",
                    "not_steerable" => "input.steer_rejected",
                    _ => "input.steer_pending",
                },
                payload,
            });
        }
        let valid = request.request_id
            == format!("{}:{}", request.input_id, request.delivery_generation)
            && request.revision > 0
            && request.delivery_generation > 0
            && request.content.len() <= MAX_CONTENT_BYTES;
        let accepted = valid && eligible && self.enabled;
        self.entries.insert(
            request.request_id.clone(),
            Entry {
                payload: payload.clone(),
                digest: hash,
                disposition: if accepted { "pending" } else { "not_steerable" },
            },
        );
        if !accepted {
            return Ok(Admission::Reply {
                kind: "input.steer_rejected",
                payload,
            });
        }
        Ok(Admission::Write {
            request_id: request.request_id,
            frame: json!({"type":"user",
                "message":{"role":"user","content":content}}),
        })
    }

    pub fn shipped(&mut self, request_id: &str) -> Option<Value> {
        let entry = self.entries.get_mut(request_id)?;
        entry.disposition = "shipped";
        Some(entry.payload.clone())
    }

    /// Input this process may still read beyond its current turn: a steering
    /// message queued for or already flushed to Claude's stdin. Such a process
    /// must not defer a question to its saved session
    /// (`g5.chat.native-question-recovery`).
    pub fn admitted_any(&self) -> bool {
        self.entries
            .values()
            .any(|entry| matches!(entry.disposition, "pending" | "shipped"))
    }
}
