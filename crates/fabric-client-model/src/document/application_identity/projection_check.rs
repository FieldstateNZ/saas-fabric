//! Whether a stored document's projected entries are still its own
//! projection (D01-19b), as one answer that never fails: the read-time
//! report and the convergence guard both need an answer for a document
//! whose projection cannot be computed, not an error.

use std::collections::BTreeSet;

use crate::{ClientDocument, OidcClientId, ProjectionCheck};

impl ClientDocument {
    /// Whether the stored projected entries are still this document's own
    /// projection, for reporting on read and for deciding what a
    /// projection-based convergence may converge. Never fails: a projection
    /// that cannot be computed is [`ProjectionCheck::Unverifiable`].
    #[must_use]
    pub fn projection_check(&self) -> ProjectionCheck {
        match self.projection_divergences() {
            Ok(divergences) if divergences.is_empty() => ProjectionCheck::Consistent,
            Ok(divergences) => ProjectionCheck::Diverged(divergences),
            Err(_) => ProjectionCheck::Unverifiable {
                unverified: self.possibly_projected(),
            },
        }
    }

    /// The declared clients that may be projected entries: those an
    /// assignment names, or every declared client when the assignments
    /// cannot be read.
    fn possibly_projected(&self) -> Vec<OidcClientId> {
        let assigned = self.product().map(|product| {
            product
                .applications
                .into_iter()
                .map(|assignment| assignment.application_id.as_str().to_owned())
                .collect::<BTreeSet<_>>()
        });

        self.client()
            .identity
            .clients
            .iter()
            .filter(|client| {
                assigned
                    .as_ref()
                    .map_or(true, |assigned| assigned.contains(client.id.as_str()))
            })
            .map(|client| client.id.clone())
            .collect()
    }
}
