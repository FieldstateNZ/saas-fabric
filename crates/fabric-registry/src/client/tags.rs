//! Listing every tag a repository has published.

use fabric_platform_management::RegistryError;

use crate::client::link::next_page;
use crate::client::wire::TagList;
use crate::client::{bounds, OciRegistry};
use crate::errors::{status_failure, unreadable};
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
        let mut url = self.url(repository, &format!("tags/list?n={PAGE_SIZE}"));
        let mut found = Vec::new();

        for page in 0..MAX_PAGES {
            let response = self.get(OPERATION, repository, &url, "application/json").await?;

            if response.status() == reqwest::StatusCode::NOT_FOUND && page == 0 {
                // No such repository. An empty list rather than an error: a
                // component whose image has never been published is a state
                // discovery can describe. Only on the first page: a later
                // page that is not there is a listing cut short, and
                // returning what came before it would be a truncated list.
                return Ok(Vec::new());
            }

            if !response.status().is_success() {
                return Err(status_failure(OPERATION, response.status(), response.headers()));
            }

            let next = next_page(&response, &self.origin, OPERATION)?;
            let body = bounded_body(response, bounds::TAG_PAGE, OPERATION).await?;
            let page: TagList = serde_json::from_slice(&body).map_err(|_| unreadable(OPERATION))?;

            found.extend(page.tags.unwrap_or_default());

            match next {
                Some(next) => url = next,
                None => return Ok(found),
            }
        }

        Err(RegistryError::Unavailable {
            detail: format!("{OPERATION} paged past {MAX_PAGES} pages"),
        })
    }
}
