//! The referrers API's list of what is attached to a digest.

use reqwest::StatusCode;

use fabric_platform_management::RegistryError;

use crate::client::link::next_page;
use crate::client::media::{self, OCI_INDEX};
use crate::client::wire::{Descriptor, Errors, Manifest};
use crate::client::{bounds, OciRegistry};
use crate::errors::{status_failure, unreadable};
use crate::transport::bounded_body;

/// What listing referrers is called in messages.
const OPERATION: &str = "listing referrers";

impl OciRegistry {
    /// The referrers API's list, or `None` where the registry does not serve
    /// the API.
    ///
    /// A `200` of any content type but an OCI index, or a `404` whose error is
    /// not `NAME_UNKNOWN`, is a registry without the API — GHCR answers
    /// `404 MANIFEST_UNKNOWN`. A `404 NAME_UNKNOWN` is a repository that does
    /// not exist, and that is an error: *nothing attached* would be a claim
    /// about a repository nobody could read.
    pub(super) async fn referrers_api(
        &self,
        repository: &str,
        subject: &str,
    ) -> Result<Option<Vec<Descriptor>>, RegistryError> {
        let mut url = self.url(repository, &format!("referrers/{subject}"));
        let mut found = Vec::new();

        for page in 0..bounds::REFERRER_PAGES {
            let response = self.get(OPERATION, repository, &url, OCI_INDEX).await?;
            let status = response.status();

            if status == StatusCode::NOT_FOUND && page == 0 {
                let body = bounded_body(response, bounds::REFERRERS, OPERATION).await?;
                let errors: Errors = serde_json::from_slice(&body).unwrap_or_default();
                if errors.errors.iter().any(|error| error.code == "NAME_UNKNOWN") {
                    return Err(RegistryError::Refused {
                        detail: format!("{OPERATION}: the repository {repository} does not exist"),
                    });
                }
                return Ok(None);
            }
            if !status.is_success() {
                return Err(status_failure(OPERATION, status, response.headers()));
            }
            if !media::is(&response, OCI_INDEX) {
                if page == 0 {
                    return Ok(None);
                }
                return Err(unreadable(OPERATION));
            }

            let next = next_page(&response, &self.origin, OPERATION)?;
            let body = bounded_body(response, bounds::REFERRERS, OPERATION).await?;
            let index: Manifest = serde_json::from_slice(&body).map_err(|_| unreadable(OPERATION))?;
            found.extend(index.manifests.unwrap_or_default());

            match next {
                Some(next) => url = next,
                None => return Ok(Some(found)),
            }
        }

        Err(RegistryError::Unavailable {
            detail: format!("{OPERATION} paged past {} pages", bounds::REFERRER_PAGES),
        })
    }
}
