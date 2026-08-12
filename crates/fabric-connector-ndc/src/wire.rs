//! The NDC wire format, hand-written from the published specification.
//!
//! # Reading this module
//!
//! These types mirror NDC 0.2.13 exactly, including field names and tag values,
//! because they are serialised straight onto the wire. That means they do *not*
//! follow this codebase's usual naming — `predicate` rather than `filter`,
//! `order_by` rather than `sort` — and they should not be tidied up. If a name
//! here looks wrong, check the specification before changing it.
//!
//! Only the subset the platform actually uses is modelled. NDC can express
//! relationships, nested fields, aggregates, grouping, and variable sets; the
//! Data API deliberately exposes none of those, so they are absent here.
//!
//! Everything is `pub(crate)`. None of it escapes this crate.
//!
//! # The implemented subset is a closed list, not a starting point
//!
//! What is modelled above is not "everything we have gotten to so far" — it
//! is everything the Data API is willing to ask a connector to do. Those are
//! different claims, and confusing them is how a platform quietly grows a
//! feature nobody reviewed for tenant safety.
//!
//! **Adding support for an NDC feature this crate does not yet implement is a
//! deliberate act, not a side effect of wiring up a field.** It means:
//! reading the relevant part of the specification, hand-writing wire types
//! for it (see the top of this module for why they cannot be borrowed from
//! `ndc-models`), and translating it with the same discipline as everything
//! already here — refuse what cannot be expressed faithfully, never
//! approximate it.
//!
//! **A capability this crate does not implement must fail as
//! [`ConnectorError::Unsupported`](fabric_connector::ConnectorError::Unsupported),
//! not be approximated, widened, or silently dropped.** This is the
//! `translate` module's one rule, and it matters most in these
//! areas, because getting any of them wrong looks exactly like success — a
//! `200` comes back, and nothing in the response says what was quietly left
//! out:
//!
//! - **Predicates.** A clause that cannot be expressed must fail the whole
//!   query, never be dropped from it. The predicate that goes missing may be
//!   the tenant isolation boundary itself — see
//!   [`IsolationModel::tenant_predicate`](fabric_connector::IsolationModel) —
//!   and losing it does not return an error, it returns rows the caller was
//!   never supposed to see.
//! - **Relationships.** Not implemented at all, and that is not a gap to
//!   close casually: synthesising a relationship traversal by chaining
//!   separate queries in this crate would change the atomicity and
//!   consistency guarantees NDC's own relationship support provides, in a way
//!   the caller has no way to see.
//! - **Mutations.** Core NDC 0.2 has no generic insert/update/delete, only
//!   procedure calls. Do not infer a mapping from a neutral mutation to a
//!   procedure the connector never declared, and do not relax the
//!   `filter_argument` requirement on update/delete — `config::validate` and
//!   `translate::mutation` both check it, independently, because the failure
//!   mode is a write reaching every tenant's rows.
//! - **Aggregates.** Not requested by this crate — the Data API does not
//!   issue aggregate queries, so
//!   [`ConnectorCapabilities::total_count`](fabric_connector::ConnectorCapabilities::total_count)
//!   is unconditionally `false`. Do not start deriving a count from something
//!   else, such as the length of a possibly-paginated row set, in place of
//!   asking the connector.
//! - **Ordering.** Only column ordering by direction is implemented.
//!   Relationship-path or expression-based ordering is not; refuse rather
//!   than fall back to an unordered, or wrongly ordered, result.
//! - **Null semantics.** How a connector evaluates a predicate over a null
//!   field is that connector's own three-valued logic, which this crate does
//!   not attempt to normalise or reinterpret on its behalf. Do not translate
//!   a neutral predicate in a way that assumes null-handling behaviour the
//!   specification does not itself guarantee.
//! - **Scalar types.** Operator support is read per scalar type from the
//!   connector's own `/schema` response at startup
//!   ([`SchemaIndex`](crate::SchemaIndex)), never assumed from a type's name.
//!   A scalar type this crate has never seen before is exactly as supported
//!   as its declared operators say, and no more.

mod capabilities;
mod expression;
mod mutation;
mod ndc_type;
mod query;
mod response;
mod schema;

pub(crate) use capabilities::NdcCapabilitiesResponse;
pub(crate) use expression::{NdcComparisonTarget, NdcComparisonValue, NdcExpression, NdcUnaryOperator};
pub(crate) use mutation::{
    NdcMutationOperation, NdcMutationRequest, NdcMutationResponse, NdcOperationResult,
};
pub(crate) use ndc_type::NdcType;
pub(crate) use query::{
    NdcField, NdcOrderBy, NdcOrderByElement, NdcOrderByTarget, NdcOrderDirection, NdcQuery, NdcQueryRequest,
};
pub(crate) use response::{NdcErrorResponse, NdcQueryResponse};
pub(crate) use schema::{NdcComparisonOperatorDefinition, NdcSchemaResponse};
