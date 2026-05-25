use std::fmt::Write as _;
use std::time::SystemTime;

use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

use crate::{ControlPlane, Event};

/// `tracing-subscriber` layer that forwards every event into a [`ControlPlane`]
/// broadcast.
///
/// Subsystem is derived from the event's `target` (the module path Cargo
/// generates from the crate name) by taking the first segment and replacing
/// underscores with dashes — `linear_bridge::sources::handler` ->
/// `linear-bridge`. Targets without a crate prefix get `None`.
pub struct ControlLayer {
    plane: ControlPlane,
}

impl ControlLayer {
    pub fn new(plane: ControlPlane) -> Self {
        Self { plane }
    }
}

impl<S> Layer<S> for ControlLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let meta = event.metadata();
        let target = meta.target().to_string();
        let subsystem = target
            .split("::")
            .next()
            .filter(|s| !s.is_empty())
            .map(|s| s.replace('_', "-"));

        let event = Event {
            id: self.plane.next_event_id(),
            level: (*meta.level()).into(),
            subsystem,
            target,
            message: visitor.message,
            fields: visitor.fields,
            timestamp: SystemTime::now(),
        };
        self.plane.emit(event);
    }
}

#[derive(Default)]
struct EventVisitor {
    message: String,
    fields: serde_json::Map<String, serde_json::Value>,
}

impl Visit for EventVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields
                .insert(field.name().into(), serde_json::Value::String(value.into()));
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().into(), serde_json::Value::Bool(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name().into(), value.into());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name().into(), value.into());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.insert(field.name().into(), value.into());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message.clear();
            let _ = write!(&mut self.message, "{value:?}");
        } else {
            self.fields.insert(
                field.name().into(),
                serde_json::Value::String(format!("{value:?}")),
            );
        }
    }
}
