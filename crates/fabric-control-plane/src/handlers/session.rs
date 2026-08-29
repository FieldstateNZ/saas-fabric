//! Obtaining an operator token, and nothing else.
//!
//! These are the only two handlers in the API that do not take an
//! [`Operator`](crate::Operator), because they are how one is obtained. They
//! are mounted only when the deployment has a sign-in posture at all.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::sign_in::SignInSurface;
use crate::sign_in::{IssuedToken, SignInError};
use crate::state::ControlPlaneState;
use crate::ControlPlaneError;

/// What the console needs in order to send the browser to the provider.
///
/// Every field is already public knowledge — a client id, a redirect URI and
/// an endpoint are all visible to anyone who watches the redirect. Nothing
/// here is a credential, and the console holds no secret: it is a public
/// client, which is why PKCE is doing the work instead.
#[derive(Serialize)]
pub(crate) struct SessionConfig<'a> {
    /// Where to send the browser.
    authorization_endpoint: &'a str,

    /// The client to authenticate as.
    client_id: &'a str,

    /// Where the provider must return the browser.
    redirect_uri: &'a str,

    /// The scopes to request.
    scope: &'static str,
}

/// What an operator's browser asks the platform to redeem.
#[derive(Deserialize)]
pub(crate) struct Redemption {
    /// The authorization code the provider handed the browser.
    code: String,

    /// The PKCE verifier the browser kept when it sent the challenge.
    code_verifier: String,
}

/// The scopes the console asks for.
///
/// `openid` because this is OIDC, and `profile` for the username the audit
/// record and the Git commit are attributed to. Notably **not** `offline_access`
/// — that is what mints a refresh token, and the console deliberately holds
/// none.
const SCOPE: &str = "openid profile";

/// Tells the console where to send the operator to authenticate.
pub(crate) async fn session_config(
    State(state): State<ControlPlaneState>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let surface = surface(&state)?;

    Ok(Json(serde_json::json!(SessionConfig {
        authorization_endpoint: surface.provider.authorization_endpoint(),
        client_id: &surface.client_id,
        redirect_uri: &surface.redirect_uri,
        scope: SCOPE,
    })))
}

/// Redeems an authorization code for the token the console will present.
pub(crate) async fn redeem_session(
    State(state): State<ControlPlaneState>,
    Json(body): Json<Redemption>,
) -> Result<Json<IssuedToken>, ControlPlaneError> {
    let surface = surface(&state)?;

    // Refused rather than "bad request": the provider would refuse a blank
    // code too, and answering here saves a round trip to be told so. The
    // console's next step is the same either way — start the sign-in again.
    if body.code.trim().is_empty() || body.code_verifier.trim().is_empty() {
        return Err(ControlPlaneError::SignInRefused);
    }

    surface
        .provider
        .redeem(&body.code, &body.code_verifier)
        .await
        .map(Json)
        .map_err(|error| match error {
            SignInError::Refused => ControlPlaneError::SignInRefused,
            SignInError::Unavailable => ControlPlaneError::SignInUnavailable,
        })
}

/// The sign-in surface, which is present whenever these routes are mounted.
///
/// The `None` arm is unreachable by construction rather than by convention —
/// [`control_plane_routes`](crate::routes) mounts neither handler without one.
/// It is still handled rather than unwrapped, because this crate denies
/// `unwrap`, and because "unreachable" is a claim that outlives the code that
/// made it true.
fn surface(state: &ControlPlaneState) -> Result<&Arc<SignInSurface>, ControlPlaneError> {
    state.sign_in.as_ref().ok_or(ControlPlaneError::SignInUnavailable)
}
