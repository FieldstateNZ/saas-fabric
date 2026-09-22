//! The axum extractor that makes the operator a handler parameter.

use std::future::{self, Future};
use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use http::request::Parts;

use crate::operator::{Operator, OperatorAuthError, OperatorAuthenticator};
use crate::{logging, ControlPlaneError};

/// Extracts the authenticated operator from the request.
///
/// The same device the runtime plane uses for tenant identity, for the same
/// reason: a handler that takes an [`Operator`] parameter cannot run without
/// one, so there is no path into control-plane logic that skipped
/// authentication. Every handler in this crate takes one — including the read
/// handlers, because who is reading a client's configuration is worth
/// recording too.
impl<S> FromRequestParts<S> for Operator
where
    S: Send + Sync,
    Arc<dyn OperatorAuthenticator>: FromRef<S>,
{
    type Rejection = ControlPlaneError;

    /// Not an `async fn`, for the same reason the runtime plane's tenant
    /// extractor is not: authenticating an operator is a header read and a
    /// lookup in an allowlist already in memory, so the future it returns is
    /// already complete and there is no suspension point in it.
    ///
    /// That matters more here than it looks. When operator authentication
    /// grows a second implementation — one that verifies a credential the
    /// platform issued — the temptation will be to call something over the
    /// network *per request*. Changing this return type is where that decision
    /// has to be made explicitly rather than arrived at.
    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> {
        let authenticator = Arc::<dyn OperatorAuthenticator>::from_ref(state);

        future::ready(authenticator.authenticate(&parts.headers).map_err(refusal))
    }
}

/// Turns why authentication failed into what the browser is told.
///
/// A missing bearer and a refused one are different questions to whoever
/// holds it — "how do I sign in" against "why was I signed out" — and ADR
/// 0024's gateway probe depends on telling them apart from the response
/// alone. A separate function rather than inlined in the impl above so the
/// mapping is unit-testable without going through axum's extractor
/// machinery at all: [`AcceptingOperator`](crate::testing::AcceptingOperator),
/// the only authenticator the integration tests in this crate drive, can
/// only distinguish a present header from an absent one, so it has no way
/// to present a refused bearer to a real router — this function is where
/// that case is actually exercised.
///
/// It is also the one funnel every `NotAnOperator` passes through —
/// [`OperatorAuthenticator::authenticate`] is called from nowhere but the
/// extractor above — which is why the log call lives here and not at each
/// of `NotAnOperator`'s sources. A wrong `azp` or role, a signature that
/// does not verify, an issuer that does not match, an expired or
/// not-yet-valid token, an algorithm this deployment does not accept: every
/// one of them becomes the same variant, and logging once, here, is what
/// makes [`ControlPlaneError::OperatorRefused`]'s claim that "the structured
/// log already says why" true for all of them rather than only the one
/// whoever wrote that comment happened to be looking at.
fn refusal(err: OperatorAuthError) -> ControlPlaneError {
    match err {
        OperatorAuthError::Missing => ControlPlaneError::Unauthenticated(err),
        OperatorAuthError::NotAnOperator => {
            logging::operator_refused("bearer token");
            ControlPlaneError::OperatorRefused
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use http::StatusCode;
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Metadata, Subscriber};

    use super::refusal;
    use crate::operator::OperatorAuthError;

    #[test]
    fn a_missing_bearer_stays_unauthenticated() {
        let error = refusal(OperatorAuthError::Missing);

        assert_eq!(error.code(), "unauthenticated");
        assert_eq!(error.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn a_refused_bearer_gets_its_own_code_and_a_fixed_message() {
        let error = refusal(OperatorAuthError::NotAnOperator);

        assert_eq!(error.code(), "operator_refused");
        assert_eq!(error.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(error.public_message(), "A bearer was presented and refused.");
    }

    /// Counts `tracing` events, so a test can pin that a call happened
    /// without a log-capturing dependency this workspace does not otherwise
    /// need. Spans are not this file's concern, so every span method is a
    /// no-op.
    struct EventCounter(Arc<AtomicUsize>);

    impl Subscriber for EventCounter {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _span: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }

        fn record(&self, _span: &Id, _values: &Record<'_>) {}
        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}
        fn enter(&self, _span: &Id) {}
        fn exit(&self, _span: &Id) {}

        fn event(&self, _event: &Event<'_>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn a_refused_bearer_is_logged_exactly_once() {
        let events = Arc::new(AtomicUsize::new(0));

        tracing::subscriber::with_default(EventCounter(events.clone()), || {
            let _ = refusal(OperatorAuthError::NotAnOperator);
        });

        // The message is deliberately not asserted on: it is a fixed label
        // (`logging::operator_refused`'s own test-free contract), and this
        // test's job is only that *a* line -- not zero, not two -- was
        // written for the one case a browser is told nothing at all.
        assert_eq!(events.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_missing_bearer_is_not_logged() {
        // Nothing about a request with no bearer is worth a warning -- see
        // `OperatorAuthError::Missing`'s own rustdoc.
        let events = Arc::new(AtomicUsize::new(0));

        tracing::subscriber::with_default(EventCounter(events.clone()), || {
            let _ = refusal(OperatorAuthError::Missing);
        });

        assert_eq!(events.load(Ordering::SeqCst), 0);
    }
}
