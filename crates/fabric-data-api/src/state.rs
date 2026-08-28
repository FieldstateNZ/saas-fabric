//! Router state for the Data API.

use std::sync::Arc;

use axum::extract::FromRef;
use fabric_identity::IdentityResolver;

use crate::DataApiService;

/// What Data API handlers can reach.
///
/// The identity resolver is here so that
/// [`TenantIdentity`](fabric_identity::TenantIdentity) works as an extractor —
/// it is pulled out through [`FromRef`]. That indirection is what lets a
/// handler declare a `TenantIdentity` parameter and be unable to run without a
/// resolved tenant.
#[derive(Clone)]
pub struct DataApiState {
    /// Executes data operations.
    pub service: Arc<DataApiService>,

    /// Derives the tenant identity context from the request.
    pub identity: Arc<IdentityResolver>,
}

impl FromRef<DataApiState> for Arc<IdentityResolver> {
    fn from_ref(state: &DataApiState) -> Self {
        Arc::clone(&state.identity)
    }
}
