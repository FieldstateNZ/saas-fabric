//! The application graph, top to bottom.

use std::sync::Arc;

use axum::Router;
use fabric_control_plane::{build_control_plane, ControlPlaneDeps};
use fabric_core::SystemClock;

use crate::config::ControlPlaneAppConfig;
use crate::startup::{adapters, integration, operator_keys, platform, serving};

/// The assembled control plane, plus the work that must outlive a request.
pub struct Application {
    /// The HTTP surface.
    pub router: Router,

    /// The address to bind.
    pub listen: String,
}

/// Wires every part of the control plane. **The whole graph is this function.**
///
/// # The order, and what it rules out
///
/// 1. The desired-state binding, because everything else is about it. It may
///    hold no repository at all — that is the production mode.
/// 2. The identity provider, and the reconciler over it.
/// 3. The operator posture's signing keys, and the sign-in surface when the
///    posture has one. Neither requires the identity provider to be reachable
///    now: keys arrive on a task, and a control plane that refused to start
///    without its provider could not be used to diagnose the provider.
/// 4. The API, which is given the repository and the reconciliation status —
///    and **not** the provider. There is no wiring here by which a handler
///    could reach Keycloak, which is the structural form of ADR 0008.
/// 5. The Git connection flows, each given the binding it drives so that an
///    operator connecting a repository takes effect without a restart. There
///    are two, and they are separate all the way down: two applications on
///    the host, two records in the store, two things an operator may connect
///    or forget without touching the other.
///
/// There is no sixth step for *clients* any more. A reconciliation loop used to
/// be spawned here, holding a service account's credential; ADR 0012 removed
/// that credential, so convergence happens when an operator asks and carries
/// their authority rather than the platform's.
///
/// 6. Platform Management, when this deployment manages a platform repository,
///    and the sweep that advances it. This one *is* unattended, and
///    deliberately: a policy that said `automatic` and waited for a click
///    would be a manual policy with a longer name. It carries no operator's
///    authority because it changes no client — it moves a version pin in a
///    repository the deployment was given, under a policy that repository
///    states.
///
///    The sweep starts *after* the router is built. One that started earlier
///    could advance an environment nobody could yet look at.
///
/// # Errors
///
/// Returns a message from whichever step failed. What is fatal here changed
/// with operator-managed desired state, and the distinction is the point:
///
/// - **Fatal** — configuration this deployment *stated* and stated wrongly: a
///   Git repository it named with a credential that is not set, an identity
///   provider URL that will not parse. A pipeline should catch these, and
///   starting anyway would hide a mistake behind a healthy-looking process.
/// - **Not fatal** — having no desired-state repository at all, and being
///   unable to reach the identity provider right now. Neither is a
///   misconfiguration; the first is a platform waiting for an operator and the
///   second is an outage. Refusing to start for either would take away the
///   console that exists to resolve them.
pub async fn build(config: &ControlPlaneAppConfig) -> Result<Application, String> {
    let clock = SystemClock::shared();

    let repository = adapters::desired_state(&config.desired_state, Arc::clone(&clock)).await?;
    let identity_provider = adapters::identity_provider(&config.identity_provider)?;

    let (keys, sign_in) = operator_keys::establish(&config.control_plane.operator)?;

    // Before the flows, because one of them connects it.
    let platform_management = platform::establish(config.platform_management.as_ref(), &clock)?;

    let integrations =
        integration::establish(config, &repository, platform_management.as_ref(), &clock).await?;

    let services = build_control_plane(
        &config.control_plane,
        ControlPlaneDeps {
            desired_state: Arc::clone(&repository),
            clock: Arc::clone(&clock),
            keys,
            identity_provider,
            sign_in,
            git_integration: integrations.clients,
            client_secrets: integrations.client_secrets,

            platform: platform_management.clone(),
            platform_integration: integrations.platform,

            // Always the configured posture. The override exists for tests.
            operators: None,
        },
    )?;

    // Last, and after the router: a sweep that started before the API was
    // serving would advance an environment nobody could yet look at.
    platform::start_sweeping(
        config.platform_management.as_ref(),
        platform_management.as_ref(),
        &services.platform_sweeps,
    );

    Ok(Application {
        router: serving::compose(services.router, config),
        listen: config.listen.clone(),
    })
}
