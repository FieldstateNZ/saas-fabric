//! What one audit event says, before it is sealed into a chain.

use std::collections::BTreeMap;

use crate::audit_trail::AuditStream;

/// One operator decision, as the audit trail stores it.
///
/// Built by the functions in `crate::audit`, from the same values each
/// writes to its log line, so the durable record and the log pipeline cannot
/// disagree about what happened.
///
/// # Why `fields` is a sorted map of strings
///
/// Events differ in what they carry — a revision, a host, a digest, a
/// refusal code — and a sorted map of strings serialises the same way every
/// time, which the hash chain depends on. Every value put here is already a
/// validated type rendered as text; no secret, token or credential has a path
/// into any audit function (see `crate::audit`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRecord {
    stream: AuditStream,
    event: String,
    operation: String,
    requested_by: String,
    fields: BTreeMap<String, String>,
}

impl AuditRecord {
    /// Starts a record of `event` in `stream`.
    #[must_use]
    pub fn new(stream: AuditStream, event: &str, operation: &str, requested_by: &str) -> Self {
        Self {
            stream,
            event: event.to_owned(),
            operation: operation.to_owned(),
            requested_by: requested_by.to_owned(),
            fields: BTreeMap::new(),
        }
    }

    /// Adds one field. An empty value is omitted, as its log line's would be
    /// read: "not applicable", not "applicable and blank".
    #[must_use]
    pub fn with(mut self, name: &str, value: impl std::fmt::Display) -> Self {
        let value = value.to_string();
        if !value.is_empty() {
            self.fields.insert(name.to_owned(), value);
        }
        self
    }

    /// The chain this record belongs to.
    #[must_use]
    pub fn stream(&self) -> &AuditStream {
        &self.stream
    }

    /// The event name, the same `event` its log line carries.
    #[must_use]
    pub fn event(&self) -> &str {
        &self.event
    }

    /// The domain operation.
    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }

    /// The operator subject who asked.
    #[must_use]
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }

    /// One field's value, if the record carries it.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }
}
