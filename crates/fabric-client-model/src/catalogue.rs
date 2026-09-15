//! Versioned application definitions and operator-managed product configuration.
mod application;
mod assignment;
mod command;
mod fields;
mod mutations;
mod product;
mod validation;

use crate::{ClientRevision, DesiredStateError};
pub use application::*;
pub use command::CatalogueCommand;
pub use fields::*;
pub use product::*;
use serde::{Deserialize, Serialize};

/// A coherent snapshot, committed as one desired-state document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalogue {
    /// Applications with drafts and immutable published definitions.
    pub applications: Vec<Application>,
    /// Custom fields required for newly created clients.
    pub client_fields: Vec<ConfigurationField>,
    /// Presentation and defaults for this platform.
    pub settings: ConsoleSettings,
    /// Registered operator consoles; each manages its own environment.
    pub environments: Vec<EnvironmentRegistration>,
    /// Durable operator actions recorded with the same write as the change.
    pub activity: Vec<ProductActivity>,
    /// Version of the shared client contract.
    pub definition_version: u32,
}
/// A catalogue and the opaque revision required to replace it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredCatalogue {
    /// The desired state.
    pub catalogue: Catalogue,
    /// None until the first successful write.
    pub revision: Option<ClientRevision>,
}
impl Catalogue {
    /// Reads a persisted catalogue, validating its complete graph.
    /// # Errors
    /// Returns an error for malformed or internally inconsistent definitions.
    pub fn parse(text: &str) -> Result<Self, DesiredStateError> {
        let value: Self = serde_norway::from_str(text).map_err(|error| malformed(&error))?;
        value.validate()?;
        Ok(value)
    }
    /// Serializes a validated snapshot for storage.
    /// # Errors
    /// Returns an error when a definition is invalid or serialization fails.
    pub fn render(&self) -> Result<String, DesiredStateError> {
        self.validate()?;
        serde_norway::to_string(self).map_err(|error| malformed(&error))
    }
}
fn malformed(error: &serde_norway::Error) -> DesiredStateError {
    DesiredStateError::Malformed {
        detail: error.to_string(),
    }
}
