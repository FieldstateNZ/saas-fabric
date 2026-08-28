//! Configuration for one NDC connector instance.
//!
//! | Module | Responsibility |
//! |---|---|
//! | `connector_config` | The struct and its defaults |
//! | `connector_validation` | The entry point, and the transport checks |
//! | `argument_validation` | What a write mapping must say about its arguments |
//! | `procedures` | How a collection's writes map onto procedures |
//!
//! Everything here is answerable without contacting the connector. The checks
//! that need its `/schema` — that a mapped procedure exists, and that every
//! argument named here is one it declares — live in
//! [`registration`](crate::registration).

mod argument_validation;
mod connector_config;
mod connector_validation;
#[cfg(test)]
mod connector_validation_tests;
mod procedures;

pub use connector_config::NdcConnectorConfig;
pub use procedures::{CollectionProcedures, ProcedureBinding};
