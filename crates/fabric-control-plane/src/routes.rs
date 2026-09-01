//! The control-plane API's HTTP surface.

use axum::routing::{get, post, put};
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
/// POST   /api/reconciliation                converge every client, as you
/// GET    /api/integrations/git               can desired state be read?
/// POST   /api/integrations/git/connect       describe the app to create
/// GET    /api/integrations/git/created       host callback   (no operator)
/// GET    /api/integrations/git/install       where to install it
/// GET    /api/integrations/git/installed     host callback   (no operator)
/// GET    /api/integrations/git/repositories  what the install reaches
/// PUT    /api/integrations/git/repository    choose one
/// DELETE /api/integrations/git               forget the integration
/// GET    /api/platform/environments/{environment}   what an environment runs
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

    let integrations = Router::new()
        .route(
            "/integrations/git",
            get(handlers::get_integration).delete(handlers::disconnect),
        )
        .route("/integrations/git/connect", post(handlers::begin_connection))
        .route("/integrations/git/install", get(handlers::begin_install))
        .route("/integrations/git/repositories", get(handlers::list_repositories))
        .route("/integrations/git/repository", put(handlers::choose_repository))
        // The two the Git host redirects a browser to. They take no operator
        // — a redirect carries no bearer — and are correlated by a single-use
        // token instead.
        .route("/integrations/git/created", get(handlers::created))
        .route("/integrations/git/installed", get(handlers::installed));

    let clients = Router::new()
        .route("/reconciliation", post(handlers::converge))
        .route(
            "/platform/environments/{environment}",
            get(handlers::get_platform),
        )
        .route("/clients", get(handlers::list_clients))
        .route("/clients/{client_id}", get(handlers::get_client))
        .route(
            "/clients/{client_id}/identity",
            get(handlers::get_identity).put(handlers::put_identity),
        )
        // A wildcard tail, so `database/primary` arrives whole rather than as
        // a segment that cannot contain a separator. The router does not
        // validate it; `SecretPathTail` does, before anything downstream sees
        // it.
        //
        // Every path-bearing route sits under `entry/` because a catch-all
        // must be the last thing in a route — so `secrets/{*path}` alongside
        // `secrets/metadata/{*path}` would make a secret genuinely named
        // `metadata/db` indistinguishable from an operation.
        .route("/clients/{client_id}/secrets", get(handlers::list_secrets))
        .route(
            "/clients/{client_id}/secrets/entry/{*secret_path}",
            get(handlers::secret_metadata)
                .put(handlers::write_secret)
                .delete(handlers::delete_secret),
        )
        // The path travels in the body rather than the URL. Revealing is an
        // act, and a POST is what keeps it out of history, referrers and proxy
        // logs — the same reasoning that sets `no-store` on its response.
        .route(
            "/clients/{client_id}/secrets/reveal",
            post(handlers::reveal_secret),
        );

    Router::new()
        .nest(API_PREFIX, clients.merge(session).merge(integrations))
        .with_state(state)
}
