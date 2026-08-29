//! Where the Git host returns the operator's browser.
//!
//! # These answer with a redirect, not with JSON
//!
//! The caller is a browser mid-navigation, not the console's fetch layer. It
//! has nowhere to render a JSON error and no code waiting for one, so both
//! handlers send it back to the console with a short outcome in the query
//! string and let the console explain what happened.
//!
//! # Nothing here is trusted except the correlation token
//!
//! Every value arrives in a query string an attacker can write. The token is
//! what makes the callback ours; the rest is passed to the flow, which
//! verifies it against the host before recording anything.

use axum::extract::{Query, State};
use axum::response::Redirect;
use serde::Deserialize;

use crate::state::ControlPlaneState;
use crate::ControlPlaneError;

/// What the host appends after the application is created.
#[derive(Deserialize)]
pub(crate) struct Creation {
    /// The one-time code redeemed for the application.
    #[serde(default)]
    code: String,

    /// The correlation token this platform issued.
    #[serde(default)]
    state: String,
}

/// What the host appends after the application is installed.
#[derive(Deserialize)]
pub(crate) struct InstallationCallback {
    /// The installation the operator just approved.
    #[serde(default)]
    installation_id: String,

    /// The correlation token this platform issued.
    #[serde(default)]
    state: String,
}

/// Completes the application's creation.
pub(crate) async fn created(
    State(state): State<ControlPlaneState>,
    Query(query): Query<Creation>,
) -> Result<Redirect, ControlPlaneError> {
    let service = state.git_integration()?;

    let outcome = service.complete_creation(&query.code, &query.state).await;

    Ok(back_to_console(&state, "created", &outcome))
}

/// Completes the installation.
pub(crate) async fn installed(
    State(state): State<ControlPlaneState>,
    Query(query): Query<InstallationCallback>,
) -> Result<Redirect, ControlPlaneError> {
    let service = state.git_integration()?;

    let outcome = service
        .complete_install(&query.installation_id, &query.state)
        .await;

    Ok(back_to_console(&state, "installed", &outcome))
}

/// Sends the browser back to the console, saying what happened.
///
/// The outcome is a short stable word, never the error's message: this lands
/// in an address bar, and a message assembled from an upstream failure is not
/// something to put there.
fn back_to_console<E>(state: &ControlPlaneState, step: &str, outcome: &Result<(), E>) -> Redirect {
    let base = state.public_base_url.trim_end_matches('/');

    match outcome {
        Ok(()) => Redirect::to(&format!("{base}/?git={step}")),
        Err(_) => Redirect::to(&format!("{base}/?git_error={step}")),
    }
}
