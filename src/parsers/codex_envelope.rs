use serde_json::{Map, Value};

/// Shared structural view over one decoded Codex JSONL envelope.
///
/// The bounded JSONL reader owns bytes, JSON decoding, and durable offsets.
/// This type owns only the common Codex envelope fields; accounting and tracer
/// consumers keep their own domain models and state machines.
pub(crate) struct CodexEnvelopeRecord {
    value: Value,
}

impl CodexEnvelopeRecord {
    pub(crate) fn new(value: Value) -> Self {
        Self { value }
    }

    pub(crate) fn kind(&self) -> Option<&str> {
        self.value.get("type").and_then(Value::as_str)
    }

    pub(crate) fn timestamp(&self) -> Option<&str> {
        self.value.get("timestamp").and_then(Value::as_str)
    }

    pub(crate) fn payload(&self) -> Option<&Map<String, Value>> {
        self.value.get("payload").and_then(Value::as_object)
    }

    pub(crate) fn value(&self) -> &Value {
        &self.value
    }
}
