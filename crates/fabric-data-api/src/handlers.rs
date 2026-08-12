//! Request handlers.
//!
//! Each handler extracts, delegates, and converts. No business logic lives
//! here — that is [`DataApiService`](crate::DataApiService)'s job — and no
//! handler assembles a status code by hand, because
//! [`DataApiError`](crate::DataApiError) knows its own.
//!
//! Every handler takes a [`TenantIdentity`](fabric_identity::TenantIdentity)
//! parameter. That is not decoration: it is an extractor, so a handler cannot
//! run without a resolved tenant. "Did we remember to check the tenant?" is a
//! compile-time question in this crate.

mod create_resource;
mod delete_resource;
mod list_resource;
mod read_resource;
mod update_resource;

pub(crate) use create_resource::create_resource;
pub(crate) use delete_resource::delete_resource;
pub(crate) use list_resource::list_resource;
pub(crate) use read_resource::read_resource;
pub(crate) use update_resource::update_resource;

use fabric_core::LogicalResourceName;

use crate::DataApiError;

/// Parses the resource segment of the path.
///
/// Shared by every handler so the validation cannot drift between them. The
/// name arrives from the URL, so it is caller-controlled and validated before
/// it is used to look anything up.
///
/// # Errors
///
/// [`DataApiError::BadRequest`] if the segment is not a valid resource name.
fn parse_resource(raw: &str) -> Result<LogicalResourceName, DataApiError> {
    LogicalResourceName::try_new(raw)
        .map_err(|error| DataApiError::BadRequest(format!("invalid resource name: {error}")))
}
