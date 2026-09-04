//! Implementing the desired-state port over the platform repository.
//!
//! Every method here is wrapped in the repository's operation budget. That is
//! the port's half of a contract the trait states: the platform binding holds a
//! lock across these calls, so one that ran unboundedly would block an
//! operator's disconnect for as long as a failing Git host felt like taking.
//! See [`within_budget`](PlatformGitRepository::within_budget).

use fabric_platform_management::{
    ComponentDesired, DesiredRevision, DesiredState, DesiredStateError, Hold, Release, ReleaseUnit,
};

use crate::PlatformGitRepository;

mod budget;
mod errors;
mod reading;
mod wanted;

use wanted::{unit_from, wanted_from};

#[async_trait::async_trait]
impl DesiredState for PlatformGitRepository {
    async fn components(&self, environment: &str) -> Result<Vec<String>, DesiredStateError> {
        self.within_budget(async {
            let manifest = self.components_manifest(environment).await?;

            Ok(manifest.components.keys().cloned().collect())
        })
        .await
    }

    async fn component(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<ComponentDesired, DesiredStateError> {
        self.within_budget(self.read_component(environment, component))
            .await
    }

    async fn advance(
        &self,
        environment: &str,
        component: &str,
        release: &Release,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.within_budget(async {
            self.set_component_desired_state(environment, component, &wanted_from(release), at, message)
                .await?;

            Ok(())
        })
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
        self.within_budget(async {
            self.roll_back_component(environment, component, &unit_from(unit), hold, at, message)
                .await?;

            Ok(())
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
        self.within_budget(async {
            self.set_component_hold(environment, component, Some(hold), at, message)
                .await?;

            Ok(())
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
        self.within_budget(async {
            self.set_component_hold(environment, component, None, at, message)
                .await?;

            Ok(())
        })
        .await
    }
}
