//! A component as a save names it (ADR 0026 section 7), and what it becomes
//! beside the stored draft.
use super::validation::invalid;
use super::{ApplicationComponent, ApplicationDefinition, ComponentKind, UpdatePolicy};
use crate::{ClientId, DesiredStateError};
use serde::{Deserialize, Serialize};

/// A component as a save names it, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DraftComponent {
    /// A container image, exactly as authored.
    Container(AuthoredComponent),
    /// A chart installation, exactly as authored.
    Helm(AuthoredComponent),
    /// A platform capability, exactly as authored.
    Capability(AuthoredComponent),
    /// A described component: only what an operator decides about it. A
    /// body that carries its reference, version, resolution, a digest or
    /// declared content is refused, not ignored.
    Described(DescribedDraft),
}

/// A `container`, `helm` or `capability` component: today's shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthoredComponent {
    /// Stable key within its application.
    pub id: ClientId,
    /// Display name.
    pub name: String,
    /// OCI image, chart reference, or capability name.
    pub reference: String,
    /// Pinned artifact version or digest; empty for platform capabilities.
    pub version: String,
    /// Included for every plan.
    pub required: bool,
    /// Automatic or manual advancement intent.
    pub policy: UpdatePolicy,
}

/// A described component as a save names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DescribedDraft {
    /// Stable key within its application.
    pub id: ClientId,
    /// Display name.
    pub name: String,
    /// Included for every plan.
    pub required: bool,
    /// Automatic or manual advancement intent.
    pub policy: UpdatePolicy,
}

impl DraftComponent {
    /// The component this names, given the stored draft.
    pub(super) fn into_component(
        self,
        stored: &ApplicationDefinition,
    ) -> Result<ApplicationComponent, DesiredStateError> {
        let (id, kind) = match &self {
            Self::Container(authored) => (&authored.id, ComponentKind::Container),
            Self::Helm(authored) => (&authored.id, ComponentKind::Helm),
            Self::Capability(authored) => (&authored.id, ComponentKind::Capability),
            Self::Described(described) => (&described.id, ComponentKind::Described),
        };
        let kept = stored.components.iter().find(|component| &component.id == id);
        if let Some(kept) = kept.filter(|kept| kept.kind != kind) {
            return Err(invalid(format!(
                "Component '{id}' is {} and a save cannot make it {}: remove it in one save, then add it in another",
                kept.kind.as_str(),
                kind.as_str()
            )));
        }
        let authored = match self {
            Self::Container(authored) | Self::Helm(authored) | Self::Capability(authored) => authored,
            Self::Described(described) => {
                let resolution = kept.and_then(|kept| kept.resolution.clone()).ok_or_else(|| {
                    invalid(format!(
                        "Component '{}' is described and has no stored resolution: select a version to add it",
                        described.id
                    ))
                })?;
                return Ok(ApplicationComponent {
                    id: described.id,
                    name: described.name,
                    kind,
                    reference: resolution.repository.to_string(),
                    version: resolution.version.to_string(),
                    required: described.required,
                    policy: described.policy,
                    resolution: Some(resolution),
                });
            }
        };
        Ok(ApplicationComponent {
            id: authored.id,
            name: authored.name,
            kind,
            reference: authored.reference,
            version: authored.version,
            required: authored.required,
            policy: authored.policy,
            resolution: None,
        })
    }
}
