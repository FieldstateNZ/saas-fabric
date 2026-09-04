//! Passing every operation through to whatever is bound.
//!
//! Delegation, and two things around it. Every call takes the read guard and
//! **keeps it until the delegated call has finished**, so an operation runs to
//! completion against the repository it started against and a rebind waits for
//! it. And every revision that leaves here is tagged with the generation it was
//! read at, while every revision that arrives on a write must carry that same
//! generation and is stripped of it before the adapter sees it.
//!
//! The repository is resolved before the tag is checked, and that order is
//! deliberate: a platform with nothing bound answers `NotConnected`, which is a
//! state an operator acts on, rather than `Conflict`, which would send them to
//! retry something that has nothing to retry against.

use crate::binding::{generation, PlatformDesiredState};
use crate::{ComponentDesired, DesiredRevision, DesiredState, DesiredStateError, Hold, Release, ReleaseUnit};

#[async_trait::async_trait]
impl DesiredState for PlatformDesiredState {
    async fn components(&self, environment: &str) -> Result<Vec<String>, DesiredStateError> {
        let live = self.live().await;

        live.repository()?.components(environment).await
    }

    async fn component(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<ComponentDesired, DesiredStateError> {
        let live = self.live().await;
        let mut desired = live.repository()?.component(environment, component).await?;

        desired.revision = generation::tag(live.generation(), &desired.revision);

        Ok(desired)
    }

    async fn advance(
        &self,
        environment: &str,
        component: &str,
        release: &Release,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        let live = self.live().await;
        let repository = live.repository()?;
        let at = generation::untag(live.generation(), at)?;

        repository
            .advance(environment, component, release, &at, message)
            .await
    }

    async fn roll_back(
        &self,
        environment: &str,
        component: &str,
        unit: &ReleaseUnit,
        hold: &Hold,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        let live = self.live().await;
        let repository = live.repository()?;
        let at = generation::untag(live.generation(), at)?;

        repository
            .roll_back(environment, component, unit, hold, &at, message)
            .await
    }

    async fn pause(
        &self,
        environment: &str,
        component: &str,
        hold: &Hold,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        let live = self.live().await;
        let repository = live.repository()?;
        let at = generation::untag(live.generation(), at)?;

        repository.pause(environment, component, hold, &at, message).await
    }

    async fn resume(
        &self,
        environment: &str,
        component: &str,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        let live = self.live().await;
        let repository = live.repository()?;
        let at = generation::untag(live.generation(), at)?;

        repository.resume(environment, component, &at, message).await
    }
}
