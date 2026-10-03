//! The referrers tag schema's list of what is attached to a digest: an
//! index at the tag `sha256-<hex>`, maintained by whoever publishes.

use reqwest::StatusCode;

use fabric_platform_management::RegistryError;

use crate::client::media::{self, MANIFEST_TYPES, OCI_INDEX};
use crate::client::wire::{Descriptor, Manifest};
use crate::client::{bounds, OciRegistry};
use crate::errors::status_failure;
use crate::transport::bounded_body;

/// What the referrers tag schema holds for a digest.
pub(super) enum TagSchema {
    /// The tag does not exist: nothing is attached through it.
    Nothing,

    /// An OCI index: the list.
    Listed(Vec<Descriptor>),

    /// Something that is not an OCI index.
    NotAnIndex,
}

impl OciRegistry {
    /// The referrers tag schema's list: the index at the tag `sha256-<hex>`.
    ///
    /// Read whether or not the API answered, because a component descriptor
    /// attached before a registry started serving the API is listed only
    /// here. Not hashed: it is a list somebody maintains, and nothing in it
    /// is trusted until each manifest it names is fetched by digest.
    pub(super) async fn referrers_tag(
        &self,
        repository: &str,
        subject: &str,
    ) -> Result<TagSchema, RegistryError> {
        let operation = "reading the referrers tag";
        let tag = subject.replacen(':', "-", 1);
        let url = self.url(repository, &format!("manifests/{tag}"));
        // Every manifest type, not only an index: a registry that negotiates
        // strictly answers `404` for a stored type the request did not
        // accept, which would turn "the tag holds something else" into
        // "nothing is attached".
        let response = self.get(operation, repository, &url, MANIFEST_TYPES).await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(TagSchema::Nothing);
        }
        if !response.status().is_success() {
            return Err(status_failure(operation, response.status(), response.headers()));
        }
        if !media::is(&response, OCI_INDEX) {
            return Ok(TagSchema::NotAnIndex);
        }

        let body = bounded_body(response, bounds::MANIFEST, operation).await?;

        Ok(match serde_json::from_slice::<Manifest>(&body) {
            Ok(index)
                if index
                    .media_type
                    .as_deref()
                    .is_none_or(|stated| stated == OCI_INDEX) =>
            {
                index.manifests.map_or(TagSchema::NotAnIndex, TagSchema::Listed)
            }
            _ => TagSchema::NotAnIndex,
        })
    }
}
