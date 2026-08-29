//! Keeping the operator posture supplied with the provider's signing keys.
//!
//! # Why a task rather than a fetch per request
//!
//! Authenticating an operator happens on every request, and the extractor that
//! does it is deliberately synchronous so that a network call cannot quietly
//! appear in front of every one of them. That decision has to be paid for
//! somewhere, and this is where: a task re-reads the key set on an interval
//! and swaps it in.

use std::sync::Arc;
use std::time::Duration;

use fabric_control_plane::{KeyHolder, OperatorConfig, SignInSurface, VerificationKeys};
use fabric_keycloak::RealmSignIn;

/// How long to wait for the provider's key document.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Builds the operator posture's key set and sign-in surface.
///
/// Returns an empty key holder and no sign-in for the trusted-header posture,
/// which needs neither.
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

    spawn_refresh(
        Arc::clone(&realm),
        Arc::clone(&keys),
        Duration::from_secs(*jwks_refresh_seconds),
    );

    Ok((keys, Some(surface)))
}

/// Re-reads the key set for as long as the process runs.
///
/// The first read happens immediately, before the first sleep, so a healthy
/// deployment is serving operators within a moment of starting rather than
/// after one whole interval.
fn spawn_refresh(realm: Arc<RealmSignIn>, keys: Arc<KeyHolder>, interval: Duration) {
    tokio::spawn(async move {
        loop {
            match realm
                .signing_keys()
                .await
                .and_then(|document| VerificationKeys::parse(&document))
            {
                Ok(read) => {
                    tracing::info!(
                        event = "control_plane.operator_keys_refreshed",
                        keys = read.len(),
                        "read the identity provider's signing keys"
                    );
                    keys.replace(read);
                }

                // Warn and keep the keys already held. A provider that is
                // briefly unreachable should not sign every operator out; the
                // keys in hand stay valid until they rotate.
                Err(error) => tracing::warn!(
                    event = "control_plane.operator_keys_unavailable",
                    error = %error,
                    "could not read the identity provider's signing keys; keeping the set in hand"
                ),
            }

            tokio::time::sleep(interval).await;
        }
    });
}
