//! The bodies of the three lifecycle writes.

use fabric_client_model::ClientId;

/// `POST /api/clients/{clientId}/tombstone`.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TombstoneRequest {
    /// Why the client is being taken out of service.
    pub(crate) reason: String,
}

/// `POST /api/clients/{clientId}/deletion-confirmation`.
///
/// Names the client again, so a confirmation cannot be replayed against a
/// different path by accident.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeletionConfirmationRequest {
    /// The id of the client whose deletion is confirmed.
    pub(crate) confirm_client_id: ClientId,
}

/// `POST /api/clients/{clientId}/restoration`.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RestorationRequest {
    /// Why the client is being brought back into service.
    pub(crate) reason: String,
}
