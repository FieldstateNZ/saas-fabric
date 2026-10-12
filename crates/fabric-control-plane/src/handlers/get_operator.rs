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
/// # Kept one release, subject only (ADR 0024 §3)
///
/// The console's gateway probe moved to `GET /api/user/current`, which takes
/// the same [`Operator`] and so answers the same question the same way: a
/// `401` from the extractor before any handler runs, `unauthenticated` with
/// no bearer and `operator_refused` for a refused one
/// ([`ControlPlaneError::OperatorRefused`](crate::ControlPlaneError::OperatorRefused)),
/// and a `200` only for a verified operator. This route keeps answering, with
/// the attribution name and nothing else, for one release, so a caller that
/// still reads it has that long to move.
pub(crate) async fn get_operator(operator: Operator) -> Json<OperatorResponse> {
    Json(OperatorResponse {
        subject: operator.subject().to_owned(),
    })
}
