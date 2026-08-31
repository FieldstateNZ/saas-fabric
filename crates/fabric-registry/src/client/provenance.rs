//! What an artifact says about where it came from.

use fabric_platform_management::{Provenance, RegistryError};

use crate::client::resolve::MANIFEST_TYPES;
use crate::client::wire::{Config, Manifest};
use crate::client::OciRegistry;
use crate::errors::transport_failure;

/// The label a build stamps its source commit into.
const REVISION: &str = "org.opencontainers.image.revision";

impl OciRegistry {
    /// What the artifact says about where it came from.
    ///
    /// For a plain image manifest that is one label. For an index it is
    /// **every supported child**, and they must agree: reading one platform's
    /// label proves that platform's provenance, not the artifact's, and "the
    /// architecture we happen to run today" is not a fact about the image.
    ///
    /// A child is supported when it declares a real platform. Build systems
    /// put attestation manifests in the same index under
    /// `unknown/unknown` — they carry no revision, and inspecting them would
    /// make every multi-architecture image look unprovenanced.
    pub(super) async fn provenance_of(
        &self,
        repository: &str,
        manifest: &Manifest,
    ) -> Result<Provenance, RegistryError> {
        let mut agreed: Option<String> = None;

        for config in self.configs_of(repository, manifest).await? {
            let Some(revision) = self.revision_in(repository, &config).await? else {
                return Ok(Provenance::Absent);
            };

            match &agreed {
                None => agreed = Some(revision),
                Some(first) if first == &revision => {}
                Some(_) => return Ok(Provenance::Disagreed),
            }
        }

        Ok(agreed.map_or(Provenance::Absent, Provenance::Agreed))
    }

    /// The config blob of every manifest whose provenance counts.
    async fn configs_of(&self, repository: &str, manifest: &Manifest) -> Result<Vec<String>, RegistryError> {
        if let Some(config) = &manifest.config {
            return Ok(vec![config.digest.clone()]);
        }

        let Some(entries) = &manifest.manifests else {
            return Ok(Vec::new());
        };

        let mut configs = Vec::new();

        for entry in entries {
            let Some(platform) = &entry.platform else {
                continue;
            };
            if platform.os == "unknown" || platform.architecture == "unknown" {
                continue;
            }

            let url = self.url(repository, &format!("manifests/{}", entry.digest));
            let response = self
                .get("reading a manifest", repository, &url, MANIFEST_TYPES)
                .await?;

            if !response.status().is_success() {
                // A child the index names and the registry will not serve. Not
                // an absence of provenance so much as an absence of the image;
                // either way this is not something to promote.
                return Ok(Vec::new());
            }

            let inner: Manifest = response
                .json()
                .await
                .map_err(|error| transport_failure("reading a manifest", &error))?;

            match inner.config {
                Some(config) => configs.push(config.digest),
                None => return Ok(Vec::new()),
            }
        }

        Ok(configs)
    }

    /// The revision label in one config blob.
    async fn revision_in(&self, repository: &str, config: &str) -> Result<Option<String>, RegistryError> {
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
