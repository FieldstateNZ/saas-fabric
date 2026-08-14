//! Translation between the neutral model and the NDC wire format.
//!
//! Everything here is one-way plumbing with one rule: **never widen**. If a
//! neutral operation cannot be expressed faithfully in NDC, the translation
//! fails with [`ConnectorError::Unsupported`](fabric_connector::ConnectorError)
//! rather than emitting something close. A predicate that gets quietly dropped
//! in translation might be the one scoping rows to a tenant, and the resulting
//! bug returns a `200` with too many rows — the hardest kind to notice.
//!
//! See the `wire` module's docs (`src/wire.rs`) for the full policy this
//! implements — which areas matter most, and why.
//!
//! # Refusals have two audiences
//!
//! `ConnectorError::Unsupported` is the one error whose capability name
//! `fabric-data-api` forwards to an application, so a refusal raised here may
//! name the capability the platform asked for and nothing physical. That is no
//! longer a rule this crate has to remember: the name is a closed
//! [`UnsupportedFeature`](fabric_connector::UnsupportedFeature) with nowhere to
//! put a collection, field, or procedure, and the identifiers go in the
//! [`RefusalDetail`](fabric_connector::RefusalDetail) alongside it, which
//! reaches an operator's log and no response body.
//!
//! So raise refusals with `UnsupportedFeature::…refused_because(detail)` and
//! put the physical specifics in the detail, where they are useful.

mod capabilities;
#[cfg(test)]
mod capabilities_tests;
mod expression;
#[cfg(test)]
mod expression_tests;
mod membership;
#[cfg(test)]
mod membership_tests;
mod mutation;
#[cfg(test)]
mod mutation_tests;
mod null_check;
mod procedure_arguments;
mod query;
#[cfg(test)]
mod query_tests;
#[cfg(test)]
mod refusal_tests;
mod response;
#[cfg(test)]
mod response_tests;

pub(crate) use capabilities::to_capabilities;
pub(crate) use expression::to_expression;
pub(crate) use mutation::to_mutation_request;
pub(crate) use query::to_query_request;
pub(crate) use response::{to_mutation_outcome, to_query_outcome};
