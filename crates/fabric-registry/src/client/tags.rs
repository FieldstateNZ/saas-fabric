//! Listing every tag a repository has published.

use fabric_platform_management::RegistryError;

use crate::client::wire::TagList;
use crate::client::OciRegistry;
use crate::errors::{status_failure, transport_failure};

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

impl OciRegistry {
    /// Every tag, following pagination.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked, or if it paged
    /// further than [`MAX_PAGES`]. A truncated list is not returned: it would
    /// look exactly like a component whose newer versions do not exist, and
    /// discovery would quietly stop advancing.
    pub(super) async fn list_tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        let mut url = self.url(repository, &format!("tags/list?n={PAGE_SIZE}"));
        let mut found = Vec::new();

        for _ in 0..MAX_PAGES {
            let response = self
                .get("listing tags", repository, &url, "application/json")
                .await?;

            if response.status() == reqwest::StatusCode::NOT_FOUND {
                // No such repository. An empty list rather than an error: a
                // component whose image has never been published is a state
                // discovery can describe.
                return Ok(Vec::new());
            }

            if !response.status().is_success() {
                return Err(status_failure(
                    "listing tags",
                    response.status(),
                    response.headers(),
                ));
            }

            let next = next_page(&response, &self.base_url);

            let page: TagList = response
                .json()
                .await
                .map_err(|error| transport_failure("listing tags", &error))?;

            found.extend(page.tags.unwrap_or_default());

            match next {
                Some(next) => url = next,
                None => return Ok(found),
            }
        }

        Err(RegistryError::Unavailable {
            detail: format!("listing tags paged past {MAX_PAGES} pages"),
        })
    }
}

/// The `Link: <...>; rel="next"` URL, if the registry sent one.
///
/// The value is a path on the same registry, and it is resolved against the
/// base URL this client was built for rather than followed wherever it points.
/// A registry that answered with an absolute URL elsewhere would otherwise be
/// telling this adapter where to send its next request.
fn next_page(response: &reqwest::Response, base_url: &str) -> Option<String> {
    let link = response.headers().get(reqwest::header::LINK)?.to_str().ok()?;

    if !link.contains("rel=\"next\"") {
        return None;
    }

    let start = link.find('<')? + 1;
    let end = link[start..].find('>')? + start;
    let target = link.get(start..end)?;

    if target.starts_with('/') {
        return Some(format!("{base_url}{target}"));
    }

    None
}
