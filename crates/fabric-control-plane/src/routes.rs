//! The control-plane API's HTTP surface.

use axum::routing::get;
use axum::Router;

use crate::handlers;
use crate::state::ControlPlaneState;

/// The path prefix every control-plane route is nested under.
///
/// # Why this is not versioned, when the Data API's prefix is
///
/// The Data API is consumed by *applications the platform does not own*, so a
/// breaking change there has to ship as a second path served alongside the
/// first — hence `/v1/data`. This API is consumed by exactly one client, the
/// operator UI in this repository, and the two are built and deployed
/// together. Versioning a path whose only caller ships in the same image would
/// be ceremony, not compatibility.
///
/// That reasoning stops holding the moment anything else calls this API. If
/// that day comes, the answer is the Data API's: mount `/api/v1` alongside
/// `/api` rather than changing what `/api` means.
pub const API_PREFIX: &str = "/api";

/// Builds the control-plane router.
///
/// Every path this crate serves is visible here, in one file.
///
/// ```text
/// GET /api/session                       where to sign in   (no operator)
/// POST /api/session                      redeem a code      (no operator)
/// GET /api/integrations/git              can the platform read desired state?
/// GET /api/clients                       list clients
/// GET /api/clients/{clientId}            one client's overview
/// GET /api/clients/{clientId}/identity   its identity, and reconciliation state
/// PUT /api/clients/{clientId}/identity   replace its identity  (If-Match required)
/// ```
///
/// Note what is not here: nothing that names a file, nothing that edits a
/// document as text, and nothing that reaches an identity provider (§8, ADR
/// 0008). Note also what `PUT` means here and does not mean in the Data API —
/// this is a genuine whole-resource replacement, so `PUT` is the honest verb.
pub(crate) fn control_plane_routes(state: ControlPlaneState) -> Router {
    // Mounted only when the deployment has a sign-in. Under the trusted-header
    // posture there is nothing to sign in to, and a route that exists in order
    // to refuse every call is a route somebody eventually makes work.
    let session = if state.sign_in.is_some() {
        Router::new().route(
            "/session",
            get(handlers::session_config).post(handlers::redeem_session),
        )
    } else {
        Router::new()
    };

    let integrations = Router::new().route("/integrations/git", get(handlers::get_integration));

    let clients = Router::new()
        .route("/clients", get(handlers::list_clients))
        .route("/clients/{client_id}", get(handlers::get_client))
        .route(
            "/clients/{client_id}/identity",
            get(handlers::get_identity).put(handlers::put_identity),
        );

    Router::new()
        .nest(API_PREFIX, clients.merge(session).merge(integrations))
        .with_state(state)
}
