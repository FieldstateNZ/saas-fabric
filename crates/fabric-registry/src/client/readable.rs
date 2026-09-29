//! Whether a repository is readable through a registry, and its tags for
//! the picker (ADR 0026 section 5).
//!
//! # Why "not readable" is an answer, and every other refusal an error
//!
//! A `401`, `403` or `404` to a repository's tag listing is one answer —
//! *not readable through this registry* — because Fabric adds no distinction
//! the registry did not make. Everything else that stops a listing is not
//! that answer and must not read as it: a challenge naming a realm the
//! kind's rule refuses names both origins, an address the policy refuses
//! reads as every such refusal does, and a credential its realm refused is
//! [`RegistryError::Denied`]. So the answer is a value, and those stay
//! errors a caller can show as they are.

use fabric_platform_management::RegistryError;
use reqwest::StatusCode;

use crate::client::tags::Listing;
use crate::client::wire::TagList;
use crate::client::{bounds, OciRegistry};
use crate::errors::{rate_limited, status_failure, unreadable};
use crate::transport::bounded_body;

/// Whether a repository's tag listing answered through a registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readability<T> {
    /// It answered, with this.
    Readable(T),

    /// It answered `401`, `403` or `404`: not readable through this
    /// registry.
    NotReadable,
}

impl OciRegistry {
    /// Proves a repository is readable through this registry: its tag
    /// listing answered successfully, with the credential when it was
    /// registered for the repository.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Denied`] if the realm refused the credential itself;
    /// [`RegistryError::Refused`] for a realm, address or redirect the rules
    /// refuse, or any other status; [`RegistryError::Unavailable`] if it
    /// could not be asked.
    pub async fn prove_repository(&self, repository: &str) -> Result<Readability<()>, RegistryError> {
        let operation = "proving a repository";
        let url = self.url(repository, "tags/list?n=1");
        let response = self.get(operation, repository, &url, "application/json").await?;
        let status = response.status();

        let closed = status == StatusCode::UNAUTHORIZED
            || status == StatusCode::NOT_FOUND
            || (status == StatusCode::FORBIDDEN && !rate_limited(status, response.headers()));
        if closed {
            return Ok(Readability::NotReadable);
        }
        if !status.is_success() {
            return Err(status_failure(operation, status, response.headers()));
        }

        let body = bounded_body(response, bounds::TAG_PAGE, operation).await?;
        serde_json::from_slice::<TagList>(&body).map_err(|_| unreadable(operation))?;
        Ok(Readability::Readable(()))
    }

    /// Every tag a repository has published, for the picker, or
    /// [`Readability::NotReadable`] when its listing answered `401`, `403`
    /// or `404`, as a proof would.
    ///
    /// # Errors
    ///
    /// As [`prove_repository`](Self::prove_repository), and as
    /// [`Registry::tags`](fabric_platform_management::Registry::tags) for a
    /// listing that stops part-way.
    pub async fn version_tags(&self, repository: &str) -> Result<Readability<Vec<String>>, RegistryError> {
        Ok(match self.listing(repository).await? {
            Listing::Tags(tags) => Readability::Readable(tags),
            Listing::Absent | Listing::Closed(_) => Readability::NotReadable,
        })
    }
}
