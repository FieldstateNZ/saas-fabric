//! Explicit catalogue commands prevent callers from forging releases or activity.
use super::{ApplicationDefinition, ConfigurationField, ConsoleSettings, EnvironmentRegistration};
use crate::ClientId;
use serde::{Deserialize, Serialize};

/// A validated catalogue mutation, applied against an expected revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum CatalogueCommand {
    /// Start an application draft.
    CreateApplication {
        /// Stable application key.
        id: ClientId,
        /// Initial name.
        name: String,
    },
    /// Replace an application's editable draft.
    SaveApplication {
        /// Application key.
        id: ClientId,
        /// Updated draft.
        definition: ApplicationDefinition,
    },
    /// Publish an immutable snapshot of the current draft.
    PublishApplication {
        /// Application key.
        id: ClientId,
        /// Release note.
        note: String,
    },
    /// Replace the shared client contract.
    SaveDefinition {
        /// Custom client fields.
        fields: Vec<ConfigurationField>,
    },
    /// Update platform display and defaults.
    SaveSettings {
        /// The updated settings.
        settings: ConsoleSettings,
    },
    /// Register or update another operator console.
    SaveEnvironment {
        /// The environment's public operator endpoint.
        environment: EnvironmentRegistration,
    },
}
