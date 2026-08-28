//! Structured log events for the Keycloak adapter.
//!
//! Nothing here is handed a credential, a token, or a Keycloak response body.
//! The adapter's failures are already reduced to a sanitised sentence by
//! `admin::errors` before anything can log them, and the endpoint is the only
//! deployment detail these lines carry.

use fabric_core::{event_id, EventType};

use crate::DOMAIN_ID;

/// Keycloak reported a role this model cannot represent.
///
/// Trace, not warn. It is expected rather than exceptional — a realm an
/// operator has also configured by hand will have roles SaaS Fabric never
/// declared — and it changes no decision, because a role the platform declared
/// always parses. It is recorded at all so that "why is my role not showing
/// up?" has somewhere to look.
///
/// The name is deliberately not logged: it is a value from an external system,
/// and this is the one place text from Keycloak could reach a log line.
pub(crate) fn unmodellable_role_ignored() {
    tracing::trace!(
        event = "keycloak.unmodellable_role_ignored",
        event_id = event_id(DOMAIN_ID, EventType::Trace, 1),
        "a realm role was reported that the platform's model cannot hold; ignoring it"
    );
}
