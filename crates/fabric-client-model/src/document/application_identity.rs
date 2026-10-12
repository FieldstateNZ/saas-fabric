//! Publishes assigned applications into the existing identity reconciliation contract.
mod callbacks;
#[cfg(test)]
mod host_kind_tests;
mod legacy_scheme;
mod projection;
mod projection_check;
#[cfg(test)]
mod projection_tests;
#[cfg(test)]
mod scheme_tests;

use crate::{
    ClientDocument, ClientProtocol, DesiredStateError, OidcClient, OidcClientId, PkceMethod,
    PrivateNetworkScheme,
};

pub(super) use callbacks::check_scheme;

impl ClientDocument {
    pub(super) fn with_application_identity(&self) -> Result<Self, DesiredStateError> {
        self.with_application_identity_over(self.client().private_network_scheme)
    }

    /// As [`Self::with_application_identity`], with `scheme` standing for the
    /// one the document declares. Only the legacy-scheme check passes
    /// anything else, and only to compare: see `legacy_scheme`.
    fn with_application_identity_over(
        &self,
        scheme: Option<PrivateNetworkScheme>,
    ) -> Result<Self, DesiredStateError> {
        let mut identity = self.client().identity.clone();
        for assignment in self.product()?.applications {
            let redirect = callbacks::redirect(
                &self.client().id,
                &self.client().hosts,
                scheme,
                assignment.application_id.as_str(),
                &assignment.release.definition.domain,
            )?;
            let application = OidcClient {
                id: OidcClientId::try_new(assignment.application_id.as_str())
                    .map_err(|error| identifier(&error))?,
                protocol: ClientProtocol::Oidc,
                pkce: PkceMethod::S256,
                redirect,
            };
            if let Some(existing) = identity
                .clients
                .iter_mut()
                .find(|client| client.id == application.id)
            {
                *existing = application;
            } else {
                identity.clients.push(application);
            }
        }
        self.with_identity(identity)
    }
}

fn identifier(error: &fabric_core::IdentifierError) -> DesiredStateError {
    DesiredStateError::InvalidField {
        field: "applications",
        detail: error.to_string(),
    }
}
