//! The Data API's public request and response shapes.
//!
//! These are the platform's own contract, deliberately unrelated to any
//! connector protocol. Nothing here mirrors NDC, and nothing here should start
//! to: the day these shapes become a connector's shapes is the day applications
//! acquire a dependency on the execution layer.

mod field_reference;
mod list_query;
#[cfg(test)]
mod list_query_tests;
mod query_string;
mod responses;

pub use list_query::ListQuery;
pub use responses::{ListResponse, PagingInfo, RowResponse, WriteResponse};
