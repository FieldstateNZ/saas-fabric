//! Resolving a tag to a digest and the provenance baked into the image.

use fabric_platform_management::{RegistryError, Resolved};

use crate::client::wire::{Config, Manifest};
use crate::client::OciRegistry;
use crate::errors::{status_failure, transport_failure};

/// The label a build stamps its source commit into.
const REVISION: &str = "org.opencontainers.image.revision";

/// The header a registry reports a manifest's digest in.
const CONTENT_DIGEST: &str = "docker-content-digest";

/// Both the OCI and the older Docker media types, image and index.
const MANIFEST_TYPES: &str = "application/vnd.oci.image.manifest.v1+json, \
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

        let revision = self.revision_of(repository, &manifest).await?;

        Ok(Some(Resolved { digest, revision }))
    }

    /// The source commit an image was built from, if it says.
    ///
    /// For an index, the labels live on a per-architecture manifest, so one is
    /// descended into. `linux/amd64` is what the platform runs; an index
    /// carrying no such entry has nothing this could deploy, and reporting no
    /// revision leaves discovery treating the version as still publishing
    /// rather than promoting something unrunnable.
    async fn revision_of(
        &self,
        repository: &str,
        manifest: &Manifest,
    ) -> Result<Option<String>, RegistryError> {
        let config = match (&manifest.config, &manifest.manifests) {
            (Some(config), _) => config.digest.clone(),
            (None, Some(entries)) => {
                let Some(entry) = entries.iter().find(|entry| {
                    entry
                        .platform
                        .as_ref()
                        .is_some_and(|platform| platform.os == "linux" && platform.architecture == "amd64")
                }) else {
                    return Ok(None);
                };

                let url = self.url(repository, &format!("manifests/{}", entry.digest));
                let response = self
                    .get("reading a manifest", repository, &url, MANIFEST_TYPES)
                    .await?;

                if !response.status().is_success() {
                    return Ok(None);
                }

                let inner: Manifest = response
                    .json()
                    .await
                    .map_err(|error| transport_failure("reading a manifest", &error))?;

                match inner.config {
                    Some(config) => config.digest,
                    None => return Ok(None),
                }
            }
            (None, None) => return Ok(None),
        };

        let url = self.url(repository, &format!("blobs/{config}"));
        let response = self
            .get("reading an image config", repository, &url, "*/*")
            .await?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let config: Config = response
            .json()
            .await
            .map_err(|error| transport_failure("reading an image config", &error))?;

        Ok(config
            .config
            .and_then(|labels| labels.labels)
            .and_then(|labels| labels.get(REVISION).cloned()))
    }
}
