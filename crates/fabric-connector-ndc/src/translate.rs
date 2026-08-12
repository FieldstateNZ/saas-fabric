//! Translation between the neutral model and the NDC wire format.
//!
//! Everything here is one-way plumbing with one rule: **never widen**. If a
//! neutral operation cannot be expressed faithfully in NDC, the translation
//! fails with [`ConnectorError::Unsupported`](fabric_connector::ConnectorError)
//! rather than emitting something close. A predicate that gets quietly dropped
//! in translation might be the one scoping rows to a tenant, and the resulting
//! bug returns a `200` with too many rows — the hardest kind to notice.

mod capabilities;
mod expression;
mod mutation;
mod query;
mod response;

pub(crate) use capabilities::to_capabilities;
pub(crate) use expression::to_expression;
pub(crate) use mutation::to_mutation_request;
pub(crate) use query::to_query_request;
pub(crate) use response::{to_mutation_outcome, to_query_outcome};
