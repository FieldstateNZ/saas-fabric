//! Versioned application definitions and operator-managed product configuration.
mod application;
mod assignment;
mod command;
mod fields;
mod mutations;
mod product;
mod schema;
mod validation;

use crate::{ClientRevision, DesiredStateError};
pub use application::*;
pub use command::CatalogueCommand;
pub use fields::*;
pub use product::*;
use schema::Envelope;
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
    /// Reads a persisted catalogue: checks its `apiVersion`/`kind` envelope,
    /// then parses and validates the `spec` it wraps.
    ///
    /// The envelope is checked first, and separately from `spec`, for the
    /// same reason the client document checks its own kind before parsing —
    /// see `document::parse`: a document that is not a catalogue at all gets
    /// told so, rather than being read as a catalogue with every field
    /// missing.
    /// # Errors
    /// Returns an error for a missing or mismatched envelope, or for
    /// malformed or internally inconsistent definitions.
    pub fn parse(text: &str) -> Result<Self, DesiredStateError> {
        let raw: serde_norway::Value = serde_norway::from_str(text).map_err(|error| malformed(&error))?;
        schema::check_document_kind(&raw)?;
        let envelope: Envelope = serde_norway::from_value(raw).map_err(|error| malformed(&error))?;
        envelope.spec.validate()?;
        Ok(envelope.spec)
    }
    /// Serializes a validated snapshot for storage, wrapped in the same
    /// `apiVersion`/`kind` envelope every desired-state document carries.
    ///
    /// The envelope is storage-only: the HTTP API's [`StoredCatalogue`]
    /// serialises the catalogue body directly, with nothing about it changed
    /// by this.
    /// # Errors
    /// Returns an error when a definition is invalid or serialization fails.
    pub fn render(&self) -> Result<String, DesiredStateError> {
        self.validate()?;
        let envelope = Envelope::wrapping(self.clone());
        serde_norway::to_string(&envelope).map_err(|error| malformed(&error))
    }
}
fn malformed(error: &serde_norway::Error) -> DesiredStateError {
    DesiredStateError::CatalogueMalformed {
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_catalogue_round_trips_through_render_and_parse() {
        let mut catalogue = Catalogue::default();
        catalogue.settings.platform_name = "Operator platform".into();

        let text = catalogue.render().unwrap();

        assert!(text.starts_with("apiVersion: fabric.fieldstate.nz/v1\nkind: Catalogue\n"));
        assert_eq!(Catalogue::parse(&text).unwrap(), catalogue);
    }

    #[test]
    fn a_document_with_no_api_version_is_refused() {
        let text = "kind: Catalogue\nspec:\n  applications: []\n";

        let error = Catalogue::parse(text).unwrap_err();

        assert!(
            matches!(error, DesiredStateError::UnknownDocumentKind { expected, .. } if expected == "fabric.fieldstate.nz/v1/Catalogue"),
            "{error}"
        );
    }

    #[test]
    fn a_document_with_the_wrong_kind_is_refused() {
        let text = "apiVersion: fabric.fieldstate.nz/v1\nkind: Client\nspec:\n  applications: []\n";

        let error = Catalogue::parse(text).unwrap_err();

        assert!(
            matches!(error, DesiredStateError::UnknownDocumentKind { expected, .. } if expected == "fabric.fieldstate.nz/v1/Catalogue"),
            "{error}"
        );
    }

    #[test]
    fn a_malformed_catalogue_does_not_say_it_is_a_client_document() {
        let text =
            "apiVersion: fabric.fieldstate.nz/v1\nkind: Catalogue\nspec:\n  applications: \"not a list\"\n";

        let error = Catalogue::parse(text).unwrap_err();

        assert!(
            matches!(error, DesiredStateError::CatalogueMalformed { .. }),
            "{error}"
        );
        assert!(!error.to_string().contains("client document"), "{error}");
    }

    #[test]
    fn a_document_with_the_wrong_version_is_refused() {
        let text = "apiVersion: fabric.fieldstate.nz/v2\nkind: Catalogue\nspec:\n  applications: []\n";

        let error = Catalogue::parse(text).unwrap_err();

        assert!(
            matches!(error, DesiredStateError::UnknownDocumentKind { expected, .. } if expected == "fabric.fieldstate.nz/v1/Catalogue"),
            "{error}"
        );
    }
}
