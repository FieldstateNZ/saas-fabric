//! Configuration for one NDC connector instance.
//!
//! | Module | Responsibility |
//! |---|---|
//! | `connector_config` | The struct and its defaults |
//! | `connector_validation` | What makes a configuration safe |
//! | `procedures` | How a collection's writes map onto procedures |

mod connector_config;
mod connector_validation;
#[cfg(test)]
mod connector_validation_tests;
mod procedures;

pub use connector_config::NdcConnectorConfig;
pub use procedures::{CollectionProcedures, ProcedureBinding};
