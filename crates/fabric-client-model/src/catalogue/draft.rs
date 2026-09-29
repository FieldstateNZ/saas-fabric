//! What saving an application's draft may say: the definition without
//! anything the server resolves (ADR 0026 section 7).
use super::{
    ApplicationDefinition, ApplicationFeature, ApplicationPlan, ApplicationResource, ConfigurationField,
    DraftComponent, NavigationItem,
};
use crate::DesiredStateError;
use serde::{Deserialize, Serialize};

/// The body of a `saveApplication` command: an [`ApplicationDefinition`]'s
/// fields, with each component in [`DraftComponent`]'s shape.
///
/// # Why a shape of its own, and not the stored definition
///
/// A described component's reference, version and resolution are what the
/// server observed when an operator selected a version; a save that could
/// carry them could forge one. So a save names a described component and
/// says only what an operator decides about it, and
/// [`into_definition`](Self::into_definition) takes everything else from
/// what is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationDraft {
    /// Display name.
    pub name: String,
    /// Product description.
    pub description: String,
    /// Optional hostname template containing `{client}`.
    pub domain: String,
    /// Deployables and platform capabilities, by kind.
    pub components: Vec<DraftComponent>,
    /// Product features and their implementation dependencies.
    pub features: Vec<ApplicationFeature>,
    /// Plans granting features.
    pub plans: Vec<ApplicationPlan>,
    /// Authored configuration fields.
    pub fields: Vec<ConfigurationField>,
    /// Client shell navigation, filtered by plan.
    pub navigation: Vec<NavigationItem>,
    /// Authored Data API resources, which a body may omit as a stored
    /// definition may.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<ApplicationResource>,
}

impl ApplicationDraft {
    /// The definition this save makes of `stored`, the draft it replaces.
    ///
    /// A described component the save names keeps its stored resolution; a
    /// component the save omits is dropped. Refused: a described component
    /// with no stored resolution -- only selecting a version creates or
    /// re-resolves one -- and any component whose stored kind differs from
    /// the one the save gives it, in either direction, even when the same
    /// save omits the old one. So turning a described component back into
    /// free text takes two saves: one that drops it, one that adds it.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError::InvalidField`] naming the component.
    pub fn into_definition(
        self,
        stored: &ApplicationDefinition,
    ) -> Result<ApplicationDefinition, DesiredStateError> {
        let components = self
            .components
            .into_iter()
            .map(|component| component.into_component(stored))
            .collect::<Result<_, _>>()?;
        Ok(ApplicationDefinition {
            name: self.name,
            description: self.description,
            domain: self.domain,
            components,
            features: self.features,
            plans: self.plans,
            fields: self.fields,
            navigation: self.navigation,
            resources: self.resources,
        })
    }
}
