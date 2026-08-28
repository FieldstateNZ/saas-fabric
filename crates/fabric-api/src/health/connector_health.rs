//! What one connector's health check produced, and which connector produced
//! it.

/// One connector's id and what its check produced.
pub(super) struct ConnectorOutcome {
    /// The connector's registered id. Physical infrastructure identity, so it
    /// reaches an authorised caller only (§2, §29).
    pub(super) id: String,

    /// What the check said, or that it did not say anything in time.
    pub(super) health: ConnectorHealth,
}

/// The outcome of checking one connector.
///
/// # Why three states and not a bool
///
/// The sweep gives up at a deadline (see
/// [`connector_sweep`](crate::health::connector_sweep)), so "this check had
/// not answered yet" is a real outcome and it is **not** the same as "this
/// connector answered that it is broken".
///
/// Folding the two together would invert the policy the whole readiness
/// decision exists to protect. Configuration is identical across replicas, so
/// one slow backend is slow for all of them; counting *unfinished* as *failed*
/// would flip every replica unready over a backend nobody has yet shown to be
/// down, removing 100% of capacity to protest an unanswered question. Keeping
/// them apart lets [`readiness_state`](crate::health::readiness_state) treat
/// absence of evidence as what it is.
#[derive(Debug)]
pub(super) enum ConnectorHealth {
    /// The connector answered, and it is serviceable.
    Healthy,

    /// The connector answered, and it is not. Carries the backend's own
    /// message, which is internal detail: `ConnectorError::Rejected` can name
    /// physical tables, servers, and schemas (§2, §29), so it is only ever
    /// rendered for an authorised caller.
    Unhealthy(String),

    /// The check had not answered by the probe's deadline. Nothing is known
    /// about this connector either way.
    Unknown,
}

impl ConnectorHealth {
    /// The stable name reported in the probe body.
    ///
    /// A string rather than a bool precisely because there are three answers;
    /// see the type's own docs.
    pub(super) const fn status(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Unhealthy(_) => "unhealthy",
            Self::Unknown => "unknown",
        }
    }

    /// The internal detail behind a failure, if there is any.
    pub(super) fn reason(&self) -> Option<&str> {
        match self {
            Self::Unhealthy(reason) => Some(reason),
            Self::Healthy | Self::Unknown => None,
        }
    }

    /// Whether the connector answered that it is serviceable.
    pub(super) const fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Whether the check ran out of time before answering.
    pub(super) const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}
