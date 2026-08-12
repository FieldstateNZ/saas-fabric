//! Configuration for one NDC connector instance.

mod connector_config;
mod procedures;

pub use connector_config::NdcConnectorConfig;
pub use procedures::{CollectionProcedures, ProcedureBinding};
