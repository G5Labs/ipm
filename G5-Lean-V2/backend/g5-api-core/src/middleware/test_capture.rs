//! Test-only tracing capture support.
//!
//! Collects emitted `tracing` events so unit tests can assert on the
//! `g5_audit` audit stream (`system_invariants/observability_audit` [D5]/[D6]).
//! Nothing in this repo previously asserted on emitted tracing events; this
//! helper is shared by the audit middleware tests and the telemetry route
//! tests. Compiled only under `cfg(test)`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context as LayerContext, Layer, Layered};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::{LookupSpan, Registry};

/// One captured tracing event: its target plus all fields rendered to
/// strings (Display-recorded fields come through verbatim; numeric fields
/// via `to_string`).
#[derive(Debug, Clone)]
pub struct CapturedEvent {
    pub target: String,
    /// Rendered `tracing::Level` (e.g. "INFO") — audit events must stay
    /// INFO for every outcome class.
    pub level: String,
    pub fields: BTreeMap<String, String>,
}

impl CapturedEvent {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }
}

pub type Events = Arc<Mutex<Vec<CapturedEvent>>>;

struct FieldVisitor(BTreeMap<String, String>);

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

pub struct CaptureLayer {
    events: Events,
}

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: LayerContext<'_, S>) {
        let mut visitor = FieldVisitor(BTreeMap::new());
        event.record(&mut visitor);
        self.events.lock().unwrap().push(CapturedEvent {
            target: event.metadata().target().to_string(),
            level: event.metadata().level().to_string(),
            fields: visitor.0,
        });
    }
}

/// Builds a subscriber that records every event into the returned buffer.
/// Use with `tracing::instrument::WithSubscriber::with_subscriber` around
/// the future under test. The concrete type lets a caller stack further
/// layers (the g5-api integration tests add a formatting layer) before
/// installing it as the process-wide subscriber.
pub fn capture_subscriber() -> (Layered<CaptureLayer, Registry>, Events) {
    let (layer, events) = capture_layer();
    (Registry::default().with(layer), events)
}

/// The capture layer alone, for callers that compose their own subscriber,
/// for example to filter which events are recorded (`Layer::with_filter`).
pub fn capture_layer() -> (CaptureLayer, Events) {
    let events: Events = Arc::new(Mutex::new(Vec::new()));
    let layer = CaptureLayer {
        events: events.clone(),
    };
    (layer, events)
}

/// Returns the captured events emitted on the `g5_audit` target.
pub fn captured_g5_audit_events(events: &Events) -> Vec<CapturedEvent> {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|e| e.target == "g5_audit")
        .cloned()
        .collect()
}
