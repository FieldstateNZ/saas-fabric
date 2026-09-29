//! Which repositories an environment's components are read from, without
//! reading any of them.

use std::collections::BTreeMap;

use crate::discovery::{together, Read};
use crate::service::{PlatformError, PlatformManagement};

impl PlatformManagement {
    /// Every image component of `environment`, with the registry repository
    /// each of its images is read from, by role, as desired state pins them.
    /// A chart component has no images and is not listed.
    ///
    /// # Why desired state alone
    ///
    /// The registries section says which registry each managed component is
    /// read through (ADR 0026 section 5). That is a fact about what the
    /// environment pins, not about what a registry holds, so answering it
    /// asks no registry: a page that lists registries must not cost a sweep's
    /// worth of registry reads, or fail because one registry is down.
    ///
    /// The components are read concurrently, so the answer costs about two
    /// Git reads' time rather than one per component, and the first that
    /// fails settles it: the reads still out are dropped.
    ///
    /// Changes nothing, like [`statuses`](Self::statuses).
    ///
    /// # Errors
    ///
    /// [`PlatformError`] if desired state cannot be read. One component that
    /// cannot be read fails the whole call, as it does for `statuses`: a
    /// partial answer would be read as a complete one.
    pub async fn image_repositories(
        &self,
        environment: &str,
    ) -> Result<BTreeMap<String, BTreeMap<String, String>>, PlatformError> {
        let desired_state = self.desired_state();
        let components = desired_state.components(environment).await?;
        let reads: Vec<Read<'_, _>> = components
            .iter()
            .map(|component| desired_state.component(environment, component))
            .collect();
        let answers = together(reads, |so_far| so_far.iter().flatten().any(Result::is_err)).await;

        let mut images = BTreeMap::new();
        for (component, answer) in components.into_iter().zip(answers) {
            // A read left out only when another failed, which this reaches.
            let Some(desired) = answer.transpose()? else {
                continue;
            };
            if let Some(repositories) = desired.source.image_repositories() {
                images.insert(component, repositories.clone());
            }
        }
        Ok(images)
    }
}
