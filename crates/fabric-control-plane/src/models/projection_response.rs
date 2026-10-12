//! Whether a client's stored projected entries are still its own
//! projection, on the wire (D01-19b).

use fabric_client_model::{OidcClientId, ProjectedClientDivergence, ProjectionCheck};

/// Reported on every client read, so the console shows a stored
/// inconsistency before anyone tries a save. A fact about the stored
/// document: it never claims to know who edited it, and it names fields,
/// never callbacks.
///
/// - `consistent`: every previously assigned projected entry matches what
///   the stored document projects.
/// - `diverged`: `divergences` names each projected client that does not,
///   and the first field that differs. A product save and a re-assertion
///   are refused, and convergence withholds the declared ones.
/// - `unverifiable`: the stored document's projection cannot be computed,
///   so `unverified` names the declared clients that may be projected
///   entries. Convergence withholds them.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub(crate) enum ProjectionResponse {
    /// The stored projection is the document's own.
    Consistent,
    /// Stored projected entries differ from their projection.
    Diverged {
        /// Each one, with the first field that differs.
        divergences: Vec<ProjectedClientDivergence>,
    },
    /// The stored projection cannot be computed.
    Unverifiable {
        /// The declared clients that may be projected entries.
        unverified: Vec<OidcClientId>,
    },
}

impl From<ProjectionCheck> for ProjectionResponse {
    fn from(check: ProjectionCheck) -> Self {
        match check {
            ProjectionCheck::Consistent => Self::Consistent,
            ProjectionCheck::Diverged(divergences) => Self::Diverged { divergences },
            ProjectionCheck::Unverifiable { unverified } => Self::Unverifiable { unverified },
        }
    }
}
