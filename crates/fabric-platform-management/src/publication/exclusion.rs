//! Why the generated issuer registry leaves a bound tenant out.

/// Why a bound tenant is not in the generated registry. Each kind is its
/// own variant so a temporary exclusion (evidence not yet re-established)
/// is never read as a recorded retirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exclusion {
    /// The client's lifecycle is incomplete; each unmet condition is named
    /// with its evidence, as `condition: detail`. Identity evidence pending
    /// reconciliation -- after a control-plane restart, or after a client
    /// write -- lands here, and the issuer returns in a later generation
    /// once the client is reconciled.
    NotReady {
        /// The unmet readiness conditions.
        unmet: Vec<String>,
    },
    /// Retirement is recorded: tombstoned, or deletion confirmed.
    Retired {
        /// Which retired state.
        state: String,
    },
    /// No client in desired state names this tenant.
    NoClient,
    /// The client's realm does not form an issuer the registry accepts.
    InvalidIssuer {
        /// Why.
        detail: String,
    },
}

impl Exclusion {
    /// The kind, as the API names it.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NotReady { .. } => "notReady",
            Self::Retired { .. } => "retired",
            Self::NoClient => "noClient",
            Self::InvalidIssuer { .. } => "invalidIssuer",
        }
    }
}

impl std::fmt::Display for Exclusion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotReady { unmet } => write!(formatter, "not ready: {}", unmet.join("; ")),
            Self::Retired { state } => write!(formatter, "retirement is recorded ({state})"),
            Self::NoClient => formatter.write_str("no client in desired state names this tenant"),
            Self::InvalidIssuer { detail } => write!(formatter, "no valid issuer: {detail}"),
        }
    }
}
