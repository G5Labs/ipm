//! Which assistant segment of an attempt owns each journaled unit
//! (`g5.chat.replay`, `g5.chat.live-delivery`).
//!
//! An attempt's reply is one or more assistant segments: the canonical
//! message of the attempt and, after each flushed steering input, the
//! steered-reply message derived from that input's reservation. A text,
//! thought or tool unit stays in the segment where it first appeared even if
//! input is flushed midway; only new units follow the new input. Both ids are
//! deterministic, so the live relay derives the owner of every event without
//! a database read, and the history batch assigns the same owner.

use std::collections::HashMap;

use serde_json::Value;
use uuid::Uuid;

use super::task_lifecycle::canonical_agent_message_id;

/// The payload field that identifies a unit of this kind, and the kinds that
/// share that identity space.
pub(crate) fn unit_binding(kind: &str) -> Option<(&'static str, &'static [&'static str])> {
    match kind {
        "message.delta" | "message.final" | "message.retracted" => Some((
            "block_id",
            &["message.delta", "message.final", "message.retracted"],
        )),
        "thought.delta" | "thought.final" => {
            Some(("block_id", &["thought.delta", "thought.final"]))
        }
        "tool.started" | "tool.progress" | "tool.completed" => Some((
            "call_id",
            &["tool.started", "tool.progress", "tool.completed"],
        )),
        _ => None,
    }
}

/// The message a flushed steering input's reply segment uses
/// (`g5.conversation.delivery.steering`).
#[must_use]
pub fn steered_reply_message_id(reservation_id: Uuid) -> Uuid {
    Uuid::new_v5(&reservation_id, b"agent-steered-reply")
}

#[derive(Debug, Clone)]
pub struct SegmentMap {
    task_id: Uuid,
    attempt: i32,
    current: Uuid,
    /// `(identity space, unit id)` → owning message.
    owners: HashMap<(&'static str, String), Uuid>,
}

impl SegmentMap {
    /// A map that starts at the attempt's canonical segment.
    #[must_use]
    pub fn new(task_id: Uuid, attempt: i32) -> Self {
        Self {
            task_id,
            attempt,
            current: canonical_agent_message_id(task_id, attempt),
            owners: HashMap::new(),
        }
    }

    /// A map whose current segment is already known (seeded from persisted
    /// rows after a restart).
    #[must_use]
    pub fn with_current(task_id: Uuid, attempt: i32, current: Uuid) -> Self {
        let mut map = Self::new(task_id, attempt);
        map.current = current;
        map
    }

    #[must_use]
    pub fn current(&self) -> Uuid {
        self.current
    }

    #[must_use]
    pub fn task_id(&self) -> Uuid {
        self.task_id
    }

    #[must_use]
    pub fn attempt(&self) -> i32 {
        self.attempt
    }

    /// Record a persisted ownership fact (a unit already stored under a
    /// message before this map existed).
    pub fn learn(&mut self, kind: &str, unit: &str, owner: Uuid) {
        if let Some((field, _)) = unit_binding(kind) {
            self.owners
                .entry((field, unit.to_string()))
                .or_insert(owner);
        }
    }

    /// The unit id of an event, if its kind has one.
    #[must_use]
    pub fn unit_of(kind: &str, payload: &Value) -> Option<(&'static str, String)> {
        let (field, _) = unit_binding(kind)?;
        payload[field]
            .as_str()
            .filter(|unit| !unit.is_empty())
            .map(|unit| (field, unit.to_string()))
    }

    /// The owner of a unit already seen, without assigning one.
    #[must_use]
    pub fn known_owner(&self, kind: &str, payload: &Value) -> Option<Uuid> {
        let key = Self::unit_of(kind, payload)?;
        self.owners.get(&key).copied()
    }

    /// Assign the event to its owning segment: the segment that first saw
    /// its unit, otherwise the current one, which a new unit then joins.
    pub fn assign(&mut self, kind: &str, payload: &Value) -> Uuid {
        match Self::unit_of(kind, payload) {
            Some(key) => *self.owners.entry(key).or_insert(self.current),
            None => self.current,
        }
    }

    /// A flushed steering input starts the next segment; later new units
    /// belong to it.
    pub fn observe_shipped(&mut self, payload: &Value) {
        if let Some(reservation) = payload["reservation_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
        {
            self.current = steered_reply_message_id(reservation);
        }
    }

    /// Replace the current segment with the one persisted rows report.
    pub fn set_current(&mut self, current: Uuid) {
        self.current = current;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn units_stay_in_their_first_segment_and_new_units_follow_input() {
        let task = Uuid::from_u128(7);
        let mut map = SegmentMap::new(task, 1);
        let first = map.current();
        assert_eq!(first, canonical_agent_message_id(task, 1));
        let delta = json!({"block_id": "b1", "text_delta": "hi"});
        assert_eq!(map.assign("message.delta", &delta), first);
        let reservation = Uuid::from_u128(9);
        map.observe_shipped(&json!({"reservation_id": reservation.to_string()}));
        let second = steered_reply_message_id(reservation);
        assert_eq!(map.current(), second);
        // The started block completes in its original segment.
        assert_eq!(
            map.assign(
                "message.final",
                &json!({"block_id": "b1", "content": "hi!"})
            ),
            first
        );
        // A tool started before the flush stays; a new block moves.
        assert_eq!(
            map.assign(
                "message.delta",
                &json!({"block_id": "b2", "text_delta": "x"})
            ),
            second
        );
        assert_eq!(
            map.assign("tool.started", &json!({"call_id": "c1"})),
            second
        );
        assert_eq!(map.assign("notice", &json!({"message": "m"})), second);
    }

    #[test]
    fn learned_persisted_owners_win_over_the_current_segment() {
        let task = Uuid::from_u128(1);
        let other = Uuid::from_u128(2);
        let mut map = SegmentMap::with_current(task, 3, other);
        let owner = Uuid::from_u128(5);
        map.learn("tool.started", "call-9", owner);
        assert_eq!(
            map.known_owner("tool.completed", &json!({"call_id": "call-9"})),
            Some(owner)
        );
        assert_eq!(
            map.assign("tool.completed", &json!({"call_id": "call-9"})),
            owner
        );
        assert_eq!(
            map.assign("tool.started", &json!({"call_id": "call-10"})),
            other
        );
    }
}
