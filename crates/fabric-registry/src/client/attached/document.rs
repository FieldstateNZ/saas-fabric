//! The one attached component descriptor's layer, read byte for byte.

use fabric_platform_management::{Attached, AttachedDescriptor, RegistryError, Unusable};

use super::candidate::Checked;
use crate::client::blob::{Blob, Found};
use crate::client::OciRegistry;

impl OciRegistry {
    /// The one attached component descriptor's layer, byte for byte.
    ///
    /// A layer the registry does not serve, beside a manifest it does, is a
    /// component descriptor that cannot be used, not one that is absent; so
    /// is a layer whose bytes are not the size its manifest declares. Both
    /// are the publisher's fault in one version, answered as
    /// [`Unusable::Malformed`] rather than as an error that would stop every
    /// discovery pass until the artifact is deleted. Bytes that do not hash to
    /// their digest are still refused: that is not a publisher's mistake.
    pub(super) async fn document(
        &self,
        repository: &str,
        checked: Checked,
    ) -> Result<Attached, RegistryError> {
        let blob = Blob {
            digest: &checked.layer_digest,
            size: Some(checked.layer_size),
            most: fabric_component::MAX_DOCUMENT_BYTES,
            operation: "reading a component descriptor",
        };

        let content = match self.blob(repository, &blob).await? {
            Found::Missing | Found::OtherSize => {
                return Ok(Attached::Unusable {
                    reason: Unusable::Malformed,
                })
            }
            Found::Bytes(content) => content,
        };

        Ok(Attached::One(AttachedDescriptor {
            digest: checked.digest,
            artifact_type: checked.artifact_type,
            revision: checked.revision,
            version: checked.version,
            document: content.bytes.to_vec(),
        }))
    }
}
