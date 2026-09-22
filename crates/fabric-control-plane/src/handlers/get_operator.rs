//! `GET /api/operator`

use axum::Json;

use crate::models::OperatorResponse;
use crate::Operator;

/// The signed-in operator's subject.
///
/// Its own endpoint rather than something the console infers from a token it
/// cannot always read: every request already goes through the same
/// [`Operator`] extractor, so this is the extractor's answer with nothing
/// else attached.
///
/// # A contract the console now depends on (ADR 0024)
///
/// The console probes this exact route to notice a session the gateway
/// already established, before it ever tries a sign-in of its own — see
/// `apps/control-plane-ui/src/session/gateway.ts`. That only works because
/// the [`Operator`] extractor refuses an anonymous request with `401` before
/// this handler runs at all: a `200` here has to mean a verified operator and
/// nothing else. A future `GET /api/user/current` that answers this same
/// question must keep the same property — it must never answer `200` for
/// "not signed in", or the probe that depends on it stops meaning anything.
///
/// The extractor's `401` also carries one more thing the probe reads: a
/// bearer the gateway forwarded and this platform refused answers
/// `operator_refused`, not the plain `unauthenticated` a request with no
/// bearer at all gets ([`ControlPlaneError::OperatorRefused`](crate::ControlPlaneError::OperatorRefused)).
/// Without that distinction, the one case a first-ever page load cannot
/// otherwise catch — a gateway session whose forwarded token this platform
/// refuses — would fall through to the console's own sign-in and quietly
/// leave the browser holding a token, which ADR 0024 §2 forbids.
pub(crate) async fn get_operator(operator: Operator) -> Json<OperatorResponse> {
    Json(OperatorResponse {
        subject: operator.subject().to_owned(),
    })
}
