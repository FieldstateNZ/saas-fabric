//! Turning a connector's HTTP response into a value, or the right error.
//!
//! Kept apart from [`NdcHttpClient`](super::NdcHttpClient) because only half
//! of this needs a network. Reading the body off the wire does;
//! *interpreting* what came back does not, and that half is where the
//! interesting failures live — a connector that answers `200` with an HTML
//! error page, or with JSON missing a field we require.

use fabric_connector::{ConnectorError, ConnectorId};
use serde::de::DeserializeOwned;

use crate::client::error_mapping::{malformed, rejected, result_lost};

/// Reads a response off the wire and decodes it.
///
/// # A body that stops arriving is not a transport outage
///
/// By the time this runs the status line has already been read, and that is a
/// fact worth more than any `reqwest::Error` discriminator. If it said `2xx`,
/// the backend ran the operation and reported success; a body that then dies
/// mid-stream has lost the *result*, not the effect. Reporting that as
/// "unreachable" told a caller its write had not happened when the backend had
/// already committed it — and, through a 503, invited a retry that would commit
/// it again.
///
/// A non-success status takes the other path: the backend said it failed, and
/// [`rejected`] with an empty body degrades to naming the status, which is
/// exactly as much as is known once the body is gone.
///
/// # Errors
///
/// [`ConnectorError::ResultLost`] if a success body cannot be read,
/// [`ConnectorError::Rejected`] if a failure body cannot be read, otherwise
/// whatever [`decode_body`] decides.
pub(crate) async fn decode<T: DeserializeOwned>(
    connector: &ConnectorId,
    response: reqwest::Response,
) -> Result<T, ConnectorError> {
    let status = response.status();

    let body = match response.bytes().await {
        Ok(body) => body,
        Err(error) if status.is_success() => return Err(result_lost(connector, error)),
        Err(_) => return Err(rejected(connector, status, &[])),
    };

    decode_body(connector, status, &body)
}

/// Turns a status and body into a decoded value, or the right error.
///
/// Split from [`decode`] so response *parsing* is unit-testable on its own:
/// this is what makes a malformed `/capabilities` body — not JSON at all, or
/// JSON missing a field the response type requires — something we can assert
/// on directly, rather than something we can only discover by standing up a
/// fake connector over HTTP. The negotiation path in
/// [`registration`](crate::registration) calls
/// `get::<NdcCapabilitiesResponse>`, which bottoms out here for exactly that
/// reason.
///
/// The two failure modes stay distinct. A non-success status is the
/// connector *telling* us something went wrong, and is reported as
/// [`ConnectorError::Rejected`]; a success status we cannot parse means the
/// connector is not speaking the protocol we think it is, and is reported as
/// [`ConnectorError::MalformedResponse`]. Collapsing them would leave an
/// operator unable to tell "the database refused this query" from "this is
/// not an NDC connector".
///
/// # Errors
///
/// [`ConnectorError::Rejected`] for a non-success status,
/// [`ConnectorError::MalformedResponse`] for a body that will not parse.
pub(crate) fn decode_body<T: DeserializeOwned>(
    connector: &ConnectorId,
    status: reqwest::StatusCode,
    body: &[u8],
) -> Result<T, ConnectorError> {
    if !status.is_success() {
        return Err(rejected(connector, status, body));
    }

    serde_json::from_slice(body).map_err(|error| malformed(connector, error.to_string()))
}
