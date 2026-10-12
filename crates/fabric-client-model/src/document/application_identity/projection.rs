//! What a stored document's own projection says its projected clients
//! should be, and where the stored entries no longer match (D01-19b).
//!
//! The comparison is against the **same stored document**: its client id, its
//! hosts and the release snapshot each assignment pinned. Never against a
//! request or the catalogue as it is now — either would mistake a legitimate
//! host change or release upgrade for a hand edit.

use std::collections::BTreeSet;

use crate::{ClientDocument, DesiredStateError, OidcClient, ProjectedClientDivergence, ProjectedClientField};

impl ClientDocument {
    /// The entries this document's own assignments project, computed from
    /// its stored hosts, client id and pinned release snapshots.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError`] if the product section does not parse or
    /// the stored assignments cannot be projected at all.
    pub fn projected_clients(&self) -> Result<Vec<OidcClient>, DesiredStateError> {
        let projected = self.with_application_identity()?;
        let ids: BTreeSet<String> = self
            .product()?
            .applications
            .iter()
            .map(|assignment| assignment.application_id.as_str().to_owned())
            .collect();

        Ok(projected
            .client()
            .identity
            .clients
            .iter()
            .filter(|client| ids.contains(client.id.as_str()))
            .cloned()
            .collect())
    }

    /// The first stored projected client that no longer matches
    /// [`Self::projected_clients`], if any. Hand-declared clients are never
    /// compared.
    ///
    /// # Errors
    ///
    /// As [`Self::projected_clients`].
    pub fn projection_divergence(&self) -> Result<Option<ProjectedClientDivergence>, DesiredStateError> {
        let stored = &self.client().identity.clients;

        for expected in self.projected_clients()? {
            let field = match stored.iter().find(|client| client.id == expected.id) {
                None => Some(ProjectedClientField::Missing),
                Some(actual) => difference(&expected, actual),
            };
            if let Some(field) = field {
                return Ok(Some(ProjectedClientDivergence {
                    client: expected.id,
                    field,
                }));
            }
        }

        Ok(None)
    }
}

/// The first part of `actual` that differs from `expected`. Callbacks
/// compare as a set: reordering them is not a change.
fn difference(expected: &OidcClient, actual: &OidcClient) -> Option<ProjectedClientField> {
    let uris = |client: &OidcClient| client.redirect.uris().iter().cloned().collect::<BTreeSet<_>>();

    if actual.protocol != expected.protocol {
        Some(ProjectedClientField::Protocol)
    } else if actual.pkce != expected.pkce {
        Some(ProjectedClientField::Pkce)
    } else if actual.redirect.kind() != expected.redirect.kind() {
        Some(ProjectedClientField::RedirectStrategy)
    } else if uris(actual) != uris(expected) {
        Some(ProjectedClientField::RedirectUris)
    } else {
        None
    }
}
