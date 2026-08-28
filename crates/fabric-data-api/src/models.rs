//! The Data API's public request and response shapes.
//!
//! These are the platform's own contract, deliberately unrelated to any
//! connector protocol. Nothing here mirrors NDC, and nothing here should start
//! to: the day these shapes become a connector's shapes is the day applications
//! acquire a dependency on the execution layer.
//!
//! Responses are split one type per file rather than kept in one `responses`
//! module, because [`RowResponse`] is no longer a plain serialisation shape: it
//! is where the resource's field allowlist is applied on the way out, and that
//! reasoning needs room to be read on its own.

mod discriminator;
mod field_reference;
mod list_query;
#[cfg(test)]
mod list_query_tests;
mod list_response;
mod query_string;
mod row_response;
mod visible_fields;
mod writable_fields;
mod write_response;

pub use list_query::ListQuery;
pub use list_response::{ListResponse, PagingInfo};
pub use row_response::RowResponse;
pub(crate) use visible_fields::VisibleFields;
pub(crate) use writable_fields::WritableFields;
pub use write_response::WriteResponse;
