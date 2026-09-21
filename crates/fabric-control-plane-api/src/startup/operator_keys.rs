//! Keeping the operator posture supplied with the provider's signing keys.
//!
//! # Why a task rather than a fetch per request
//!
//! Authenticating an operator happens on every request, and the extractor that
//! does it is deliberately synchronous so that a network call cannot quietly
//! appear in front of every one of them. That decision has to be paid for
//! somewhere, and this is where: a task re-reads the key set on an interval
//! and swaps it in.
//!
//! The task itself is [`refresh`], kept in its own file: this one only
//! establishes the posture, which is a different concept from keeping it
//! current.

use std::sync::Arc;
use std::time::Duration;

use fabric_control_plane::{KeyHolder, OperatorConfig, SignInSurface};
use fabric_keycloak::RealmSignIn;

mod refresh;

/// How long to wait for the provider's key document.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Builds the operator posture's key set and sign-in surface.
///
/// OIDC is the only posture, so this always returns a sign-in surface and
/// starts the key refresh. The `Option` in the return type is the seam a
/// harness uses to compose an authenticator directly and skip sign-in; nothing
/// this function builds ever leaves it `None`.
///
/// # Errors
///
/// Returns a message if the sign-in adapter cannot be built. **Not** if the
/// provider is unreachable: a control plane that refuses to start because its
/// identity provider is down is a control plane that cannot be used to
/// diagnose why its identity provider is down.
pub(super) fn establish(
    config: &OperatorConfig,
) -> Result<(Arc<KeyHolder>, Option<Arc<SignInSurface>>), String> {
    let keys = KeyHolder::empty();

    // Irrefutable: there is one posture. Left as a destructure rather than
    // collapsed into field access so that adding a second is a compile error
    // here, where the decision about what it can supply has to be made.
    let OperatorConfig::Oidc {
        issuer,
        reachable_at,
        client_id,
        redirect_uri,
        jwks_refresh_seconds,
        ..
    } = config;

    // One URL unless a deployment states two, which it does when the address
    // the browser uses is not one this pod can resolve.
    let reachable_at = if reachable_at.trim().is_empty() {
        issuer
    } else {
        reachable_at
    };

    let realm = Arc::new(RealmSignIn::new(
        issuer,
        reachable_at,
        client_id,
        redirect_uri,
        FETCH_TIMEOUT,
    )?);

    let surface = Arc::new(SignInSurface {
        provider: Arc::clone(&realm) as Arc<dyn fabric_control_plane::OperatorSignIn>,
        client_id: client_id.clone(),
        redirect_uri: redirect_uri.clone(),
    });

    refresh::spawn(
        Arc::clone(&realm) as Arc<dyn refresh::SigningKeySource>,
        Arc::clone(&keys),
        Duration::from_secs(*jwks_refresh_seconds),
        issuer.clone(),
    );

    Ok((keys, Some(surface)))
}
