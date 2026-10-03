//! Rendering a component's source into its component descriptor.
use super::ComponentSource;
use crate::descriptor::{ComponentDescriptor, ComponentSpec, ComponentVersion, Digest, ImageReference, Role};
use crate::errors::{invalid, ContractError};
use std::collections::BTreeMap;

impl ComponentSource {
    /// Renders the component descriptor for `version`, given the digest
    /// each role's image was pushed at.
    ///
    /// # Errors
    ///
    /// Returns [`ContractError::Invalid`] when `digests` does not name
    /// exactly the source's roles (naming each missing and extra one), for
    /// a version that is not one, and for a descriptor that breaks a rule,
    /// including one whose canonical bytes exceed
    /// [`MAX_DOCUMENT_BYTES`](crate::MAX_DOCUMENT_BYTES).
    pub fn render(
        &self,
        version: &str,
        digests: &BTreeMap<Role, Digest>,
    ) -> Result<ComponentDescriptor, ContractError> {
        let missing = names(
            self.spec
                .images
                .keys()
                .filter(|role| !digests.contains_key(*role)),
        );
        let extra = names(
            digests
                .keys()
                .filter(|role| !self.spec.images.contains_key(*role)),
        );
        if !missing.is_empty() || !extra.is_empty() {
            return Err(invalid(format!(
                "Digests must name exactly the component's roles; missing: [{missing}]; extra: [{extra}]"
            )));
        }
        // The roles were just checked equal, and both maps iterate in role
        // order, so zipping pairs each image with its own digest.
        let images = self
            .spec
            .images
            .iter()
            .zip(digests.values())
            .map(|((role, image), digest)| {
                let reference = ImageReference {
                    repository: image.repository.clone(),
                    digest: digest.clone(),
                };
                (role.clone(), reference)
            })
            .collect();
        let spec = self.spec.clone();
        ComponentDescriptor::new(ComponentSpec {
            name: spec.name,
            title: spec.title,
            description: spec.description,
            version: ComponentVersion::try_new(version)?,
            images,
            capabilities: spec.capabilities,
            fields: spec.fields,
            resources: spec.resources,
        })
    }
}

/// Roles as a comma-separated list, for a message.
fn names<'a>(roles: impl Iterator<Item = &'a Role>) -> String {
    roles.map(Role::as_str).collect::<Vec<_>>().join(", ")
}
