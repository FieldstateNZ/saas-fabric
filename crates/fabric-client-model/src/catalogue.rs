//! Versioned application definitions and operator-managed product configuration.
mod api_version;
mod application;
mod assignment;
mod command;
mod component;
#[cfg(test)]
mod described_tests;
mod draft;
mod draft_component;
#[cfg(test)]
mod draft_tests;
mod effective;
#[cfg(test)]
mod effective_tests;
mod fields;
mod mutations;
mod product;
mod resource;
mod runtime_catalogue;
mod schema;
mod selection;
#[cfg(test)]
mod selection_tests;
mod validation;

use crate::{ClientRevision, DesiredStateError};
pub use application::*;
pub use command::CatalogueCommand;
pub use component::{ApplicationComponent, ComponentKind, ComponentResolution, UpdatePolicy};
pub use draft::ApplicationDraft;
pub use draft_component::{AuthoredComponent, DescribedDraft, DraftComponent};
pub use fields::*;
pub use product::*;
pub use resource::ApplicationResource;
pub use runtime_catalogue::{CatalogueConflict, DerivedCatalogue, DerivedResource};
use schema::Envelope;
pub use selection::ComponentSelectionError;
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
    /// then parses the `spec` it wraps, refuses a described component under
    /// `fabric.fieldstate.nz/v1` (naming both versions), and validates it.
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
        let version = schema::check_document_kind(&raw)?;
        let envelope: Envelope = serde_norway::from_value(raw).map_err(|error| malformed(&error))?;
        version.check_expresses(&envelope.spec)?;
        envelope.spec.validate()?;
        Ok(envelope.spec)
    }
    /// Serializes a validated snapshot for storage, wrapped in the same
    /// `apiVersion`/`kind` envelope every desired-state document carries, at
    /// the lowest `apiVersion` that expresses it: `v2` while any draft or
    /// release holds a described component, `v1` otherwise.
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
            matches!(error, DesiredStateError::UnknownDocumentKind { expected, .. } if expected.contains("fabric.fieldstate.nz/v1/Catalogue")),
            "{error}"
        );
    }

    #[test]
    fn a_document_with_the_wrong_kind_is_refused() {
        let text = "apiVersion: fabric.fieldstate.nz/v1\nkind: Client\nspec:\n  applications: []\n";

        let error = Catalogue::parse(text).unwrap_err();

        assert!(
            matches!(error, DesiredStateError::UnknownDocumentKind { expected, .. } if expected.contains("fabric.fieldstate.nz/v1/Catalogue")),
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
        let text = "apiVersion: fabric.fieldstate.nz/v3\nkind: Catalogue\nspec:\n  applications: []\n";

        let error = Catalogue::parse(text).unwrap_err();

        assert!(
            matches!(error, DesiredStateError::UnknownDocumentKind { expected, .. } if expected.contains("fabric.fieldstate.nz/v1/Catalogue")),
            "{error}"
        );
    }

    #[test]
    fn a_release_with_no_resources_key_parses_and_re_renders_without_inventing_one() {
        // Every release published before ADR 0023 part 3 has no `resources`
        // key at all. `#[serde(default)]` is what lets this parse; the test
        // that matters is the one after it -- that rendering the parsed
        // result back does not add the key, which would otherwise turn every
        // stored release into a diff on the catalogue's next unrelated save.
        let text = "apiVersion: fabric.fieldstate.nz/v1
kind: Catalogue
spec:
  applications:
  - id: analytics
    draft:
      name: Analytics
      description: ''
      domain: ''
      components: []
      features: []
      plans: []
      fields: []
      navigation: []
    releases: []
  clientFields: []
  settings:
    platformName: SaaS Fabric
    defaultRegion: New Zealand
    timezone: Pacific/Auckland
  environments: []
  activity: []
  definitionVersion: 0
";

        let catalogue = Catalogue::parse(text).unwrap();

        assert!(catalogue.applications[0].draft.resources.is_empty());

        let rendered = catalogue.render().unwrap();

        assert!(!rendered.contains("resources"), "{rendered}");
    }
}
