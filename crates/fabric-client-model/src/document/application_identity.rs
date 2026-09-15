//! Publishes assigned applications into the existing identity reconciliation contract.
use crate::{
    ClientDocument, ClientProtocol, DesiredStateError, OidcClient, OidcClientId, PkceMethod,
    RedirectStrategy, RedirectStrategyKind, RedirectUri,
};
impl ClientDocument {
    pub(super) fn with_application_identity(&self) -> Result<Self, DesiredStateError> {
        let mut identity = self.client().identity.clone();
        for assignment in self.product()?.applications {
            let definition = &assignment.release.definition;
            let mut uris = Vec::new();
            if !definition.domain.is_empty() {
                let host = definition.domain.replace("{client}", self.client().id.as_str());
                uris.push(
                    RedirectUri::try_new(format!("https://{host}/callback"))
                        .map_err(|error| identifier(&error))?,
                );
            }
            for host in &self.client().hosts {
                uris.push(
                    RedirectUri::try_new(format!("https://{host}/{}/callback", assignment.application_id))
                        .map_err(|error| identifier(&error))?,
                );
            }
            if uris.is_empty() {
                return Err(DesiredStateError::InvalidField {
                    field: "hosts",
                    detail: "An assigned application requires a client hostname or application domain".into(),
                });
            }
            let application = OidcClient {
                id: OidcClientId::try_new(assignment.application_id.as_str())
                    .map_err(|error| identifier(&error))?,
                protocol: ClientProtocol::Oidc,
                pkce: PkceMethod::S256,
                redirect: RedirectStrategy::try_new(RedirectStrategyKind::ClaimedHttps, uris)?,
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
