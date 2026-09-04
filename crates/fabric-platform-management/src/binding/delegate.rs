//! Passing every operation through to whatever is bound.
//!
//! Delegation, and two things around it. Every call hands the read guard, the
//! repository and owned copies of its arguments to a task that **keeps the
//! guard until the delegated call has an outcome** — so an operation runs to
//! completion against the repository it started against, a rebind waits for it,
//! and a caller that stops waiting cancels nothing; `binding/holding.rs` says
//! why the task is load-bearing rather than tidy. And every revision that
//! leaves here is tagged with the generation it was read at, while one arriving
//! on a write must carry that same generation and is stripped of it before the
//! adapter sees it.

use crate::binding::holding::outliving;
use crate::binding::{generation, PlatformDesiredState};
use crate::{ComponentDesired, DesiredRevision, DesiredState, DesiredStateError, Hold, Release};

#[async_trait::async_trait]
impl DesiredState for PlatformDesiredState {
    async fn components(&self, environment: &str) -> Result<Vec<String>, DesiredStateError> {
        let live = self.held().await;
        let repository = live.repository()?;
        let environment = environment.to_owned();

        outliving(live, async move { repository.components(&environment).await }).await
    }

    async fn component(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<ComponentDesired, DesiredStateError> {
        let live = self.held().await;
        let repository = live.repository()?;
        let read_at = live.generation();
        let (environment, component) = (environment.to_owned(), component.to_owned());

        let mut desired = outliving(live, async move {
            repository.component(&environment, &component).await
        })
        .await?;

        desired.revision = generation::tag(read_at, &desired.revision);

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
        let (live, repository, environment, component, at) = self.writing(environment, component, at).await?;
        let (release, message) = (release.clone(), message.to_owned());

        outliving(live, async move {
            repository
                .advance(&environment, &component, &release, &at, &message)
                .await
        })
        .await
    }

    async fn roll_back(
        &self,
        environment: &str,
        component: &str,
        release: &Release,
        hold: &Hold,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        let (live, repository, environment, component, at) = self.writing(environment, component, at).await?;
        let (release, hold, message) = (release.clone(), hold.clone(), message.to_owned());

        outliving(live, async move {
            repository
                .roll_back(&environment, &component, &release, &hold, &at, &message)
                .await
        })
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
        let (live, repository, environment, component, at) = self.writing(environment, component, at).await?;
        let (hold, message) = (hold.clone(), message.to_owned());

        outliving(live, async move {
            repository
                .pause(&environment, &component, &hold, &at, &message)
                .await
        })
        .await
    }

    async fn resume(
        &self,
        environment: &str,
        component: &str,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        let (live, repository, environment, component, at) = self.writing(environment, component, at).await?;
        let message = message.to_owned();

        outliving(live, async move {
            repository.resume(&environment, &component, &at, &message).await
        })
        .await
    }
}
