//! Obtaining an anonymous pull token.

use fabric_platform_management::RegistryError;

use crate::client::wire::PullToken;
use crate::client::{bounds, OciRegistry};
use crate::errors::{send_failure, status_failure, unreadable};
use crate::transport::bounded_body;

impl OciRegistry {
    /// A pull token for one repository, from the cache unless `fresh`.
    pub(super) async fn token(
        &self,
        operation: &str,
        repository: &str,
        fresh: bool,
    ) -> Result<String, RegistryError> {
        let path = self.path(repository).to_owned();

        if !fresh {
            if let Some(cached) = self.cached(&path) {
                return Ok(cached);
            }
        }

        let url = format!(
            "{base}/token?service={host}&scope=repository:{path}:pull",
            base = self.base_url,
            host = self.registry_host
        );

        let response = self
            .api
            .get(&url)
            .send()
            .await
            .map_err(|error| send_failure(operation, &error))?;

        if !response.status().is_success() {
            return Err(status_failure(operation, response.status(), response.headers()));
        }

        let body = bounded_body(response, bounds::TOKEN, operation).await?;
        let minted: PullToken = serde_json::from_slice(&body).map_err(|_| unreadable(operation))?;
        let Some(token) = minted.bearer() else {
            return Err(unreadable(operation));
        };

        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.insert(path, token.clone());
        }

        Ok(token)
    }

    /// The cached token for a repository path, if there is one.
    fn cached(&self, path: &str) -> Option<String> {
        self.tokens.lock().ok()?.get(path).cloned()
    }
}
