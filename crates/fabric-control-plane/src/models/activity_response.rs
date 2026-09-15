//! What the activity feed looks like on the wire.

use fabric_client_model::catalogue::ProductActivity;

/// Every recorded action an operator can see, newest first.
///
/// An object with one field rather than a bare array, for the same reason
/// [`ClientListResponse`](super::ClientListResponse) is: the shape can grow a
/// paging cursor later without becoming a different response entirely.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ActivityResponse {
    /// Catalogue and per-client actions, merged and sorted by the caller.
    pub(crate) activity: Vec<ProductActivity>,
}
