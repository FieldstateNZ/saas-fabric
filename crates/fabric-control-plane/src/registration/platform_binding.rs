//! Platform Management, and the one environment a deployment manages.

use std::sync::Arc;

/// Platform Management, and the environment it manages.
///
/// # Why the environment is not a request parameter
///
/// It reaches the platform repository as a path segment —
/// `environments/<name>/components.yaml` — so a caller who could name it could
/// name a path. Specification section 31.7 forbids exactly that, and the
/// cheapest way to satisfy it is to have nowhere for a caller to say it: a
/// deployment manages one environment, stated in its configuration, and the
/// route takes no name at all.
#[derive(Clone)]
pub struct PlatformBinding {
    /// The service.
    pub service: Arc<fabric_platform_management::PlatformManagement>,

    /// The environment it manages, from this deployment's configuration.
    pub environment: String,

    /// The late-bound repository, so a connected integration can point it
    /// somewhere and a disconnected one can take it away.
    pub repository: Arc<fabric_platform_management::PlatformDesiredState>,
}
