//! Listing every tag a repository has published.

use fabric_platform_management::RegistryError;
use reqwest::StatusCode;

use crate::client::link::next_page;
use crate::client::wire::TagList;
use crate::client::{bounds, OciRegistry};
use crate::errors::{rate_limited, status_failure, unreadable};
use crate::transport::bounded_body;

/// How many pages are followed before giving up.
///
/// A bound rather than a `while true`: a registry answering with a `Link` that
/// points at itself would otherwise be an infinite loop inside a discovery
/// pass. At the page size asked for below this is far more tags than any
/// component here will have, and exhausting it is reported rather than
/// silently truncating the answer.
const MAX_PAGES: usize = 50;

/// Tags per page.
const PAGE_SIZE: usize = 100;

/// What listing tags is called in messages.
const OPERATION: &str = "listing tags";

/// What a repository's listing answered.
pub(super) enum Listing {
    /// Every tag, across every page.
    Tags(Vec<String>),

    /// Its first page answered `404`: no such repository.
    Absent,

    /// Its first page answered `401` or `403` — closed to this client — as
    /// the error [`Registry::tags`](fabric_platform_management::Registry::tags)
    /// reports it with.
    Closed(RegistryError),
}

impl OciRegistry {
    /// Every tag, following pagination on the registry's own origin.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked, if a page passed
    /// its bound, if a `Link` named another origin, or if it paged further
    /// than [`MAX_PAGES`]. A truncated list is not returned: it would look
    /// exactly like a component whose newer versions do not exist, and
    /// discovery would quietly stop advancing.
    pub(super) async fn list_tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        match self.listing(repository).await? {
            Listing::Tags(tags) => Ok(tags),
            // No such repository. An empty list rather than an error: a
            // component whose image has never been published is a state
            // discovery can describe.
            Listing::Absent => Ok(Vec::new()),
            Listing::Closed(refused) => Err(refused),
        }
    }

    /// The listing, with a first page that answered `401`, `403` or `404`
    /// told apart from every other failure.
    ///
    /// # Errors
    ///
    /// As [`list_tags`](Self::list_tags), for everything else.
    pub(super) async fn listing(&self, repository: &str) -> Result<Listing, RegistryError> {
        let mut url = self.url(repository, &format!("tags/list?n={PAGE_SIZE}"));
        let mut found = Vec::new();

        for page in 0..MAX_PAGES {
            let response = self.get(OPERATION, repository, &url, "application/json").await?;
            let status = response.status();

            // Only on the first page: a later page that is not there, or
            // not readable, is a listing cut short, and returning what came
            // before it would be a truncated list.
            if page == 0 && status == StatusCode::NOT_FOUND {
                return Ok(Listing::Absent);
            }
            let closed = status == StatusCode::UNAUTHORIZED
                || (status == StatusCode::FORBIDDEN && !rate_limited(status, response.headers()));
            if page == 0 && closed {
                return Ok(Listing::Closed(status_failure(
                    OPERATION,
                    status,
                    response.headers(),
                )));
            }

            if !status.is_success() {
                return Err(status_failure(OPERATION, status, response.headers()));
            }

            let next = next_page(&response, &self.origin, OPERATION)?;
            let body = bounded_body(response, bounds::TAG_PAGE, OPERATION).await?;
            let page: TagList = serde_json::from_slice(&body).map_err(|_| unreadable(OPERATION))?;

            found.extend(page.tags.unwrap_or_default());

            match next {
                Some(next) => url = next,
                None => return Ok(Listing::Tags(found)),
            }
        }

        Err(RegistryError::Unavailable {
            detail: format!("{OPERATION} paged past {MAX_PAGES} pages"),
        })
    }
}
