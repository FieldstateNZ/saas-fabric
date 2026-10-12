//! What a stored document's own projection says its projected clients
//! should be, and where the stored entries no longer match (D01-19b): read
//! by a product save, by every client read, and by every convergence.
//!
//! The comparison is against the **same stored document**: its client id, its
//! hosts and the release snapshot each assignment pinned. Never against a
//! request or the catalogue as it is now — either would mistake a legitimate
//! host change or release upgrade for a hand edit.

use std::collections::BTreeSet;

use crate::{
    ClientDocument, DesiredStateError, OidcClient, PrivateNetworkScheme, ProjectedClientDivergence,
    ProjectedClientField,
};

impl ClientDocument {
    /// The entries this document's own assignments project, computed from
    /// its stored hosts, client id and pinned release snapshots.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError`] if the product section does not parse or
    /// the stored assignments cannot be projected at all.
    pub fn projected_clients(&self) -> Result<Vec<OidcClient>, DesiredStateError> {
        self.projected_clients_over(self.client().private_network_scheme)
    }

    /// As [`Self::projected_clients`], under `scheme` in place of the one
    /// the document declares.
    pub(super) fn projected_clients_over(
        &self,
        scheme: Option<PrivateNetworkScheme>,
    ) -> Result<Vec<OidcClient>, DesiredStateError> {
        let projected = self.with_application_identity_over(scheme)?;
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

    /// Every stored projected client that no longer matches
    /// [`Self::projected_clients`], one per client, in the order the projection lists them.
    /// Hand-declared clients are never compared.
    ///
    /// # Errors
    ///
    /// As [`Self::projected_clients`].
    pub fn projection_divergences(&self) -> Result<Vec<ProjectedClientDivergence>, DesiredStateError> {
        self.divergences_over(self.client().private_network_scheme)
    }

    /// As [`Self::projection_divergences`], under `scheme`.
    pub(super) fn divergences_over(
        &self,
        scheme: Option<PrivateNetworkScheme>,
    ) -> Result<Vec<ProjectedClientDivergence>, DesiredStateError> {
        let stored = &self.client().identity.clients;

        Ok(self
            .projected_clients_over(scheme)?
            .into_iter()
            .filter_map(|expected| {
                let field = match stored.iter().find(|client| client.id == expected.id) {
                    None => Some(ProjectedClientField::Missing),
                    Some(actual) => difference(&expected, actual),
                };
                field.map(|field| ProjectedClientDivergence {
                    client: expected.id,
                    field,
                })
            })
            .collect())
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
