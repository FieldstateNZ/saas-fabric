//! The control-plane API's HTTP surface.
//!
//! At the 150-line limit: one function, `control_plane_routes`, with the
//! table naming every path it serves kept beside the `.route(...)` calls it
//! describes, so the two cannot drift apart.

use axum::routing::{get, post, put};
use axum::Router;

use crate::handlers;
use crate::state::ControlPlaneState;

mod integrations;

/// The path prefix every control-plane route is nested under.
///
/// # Why this is not versioned, when the Data API's prefix is
///
/// The Data API is consumed by applications the platform does not own, so a
/// breaking change there ships as a second path alongside the first — hence
/// `/v1/data`. This API has exactly one caller, the operator UI in this
/// repository, built and deployed together — versioning a path whose only
/// caller ships in the same image would be ceremony, not compatibility.
///
/// That stops holding the moment anything else calls this API. The answer
/// then is the Data API's: mount `/api/v1` alongside `/api`.
pub const API_PREFIX: &str = "/api";

/// Builds the control-plane router.
///
/// Every path this crate serves is visible here, in one file.
///
/// ```text
/// GET/POST   /api/session                          sign-in start / redeem a code   (no operator)
/// POST       /api/reconciliation                    converge every client, as you
/// One handler set is mounted twice (routes::integrations); each line below is two real routes:
/// GET/DELETE /api/integrations/{git,platform}             is it connected? / forget it
/// POST       /api/integrations/{git,platform}/connect     describe the app to create
/// GET        /api/integrations/{git,platform}/install     where to install it
/// GET        /api/integrations/{git,platform}/repositories  what the install reaches
/// PUT        /api/integrations/{git,platform}/repository  choose one
/// GET        /api/integrations/{git,platform}/created     host callback   (no operator)
/// GET        /api/integrations/{git,platform}/installed   host callback   (no operator)
/// GET        /api/platform                          what this environment runs
/// PUT/DELETE /api/platform/components/{c}/hold      stop it advancing / let it advance again
/// GET        /api/platform/components/{c}/versions  what it could go back to
/// POST       /api/platform/components/{c}/rollback  put it back on one
/// GET/POST   /api/catalogue                         the product catalogue / apply one command
/// GET        /api/activity                          every recorded action, newest first
/// GET        /api/operator                          who is signed in
/// GET/POST   /api/clients                           list clients / create one
/// GET        /api/clients/{clientId}                one client's overview
/// GET/PUT    /api/clients/{clientId}/product        its product config / replace it   (If-Match)
/// GET/PUT    /api/clients/{clientId}/identity       its identity and reconciliation state / replace it
/// GET        /api/clients/{clientId}/secrets                    list its secret paths
/// GET/PUT/DELETE /api/clients/{clientId}/secrets/entry/{path}  metadata / write / delete a version
/// POST       /api/clients/{clientId}/secrets/reveal             reveal values   (path in the body)
/// ```
///
/// Not here: anything that names a file, edits a document as text, or reaches
/// an identity provider (§8, ADR 0008). `PUT` means what the Data API does
/// not: a genuine whole-resource replacement.
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
        // The component *is* named, and the environment still is not: a
        // component name is a key looked up in a manifest this platform
        // already trusts, unlike the environment parameter that used to be here.
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
        .route(
            "/catalogue",
            get(handlers::get_catalogue).post(handlers::change_catalogue),
        )
        .route("/activity", get(handlers::list_activity))
        .route("/operator", get(handlers::get_operator))
        .route(
            "/clients",
            get(handlers::list_clients).post(handlers::create_client),
        )
        .route(
            "/clients/{client_id}/product",
            get(handlers::get_product).put(handlers::put_product),
        )
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
