//! What the signed-in operator looks like on the wire.

/// The authenticated operator's subject.
///
/// Its own endpoint, separate from every other response that already carries
/// an operator's authority, because the console needs to know who is signed
/// in *before* it has read or written anything a client's subject could be
/// inferred from.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperatorResponse {
    /// The subject the operator authenticator accepted.
    pub(crate) subject: String,
}
