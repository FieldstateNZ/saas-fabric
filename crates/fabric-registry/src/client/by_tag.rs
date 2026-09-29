//! Resolving a tag to the manifest it points at, proven by hashing.

use reqwest::{Method, Response, StatusCode};

use fabric_platform_management::RegistryError;

use crate::client::digest::{sha256, Content};
use crate::client::fetch::Held;
use crate::client::media::MANIFEST_TYPES;
use crate::client::send::Via;
use crate::client::{bounds, OciRegistry};
use crate::errors::status_failure;
use crate::transport::bounded_body;

/// The header a registry reports a manifest's digest in.
const CONTENT_DIGEST: &str = "docker-content-digest";

/// What reading a manifest is called in messages.
const OPERATION: &str = "reading a manifest";

impl OciRegistry {
    /// The manifest `tag` points at now, or `None` if it points nowhere.
    ///
    /// # A header is a pointer, never a fact
    ///
    /// `HEAD` first, because a `HEAD` does not count against a registry's
    /// pull quota where a `GET` does. If it names a digest, that digest's
    /// bytes are what matters: held already when this client has verified
    /// them, fetched by digest and hashed otherwise. If it names none, or the
    /// registry does not answer `HEAD` (`405`), the tag's manifest is read
    /// and hashed, and a digest header on it must agree with the hash. Either
    /// way the digest returned is one this client computed.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked, if a header
    /// names a digest that is not `sha256`, or if bytes and digest disagree.
    pub(super) async fn manifest_by_tag(
        &self,
        repository: &str,
        tag: &str,
    ) -> Result<Option<Content>, RegistryError> {
        let url = self.url(repository, &format!("manifests/{tag}"));
        let head = self
            .send(
                Method::HEAD,
                Via::Api,
                OPERATION,
                repository,
                &url,
                MANIFEST_TYPES,
            )
            .await?;

        if head.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let pointer = if head.status().is_success() {
            named_digest(&head)?
        } else if head.status() == StatusCode::METHOD_NOT_ALLOWED {
            None
        } else {
            return Err(status_failure(OPERATION, head.status(), head.headers()));
        };

        if let Some(digest) = pointer {
            return self.manifest_by_digest(repository, &digest, Held::Use).await;
        }

        let response = self.get(OPERATION, repository, &url, MANIFEST_TYPES).await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(status_failure(OPERATION, response.status(), response.headers()));
        }

        let claimed = named_digest(&response)?;
        let body = bounded_body(response, bounds::MANIFEST, OPERATION).await?;
        let content = Content::hashed(body);

        if claimed.is_some_and(|claimed| claimed != content.digest) {
            // The tag passed the tag grammar before any request was built,
            // so it is safe to show.
            return Err(RegistryError::Refused {
                detail: format!("{tag} was served with a digest header that is not the digest of its bytes"),
            });
        }

        self.verified.insert(&content);

        Ok(Some(content))
    }
}

/// The digest a response's header names, if it names one.
///
/// # Errors
///
/// [`RegistryError::Refused`] for a header naming a digest this client does
/// not compute, or one that is not text.
fn named_digest(response: &Response) -> Result<Option<String>, RegistryError> {
    let Some(value) = response.headers().get(CONTENT_DIGEST) else {
        return Ok(None);
    };

    let text = value.to_str().map_err(|_| RegistryError::Refused {
        detail: "a manifest's digest header is not text".to_owned(),
    })?;

    sha256(text.trim(), "a manifest's digest header").map(|digest| Some(digest.to_owned()))
}
