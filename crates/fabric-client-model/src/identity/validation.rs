//! The rules an identity configuration must satisfy before it is written.
//!
//! Kept apart from the type's own file because these are a different concern
//! from the shape being checked, and because every one of them is a rule an
//! operator can break from the UI — so each deserves its own explanation.

use crate::identity::required_roles;
use crate::{DesiredStateError, IdentityConfiguration, OidcClient};

impl IdentityConfiguration {
    /// Checks everything the type system cannot.
    ///
    /// Run at three points, deliberately: when a stored document is parsed,
    /// when an operator submits a change, and again after the change has been
    /// merged into the document. The third looks redundant and is not — it is
    /// what makes "the repository never holds a document this model would
    /// refuse to read" true by construction rather than by review.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError`] naming the first rule broken, using the
    /// field's dotted path as it appears in the document so the message points
    /// at something the operator can see.
    pub fn validate(&self) -> Result<(), DesiredStateError> {
        self.check_roles()?;
        self.check_clients()
    }

    /// Roles must be unique and must include the platform's required pair.
    fn check_roles(&self) -> Result<(), DesiredStateError> {
        for (index, role) in self.roles.iter().enumerate() {
            let duplicated = self.roles.iter().take(index).any(|earlier| earlier == role);

            if duplicated {
                return Err(DesiredStateError::Duplicate {
                    field: "spec.identity.roles",
                    value: role.to_string(),
                });
            }
        }

        if let Some(missing) = required_roles::first_missing(&self.roles) {
            return Err(DesiredStateError::RequiredRoleMissing { role: missing });
        }

        Ok(())
    }

    /// Application clients must be uniquely named and individually usable.
    fn check_clients(&self) -> Result<(), DesiredStateError> {
        for (index, client) in self.clients.iter().enumerate() {
            let duplicated = self
                .clients
                .iter()
                .take(index)
                .any(|earlier| earlier.id == client.id);

            if duplicated {
                return Err(DesiredStateError::Duplicate {
                    field: "spec.identity.clients",
                    value: client.id.to_string(),
                });
            }

            check_client_is_usable(client)?;
        }

        Ok(())
    }
}

/// Refuses a client that could never complete an authentication flow.
///
/// A client with no redirect URI is not a client that is "not finished yet" —
/// reconciliation would create it, Keycloak would accept it, and the first
/// login attempt would fail with an error the operator sees weeks later and in
/// a different system.
fn check_client_is_usable(client: &OidcClient) -> Result<(), DesiredStateError> {
    if client.redirect_uris.is_empty() {
        return Err(DesiredStateError::InvalidField {
            field: "spec.identity.clients",
            detail: format!(
                "{} declares no redirect URI, so it could never sign a user in",
                client.id
            ),
        });
    }

    Ok(())
}
