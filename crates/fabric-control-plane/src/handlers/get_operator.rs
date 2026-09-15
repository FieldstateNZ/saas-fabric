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
pub(crate) async fn get_operator(operator: Operator) -> Json<OperatorResponse> {
    Json(OperatorResponse {
        subject: operator.subject().to_owned(),
    })
}
