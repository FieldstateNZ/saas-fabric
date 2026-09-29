//! The authored form: `component.yaml`, the same document without a
//! version or digests, which a publisher renders into a component
//! descriptor (ADR 0026 section 2).
mod render;
use super::{envelope, ComponentName, Repository, Role};
use crate::errors::{invalid, ContractError};
use crate::{ApplicationResource, ConfigurationField, PlatformCapability};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One image as a publisher writes it: its repository, before any digest
/// exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceImage {
    /// The image's repository, written in full.
    pub repository: Repository,
}

/// [`ComponentSpec`](super::ComponentSpec) without `version`, and with each image's repository
/// only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentSourceSpec {
    /// As [`ComponentSpec::name`](super::ComponentSpec::name).
    pub name: ComponentName,
    /// As [`ComponentSpec::title`](super::ComponentSpec::title).
    pub title: String,
    /// As [`ComponentSpec::description`](super::ComponentSpec::description).
    #[serde(default)]
    pub description: String,
    /// Every image, by role, without digests.
    pub images: BTreeMap<Role, SourceImage>,
    /// As [`ComponentSpec::capabilities`](super::ComponentSpec::capabilities).
    #[serde(default)]
    pub capabilities: Vec<PlatformCapability>,
    /// As [`ComponentSpec::fields`](super::ComponentSpec::fields).
    #[serde(default)]
    pub fields: Vec<ConfigurationField>,
    /// As [`ComponentSpec::resources`](super::ComponentSpec::resources).
    #[serde(default)]
    pub resources: Vec<ApplicationResource>,
}

/// A component's authored source, read from its repository's
/// `component.yaml`.
///
/// # Why YAML is read here and never published
///
/// This is the publisher's own file, read by the publisher's own release,
/// so it is trusted input: YAML aliases, which can amplify a small document
/// into a large one, are not a concern here. They are why what travels to a
/// server is the JSON [`render`](Self::render) produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentSource {
    /// What the component is, before it is built.
    spec: ComponentSourceSpec,
}

impl ComponentSource {
    /// Reads `component.yaml`: the envelope first, as a component descriptor
    /// checks its own, then the spec, refusing unknown fields. Nothing is
    /// validated beyond the types until [`render`](Self::render).
    ///
    /// # Errors
    ///
    /// Returns [`ContractError`] for YAML that does not parse, the wrong
    /// envelope, or a spec of the wrong shape.
    pub fn from_yaml(text: &str) -> Result<Self, ContractError> {
        let raw: serde_norway::Value = serde_norway::from_str(text).map_err(|error| malformed(&error))?;
        let string_at = |key: &str| raw.get(key).and_then(serde_norway::Value::as_str);
        envelope::check(string_at("apiVersion"), string_at("kind"))?;
        let document: envelope::Read<ComponentSourceSpec> =
            serde_norway::from_value(raw).map_err(|error| malformed(&error))?;
        Ok(Self { spec: document.spec })
    }

    /// What the source says.
    #[must_use]
    pub fn spec(&self) -> &ComponentSourceSpec {
        &self.spec
    }

    /// Each image's role and repository, in role order.
    pub fn images(&self) -> impl Iterator<Item = (&Role, &Repository)> {
        self.spec
            .images
            .iter()
            .map(|(role, image)| (role, &image.repository))
    }
}

/// A `component.yaml` that could not be read.
fn malformed(error: &serde_norway::Error) -> ContractError {
    invalid(format!("The component source could not be read: {error}"))
}
