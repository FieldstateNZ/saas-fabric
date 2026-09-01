//! The public half of a stored integration.

use serde::Serialize;

/// What an operator is told about the application itself.
///
/// Public identifiers only. The slug and the account are visible to anyone
/// looking at the organisation's settings page; the private key is not here,
/// is not referenced here, and is not obtainable through this API at all.
#[derive(Serialize)]
pub(super) struct Application {
    /// The application's slug on the host.
    pub slug: String,

    /// The account it is installed on, once it has been installed.
    pub account: Option<String>,

    /// Whether an installation exists.
    pub installed: bool,

    /// The repository this integration settled on, once it has.
    pub repository: Option<String>,
}

/// Describes a stored integration, without saying anything secret about it.
pub(super) fn describe(integration: &crate::GitIntegration) -> Application {
    Application {
        slug: integration.app_slug.clone(),
        account: integration
            .installation
            .as_ref()
            .map(|installation| installation.account.clone()),
        installed: integration.installation.is_some(),
        repository: integration.repository().map(crate::SelectedRepository::describe),
    }
}
