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
