//! Resolving a tag to a digest and the provenance baked into the image.

use fabric_platform_management::{RegistryError, Resolved};

use crate::client::wire::Manifest;
use crate::client::OciRegistry;
use crate::errors::{status_failure, transport_failure};

/// The header a registry reports a manifest's digest in.
const CONTENT_DIGEST: &str = "docker-content-digest";

/// Both the OCI and the older Docker media types, image and index.
pub(super) const MANIFEST_TYPES: &str = "application/vnd.oci.image.manifest.v1+json, \
     application/vnd.docker.distribution.manifest.v2+json, \
     application/vnd.oci.image.index.v1+json, \
     application/vnd.docker.distribution.manifest.list.v2+json";

impl OciRegistry {
    /// What a tag resolves to, or `None` if it is not published.
    ///
    /// The digest returned is the **tag's own** manifest digest, which is what
    /// a deployment should pin — for a multi-architecture image that is the
    /// index, not one platform's manifest.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked. A missing tag is
    /// `Ok(None)`, because a version published to two of three repositories is
    /// an ordinary window and not a fault.
    pub(super) async fn resolve_tag(
        &self,
        repository: &str,
        tag: &str,
    ) -> Result<Option<Resolved>, RegistryError> {
        let url = self.url(repository, &format!("manifests/{tag}"));
        let response = self
            .get("reading a manifest", repository, &url, MANIFEST_TYPES)
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(status_failure(
                "reading a manifest",
                response.status(),
                response.headers(),
            ));
        }

        let digest = response
            .headers()
            .get(CONTENT_DIGEST)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);

        let manifest: Manifest = response
            .json()
            .await
            .map_err(|error| transport_failure("reading a manifest", &error))?;

        let Some(digest) = digest else {
            return Err(RegistryError::Refused {
                detail: format!("{tag} was returned without a content digest"),
            });
        };

        let provenance = self.provenance_of(repository, &manifest).await?;

        Ok(Some(Resolved { digest, provenance }))
    }
}
