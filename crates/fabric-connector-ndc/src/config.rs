//! Configuration for one NDC connector instance.
//!
//! | Module | Responsibility |
//! |---|---|
//! | `connector_config` | The struct and its defaults |
//! | `connector_validation` | The entry point, and the transport checks |
//! | `argument_validation` | What a write mapping must say about its payload and filter arguments |
//! | `key_argument_validation` | What a write mapping must say about its key arguments and payload shape |
//! | `procedures` | How a collection's writes map onto procedures |
//! | `procedure_binding` | One procedure and the argument names it expects |
//! | `payload_shape` | The JSON shape a mapping's payload argument expects |
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
mod key_argument_validation;
mod payload_shape;
#[cfg(test)]
mod payload_shape_tests;
mod procedure_binding;
mod procedures;

pub use connector_config::NdcConnectorConfig;
pub use payload_shape::PayloadShape;
pub use procedure_binding::ProcedureBinding;
pub use procedures::CollectionProcedures;
