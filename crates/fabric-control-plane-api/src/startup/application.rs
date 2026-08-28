//! The application graph, top to bottom.

use std::sync::Arc;

use axum::Router;
use fabric_control_plane::{build_control_plane, ReconciliationLoop, ReconciliationLoopHandle};
use fabric_core::SystemClock;
use fabric_reconciliation::IdentityReconciler;

use crate::config::ControlPlaneAppConfig;
use crate::startup::{adapters, operator_keys, serving};

/// The assembled control plane, plus the work that must outlive a request.
pub struct Application {
    /// The HTTP surface.
    pub router: Router,

    /// The address to bind.
    pub listen: String,

    /// The background reconciliation loop. Held so it can be stopped on
    /// shutdown; dropping it orphans the task.
    pub reconciliation: ReconciliationLoopHandle,
}

/// Wires every part of the control plane. **The whole graph is this function.**
///
/// # The order, and what it rules out
///
/// 1. The desired-state repository, because everything else is about it.
/// 2. The identity provider, and the reconciler over it.
/// 3. The operator posture's signing keys, and the sign-in surface when the
///    posture has one. Neither requires the identity provider to be reachable
///    now: keys arrive on a task, and a control plane that refused to start
///    without its provider could not be used to diagnose the provider.
/// 4. The API, which is given the repository and the reconciliation status —
///    and **not** the provider. There is no wiring here by which a handler
///    could reach Keycloak, which is the structural form of ADR 0008.
/// 5. The reconciliation loop, which is the only thing holding both.
///
/// # Errors
///
/// Returns a message from whichever step failed. Every failure here is fatal:
/// a control plane that cannot reach its desired state, or cannot authenticate
/// to its identity provider, can do nothing useful, and failing at startup
/// surfaces the problem where a deployment pipeline catches it.
pub async fn build(config: &ControlPlaneAppConfig) -> Result<Application, String> {
    let clock = SystemClock::shared();

    let repository = adapters::desired_state(&config.desired_state, Arc::clone(&clock)).await?;
    let provider = adapters::identity_provider(&config.identity_provider, Arc::clone(&clock))?;
    let reconciler = Arc::new(IdentityReconciler::new(provider));

    let (keys, sign_in) = operator_keys::establish(&config.control_plane.operator)?;

    let services = build_control_plane(
        &config.control_plane,
        Arc::clone(&repository),
        Arc::clone(&clock),
        keys,
        sign_in,
    )?;

    let reconciliation = ReconciliationLoop::spawn(
        repository,
        reconciler,
        Arc::clone(&services.statuses),
        Arc::clone(&services.trigger),
        clock,
        &config.control_plane.reconciliation,
    );

    Ok(Application {
        router: serving::compose(services.router, config),
        listen: config.listen.clone(),
        reconciliation,
    })
}
