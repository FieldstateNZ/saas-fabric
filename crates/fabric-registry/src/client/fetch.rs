//! Reading a manifest by digest: fetched, bounded, hashed, and only then held.

use reqwest::StatusCode;

use fabric_platform_management::RegistryError;

use crate::client::digest::{matching, sha256, Content};
use crate::client::media::MANIFEST_TYPES;
use crate::client::{bounds, OciRegistry};
use crate::errors::status_failure;
use crate::transport::bounded_body;

/// Whether bytes already verified may stand in for a fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Held {
    /// Yes: something fresh in this same call — a `HEAD`, or a manifest just
    /// read that names this digest — already said this digest is the one
    /// wanted, so only its bytes are in question.
    Use,

    /// No: whether the digest exists in the repository *now* is part of the
    /// question, and that is an answer, which is never held.
    Ask,
}

impl OciRegistry {
    /// The manifest at `digest`, or `None` if the registry has none.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked, if `digest` is
    /// not `sha256`, if the body passes its bound, or if it does not hash to
    /// `digest`.
    pub(super) async fn manifest_by_digest(
        &self,
        repository: &str,
        digest: &str,
        held: Held,
    ) -> Result<Option<Content>, RegistryError> {
        let digest = sha256(digest, "a manifest digest")?;
        if held == Held::Use {
            if let Some(content) = self.verified.get(digest) {
                return Ok(Some(content));
            }
        }

        let operation = "reading a manifest";
        let url = self.url(repository, &format!("manifests/{digest}"));
        let response = self.get(operation, repository, &url, MANIFEST_TYPES).await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(status_failure(operation, response.status(), response.headers()));
        }

        let body = bounded_body(response, bounds::MANIFEST, operation).await?;
        let content = matching(Content::hashed(body), digest, "the manifest")?;
        self.verified.insert(&content);

        Ok(Some(content))
    }
}
