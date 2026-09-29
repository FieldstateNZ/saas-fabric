//! Reading a blob a manifest names: following its CDN by hand, bounded,
//! hashed, size-checked, and only then held.

mod follow;

use reqwest::StatusCode;

use fabric_platform_management::RegistryError;

use crate::client::digest::{matching, sha256, Content};
use crate::client::OciRegistry;
use crate::errors::status_failure;
use crate::transport::bounded_body;

/// A blob to read: its digest, what its descriptor says its size is, and
/// the most this read will take.
pub(super) struct Blob<'a> {
    /// Its digest, as a manifest named it.
    pub(super) digest: &'a str,

    /// Its size, as a manifest declared it.
    pub(super) size: Option<u64>,

    /// The most bytes read.
    pub(super) most: usize,

    /// What is being read, for messages.
    pub(super) operation: &'a str,
}

/// What reading a blob found.
///
/// # Why a size mismatch is an answer here, not an error
///
/// Bytes that hash to their digest are the right bytes; a manifest that
/// declared another size for them is a publisher's mistake in the manifest.
/// Each caller decides what that means: an image config read for its labels
/// refuses it, while a component descriptor's layer makes that one component
/// descriptor unusable, so one publisher's fault in one version does not stop
/// a discovery pass.
pub(super) enum Found {
    /// The registry has no such blob.
    Missing,

    /// The bytes hash to the digest, and are not the size the manifest
    /// declared.
    OtherSize,

    /// The bytes, hashed, and the declared size when one was.
    Bytes(Content),
}

impl OciRegistry {
    /// A blob's bytes, or why there are none.
    ///
    /// Always one a manifest just read names, so bytes already verified may
    /// stand in: whether it exists was never the question.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked, if the digest is
    /// not `sha256`, if the declared size or the body passes the bound, if a
    /// redirect was refused, or if the bytes do not hash to the digest.
    pub(super) async fn blob(&self, repository: &str, blob: &Blob<'_>) -> Result<Found, RegistryError> {
        let digest = sha256(blob.digest, "a blob digest")?;
        let operation = blob.operation;

        if blob
            .size
            .is_some_and(|size| usize::try_from(size).map_or(true, |size| size > blob.most))
        {
            return Err(RegistryError::Refused {
                detail: format!("{operation}: its declared size is more than {} bytes", blob.most),
            });
        }

        let content = match self.verified.get(digest) {
            Some(content) => content,
            None => match self.fetch_blob(repository, digest, blob).await? {
                Some(content) => content,
                None => return Ok(Found::Missing),
            },
        };

        if blob
            .size
            .is_some_and(|size| u64::try_from(content.bytes.len()).ok() != Some(size))
        {
            return Ok(Found::OtherSize);
        }

        Ok(Found::Bytes(content))
    }

    /// One blob from the registry, following its CDN.
    async fn fetch_blob(
        &self,
        repository: &str,
        digest: &str,
        blob: &Blob<'_>,
    ) -> Result<Option<Content>, RegistryError> {
        let operation = blob.operation;
        let url = self.url(repository, &format!("blobs/{digest}"));
        let response = self.follow_blob(repository, &url, operation).await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(status_failure(operation, response.status(), response.headers()));
        }

        let body = bounded_body(response, blob.most, operation).await?;
        let content = matching(Content::hashed(body), digest, "the blob")?;
        self.verified.insert(&content);

        Ok(Some(content))
    }
}
