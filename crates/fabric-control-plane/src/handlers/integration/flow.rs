//! Which of the two connection flows a route serves.

use std::sync::Arc;

use crate::git_integration::GitIntegrationService;
use crate::state::ControlPlaneState;
use crate::ControlPlaneError;

/// A flow, chosen by the route table and never by a caller.
///
/// The handlers beside this are written once and mounted twice. What differs
/// between the two mountings is which service they act on and which word their
/// callbacks return to the console with — both decided here, at compile time.
///
/// # Why a type, and not a path segment
///
/// A handler reading `/api/integrations/{kind}` would be a handler whose caller
/// names the thing it acts on. Section 15 forbids that, and this platform has
/// already had to close one instance of it — the environment name that used to
/// be a path segment of `GET /api/platform` and reached the repository as a
/// path segment too.
///
/// There is also nothing to generalise. The two are separate product concepts
/// with separate applications on the host, separate installations, separate
/// records and separate reasons to exist. A parameterised route would invent a
/// third, nameless thing for them to be instances of.
pub(crate) trait Flow: Send + Sync + 'static {
    /// The query key its callbacks return to the console with.
    ///
    /// Distinct per flow: both can be mid-connection at once, and a console
    /// showing "connected" against the wrong panel is worse than showing
    /// nothing.
    const OUTCOME_KEY: &'static str;

    /// The service, or a refusal saying this deployment has no such flow.
    ///
    /// # Errors
    ///
    /// Returns the refusal belonging to this flow, and they are not the same
    /// refusal. A deployment that states its own client repository has *opted
    /// out* of connecting one; a deployment that manages no platform is not
    /// opting out of anything.
    fn service(state: &ControlPlaneState) -> Result<&Arc<GitIntegrationService>, ControlPlaneError>;
}

/// Connecting the repository client configuration lives in.
pub(crate) struct ClientConfigurationFlow;

impl Flow for ClientConfigurationFlow {
    // Unchanged. The console reads this key today, and deployments have an
    // application on the host whose stored callbacks lead back to it.
    const OUTCOME_KEY: &'static str = "git";

    fn service(state: &ControlPlaneState) -> Result<&Arc<GitIntegrationService>, ControlPlaneError> {
        state
            .git_integration
            .as_ref()
            .ok_or(ControlPlaneError::IntegrationNotManaged)
    }
}

/// Connecting the repository desired platform state lives in.
pub(crate) struct PlatformManagementFlow;

impl Flow for PlatformManagementFlow {
    const OUTCOME_KEY: &'static str = "platform";

    fn service(state: &ControlPlaneState) -> Result<&Arc<GitIntegrationService>, ControlPlaneError> {
        state
            .platform_integration
            .as_ref()
            .ok_or(ControlPlaneError::PlatformNotManaged)
    }
}
