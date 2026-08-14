//! The half of a refusal that belongs to an operator, not to a caller.

/// The physical specifics behind a refusal: the collection, field, or
/// procedure that could not be served.
///
/// # Why this is a type and not a `String`
///
/// [`ConnectorError::Unsupported`](crate::ConnectorError) is the one connector
/// error with two audiences at once — an application reads its capability
/// name, an operator needs the identifiers. The failure this type prevents is
/// a future author formatting the operator's half into the caller's message.
///
/// The lock is an omission: `RefusalDetail` deliberately does **not** implement
/// [`Display`](std::fmt::Display). It therefore cannot appear in a `format!`
/// argument, an `#[error(...)]` template, or a `{}` anywhere — writing one is a
/// compile error rather than something review has to catch. Reaching the text
/// takes an explicit [`as_str`](Self::as_str), which is rare and greppable.
///
/// That is also why the `Unsupported` variant's own `Display` omits this field:
/// rendering *any* `ConnectorError` is safe, so a log line that wants the
/// detail must ask for it by name through
/// [`ConnectorError::operator_message`](crate::ConnectorError::operator_message).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefusalDetail(Option<String>);

impl RefusalDetail {
    /// No physical detail — the capability name is the whole story.
    ///
    /// What the capability gate produces: it refuses on a flag the connector
    /// declared, so there is no identifier to record beyond the operation
    /// itself, which the surrounding log span already carries.
    #[must_use]
    pub const fn none() -> Self {
        Self(None)
    }

    /// Records what an operator needs in order to act on the refusal.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self(Some(detail.into()))
    }

    /// The detail, for a log line, or `None` when there was none to record.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        self.0.as_deref()
    }
}
