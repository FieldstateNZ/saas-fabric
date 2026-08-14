//! Which audience a failure belongs to.

use crate::ConnectorError;

impl ConnectorError {
    /// Whether the failure is the platform's fault rather than the caller's.
    ///
    /// Used to decide between a 5xx and a 4xx, and to decide what may be shown
    /// to the caller. Internal failures get a generic message; the detail goes
    /// to the log.
    ///
    /// Deliberately *not* the same question as
    /// [`effect`](Self::effect). This one asks whose fault it was; that one
    /// asks whether it happened. The three transport variants share an answer
    /// here and differ there, which is exactly why one flag could never have
    /// carried both.
    #[must_use]
    pub const fn is_internal(&self) -> bool {
        matches!(
            self,
            Self::UnknownConnector(_)
                | Self::SecretUnavailable { .. }
                | Self::Unreachable { .. }
                | Self::OutcomeUnknown { .. }
                | Self::ResultLost { .. }
                | Self::MalformedResponse { .. }
                | Self::Rejected { .. }
        )
    }

    /// This failure as an operator needs to read it, for a log line.
    ///
    /// `Display` is the *safe* rendering: no variant interpolates a
    /// [`RefusalDetail`](crate::RefusalDetail), so text built from it can go to
    /// a caller. This is the unsafe one, and it exists so that the detail a
    /// refusal carries has exactly one way out and that way is named for where
    /// it may go.
    #[must_use]
    pub fn operator_message(&self) -> String {
        match self {
            Self::Unsupported { detail, .. } => match detail.as_str() {
                Some(detail) => format!("{self}: {detail}"),
                None => self.to_string(),
            },
            _ => self.to_string(),
        }
    }
}
