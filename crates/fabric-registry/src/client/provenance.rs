//! What an artifact says about where it came from.

use std::collections::BTreeSet;

use fabric_platform_management::{Provenance, RegistryError};

use crate::client::blob::{Blob, Found};
use crate::client::fetch::Held;
use crate::client::revisions::{deployable, revisions_in, verdict};
use crate::client::wire::{Config, Manifest};
use crate::client::{bounds, OciRegistry};
use crate::errors::unreadable;

impl OciRegistry {
    /// What the artifact says about where it came from, by the rule
    /// [`Provenance`] documents (ADR 0026 section 3).
    ///
    /// An image's revisions are its config's labels and its manifest's
    /// annotations. For an index they are gathered **per deployable child**
    /// — its labels, its annotations and the index's — and every child must
    /// agree: reading one platform's label proves that platform's provenance,
    /// not the artifact's, and "the architecture we happen to run today" is
    /// not a fact about the image.
    ///
    /// A deployable child declares a concrete runtime platform; an index
    /// member that does not is not a workload image. That is the rule stated
    /// in terms of what matters rather than as a fact about any one build
    /// system, and it happens to exclude the attestation manifests Buildx
    /// writes under `unknown/unknown`.
    ///
    /// A child, or a config, the registry will not serve (`404`) is an
    /// absence of the image as much as of its provenance, and reads as
    /// [`Absent`](Provenance::Absent) — wait, rather than promote. A registry
    /// that cannot be asked is an error, never an absence.
    pub(super) async fn provenance_of(
        &self,
        repository: &str,
        manifest: &Manifest,
    ) -> Result<Provenance, RegistryError> {
        if manifest.config.is_some() {
            return Ok(match self.image_revisions(repository, manifest).await? {
                Some(revisions) => verdict(&[revisions]),
                None => Provenance::Absent,
            });
        }

        let Some(entries) = &manifest.manifests else {
            return Ok(Provenance::Absent);
        };

        let shared = revisions_in(manifest.annotations.as_ref());
        let mut children = Vec::new();

        for entry in entries.iter().filter(|entry| deployable(entry)) {
            let Some(content) = self
                .manifest_by_digest(repository, &entry.digest, Held::Use)
                .await?
            else {
                return Ok(Provenance::Absent);
            };
            let child: Manifest =
                serde_json::from_slice(&content.bytes).map_err(|_| unreadable("reading a manifest"))?;

            if child.config.is_none() {
                // A deployable entry that is not itself an image.
                return Ok(Provenance::Absent);
            }
            let Some(mut revisions) = self.image_revisions(repository, &child).await? else {
                return Ok(Provenance::Absent);
            };
            revisions.extend(shared.iter().cloned());
            children.push(revisions);
        }

        Ok(verdict(&children))
    }

    /// One image manifest's revisions, or `None` if its config is not served.
    async fn image_revisions(
        &self,
        repository: &str,
        manifest: &Manifest,
    ) -> Result<Option<BTreeSet<String>>, RegistryError> {
        let mut revisions = revisions_in(manifest.annotations.as_ref());
        let Some(config) = &manifest.config else {
            return Ok(Some(revisions));
        };

        let operation = "reading an image config";
        let blob = Blob {
            digest: &config.digest,
            size: config.size,
            most: bounds::CONFIG,
            operation,
        };
        let content = match self.blob(repository, &blob).await? {
            Found::Missing => return Ok(None),
            Found::OtherSize => {
                return Err(RegistryError::Refused {
                    detail: format!("{operation}: the config is not the size its manifest declares"),
                })
            }
            Found::Bytes(content) => content,
        };

        let parsed: Config = serde_json::from_slice(&content.bytes).map_err(|_| RegistryError::Refused {
            detail: format!("{operation}: the config is not JSON this adapter reads"),
        })?;
        let labelled = parsed.config.and_then(|inner| inner.labels);
        revisions.extend(revisions_in(labelled.as_ref()));

        Ok(Some(revisions))
    }
}
