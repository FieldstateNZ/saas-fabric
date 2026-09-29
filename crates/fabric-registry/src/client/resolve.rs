//! Resolving a reference to a digest and the provenance baked into the image.

use fabric_platform_management::{RegistryError, Resolved};

use crate::client::fetch::Held;
use crate::client::reference::{reference, Reference};
use crate::client::wire::Manifest;
use crate::client::OciRegistry;
use crate::errors::unreadable;

impl OciRegistry {
    /// What a tag or a digest resolves to, or `None` if there is nothing.
    ///
    /// The digest returned is the **reference's own** manifest digest, as
    /// this client computed it, which is what a deployment should pin — for a
    /// multi-architecture image that is the index, not one platform's
    /// manifest.
    ///
    /// # Why a digest is always asked, never answered from held bytes
    ///
    /// Resolving a digest asks whether it exists in this repository *now*,
    /// and that is an answer, which is never held: bytes verified earlier —
    /// perhaps from another repository — say what the digest is, not that it
    /// is here.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked, if the
    /// reference is neither a tag nor a `sha256` digest, or if bytes and
    /// digest disagree. A missing reference is `Ok(None)`, because a version
    /// published to two of three repositories is an ordinary window and not a
    /// fault.
    pub(super) async fn resolve_reference(
        &self,
        repository: &str,
        text: &str,
    ) -> Result<Option<Resolved>, RegistryError> {
        let found = match reference(text)? {
            Reference::Tag(tag) => self.manifest_by_tag(repository, tag).await?,
            Reference::Digest(digest) => self.manifest_by_digest(repository, digest, Held::Ask).await?,
        };

        let Some(content) = found else {
            return Ok(None);
        };

        let manifest: Manifest =
            serde_json::from_slice(&content.bytes).map_err(|_| unreadable("reading a manifest"))?;
        let provenance = self.provenance_of(repository, &manifest).await?;

        Ok(Some(Resolved {
            digest: content.digest,
            provenance,
        }))
    }
}
