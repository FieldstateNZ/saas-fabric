//! Starting a connection, and completing the application's creation.

use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::git_integration::{AppCreationRequest, FlowStep, GitIntegration, SecretName};
use crate::logging;
use crate::Operator;

impl GitIntegrationService {
    /// Describes the application to create, and starts a flow for it.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::Refused`] if the organisation is not a
    /// plausible account name, or if the platform's randomness is unavailable.
    pub fn begin_connection(
        &self,
        operator: &Operator,
        organisation: &str,
    ) -> Result<AppCreationRequest, IntegrationError> {
        let organisation = organisation.trim();

        // Checked here rather than left to the host, because this value is
        // interpolated into a URL the operator's browser is then sent to.
        if !is_account_name(organisation) {
            return Err(IntegrationError::Refused(
                "that is not a valid organisation name".to_owned(),
            ));
        }

        let state = self
            .flows
            .begin(
                operator.subject(),
                FlowStep::Creation,
                self.clock.now_unix_seconds(),
            )
            .map_err(IntegrationError::Refused)?;

        logging::integration_flow_started(operator.subject(), "creation");

        Ok(self.provisioning.creation_request(organisation, &state))
    }

    /// Completes creation: redeems the code and records the application.
    ///
    /// **The key is stored before the record is written.** A record without
    /// its key describes an application this platform can never authenticate
    /// as, and the key is returned exactly once — so if the write of the
    /// record fails, the flow can be repeated, whereas the reverse order would
    /// leave an integration nothing could fix.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::NotOurFlow`] if the callback does not name
    /// a live flow, or a failure from the host or either store.
    pub async fn complete_creation(&self, code: &str, state: &str) -> Result<(), IntegrationError> {
        let flow = self
            .flows
            .consume(state, FlowStep::Creation, self.clock.now_unix_seconds())
            .ok_or(IntegrationError::NotOurFlow)?;

        let created = self.provisioning.redeem_creation(code).await?;

        self.secrets
            .put(&SecretName::new(self.kind.private_key()), &created.private_key)
            .await?;

        self.store
            .save(
                self.kind,
                &GitIntegration::created(&created.app_id, &created.app_slug),
            )
            .await?;

        logging::integration_app_created(&flow.operator, &created.app_slug);

        Ok(())
    }

    /// Where the operator installs the application.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::NotConnected`] if no application has been
    /// created yet.
    pub async fn begin_install(&self, operator: &Operator) -> Result<String, IntegrationError> {
        let integration = self.current().await?.ok_or(IntegrationError::NotConnected)?;

        let state = self
            .flows
            .begin(
                operator.subject(),
                FlowStep::Installation,
                self.clock.now_unix_seconds(),
            )
            .map_err(IntegrationError::Refused)?;

        logging::integration_flow_started(operator.subject(), "installation");

        Ok(self.provisioning.install_url(&integration.app_slug, &state))
    }
}

/// Whether this looks like an account name on the host.
///
/// GitHub's own rule: alphanumerics and hyphens, not starting or ending with
/// one, at most 39 characters. Enforcing it here means the value cannot carry
/// a path separator or a query into the URL the browser is handed.
fn is_account_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 39
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}
