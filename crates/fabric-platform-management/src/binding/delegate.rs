//! Passing every operation through to whatever is bound.
//!
//! Pure delegation, and one `required()` per call. That is the whole of it: a
//! binding decides *whether* there is a repository, never what an operation
//! means once there is one.

use crate::binding::PlatformDesiredState;
use crate::{ComponentDesired, DesiredState, DesiredStateError, Hold, ReleaseUnit};

#[async_trait::async_trait]
impl DesiredState for PlatformDesiredState {
    async fn components(&self, environment: &str) -> Result<Vec<String>, DesiredStateError> {
        self.required()?.components(environment).await
    }

    async fn component(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<ComponentDesired, DesiredStateError> {
        self.required()?.component(environment, component).await
    }

    async fn advance(
        &self,
        environment: &str,
        component: &str,
        unit: &ReleaseUnit,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.required()?
            .advance(environment, component, unit, message)
            .await
    }

    async fn pause(
        &self,
        environment: &str,
        component: &str,
        hold: &Hold,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.required()?
            .pause(environment, component, hold, message)
            .await
    }

    async fn resume(
        &self,
        environment: &str,
        component: &str,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.required()?.resume(environment, component, message).await
    }
}
