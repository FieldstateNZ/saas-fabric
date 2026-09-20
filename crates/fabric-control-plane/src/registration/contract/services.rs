//! What building the control plane hands back to the host.

use std::sync::Arc;

use axum::Router;
use fabric_reconciliation::ReconciliationStatusStore;

/// What building the control plane produces.
///
/// The router is the obvious half. The rest are handed back because the
/// **reconciliation loop is the host's to start**, not this function's: a
/// process that only wants to serve the API — a test, or a replica deliberately
/// running read-only — should be able to have one without a background task
/// sweeping every client behind it.
pub struct ControlPlaneServices {
    /// The HTTP surface.
    pub router: Router,

    /// What is known about whether desired state has taken effect.
    pub statuses: Arc<ReconciliationStatusStore>,

    /// What the last sweep observed about reading desired state.
    ///
    /// Held by the loop, which records into it, and by the API, which reports
    /// it. Nothing else needs it, and nothing else may write to it.
    pub health: Arc<crate::IntegrationHealth>,

    /// What the last platform sweep found, and whether one is running.
    ///
    /// Handed back for the same reason as the reconciliation loop: **starting
    /// the sweep is the host's**, on the cadence its deployment configures. A
    /// process that only wants to serve the API should be able to, without a
    /// background task advancing environments behind it.
    pub platform_sweeps: Arc<fabric_platform_management::SweepState>,

    /// Publishes this environment's runtime documents, when both a platform
    /// and a publication target are configured.
    ///
    /// `None` for every deployment that manages no platform, or manages one
    /// but states no `[platform_management.publication]` section — handed
    /// back, rather than started here, for the same reason as
    /// `platform_sweeps`: **starting the schedule is the host's**. The same
    /// value already lives on `PlatformBinding::publisher` behind
    /// `ControlPlaneState`, so a scheduled pass and an operator's `POST
    /// /api/platform/publication` are racing the same guard rather than two
    /// independent ones.
    pub publisher: Option<Arc<fabric_platform_management::RuntimePublisher>>,

    /// What the last publication pass found, and whether one is running.
    ///
    /// The same instance `PlatformBinding::publication` carries, handed back
    /// so the host can pass it to the schedule it starts — see
    /// `platform_sweeps` above for why this is returned rather than acted
    /// on here. Present even when `publisher` is `None`, for the reason
    /// `PlatformBinding::publication`'s own rustdoc gives.
    pub publication: Arc<fabric_platform_management::PublicationState>,
}
