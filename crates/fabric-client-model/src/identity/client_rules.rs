//! The rules one application client must satisfy.
//!
//! Split out of `validation` because the client rules are now the substantial
//! half — an entitlement, a wildcard policy and a deferral, each with its own
//! argument — while the rules that remain there are about the identity
//! configuration as a whole.

use crate::{DesiredStateError, OidcClient, RedirectStrategyKind};

/// The dotted path these refusals name.
const FIELD: &str = "spec.identity.clients";

/// The phase that will carry a private-use scheme end to end.
///
/// `pub`, not `pub(crate)`: the Keycloak adapter's own refusal of a
/// `customScheme` client that bypassed this validation (a regression, not a
/// normal path — see `fabric_keycloak::provider::declaration`) names the same
/// phase, and a hard-coded copy there is exactly the kind of string that
/// drifts the moment this one changes.
pub const CUSTOM_SCHEME_PHASE: &str = "Lane E phase 2";

/// Checks one declared application client.
///
/// # Errors
///
/// Returns [`DesiredStateError::Deferred`] for a strategy this model can
/// represent but this phase does not reconcile, and
/// [`DesiredStateError::InvalidField`] for a client with no callback, a
/// callback its strategy does not admit, or a wildcard where its strategy does
/// not permit one.
pub(super) fn check(client: &OidcClient) -> Result<(), DesiredStateError> {
    check_strategy_is_carried(client)?;

    match client.redirect.first_complaint() {
        Some(detail) => Err(DesiredStateError::InvalidField {
            field: FIELD,
            detail: format!("{} {detail}", client.id),
        }),
        None => Ok(()),
    }
}

/// Refuses a strategy this phase does not reconcile.
///
/// # Why this is a refusal and not a warning
///
/// Keycloak would accept the string. What has not been designed is the
/// matching semantics that make a private-use scheme *safe*:
/// `com.example.app:/callback` and `com.example.app://callback` are matched
/// differently by Keycloak, by AppAuth-Android and by
/// `ASWebAuthenticationSession`, and a scheme any other application on the
/// device can also register is the interception RFC 8252 §8.6 warns about.
/// Writing a variant nobody has signed in through would assert a security
/// boundary with no evidence behind it.
///
/// It is refused here, on the same three-point schedule as every other rule,
/// so the document is never written — rather than written and then failing at
/// the adapter, where the operator is no longer looking.
fn check_strategy_is_carried(client: &OidcClient) -> Result<(), DesiredStateError> {
    let RedirectStrategyKind::CustomScheme(scheme) = client.redirect.kind() else {
        return Ok(());
    };

    Err(DesiredStateError::Deferred {
        field: FIELD,
        phase: CUSTOM_SCHEME_PHASE,
        detail: format!(
            "{} declares the private-use scheme {scheme}, which is representable but not yet \
             reconciled; a desktop shell should use a loopback callback under the development \
             strategy, which RFC 8252 §7.3 recommends over a private-use scheme in any case",
            client.id
        ),
    })
}
