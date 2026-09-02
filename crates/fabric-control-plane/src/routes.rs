//! The control-plane API's HTTP surface.

use axum::routing::{get, post, put};
use axum::Router;

use crate::handlers;
use crate::state::ControlPlaneState;

mod integrations;

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
/// GET    /api/integrations/platform            has an application been made?
/// POST   /api/integrations/platform/connect    describe the app to create
/// GET    /api/integrations/platform/created    host callback   (no operator)
/// GET    /api/integrations/platform/install    where to install it
/// GET    /api/integrations/platform/installed  host callback   (no operator)
/// GET    /api/integrations/platform/repositories  what the install reaches
/// PUT    /api/integrations/platform/repository    choose one
/// DELETE /api/integrations/platform            forget the integration
/// GET    /api/platform                        what this environment runs
/// PUT    /api/platform/components/{c}/hold    stop it advancing
/// DELETE /api/platform/components/{c}/hold    let it advance again
/// GET    /api/platform/components/{c}/versions   what it could go back to
/// POST   /api/platform/components/{c}/rollback   put it back on one
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

    let clients = Router::new()
        .route("/reconciliation", post(handlers::converge))
        .route("/platform", get(handlers::get_platform))
        // The component *is* named, and the environment still is not. A
        // component name is a key looked up in a manifest this platform
        // already read and trusts; it reaches no path, no registry and no
        // other locator, which is what makes it unlike the environment
        // parameter that used to be here.
        .route(
            "/platform/components/{component}/hold",
            put(handlers::pause_component).delete(handlers::resume_component),
        )
        .route(
            "/platform/components/{component}/versions",
            get(handlers::rollback_candidates),
        )
        // A POST, because it is an act rather than a resource an operator
        // composed: they name a version, and what gets written — three digests
        // and a hold, in one commit — is the platform's to resolve.
        .route(
            "/platform/components/{component}/rollback",
            post(handlers::roll_back_component),
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
        .nest(
            API_PREFIX,
            clients
                .merge(session)
                .merge(integrations::client_configuration())
                .merge(integrations::platform_management()),
        )
        .with_state(state)
}
